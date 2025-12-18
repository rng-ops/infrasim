//! Traffic shaping for QoS profiles
//!
//! Provides userspace traffic shaping including:
//! - Latency injection
//! - Jitter simulation
//! - Packet loss
//! - Bandwidth limiting
//! - Packet padding

use crate::{types::QosProfileSpec, Result};
use parking_lot::Mutex;
use rand::Rng;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, trace};

/// Traffic shaper that applies QoS rules to network traffic
#[derive(Clone)]
pub struct TrafficShaper {
    config: Arc<Mutex<QosProfileSpec>>,
    stats: Arc<Mutex<TrafficStats>>,
    token_bucket: Arc<Mutex<TokenBucket>>,
}

impl TrafficShaper {
    /// Create a new traffic shaper with the given QoS profile
    pub fn new(spec: QosProfileSpec) -> Self {
        let rate_limit = if spec.rate_limit_mbps > 0 {
            spec.rate_limit_mbps as u64 * 1_000_000 / 8 // Convert Mbps to bytes/sec
        } else {
            u64::MAX // Unlimited
        };

        let burst_size = if spec.burst_size_kb > 0 {
            spec.burst_size_kb as u64 * 1024
        } else {
            rate_limit / 10 // Default 100ms worth of tokens
        };

        Self {
            config: Arc::new(Mutex::new(spec)),
            stats: Arc::new(Mutex::new(TrafficStats::default())),
            token_bucket: Arc::new(Mutex::new(TokenBucket::new(rate_limit, burst_size))),
        }
    }

    /// Update the QoS profile
    pub fn update_config(&self, spec: QosProfileSpec) {
        *self.config.lock() = spec;
    }

    /// Get current statistics
    pub fn stats(&self) -> TrafficStats {
        self.stats.lock().clone()
    }

    /// Process an outgoing packet, applying shaping rules
    /// Returns the delay to apply before sending, and whether to drop
    pub async fn shape_packet(&self, packet_size: usize) -> ShapingDecision {
        let config = self.config.lock().clone();
        let mut stats = self.stats.lock();

        stats.packets_total += 1;
        stats.bytes_total += packet_size as u64;

        // Check for packet loss
        if config.loss_percent > 0.0 {
            let mut rng = rand::thread_rng();
            if rng.gen::<f32>() * 100.0 < config.loss_percent {
                stats.packets_dropped += 1;
                debug!("Dropping packet due to simulated loss");
                return ShapingDecision::Drop;
            }
        }

        // Calculate delay (latency + jitter)
        let mut delay_ms = config.latency_ms;
        if config.jitter_ms > 0 {
            let mut rng = rand::thread_rng();
            let jitter = rng.gen_range(0..=config.jitter_ms);
            delay_ms = delay_ms.saturating_add(jitter);
        }

        // Apply rate limiting via token bucket
        let mut token_bucket = self.token_bucket.lock();
        token_bucket.refill();

        let actual_size = if config.packet_padding_bytes > 0 {
            packet_size + config.packet_padding_bytes as usize
        } else {
            packet_size
        };

        let rate_delay = if !token_bucket.consume(actual_size as u64) {
            // Calculate how long to wait for tokens
            let tokens_needed = actual_size as u64 - token_bucket.tokens;
            let wait_time = (tokens_needed * 1000) / token_bucket.rate.max(1);
            stats.packets_delayed += 1;
            wait_time as u32
        } else {
            0
        };

        let total_delay = delay_ms + rate_delay;

        if config.packet_padding_bytes > 0 {
            ShapingDecision::SendPadded {
                delay: Duration::from_millis(total_delay as u64),
                padding: config.packet_padding_bytes as usize,
            }
        } else if total_delay > 0 {
            ShapingDecision::Delay(Duration::from_millis(total_delay as u64))
        } else {
            ShapingDecision::Send
        }
    }

    /// Create a shaping channel pair for async packet processing
    pub fn create_channel(
        &self,
        buffer_size: usize,
    ) -> (TrafficShaperTx, TrafficShaperRx) {
        let (tx, rx) = mpsc::channel(buffer_size);
        (
            TrafficShaperTx {
                shaper: self.clone(),
                tx,
            },
            TrafficShaperRx { rx },
        )
    }
}

/// Shaping decision for a packet
#[derive(Debug, Clone)]
pub enum ShapingDecision {
    /// Send immediately
    Send,
    /// Delay before sending
    Delay(Duration),
    /// Send with padding
    SendPadded { delay: Duration, padding: usize },
    /// Drop the packet
    Drop,
}

