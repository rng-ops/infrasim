//! Core types for InfraSim

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Resource metadata common to all resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMeta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub annotations: HashMap<String, String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub generation: i64,
}

impl ResourceMeta {
    pub fn new(name: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            labels: HashMap::new(),
            annotations: HashMap::new(),
            created_at: now,
            updated_at: now,
            generation: 1,
        }
    }

    pub fn with_labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels = labels;
        self
    }

    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp();
        self.generation += 1;
    }
}

/// VM state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmState {
    Pending,
    Running,
    Stopped,
    Paused,
    Error,
}

impl Default for VmState {
    fn default() -> Self {
        Self::Pending
    }
}

impl std::fmt::Display for VmState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmState::Pending => write!(f, "pending"),
            VmState::Running => write!(f, "running"),
            VmState::Stopped => write!(f, "stopped"),
            VmState::Paused => write!(f, "paused"),
            VmState::Error => write!(f, "error"),
        }
    }
}

/// Network mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    User,
    VmnetShared,
    VmnetBridged,
}

impl Default for NetworkMode {
    fn default() -> Self {
        Self::User
    }
}

/// Volume kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VolumeKind {
    Disk,
    Weights,
}

impl Default for VolumeKind {
    fn default() -> Self {
        Self::Disk
    }
}

/// VM specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSpec {
    pub arch: String,
    pub machine: String,
    pub cpu_cores: u32,
    pub memory_mb: u64,
    #[serde(default)]
    pub volume_ids: Vec<String>,
    #[serde(default)]
    pub network_ids: Vec<String>,
    pub qos_profile_id: Option<String>,
    #[serde(default)]
    pub enable_tpm: bool,
    pub boot_disk_id: Option<String>,
    #[serde(default)]
    pub extra_args: HashMap<String, String>,
    #[serde(default)]
    pub compatibility_mode: bool,
}

impl Default for VmSpec {
    fn default() -> Self {
        Self {
            arch: "aarch64".to_string(),
            machine: "virt".to_string(),
            cpu_cores: 2,
            memory_mb: 2048,
            volume_ids: Vec::new(),
            network_ids: Vec::new(),
            qos_profile_id: None,
            enable_tpm: false,
            boot_disk_id: None,
            extra_args: HashMap::new(),
            compatibility_mode: false,
        }
    }
}

/// VM status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmStatus {
    pub state: VmState,
    pub qemu_pid: Option<u32>,
    pub qmp_socket: Option<String>,
    pub vnc_display: Option<String>,
    pub error_message: Option<String>,
    pub uptime_seconds: u64,
}

impl Default for VmStatus {
    fn default() -> Self {
        Self {
            state: VmState::Pending,
            qemu_pid: None,
            qmp_socket: None,
            vnc_display: None,
            error_message: None,
            uptime_seconds: 0,
        }
    }
}

/// Virtual machine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vm {
    pub meta: ResourceMeta,
    pub spec: VmSpec,
    pub status: VmStatus,
}

/// Network specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSpec {
    pub mode: NetworkMode,
    pub cidr: String,
    pub gateway: Option<String>,
    pub dns: Option<String>,
    #[serde(default = "default_true")]
    pub dhcp_enabled: bool,
    #[serde(default = "default_mtu")]
    pub mtu: u32,
}

fn default_true() -> bool {
    true
}

fn default_mtu() -> u32 {
    1500
}

impl Default for NetworkSpec {
    fn default() -> Self {
        Self {
            mode: NetworkMode::User,
            cidr: "10.42.0.0/24".to_string(),
            gateway: Some("10.42.0.1".to_string()),
            dns: Some("10.42.0.1".to_string()),
            dhcp_enabled: true,
            mtu: 1500,
        }
    }
}

/// Network status
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub active: bool,
    pub bridge_interface: Option<String>,
    pub connected_vms: u32,
}

/// Network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Network {
    pub meta: ResourceMeta,
    pub spec: NetworkSpec,
    pub status: NetworkStatus,
}

/// QoS profile specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QosProfileSpec {
    #[serde(default)]
    pub latency_ms: u32,
    #[serde(default)]
    pub jitter_ms: u32,
    #[serde(default)]
    pub loss_percent: f32,
    #[serde(default)]
    pub rate_limit_mbps: u32,
    #[serde(default)]
    pub packet_padding_bytes: u32,
    #[serde(default)]
    pub burst_shaping: bool,
    #[serde(default)]
    pub burst_size_kb: u32,
}

impl Default for QosProfileSpec {
    fn default() -> Self {
        Self {
            latency_ms: 0,
            jitter_ms: 0,
            loss_percent: 0.0,
            rate_limit_mbps: 0,
            packet_padding_bytes: 0,
            burst_shaping: false,
            burst_size_kb: 0,
        }
    }
}

/// QoS profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QosProfile {
    pub meta: ResourceMeta,
    pub spec: QosProfileSpec,
}

/// Integrity configuration for volumes
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntegrityConfig {
    pub scheme: String,
    #[serde(with = "base64_bytes", default)]
    pub public_key: Vec<u8>,
    #[serde(with = "base64_bytes", default)]
    pub signature: Vec<u8>,
    pub expected_digest: Option<String>,
}

mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s.is_empty() {
            return Ok(Vec::new());
        }
        STANDARD.decode(&s).map_err(serde::de::Error::custom)
    }
}

/// Volume specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSpec {
    pub kind: VolumeKind,
    pub source: String,
    #[serde(default)]
    pub integrity: IntegrityConfig,
    #[serde(default)]
    pub read_only: bool,
    pub size_bytes: Option<u64>,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default)]
    pub overlay: bool,
}

