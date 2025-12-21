//! Mesh provider interface and WireGuard implementation
//!
//! Supports:
//! - WireGuard (implemented)
//! - Tailscale (stub for future)
//!
//! Uses x25519-dalek for key generation.

use crate::meshnet::db::{MeshnetDb, MeshPeerRecord, MeshProviderType, MeshnetIdentity};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

// ============================================================================
// Types
// ============================================================================

/// Mesh peer (user-facing, without private key)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshPeer {
    pub id: Uuid,
    pub name: String,
    pub provider: MeshProviderType,
    pub public_key: String,
    pub address: String,
    pub allowed_ips: String,
    pub endpoint: Option<String>,
    pub revoked: bool,
    pub created_at: i64,
    pub last_handshake_at: Option<i64>,
}

impl From<&MeshPeerRecord> for MeshPeer {
    fn from(record: &MeshPeerRecord) -> Self {
        Self {
            id: record.id,
            name: record.name.clone(),
            provider: record.provider,
            public_key: record.public_key.clone(),
            address: record.address.clone(),
            allowed_ips: record.allowed_ips.clone(),
            endpoint: record.endpoint.clone(),
            revoked: record.revoked_at.is_some(),
            created_at: record.created_at,
            last_handshake_at: record.last_handshake_at,
        }
    }
}

/// Peer status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStatus {
    pub id: Uuid,
    pub connected: bool,
    pub last_handshake_at: Option<i64>,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

/// WireGuard key pair
#[derive(Debug, Clone)]
pub struct WgKeyPair {
    pub private_key: String, // Base64
    pub public_key: String,  // Base64
}

/// Server (gateway) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: String,
    pub dns: Option<String>,
}

// ============================================================================
// Provider trait
// ============================================================================

/// Mesh provider interface
#[async_trait]
pub trait MeshProvider: Send + Sync {
    /// Create a new peer
    async fn create_peer(&self, user_id: Uuid, name: &str) -> Result<MeshPeer, String>;
    
    /// Render client configuration file
    fn render_client_config(&self, peer: &MeshPeerRecord, identity: &MeshnetIdentity) -> Result<String, String>;
    
    /// Revoke a peer
    async fn revoke_peer(&self, peer_id: Uuid) -> Result<(), String>;
    
    /// Get peer status
    async fn peer_status(&self, peer_id: Uuid) -> Result<PeerStatus, String>;
    
    /// List peers for a user
    async fn list_peers(&self, user_id: Uuid) -> Result<Vec<MeshPeer>, String>;
    
    /// Get a peer by ID
    async fn get_peer(&self, peer_id: Uuid) -> Result<Option<MeshPeerRecord>, String>;
}

// ============================================================================
// WireGuard implementation
// ============================================================================

/// WireGuard mesh provider
pub struct WireGuardProvider {
    db: MeshnetDb,
    gateway: GatewayConfig,
}

impl WireGuardProvider {
    pub fn new(db: MeshnetDb) -> Self {
        // Generate or load gateway keys from environment
        let gateway_private = std::env::var("WG_GATEWAY_PRIVATE_KEY").ok();
        let gateway_public = std::env::var("WG_GATEWAY_PUBLIC_KEY")
            .unwrap_or_else(|_| {
                // Generate a new key pair if not provided
                let kp = generate_wireguard_keypair();
                info!("Generated gateway public key: {}", kp.public_key);
                kp.public_key
            });
        let endpoint = std::env::var("WG_GATEWAY_ENDPOINT").ok();
        let dns = std::env::var("WG_DNS").ok();
        
        Self {
            db,
            gateway: GatewayConfig {
                public_key: gateway_public,
                endpoint,
                allowed_ips: "0.0.0.0/0, ::/0".to_string(),
                dns,
            },
        }
    }
    
    /// Derive a stable /24 subnet for a user based on their ID
    /// Returns the third octet (10.50.X.0/24)
    fn derive_user_subnet(&self, user_id: Uuid) -> u8 {
        let mut hasher = Sha256::new();
        hasher.update(user_id.as_bytes());
        let hash = hasher.finalize();
        // Use first byte of hash, mapping to 1-254 range
        let octet = (hash[0] as u16 * 253 / 255 + 1) as u8;
        octet
    }
    