/// Traffic statistics
#[derive(Debug, Clone, Default)]
pub struct TrafficStats {
    pub packets_total: u64,
    pub packets_dropped: u64,
    pub packets_delayed: u64,
    pub bytes_total: u64,
}

/// Token bucket for rate limiting
struct TokenBucket {
    tokens: u64,
    max_tokens: u64,
    rate: u64, // tokens per second
    last_refill: Instant,
}

impl TokenBucket {
    fn new(rate: u64, burst_size: u64) -> Self {
        Self {
            tokens: burst_size,
            max_tokens: burst_size,
            rate,
            last_refill: Instant::now(),
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let new_tokens = (elapsed.as_micros() as u64 * self.rate) / 1_000_000;
        
        if new_tokens > 0 {
            self.tokens = (self.tokens + new_tokens).min(self.max_tokens);
            self.last_refill = now;
        }
    }

    fn consume(&mut self, tokens: u64) -> bool {
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }
}

/// Transmit side of shaping channel
pub struct TrafficShaperTx {
    shaper: TrafficShaper,
    tx: mpsc::Sender<ShapedPacket>,
}

impl TrafficShaperTx {
    /// Send a packet through the shaper
    pub async fn send(&self, data: Vec<u8>) -> Result<()> {
        let decision = self.shaper.shape_packet(data.len()).await;
        
        match decision {
            ShapingDecision::Drop => {
                trace!("Packet dropped by shaper");
                Ok(())
            }
            ShapingDecision::Send => {
                self.tx.send(ShapedPacket { data, padding: 0 }).await
                    .map_err(|_| crate::Error::NetworkError("Channel closed".to_string()))?;
                Ok(())
            }
            ShapingDecision::Delay(delay) => {
                tokio::time::sleep(delay).await;
                self.tx.send(ShapedPacket { data, padding: 0 }).await
                    .map_err(|_| crate::Error::NetworkError("Channel closed".to_string()))?;
                Ok(())
            }
            ShapingDecision::SendPadded { delay, padding } => {
                if delay > Duration::ZERO {
                    tokio::time::sleep(delay).await;
                }
                self.tx.send(ShapedPacket { data, padding }).await
                    .map_err(|_| crate::Error::NetworkError("Channel closed".to_string()))?;
                Ok(())
            }
        }
    }
}

/// Receive side of shaping channel
pub struct TrafficShaperRx {
    rx: mpsc::Receiver<ShapedPacket>,
}

impl TrafficShaperRx {
    /// Receive the next shaped packet
    pub async fn recv(&mut self) -> Option<ShapedPacket> {
        self.rx.recv().await
    }
}

/// A shaped packet with optional padding
#[derive(Debug, Clone)]
pub struct ShapedPacket {
    pub data: Vec<u8>,
    pub padding: usize,
}

impl ShapedPacket {
    /// Get the total size including padding
    pub fn total_size(&self) -> usize {
        self.data.len() + self.padding
    }

    /// Convert to wire format with padding
    pub fn to_wire(&self) -> Vec<u8> {
        let mut wire = self.data.clone();
        if self.padding > 0 {
            wire.extend(std::iter::repeat(0u8).take(self.padding));
        }
        wire
    }
}

/// LoRaWAN network simulator
pub mod lora {
    use super::*;
    use crate::types::LoRaDeviceSpec;

    /// LoRaWAN region configuration
    #[derive(Debug, Clone)]
    pub struct LoRaRegion {
        pub name: &'static str,
        pub frequencies: Vec<f64>,
        pub max_eirp_dbm: f64,
        pub duty_cycle: f64,
    }

    impl LoRaRegion {
        /// EU868 region configuration
        pub fn eu868() -> Self {
            Self {
                name: "EU868",
                frequencies: vec![868.1, 868.3, 868.5],
                max_eirp_dbm: 16.0,
                duty_cycle: 0.01, // 1%
            }
        }

        /// US915 region configuration
        pub fn us915() -> Self {
            Self {
                name: "US915",
                frequencies: (0..72).map(|i| 902.3 + (i as f64 * 0.2)).collect(),
                max_eirp_dbm: 30.0,
                duty_cycle: 1.0, // No duty cycle limit
            }
        }