fn default_format() -> String {
    "qcow2".to_string()
}

impl Default for VolumeSpec {
    fn default() -> Self {
        Self {
            kind: VolumeKind::Disk,
            source: String::new(),
            integrity: IntegrityConfig::default(),
            read_only: false,
            size_bytes: None,
            format: "qcow2".to_string(),
            overlay: false,
        }
    }
}

/// Volume status
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VolumeStatus {
    pub ready: bool,
    pub local_path: Option<String>,
    pub digest: Option<String>,
    pub actual_size: u64,
    pub verified: bool,
}

/// Volume
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub meta: ResourceMeta,
    pub spec: VolumeSpec,
    pub status: VolumeStatus,
}

/// Console specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleSpec {
    pub vm_id: String,
    #[serde(default = "default_true")]
    pub enable_vnc: bool,
    pub vnc_port: Option<u16>,
    #[serde(default = "default_true")]
    pub enable_web: bool,
    pub web_port: Option<u16>,
    pub auth_token: Option<String>,
}

/// Console status
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsoleStatus {
    pub active: bool,
    pub vnc_host: Option<String>,
    pub vnc_port: Option<u16>,
    pub web_url: Option<String>,
    pub connected_clients: u32,
}

/// Console
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Console {
    pub meta: ResourceMeta,
    pub spec: ConsoleSpec,
    pub status: ConsoleStatus,
}

/// Snapshot specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSpec {
    pub vm_id: String,
    #[serde(default = "default_true")]
    pub include_memory: bool,
    #[serde(default = "default_true")]
    pub include_disk: bool,
    pub description: Option<String>,
}

/// Snapshot status
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnapshotStatus {
    pub complete: bool,
    pub disk_snapshot_path: Option<String>,
    pub memory_snapshot_path: Option<String>,
    pub digest: Option<String>,
    pub size_bytes: u64,
    pub encrypted: bool,
}

/// Snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub meta: ResourceMeta,
    pub spec: SnapshotSpec,
    pub status: SnapshotStatus,
}

/// Benchmark specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSpec {
    pub vm_id: String,
    pub suite_name: String,
    #[serde(default)]
    pub test_names: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u32,
    #[serde(default)]
    pub parameters: HashMap<String, String>,
}

fn default_timeout() -> u32 {
    300
}

/// Benchmark result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub test_name: String,
    pub passed: bool,
    pub score: f64,
    pub unit: String,
    pub duration_ms: u64,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Benchmark receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReceipt {
    pub run_id: String,
    pub digest: String,
    #[serde(with = "base64_bytes")]
    pub signature: Vec<u8>,
    pub timestamp: i64,
    pub signer_public_key: String,
}

/// Benchmark run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRun {
    pub meta: ResourceMeta,
    pub spec: BenchmarkSpec,
    pub results: Vec<BenchmarkResult>,
    pub receipt: Option<BenchmarkReceipt>,
    pub attestation_id: Option<String>,
}

/// Host provenance for attestation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostProvenance {
    pub qemu_version: String,
    pub qemu_args: Vec<String>,
    pub base_image_hash: String,
    pub volume_hashes: HashMap<String, String>,
    pub macos_version: String,
    pub cpu_model: String,
    pub hvf_enabled: bool,
    pub hostname: String,
    pub timestamp: i64,
}

/// Attestation report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationReport {
    pub id: String,
    pub vm_id: String,
    pub host_provenance: HostProvenance,
    pub digest: String,
    #[serde(with = "base64_bytes")]
    pub signature: Vec<u8>,
    pub created_at: i64,
    pub attestation_type: String,
}

/// LoRa device specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRaDeviceSpec {
    pub vm_id: String,
    pub region: String,
    pub device_eui: String,
    pub app_eui: String,
    #[serde(with = "base64_bytes")]
    pub app_key: Vec<u8>,
    #[serde(default = "default_sf")]
    pub spreading_factor: u32,
    #[serde(default = "default_bw")]
    pub bandwidth_khz: u32,
    #[serde(default)]
    pub loss_rate: f32,
    #[serde(default)]
    pub latency_ms: u32,
}

fn default_sf() -> u32 {
    7
}

fn default_bw() -> u32 {
    125
}

/// LoRa device status
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoRaDeviceStatus {
    pub connected: bool,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub rssi_dbm: f32,
    pub snr_db: f32,
}

/// LoRa device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRaDevice {
    pub meta: ResourceMeta,
    pub spec: LoRaDeviceSpec,
    pub status: LoRaDeviceStatus,
}

/// Run manifest for content addressing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    pub vm_config_digest: String,
    pub image_digests: HashMap<String, String>,
    pub volume_digests: HashMap<String, String>,
    pub benchmark_suite_digest: Option<String>,
    pub attestation_digest: Option<String>,
    pub timestamp: i64,
}

impl RunManifest {
    /// Compute canonical JSON for hashing
    pub fn canonical_json(&self) -> crate::Result<String> {
        // Sort all keys for deterministic output
        let sorted = serde_json::json!({
            "attestation_digest": self.attestation_digest,
            "benchmark_suite_digest": self.benchmark_suite_digest,
            "image_digests": self.image_digests,
            "timestamp": self.timestamp,
            "vm_config_digest": self.vm_config_digest,
            "volume_digests": self.volume_digests,
        });
        Ok(serde_json::to_string(&sorted)?)
    }
}