    /// Allocate the next available address in the user's subnet
    fn allocate_address(&self, user_id: Uuid) -> Result<String, String> {
        let subnet = self.derive_user_subnet(user_id);
        let peer_count = self.db.count_user_peers(user_id)? as u8;
        
        if peer_count >= 253 {
            return Err("Maximum peers reached for this subnet".to_string());
        }
        
        // Skip .0 (network) and .1 (gateway), start at .2
        let host = peer_count + 2;
        Ok(format!("10.50.{}.{}/32", subnet, host))
    }
}

#[async_trait]
impl MeshProvider for WireGuardProvider {
    async fn create_peer(&self, user_id: Uuid, name: &str) -> Result<MeshPeer, String> {
        // Generate keypair
        let keypair = generate_wireguard_keypair();
        
        // Allocate address
        let address = self.allocate_address(user_id)?;
        
        // Derive allowed IPs (the user's subnet for mesh traffic)
        let subnet = self.derive_user_subnet(user_id);
        let allowed_ips = format!("10.50.{}.0/24", subnet);
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        
        let record = MeshPeerRecord {
            id: Uuid::new_v4(),
            user_id,
            name: name.to_string(),
            provider: MeshProviderType::Wireguard,
            public_key: keypair.public_key.clone(),
            private_key_encrypted: Some(keypair.private_key.as_bytes().to_vec()), // MVP: not encrypted
            preshared_key: None,
            allowed_ips: allowed_ips.clone(),
            endpoint: None,
            keepalive: Some(25),
            address: address.clone(),
            revoked_at: None,
            last_handshake_at: None,
            created_at: now,
        };
        
        self.db.create_mesh_peer(&record)?;
        
        info!("Created WireGuard peer {} for user {} at {}", name, user_id, address);
        
        Ok(MeshPeer::from(&record))
    }
    
    fn render_client_config(&self, peer: &MeshPeerRecord, identity: &MeshnetIdentity) -> Result<String, String> {
        let private_key = peer.private_key_encrypted
            .as_ref()
            .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
            .ok_or_else(|| "Private key not available".to_string())?;
        
        let dns_line = self.gateway.dns
            .as_ref()
            .map(|d| format!("DNS = {}", d))
            .unwrap_or_default();
        
        let endpoint_line = self.gateway.endpoint
            .as_ref()
            .map(|e| format!("Endpoint = {}", e))
            .unwrap_or_else(|| "# Endpoint = your-gateway:51820".to_string());
        
        let config = format!(
r#"# WireGuard configuration for {name}
# Identity: {handle} ({fqdn})
# Generated by Meshnet Console

[Interface]
PrivateKey = {private_key}
Address = {address}
{dns_line}

[Peer]
# Mesh Gateway
PublicKey = {gateway_pubkey}
AllowedIPs = {gateway_allowed_ips}
{endpoint_line}
PersistentKeepalive = {keepalive}
"#,
            name = peer.name,
            handle = identity.handle,
            fqdn = identity.fqdn,
            private_key = private_key,
            address = peer.address,
            dns_line = dns_line,
            gateway_pubkey = self.gateway.public_key,
            gateway_allowed_ips = self.gateway.allowed_ips,
            endpoint_line = endpoint_line,
            keepalive = peer.keepalive.unwrap_or(25),
        );
        
        Ok(config)
    }
    
    async fn revoke_peer(&self, peer_id: Uuid) -> Result<(), String> {
        self.db.revoke_mesh_peer(peer_id)?;
        info!("Revoked peer {}", peer_id);
        Ok(())
    }
    
    async fn peer_status(&self, peer_id: Uuid) -> Result<PeerStatus, String> {
        let peer = self.db.get_mesh_peer(peer_id)?
            .ok_or_else(|| "Peer not found".to_string())?;
        
        // In MVP, we don't have real status - stub it
        Ok(PeerStatus {
            id: peer_id,
            connected: peer.last_handshake_at.is_some() && peer.revoked_at.is_none(),
            last_handshake_at: peer.last_handshake_at,
            bytes_sent: 0,
            bytes_received: 0,
        })
    }
    
