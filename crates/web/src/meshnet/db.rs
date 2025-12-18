//! Meshnet database schema and operations
//!
//! Tables:
//! - meshnet_users: Core user records
//! - meshnet_webauthn_credentials: WebAuthn passkey storage
//! - meshnet_identities: Identity handles with provisioning status
//! - meshnet_mesh_peers: WireGuard/Tailscale peer configurations
//! - meshnet_appliances: Downloadable appliance archives

use infrasim_common::Database;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

/// Meshnet database wrapper
#[derive(Clone)]
pub struct MeshnetDb {
    db: Database,
}

// ============================================================================
// User types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshnetUser {
    pub id: Uuid,
    pub created_at: i64,
    pub display_name: Option<String>,
    pub current_identity_handle: Option<String>,
}

// ============================================================================
// WebAuthn credential types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebAuthnCredential {
    pub id: Uuid,
    pub user_id: Uuid,
    pub credential_id: Vec<u8>,
    pub public_key: Vec<u8>,
    pub sign_count: u32,
    pub transports: Option<serde_json::Value>,
    pub created_at: i64,
}

// ============================================================================
// Identity types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProvisioningState {
    Pending,
    Active,
    Error,
}

impl Default for ProvisioningState {
    fn default() -> Self {
        Self::Pending
    }
}

impl std::fmt::Display for ProvisioningState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Active => write!(f, "active"),
            Self::Error => write!(f, "error"),
        }
    }
}

impl std::str::FromStr for ProvisioningState {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "error" => Ok(Self::Error),
            _ => Err(format!("unknown provisioning state: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshnetIdentity {
    pub id: Uuid,
    pub user_id: Uuid,
    pub handle: String,
    pub fqdn: String,
    pub matrix_id: String,
    pub status_subdomain: ProvisioningState,
    pub status_matrix: ProvisioningState,
    pub status_storage: ProvisioningState,
    pub last_error: Option<String>,
    pub created_at: i64,
}

// ============================================================================
// Mesh peer types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeshProviderType {
    Wireguard,
    Tailscale,
}

impl Default for MeshProviderType {
    fn default() -> Self {
        Self::Wireguard
    }
}

impl std::fmt::Display for MeshProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wireguard => write!(f, "wireguard"),
            Self::Tailscale => write!(f, "tailscale"),
        }
    }
}

impl std::str::FromStr for MeshProviderType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "wireguard" => Ok(Self::Wireguard),
            "tailscale" => Ok(Self::Tailscale),
            _ => Err(format!("unknown mesh provider: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshPeerRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub provider: MeshProviderType,
    pub public_key: String,
    pub private_key_encrypted: Option<Vec<u8>>,
    pub preshared_key: Option<String>,
    pub allowed_ips: String,
    pub endpoint: Option<String>,
    pub keepalive: Option<i32>,
    pub address: String,
    pub revoked_at: Option<i64>,
    pub last_handshake_at: Option<i64>,
    pub created_at: i64,
}

// ============================================================================
// Appliance types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApplianceStatus {
    Pending,
    Building,
    Ready,
    Error,
}

impl Default for ApplianceStatus {
    fn default() -> Self {
        Self::Pending
    }
}

impl std::fmt::Display for ApplianceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Building => write!(f, "building"),
            Self::Ready => write!(f, "ready"),
            Self::Error => write!(f, "error"),
        }
    }
}

impl std::str::FromStr for ApplianceStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "building" => Ok(Self::Building),
            "ready" => Ok(Self::Ready),
            "error" => Ok(Self::Error),
            _ => Err(format!("unknown appliance status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshnetAppliance {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub version: String,
    pub status: ApplianceStatus,
    pub qcow_path: Option<String>,
    pub archive_path: Option<String>,
    pub terraform_path: Option<String>,
    pub last_error: Option<String>,
    pub created_at: i64,
}

// ============================================================================
// Session types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshnetSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: i64,
    pub created_at: i64,
}

// ============================================================================
// Database implementation
// ============================================================================