        /// Get region by name
        pub fn by_name(name: &str) -> Option<Self> {
            match name.to_uppercase().as_str() {
                "EU868" => Some(Self::eu868()),
                "US915" => Some(Self::us915()),
                _ => None,
            }
        }
    }

    /// LoRa packet with RF simulation
    #[derive(Debug, Clone)]
    pub struct LoRaPacket {
        pub payload: Vec<u8>,
        pub spreading_factor: u8,
        pub bandwidth_khz: u32,
        pub frequency_mhz: f64,
        pub rssi_dbm: f32,
        pub snr_db: f32,
        pub time_on_air_ms: u64,
    }

    impl LoRaPacket {
        /// Calculate time on air for a LoRa packet
        pub fn calculate_toa(payload_len: usize, sf: u8, bw_khz: u32) -> u64 {
            // Simplified LoRa time-on-air calculation
            let n_symbol = 8.0 + ((8.0 * payload_len as f64 - 4.0 * sf as f64 + 28.0) / (4.0 * sf as f64)).ceil().max(0.0) * 5.0;
            let t_symbol = (2.0_f64.powi(sf as i32)) / (bw_khz as f64);
            (n_symbol * t_symbol) as u64
        }
    }

    /// LoRa network simulator
    pub struct LoRaSimulator {
        region: LoRaRegion,
        devices: Vec<LoRaDeviceSpec>,
    }

    impl LoRaSimulator {
        /// Create a new simulator for the given region
        pub fn new(region: LoRaRegion) -> Self {
            Self {
                region,
                devices: Vec::new(),
            }
        }

        /// Add a device to the simulation
        pub fn add_device(&mut self, spec: LoRaDeviceSpec) {
            self.devices.push(spec);
        }

        /// Simulate packet transmission with RF effects
        pub async fn transmit(&self, packet: &mut LoRaPacket, spec: &LoRaDeviceSpec) -> bool {
            let mut rng = rand::thread_rng();

            // Simulate path loss based on distance (simplified)
            let path_loss_db: f32 = rng.gen_range(60.0..120.0);
            packet.rssi_dbm = -30.0 - path_loss_db;

            // Calculate SNR
            packet.snr_db = packet.rssi_dbm + 125.0; // Simplified

            // Apply configured loss rate
            if rng.gen::<f32>() < spec.loss_rate {
                return false;
            }

            // Apply latency
            if spec.latency_ms > 0 {
                tokio::time::sleep(Duration::from_millis(spec.latency_ms as u64)).await;
            }

            // Calculate time on air
            packet.time_on_air_ms = LoRaPacket::calculate_toa(
                packet.payload.len(),
                spec.spreading_factor as u8,
                spec.bandwidth_khz,
            );

            // Simulate transmission time
            tokio::time::sleep(Duration::from_millis(packet.time_on_air_ms)).await;

            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_no_shaping() {
        let spec = QosProfileSpec::default();
        let shaper = TrafficShaper::new(spec);

        let decision = shaper.shape_packet(100).await;
        assert!(matches!(decision, ShapingDecision::Send));
    }

    #[tokio::test]
    async fn test_latency_shaping() {
        let spec = QosProfileSpec {
            latency_ms: 50,
            ..Default::default()
        };
        let shaper = TrafficShaper::new(spec);

        let decision = shaper.shape_packet(100).await;
        match decision {
            ShapingDecision::Delay(d) => {
                assert!(d.as_millis() >= 50);
            }
            _ => panic!("Expected Delay decision"),
        }
    }

    #[tokio::test]
    async fn test_packet_loss() {
        let spec = QosProfileSpec {
            loss_percent: 100.0, // 100% loss
            ..Default::default()
        };
        let shaper = TrafficShaper::new(spec);

        let decision = shaper.shape_packet(100).await;
        assert!(matches!(decision, ShapingDecision::Drop));
    }

    #[tokio::test]
    async fn test_packet_padding() {
        let spec = QosProfileSpec {
            packet_padding_bytes: 64,
            ..Default::default()
        };
        let shaper = TrafficShaper::new(spec);

        let decision = shaper.shape_packet(100).await;
        match decision {
            ShapingDecision::SendPadded { padding, .. } => {
                assert_eq!(padding, 64);
            }
            _ => panic!("Expected SendPadded decision"),
        }
    }

    #[test]
    fn test_lora_toa() {
        use lora::LoRaPacket;
        
        let toa = LoRaPacket::calculate_toa(10, 7, 125);
        assert!(toa > 0);
    }
}