    async fn list_peers(&self, user_id: Uuid) -> Result<Vec<MeshPeer>, String> {
        let records = self.db.get_mesh_peers(user_id)?;
        Ok(records.iter().map(MeshPeer::from).collect())
    }
    
    async fn get_peer(&self, peer_id: Uuid) -> Result<Option<MeshPeerRecord>, String> {
        self.db.get_mesh_peer(peer_id)
    }
}

// ============================================================================
// Key generation
// ============================================================================

/// Generate a WireGuard keypair using x25519
pub fn generate_wireguard_keypair() -> WgKeyPair {
    use rand::RngCore;
    use base64::{Engine, engine::general_purpose::STANDARD};
    
    // Generate 32 random bytes for private key
    let mut private_key_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut private_key_bytes);
    
    // WireGuard key clamping (as per spec)
    private_key_bytes[0] &= 248;
    private_key_bytes[31] &= 127;
    private_key_bytes[31] |= 64;
    
    // Compute public key via x25519 base point multiplication
    // Using the x25519-dalek crate's StaticSecret/PublicKey
    use x25519_dalek::{StaticSecret, PublicKey};
    
    let secret = StaticSecret::from(private_key_bytes);
    let public = PublicKey::from(&secret);
    
    WgKeyPair {
        private_key: STANDARD.encode(private_key_bytes),
        public_key: STANDARD.encode(public.as_bytes()),
    }
}

// ============================================================================
// Tailscale Provider Implementation
// ============================================================================

/// Tailscale node information from the local API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailscaleNode {
    pub id: String,
    pub name: String,
    #[serde(rename = "hostName")]
    pub hostname: String,
    #[serde(rename = "dnsName")]
    pub dns_name: String,
    #[serde(rename = "tailscaleIPs")]
    pub tailscale_ips: Vec<String>,
    pub online: bool,
    #[serde(rename = "exitNode")]
    pub exit_node: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    pub os: Option<String>,
}

/// Tailscale status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailscaleStatus {
    #[serde(rename = "BackendState")]
    pub backend_state: String,
    #[serde(rename = "Self")]
    pub self_node: Option<TailscaleNode>,
    #[serde(rename = "Peer")]
    pub peers: Option<std::collections::HashMap<String, TailscaleNode>>,
    #[serde(rename = "MagicDNSSuffix")]
    pub magic_dns_suffix: Option<String>,
}

/// Tailscale mesh provider
/// 
/// Uses Tailscale for control plane networking:
/// - Node discovery via Tailscale status
/// - Secure peering via Tailscale network
/// - Optional WireGuard overlay for VM traffic
pub struct TailscaleProvider {
    db: MeshnetDb,
    /// Socket path for Tailscale local API
    socket_path: String,
    /// Tailscale network domain
    tailnet: Option<String>,
}

impl TailscaleProvider {
    /// Create a new Tailscale provider
    pub fn new(db: MeshnetDb) -> Self {
        Self {
            db,
            socket_path: "/var/run/tailscale/tailscaled.sock".to_string(),
            tailnet: None,
        }
    }

    /// Create with custom socket path (for macOS or custom installs)
    pub fn with_socket(db: MeshnetDb, socket_path: &str) -> Self {
        Self {
            db,
            socket_path: socket_path.to_string(),
            tailnet: None,
        }
    }

    /// Get Tailscale status via CLI (fallback when socket unavailable)
    pub async fn get_status(&self) -> Result<TailscaleStatus, String> {
        let output = tokio::process::Command::new("tailscale")
            .arg("status")
            .arg("--json")
            .output()
            .await
            .map_err(|e| format!("Failed to run tailscale: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Tailscale status failed: {}", stderr));
        }

        serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse tailscale status: {}", e))
    }

    /// Get the current node's Tailscale IP
    pub async fn get_self_ip(&self) -> Result<String, String> {
        let output = tokio::process::Command::new("tailscale")
            .arg("ip")
            .arg("-4")
            .output()
            .await
            .map_err(|e| format!("Failed to get tailscale IP: {}", e))?;

        if !output.status.success() {
            return Err("Tailscale not connected".to_string());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Check if Tailscale is connected
    pub async fn is_connected(&self) -> bool {
        match self.get_status().await {
            Ok(status) => status.backend_state == "Running",
            Err(_) => false,
        }
    }

    /// List all peers on the Tailscale network
    pub async fn list_tailscale_peers(&self) -> Result<Vec<TailscaleNode>, String> {
        let status = self.get_status().await?;
        
        let mut nodes = Vec::new();
        
        if let Some(peers) = status.peers {
            for (_, peer) in peers {
                nodes.push(peer);
            }
        }
        
        Ok(nodes)
    }

    /// Get a specific peer by name or IP
    pub async fn get_tailscale_peer(&self, name_or_ip: &str) -> Result<Option<TailscaleNode>, String> {
        let peers = self.list_tailscale_peers().await?;
        
        Ok(peers.into_iter().find(|p| {
            p.name == name_or_ip 
            || p.hostname == name_or_ip
            || p.dns_name.starts_with(&format!("{}.", name_or_ip))
            || p.tailscale_ips.contains(&name_or_ip.to_string())
        }))
    }

    /// Send a file to a peer using Tailscale file sharing
    pub async fn send_file(&self, peer: &str, local_path: &str) -> Result<(), String> {
        let output = tokio::process::Command::new("tailscale")
            .arg("file")
            .arg("cp")
            .arg(local_path)
            .arg(format!("{}:", peer))
            .output()
            .await
            .map_err(|e| format!("Failed to send file: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("File transfer failed: {}", stderr));
        }

        Ok(())
    }

    /// Receive pending files
    pub async fn receive_files(&self, output_dir: &str) -> Result<Vec<String>, String> {
        let output = tokio::process::Command::new("tailscale")
            .arg("file")
            .arg("get")
            .arg(output_dir)
            .output()
            .await
            .map_err(|e| format!("Failed to receive files: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("File receive failed: {}", stderr));
        }

        // Parse received filenames from stdout
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().map(|s| s.to_string()).collect())
    }

    /// Register this node as an InfraSim peer
    async fn register_as_peer(&self, user_id: Uuid, name: &str) -> Result<MeshPeer, String> {
        let status = self.get_status().await?;
        
        let self_node = status.self_node
            .ok_or("Tailscale not connected")?;
        
        let tailscale_ip = self_node.tailscale_ips.first()
            .ok_or("No Tailscale IP assigned")?;
        
        // Store in local database
        let record = MeshPeerRecord {
            id: Uuid::new_v4(),
            user_id,
            name: name.to_string(),
            provider: MeshProviderType::Tailscale,
            public_key: self_node.id.clone(), // Use Tailscale node ID as "public key"
            private_key_encrypted: None, // No private key for Tailscale
            preshared_key: None, // Tailscale handles encryption
            address: tailscale_ip.clone(),
            allowed_ips: "0.0.0.0/0".to_string(), // Full mesh
            endpoint: Some(self_node.dns_name.clone()),
            keepalive: None, // Tailscale manages keepalives
            created_at: chrono::Utc::now().timestamp(),
            revoked_at: None,
            last_handshake_at: None,
        };

        self.db.create_mesh_peer(&record)?;

        Ok(MeshPeer::from(&record))
    }
}

#[async_trait]
impl MeshProvider for TailscaleProvider {
    async fn create_peer(&self, user_id: Uuid, name: &str) -> Result<MeshPeer, String> {
        // For Tailscale, "creating a peer" means registering this node
        // or tracking an external Tailscale peer
        self.register_as_peer(user_id, name).await
    }
    
    fn render_client_config(&self, peer: &MeshPeerRecord, _identity: &MeshnetIdentity) -> Result<String, String> {
        // For Tailscale, we provide connection info rather than a WireGuard config
        let config = format!(r#"# Tailscale Peer Configuration
# ===========================
# This peer is connected via Tailscale.
# 
# Peer Name:     {name}
# Tailscale IP:  {address}
# Endpoint:      {endpoint}
# Node ID:       {node_id}
#
# To connect via SSH:
#   ssh user@{address}
#   ssh user@{endpoint}
#
# To send files:
#   tailscale file cp ./file.txt {name}:
#
# To use as exit node:
#   tailscale set --exit-node={name}
"#,
            name = peer.name,
            address = peer.address,
            endpoint = peer.endpoint.clone().unwrap_or_default(),
            node_id = peer.public_key,
        );
        Ok(config)
    }
    
    async fn revoke_peer(&self, peer_id: Uuid) -> Result<(), String> {
        // Mark as revoked in local DB
        // Note: Can't actually remove from Tailscale network from here
        self.db.revoke_mesh_peer(peer_id)
    }
    
    async fn peer_status(&self, peer_id: Uuid) -> Result<PeerStatus, String> {
        let peer = self.db.get_mesh_peer(peer_id)?
            .ok_or("Peer not found")?;
        
        // Check if the peer is online via Tailscale
        let is_online = if let Ok(ts_peer) = self.get_tailscale_peer(&peer.address).await {
            ts_peer.map(|p| p.online).unwrap_or(false)
        } else {
            false
        };
        
        Ok(PeerStatus {
            id: peer_id,
            connected: is_online,
            last_handshake_at: peer.last_handshake_at,
            bytes_sent: 0, // Not available via status API
            bytes_received: 0,
        })
    }
    
    async fn list_peers(&self, user_id: Uuid) -> Result<Vec<MeshPeer>, String> {
        let records = self.db.get_mesh_peers(user_id)?;
        
        // Filter to Tailscale peers only
        let tailscale_peers: Vec<MeshPeer> = records.iter()
            .filter(|r| r.provider == MeshProviderType::Tailscale && r.revoked_at.is_none())
            .map(MeshPeer::from)
            .collect();
        
        Ok(tailscale_peers)
    }
    
    async fn get_peer(&self, peer_id: Uuid) -> Result<Option<MeshPeerRecord>, String> {
        self.db.get_mesh_peer(peer_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use infrasim_common::Database;

    fn test_provider() -> WireGuardProvider {
        let db = Database::open_memory().unwrap();
        let mdb = MeshnetDb::new(db);
        mdb.init_schema().unwrap();
        WireGuardProvider::new(mdb)
    }

    #[test]
    fn test_keypair_generation() {
        let kp = generate_wireguard_keypair();
        assert_eq!(kp.private_key.len(), 44); // Base64 of 32 bytes
        assert_eq!(kp.public_key.len(), 44);
        
        // Keys should be different
        assert_ne!(kp.private_key, kp.public_key);
    }

    #[test]
    fn test_subnet_derivation() {
        let provider = test_provider();
        let user_id = Uuid::new_v4();
        
        let subnet = provider.derive_user_subnet(user_id);
        assert!(subnet >= 1 && subnet <= 254);
        
        // Same user should get same subnet
        assert_eq!(subnet, provider.derive_user_subnet(user_id));
        
        // Different user should likely get different subnet
        let other_id = Uuid::new_v4();
        // Note: there's a small chance of collision, so we just ensure it's valid
        let other_subnet = provider.derive_user_subnet(other_id);
        assert!(other_subnet >= 1 && other_subnet <= 254);
    }

    #[tokio::test]
    async fn test_create_peer() {
        let provider = test_provider();
        let user = provider.db.create_user(Some("test")).unwrap();
        
        let peer = provider.create_peer(user.id, "laptop").await.unwrap();
        assert_eq!(peer.name, "laptop");
        assert!(!peer.public_key.is_empty());
        assert!(peer.address.starts_with("10.50."));
    }

    #[tokio::test]
    async fn test_list_peers() {
        let provider = test_provider();
        let user = provider.db.create_user(Some("test")).unwrap();
        
        provider.create_peer(user.id, "laptop").await.unwrap();
        provider.create_peer(user.id, "phone").await.unwrap();
        
        let peers = provider.list_peers(user.id).await.unwrap();
        assert_eq!(peers.len(), 2);
    }
}