impl MeshnetDb {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Initialize meshnet schema
    pub fn init_schema(&self) -> Result<(), String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute_batch(
            r#"
            -- Meshnet users
            CREATE TABLE IF NOT EXISTS meshnet_users (
                id TEXT PRIMARY KEY,
                created_at INTEGER NOT NULL,
                display_name TEXT,
                current_identity_handle TEXT
            );

            -- WebAuthn credentials
            CREATE TABLE IF NOT EXISTS meshnet_webauthn_credentials (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                credential_id BLOB NOT NULL UNIQUE,
                public_key BLOB NOT NULL,
                sign_count INTEGER NOT NULL DEFAULT 0,
                transports TEXT,
                created_at INTEGER NOT NULL,
                FOREIGN KEY(user_id) REFERENCES meshnet_users(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_meshnet_webauthn_user ON meshnet_webauthn_credentials(user_id);
            CREATE INDEX IF NOT EXISTS idx_meshnet_webauthn_cred_id ON meshnet_webauthn_credentials(credential_id);

            -- Identities (handles)
            CREATE TABLE IF NOT EXISTS meshnet_identities (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL UNIQUE,
                handle TEXT NOT NULL UNIQUE,
                fqdn TEXT NOT NULL,
                matrix_id TEXT NOT NULL,
                status_subdomain TEXT NOT NULL DEFAULT 'pending',
                status_matrix TEXT NOT NULL DEFAULT 'pending',
                status_storage TEXT NOT NULL DEFAULT 'pending',
                last_error TEXT,
                created_at INTEGER NOT NULL,
                FOREIGN KEY(user_id) REFERENCES meshnet_users(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_meshnet_identities_handle ON meshnet_identities(handle);
            CREATE INDEX IF NOT EXISTS idx_meshnet_identities_user ON meshnet_identities(user_id);

            -- Mesh peers
            CREATE TABLE IF NOT EXISTS meshnet_mesh_peers (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                name TEXT NOT NULL,
                provider TEXT NOT NULL DEFAULT 'wireguard',
                public_key TEXT NOT NULL,
                private_key_encrypted BLOB,
                preshared_key TEXT,
                allowed_ips TEXT NOT NULL,
                endpoint TEXT,
                keepalive INTEGER,
                address TEXT NOT NULL,
                revoked_at INTEGER,
                last_handshake_at INTEGER,
                created_at INTEGER NOT NULL,
                FOREIGN KEY(user_id) REFERENCES meshnet_users(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_meshnet_peers_user ON meshnet_mesh_peers(user_id);

            -- Appliances
            CREATE TABLE IF NOT EXISTS meshnet_appliances (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                qcow_path TEXT,
                archive_path TEXT,
                terraform_path TEXT,
                last_error TEXT,
                created_at INTEGER NOT NULL,
                FOREIGN KEY(user_id) REFERENCES meshnet_users(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_meshnet_appliances_user ON meshnet_appliances(user_id);

            -- Sessions
            CREATE TABLE IF NOT EXISTS meshnet_sessions (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                token_hash TEXT NOT NULL UNIQUE,
                expires_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY(user_id) REFERENCES meshnet_users(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_meshnet_sessions_token ON meshnet_sessions(token_hash);
            CREATE INDEX IF NOT EXISTS idx_meshnet_sessions_expires ON meshnet_sessions(expires_at);

            -- WebAuthn challenge store (temporary, in-memory would be better but this works)
            CREATE TABLE IF NOT EXISTS meshnet_webauthn_challenges (
                id TEXT PRIMARY KEY,
                user_id TEXT,
                challenge_data TEXT NOT NULL,
                challenge_type TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_meshnet_challenges_expires ON meshnet_webauthn_challenges(expires_at);
            "#,
        )
        .map_err(|e| e.to_string())?;
        
        info!("Meshnet database schema initialized");
        Ok(())
    }

    // ========================================================================
    // User operations
    // ========================================================================

    pub fn create_user(&self, display_name: Option<&str>) -> Result<MeshnetUser, String> {
        let id = Uuid::new_v4();
        let now = now_epoch_secs();
        
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute(
            "INSERT INTO meshnet_users (id, created_at, display_name, current_identity_handle) VALUES (?1, ?2, ?3, NULL)",
            params![id.to_string(), now, display_name],
        ).map_err(|e| e.to_string())?;
        
        Ok(MeshnetUser {
            id,
            created_at: now,
            display_name: display_name.map(String::from),
            current_identity_handle: None,
        })
    }

    pub fn get_user(&self, id: Uuid) -> Result<Option<MeshnetUser>, String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.query_row(
            "SELECT id, created_at, display_name, current_identity_handle FROM meshnet_users WHERE id = ?1",
            params![id.to_string()],
            |row| {
                Ok(MeshnetUser {
                    id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    created_at: row.get(1)?,
                    display_name: row.get(2)?,
                    current_identity_handle: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn get_user_by_credential_id(&self, credential_id: &[u8]) -> Result<Option<MeshnetUser>, String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.query_row(
            "SELECT u.id, u.created_at, u.display_name, u.current_identity_handle 
             FROM meshnet_users u 
             JOIN meshnet_webauthn_credentials c ON c.user_id = u.id 
             WHERE c.credential_id = ?1",
            params![credential_id],
            |row| {
                Ok(MeshnetUser {
                    id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    created_at: row.get(1)?,
                    display_name: row.get(2)?,
                    current_identity_handle: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    // ========================================================================
    // WebAuthn credential operations
    // ========================================================================

    pub fn store_credential(&self, cred: &WebAuthnCredential) -> Result<(), String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute(
            "INSERT INTO meshnet_webauthn_credentials (id, user_id, credential_id, public_key, sign_count, transports, created_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                cred.id.to_string(),
                cred.user_id.to_string(),
                &cred.credential_id,
                &cred.public_key,
                cred.sign_count,
                cred.transports.as_ref().map(|t| t.to_string()),
                cred.created_at,
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_credentials_for_user(&self, user_id: Uuid) -> Result<Vec<WebAuthnCredential>, String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, credential_id, public_key, sign_count, transports, created_at 
             FROM meshnet_webauthn_credentials WHERE user_id = ?1"
        ).map_err(|e| e.to_string())?;
        
        let rows = stmt.query_map(params![user_id.to_string()], |row| {
            let transports_str: Option<String> = row.get(5)?;
            Ok(WebAuthnCredential {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                user_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
                credential_id: row.get(2)?,
                public_key: row.get(3)?,
                sign_count: row.get(4)?,
                transports: transports_str.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: row.get(6)?,
            })
        }).map_err(|e| e.to_string())?;
        
        let mut creds = Vec::new();
        for row in rows {
            creds.push(row.map_err(|e| e.to_string())?);
        }
        Ok(creds)
    }

    pub fn update_credential_sign_count(&self, credential_id: &[u8], sign_count: u32) -> Result<(), String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute(
            "UPDATE meshnet_webauthn_credentials SET sign_count = ?1 WHERE credential_id = ?2",
            params![sign_count, credential_id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    // ========================================================================
    // Challenge operations (for WebAuthn state)
    // ========================================================================

    pub fn store_challenge(&self, id: &str, user_id: Option<Uuid>, challenge_type: &str, data: &str, expires_at: i64) -> Result<(), String> {
        let now = now_epoch_secs();
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute(
            "INSERT INTO meshnet_webauthn_challenges (id, user_id, challenge_type, challenge_data, expires_at, created_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, user_id.map(|u| u.to_string()), challenge_type, data, expires_at, now],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_challenge(&self, id: &str) -> Result<Option<(Option<Uuid>, String, String, i64)>, String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.query_row(
            "SELECT user_id, challenge_type, challenge_data, expires_at FROM meshnet_webauthn_challenges WHERE id = ?1",
            params![id],
            |row| {
                let user_id_str: Option<String> = row.get(0)?;
                Ok((
                    user_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn delete_challenge(&self, id: &str) -> Result<(), String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute("DELETE FROM meshnet_webauthn_challenges WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn cleanup_expired_challenges(&self) -> Result<usize, String> {
        let now = now_epoch_secs();
        let conn = self.db.connection();
        let conn = conn.lock();
        let count = conn.execute(
            "DELETE FROM meshnet_webauthn_challenges WHERE expires_at < ?1",
            params![now],
        ).map_err(|e| e.to_string())?;
        Ok(count)
    }

    // ========================================================================
    // Identity operations
    // ========================================================================

    pub fn create_identity(&self, user_id: Uuid, handle: &str, base_domain: &str, matrix_domain: &str) -> Result<MeshnetIdentity, String> {
        let id = Uuid::new_v4();
        let now = now_epoch_secs();
        let fqdn = format!("{}.{}", handle, base_domain);
        let matrix_id = format!("@{}:{}", handle, matrix_domain);
        
        let conn = self.db.connection();
        let conn = conn.lock();
        
        // Check if user already has an identity
        let existing: Option<String> = conn.query_row(
            "SELECT id FROM meshnet_identities WHERE user_id = ?1",
            params![user_id.to_string()],
            |row| row.get(0),
        ).optional().map_err(|e| e.to_string())?;
        
        if existing.is_some() {
            return Err("User already has an identity".to_string());
        }
        
        conn.execute(
            "INSERT INTO meshnet_identities (id, user_id, handle, fqdn, matrix_id, status_subdomain, status_matrix, status_storage, created_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', 'pending', 'pending', ?6)",
            params![id.to_string(), user_id.to_string(), handle, fqdn, matrix_id, now],
        ).map_err(|e| e.to_string())?;
        
        // Update user's current identity handle
        conn.execute(
            "UPDATE meshnet_users SET current_identity_handle = ?1 WHERE id = ?2",
            params![handle, user_id.to_string()],
        ).map_err(|e| e.to_string())?;
        
        Ok(MeshnetIdentity {
            id,
            user_id,
            handle: handle.to_string(),
            fqdn,
            matrix_id,
            status_subdomain: ProvisioningState::Pending,
            status_matrix: ProvisioningState::Pending,
            status_storage: ProvisioningState::Pending,
            last_error: None,
            created_at: now,
        })
    }

    pub fn get_identity_by_user(&self, user_id: Uuid) -> Result<Option<MeshnetIdentity>, String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.query_row(
            "SELECT id, user_id, handle, fqdn, matrix_id, status_subdomain, status_matrix, status_storage, last_error, created_at 
             FROM meshnet_identities WHERE user_id = ?1",
            params![user_id.to_string()],
            |row| {
                Ok(MeshnetIdentity {
                    id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    user_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
                    handle: row.get(2)?,
                    fqdn: row.get(3)?,
                    matrix_id: row.get(4)?,
                    status_subdomain: row.get::<_, String>(5)?.parse().unwrap_or_default(),
                    status_matrix: row.get::<_, String>(6)?.parse().unwrap_or_default(),
                    status_storage: row.get::<_, String>(7)?.parse().unwrap_or_default(),
                    last_error: row.get(8)?,
                    created_at: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn get_identity_by_handle(&self, handle: &str) -> Result<Option<MeshnetIdentity>, String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.query_row(
            "SELECT id, user_id, handle, fqdn, matrix_id, status_subdomain, status_matrix, status_storage, last_error, created_at 
             FROM meshnet_identities WHERE handle = ?1",
            params![handle],
            |row| {
                Ok(MeshnetIdentity {
                    id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    user_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
                    handle: row.get(2)?,
                    fqdn: row.get(3)?,
                    matrix_id: row.get(4)?,
                    status_subdomain: row.get::<_, String>(5)?.parse().unwrap_or_default(),
                    status_matrix: row.get::<_, String>(6)?.parse().unwrap_or_default(),
                    status_storage: row.get::<_, String>(7)?.parse().unwrap_or_default(),
                    last_error: row.get(8)?,
                    created_at: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn update_identity_status(
        &self,
        identity_id: Uuid,
        subdomain: Option<ProvisioningState>,
        matrix: Option<ProvisioningState>,
        storage: Option<ProvisioningState>,
        error: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        
        if let Some(s) = subdomain {
            conn.execute(
                "UPDATE meshnet_identities SET status_subdomain = ?1 WHERE id = ?2",
                params![s.to_string(), identity_id.to_string()],
            ).map_err(|e| e.to_string())?;
        }
        if let Some(s) = matrix {
            conn.execute(
                "UPDATE meshnet_identities SET status_matrix = ?1 WHERE id = ?2",
                params![s.to_string(), identity_id.to_string()],
            ).map_err(|e| e.to_string())?;
        }
        if let Some(s) = storage {
            conn.execute(
                "UPDATE meshnet_identities SET status_storage = ?1 WHERE id = ?2",
                params![s.to_string(), identity_id.to_string()],
            ).map_err(|e| e.to_string())?;
        }
        if let Some(e) = error {
            conn.execute(
                "UPDATE meshnet_identities SET last_error = ?1 WHERE id = ?2",
                params![e, identity_id.to_string()],
            ).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    // ========================================================================
    // Mesh peer operations
    // ========================================================================

    pub fn create_mesh_peer(&self, peer: &MeshPeerRecord) -> Result<(), String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute(
            "INSERT INTO meshnet_mesh_peers (id, user_id, name, provider, public_key, private_key_encrypted, preshared_key, allowed_ips, endpoint, keepalive, address, created_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                peer.id.to_string(),
                peer.user_id.to_string(),
                peer.name,
                peer.provider.to_string(),
                peer.public_key,
                peer.private_key_encrypted.as_ref(),
                peer.preshared_key.as_ref(),
                peer.allowed_ips,
                peer.endpoint.as_ref(),
                peer.keepalive,
                peer.address,
                peer.created_at,
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_mesh_peers(&self, user_id: Uuid) -> Result<Vec<MeshPeerRecord>, String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, provider, public_key, private_key_encrypted, preshared_key, allowed_ips, endpoint, keepalive, address, revoked_at, last_handshake_at, created_at 
             FROM meshnet_mesh_peers WHERE user_id = ?1 ORDER BY created_at DESC"
        ).map_err(|e| e.to_string())?;
        
        let rows = stmt.query_map(params![user_id.to_string()], |row| {
            Ok(MeshPeerRecord {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                user_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
                name: row.get(2)?,
                provider: row.get::<_, String>(3)?.parse().unwrap_or_default(),
                public_key: row.get(4)?,
                private_key_encrypted: row.get(5)?,
                preshared_key: row.get(6)?,
                allowed_ips: row.get(7)?,
                endpoint: row.get(8)?,
                keepalive: row.get(9)?,
                address: row.get(10)?,
                revoked_at: row.get(11)?,
                last_handshake_at: row.get(12)?,
                created_at: row.get(13)?,
            })
        }).map_err(|e| e.to_string())?;
        
        let mut peers = Vec::new();
        for row in rows {
            peers.push(row.map_err(|e| e.to_string())?);
        }
        Ok(peers)
    }

    pub fn get_mesh_peer(&self, id: Uuid) -> Result<Option<MeshPeerRecord>, String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.query_row(
            "SELECT id, user_id, name, provider, public_key, private_key_encrypted, preshared_key, allowed_ips, endpoint, keepalive, address, revoked_at, last_handshake_at, created_at 
             FROM meshnet_mesh_peers WHERE id = ?1",
            params![id.to_string()],
            |row| {
                Ok(MeshPeerRecord {
                    id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    user_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
                    name: row.get(2)?,
                    provider: row.get::<_, String>(3)?.parse().unwrap_or_default(),
                    public_key: row.get(4)?,
                    private_key_encrypted: row.get(5)?,
                    preshared_key: row.get(6)?,
                    allowed_ips: row.get(7)?,
                    endpoint: row.get(8)?,
                    keepalive: row.get(9)?,
                    address: row.get(10)?,
                    revoked_at: row.get(11)?,
                    last_handshake_at: row.get(12)?,
                    created_at: row.get(13)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn revoke_mesh_peer(&self, id: Uuid) -> Result<(), String> {
        let now = now_epoch_secs();
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute(
            "UPDATE meshnet_mesh_peers SET revoked_at = ?1 WHERE id = ?2",
            params![now, id.to_string()],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn count_user_peers(&self, user_id: Uuid) -> Result<usize, String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM meshnet_mesh_peers WHERE user_id = ?1 AND revoked_at IS NULL",
            params![user_id.to_string()],
            |row| row.get(0),
        ).map_err(|e| e.to_string())?;
        Ok(count as usize)
    }

    // ========================================================================
    // Appliance operations
    // ========================================================================

    pub fn create_appliance(&self, user_id: Uuid, name: &str) -> Result<MeshnetAppliance, String> {
        let id = Uuid::new_v4();
        let now = now_epoch_secs();
        let version = "1.0.0".to_string();
        
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute(
            "INSERT INTO meshnet_appliances (id, user_id, name, version, status, created_at) VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
            params![id.to_string(), user_id.to_string(), name, version, now],
        ).map_err(|e| e.to_string())?;
        
        Ok(MeshnetAppliance {
            id,
            user_id,
            name: name.to_string(),
            version,
            status: ApplianceStatus::Pending,
            qcow_path: None,
            archive_path: None,
            terraform_path: None,
            last_error: None,
            created_at: now,
        })
    }

    pub fn get_appliances(&self, user_id: Uuid) -> Result<Vec<MeshnetAppliance>, String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, version, status, qcow_path, archive_path, terraform_path, last_error, created_at 
             FROM meshnet_appliances WHERE user_id = ?1 ORDER BY created_at DESC"
        ).map_err(|e| e.to_string())?;
        
        let rows = stmt.query_map(params![user_id.to_string()], |row| {
            Ok(MeshnetAppliance {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                user_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
                name: row.get(2)?,
                version: row.get(3)?,
                status: row.get::<_, String>(4)?.parse().unwrap_or_default(),
                qcow_path: row.get(5)?,
                archive_path: row.get(6)?,
                terraform_path: row.get(7)?,
                last_error: row.get(8)?,
                created_at: row.get(9)?,
            })
        }).map_err(|e| e.to_string())?;
        
        let mut appliances = Vec::new();
        for row in rows {
            appliances.push(row.map_err(|e| e.to_string())?);
        }
        Ok(appliances)
    }

    pub fn get_appliance(&self, id: Uuid) -> Result<Option<MeshnetAppliance>, String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.query_row(
            "SELECT id, user_id, name, version, status, qcow_path, archive_path, terraform_path, last_error, created_at 
             FROM meshnet_appliances WHERE id = ?1",
            params![id.to_string()],
            |row| {
                Ok(MeshnetAppliance {
                    id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    user_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
                    name: row.get(2)?,
                    version: row.get(3)?,
                    status: row.get::<_, String>(4)?.parse().unwrap_or_default(),
                    qcow_path: row.get(5)?,
                    archive_path: row.get(6)?,
                    terraform_path: row.get(7)?,
                    last_error: row.get(8)?,
                    created_at: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn update_appliance_status(
        &self,
        id: Uuid,
        status: ApplianceStatus,
        qcow_path: Option<&str>,
        archive_path: Option<&str>,
        terraform_path: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute(
            "UPDATE meshnet_appliances SET status = ?1, qcow_path = ?2, archive_path = ?3, terraform_path = ?4, last_error = ?5 WHERE id = ?6",
            params![status.to_string(), qcow_path, archive_path, terraform_path, error, id.to_string()],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_appliance(&self, id: Uuid) -> Result<(), String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute("DELETE FROM meshnet_appliances WHERE id = ?1", params![id.to_string()])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ========================================================================
    // Session operations
    // ========================================================================

    pub fn create_session(&self, user_id: Uuid, token_hash: &str, expires_at: i64) -> Result<MeshnetSession, String> {
        let id = Uuid::new_v4();
        let now = now_epoch_secs();
        
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute(
            "INSERT INTO meshnet_sessions (id, user_id, token_hash, expires_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id.to_string(), user_id.to_string(), token_hash, expires_at, now],
        ).map_err(|e| e.to_string())?;
        
        Ok(MeshnetSession {
            id,
            user_id,
            token_hash: token_hash.to_string(),
            expires_at,
            created_at: now,
        })
    }

    pub fn get_session_by_token_hash(&self, token_hash: &str) -> Result<Option<MeshnetSession>, String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.query_row(
            "SELECT id, user_id, token_hash, expires_at, created_at FROM meshnet_sessions WHERE token_hash = ?1",
            params![token_hash],
            |row| {
                Ok(MeshnetSession {
                    id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    user_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
                    token_hash: row.get(2)?,
                    expires_at: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn delete_session(&self, token_hash: &str) -> Result<(), String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute("DELETE FROM meshnet_sessions WHERE token_hash = ?1", params![token_hash])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn cleanup_expired_sessions(&self) -> Result<usize, String> {
        let now = now_epoch_secs();
        let conn = self.db.connection();
        let conn = conn.lock();
        let count = conn.execute(
            "DELETE FROM meshnet_sessions WHERE expires_at < ?1",
            params![now],
        ).map_err(|e| e.to_string())?;
        Ok(count)
    }
}

fn now_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> MeshnetDb {
        let db = Database::open_memory().unwrap();
        let mdb = MeshnetDb::new(db);
        mdb.init_schema().unwrap();
        mdb
    }

    #[test]
    fn test_user_crud() {
        let db = test_db();
        let user = db.create_user(Some("test")).unwrap();
        assert!(user.display_name.as_deref() == Some("test"));
        
        let fetched = db.get_user(user.id).unwrap().unwrap();
        assert_eq!(fetched.id, user.id);
    }

    #[test]
    fn test_identity_crud() {
        let db = test_db();
        let user = db.create_user(None).unwrap();
        
        let identity = db.create_identity(user.id, "alice", "mesh.example.com", "matrix.example.com").unwrap();
        assert_eq!(identity.handle, "alice");
        assert_eq!(identity.fqdn, "alice.mesh.example.com");
        assert_eq!(identity.matrix_id, "@alice:matrix.example.com");
        
        let fetched = db.get_identity_by_user(user.id).unwrap().unwrap();
        assert_eq!(fetched.id, identity.id);
        
        // Cannot create second identity for same user
        let result = db.create_identity(user.id, "bob", "mesh.example.com", "matrix.example.com");
        assert!(result.is_err());
    }
}
