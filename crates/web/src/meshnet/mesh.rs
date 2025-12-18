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
// Tailscale stub (for future)
// ============================================================================

/// Tailscale mesh provider (stub)
pub struct TailscaleProvider {
    db: MeshnetDb,
}

impl TailscaleProvider {
    #[allow(dead_code)]
    pub fn new(db: MeshnetDb) -> Self {
        Self { db }
    }
}

#[async_trait]
impl MeshProvider for TailscaleProvider {
    async fn create_peer(&self, _user_id: Uuid, _name: &str) -> Result<MeshPeer, String> {
        Err("Tailscale provider not yet implemented".to_string())
    }
    
    fn render_client_config(&self, _peer: &MeshPeerRecord, _identity: &MeshnetIdentity) -> Result<String, String> {
        Err("Tailscale provider not yet implemented".to_string())
    }
    
    async fn revoke_peer(&self, _peer_id: Uuid) -> Result<(), String> {
        Err("Tailscale provider not yet implemented".to_string())
    }
    
    async fn peer_status(&self, _peer_id: Uuid) -> Result<PeerStatus, String> {
        Err("Tailscale provider not yet implemented".to_string())
    }
    
    async fn list_peers(&self, _user_id: Uuid) -> Result<Vec<MeshPeer>, String> {
        Err("Tailscale provider not yet implemented".to_string())
    }
    
    async fn get_peer(&self, _peer_id: Uuid) -> Result<Option<MeshPeerRecord>, String> {
        Err("Tailscale provider not yet implemented".to_string())
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
