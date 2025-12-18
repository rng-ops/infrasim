//! WebAuthn (FIDO2) authentication provider.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use webauthn_rs::prelude::*;
use infrasim_common::Database;

use super::types::*;
use super::provider::AuthProvider;

/// WebAuthn credential stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCredential {
    pub id: String,
    pub identity_id: String,
    pub credential_id: Vec<u8>,
    pub credential: String, // JSON serialized Passkey
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub name: Option<String>,
}

/// In-progress registration challenges
pub struct RegistrationChallenge {
    pub identity_id: String,
    pub state: PasskeyRegistration,
    pub expires_at: i64,
}

/// In-progress authentication challenges  
pub struct AuthenticationChallenge {
    pub identity_id: String,
    pub state: PasskeyAuthentication,
    pub expires_at: i64,
}

/// WebAuthn authentication provider
pub struct WebAuthnProvider {
    webauthn: Arc<Webauthn>,
    db: Database,
    /// Pending registration states (in-memory for simplicity)
    reg_challenges: RwLock<std::collections::HashMap<String, RegistrationChallenge>>,
    /// Pending auth challenges
    auth_challenges: RwLock<std::collections::HashMap<String, AuthenticationChallenge>>,
}

impl WebAuthnProvider {
    pub fn new(rp_id: &str, rp_origin: &str, rp_name: &str, db: Database) -> Result<Self, String> {
        let rp_id = rp_id.to_string();
        let rp_origin = url::Url::parse(rp_origin).map_err(|e| format!("Invalid origin: {}", e))?;
        
        let builder = WebauthnBuilder::new(&rp_id, &rp_origin)
            .map_err(|e| format!("WebAuthn builder error: {}", e))?
            .rp_name(rp_name);
        
        let webauthn = Arc::new(builder.build().map_err(|e| format!("WebAuthn build error: {}", e))?);
        
        Ok(Self {
            webauthn,
            db,
            reg_challenges: RwLock::new(std::collections::HashMap::new()),
            auth_challenges: RwLock::new(std::collections::HashMap::new()),
        })
    }

    /// Initialize WebAuthn tables
    pub fn init_schema(&self) -> Result<(), String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS webauthn_credentials (
                id TEXT PRIMARY KEY,
                identity_id TEXT NOT NULL,
                credential_id BLOB NOT NULL,
                credential_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                last_used_at INTEGER,
                name TEXT,
                FOREIGN KEY(identity_id) REFERENCES auth_identities(id)
            );
            CREATE INDEX IF NOT EXISTS idx_webauthn_identity ON webauthn_credentials(identity_id);
            CREATE INDEX IF NOT EXISTS idx_webauthn_cred_id ON webauthn_credentials(credential_id);
            "#,
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Begin WebAuthn registration for an identity
    pub async fn begin_registration(&self, identity_id: &str, display_name: &str) -> Result<CreationChallengeResponse, String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        // Get existing credentials for this user
        let existing = self.get_credentials_for_identity(identity_id).await?;
        let exclude: Vec<CredentialID> = existing.iter()
            .filter_map(|c| {
                serde_json::from_str::<Passkey>(&c.credential)
                    .ok()
                    .map(|pk| pk.cred_id().clone())
            })
            .collect();
        
        let user_id = Uuid::parse_str(identity_id)
            .unwrap_or_else(|_| Uuid::new_v4());
        
        let (ccr, reg_state) = self.webauthn
            .start_passkey_registration(user_id, display_name, display_name, Some(exclude))
            .map_err(|e| format!("Registration start failed: {}", e))?;
        
        let challenge_id = uuid::Uuid::new_v4().to_string();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        
        let mut challenges = self.reg_challenges.write().await;
        challenges.insert(challenge_id.clone(), RegistrationChallenge {
            identity_id: identity_id.to_string(),
            state: reg_state,
            expires_at: now + 300, // 5 minutes
        });
        
        Ok(ccr)
    }

    /// Complete WebAuthn registration
    pub async fn complete_registration(
        &self,
        challenge_id: &str,
        response: RegisterPublicKeyCredential,
        credential_name: Option<String>,
    ) -> Result<StoredCredential, String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let mut challenges = self.reg_challenges.write().await;
        let challenge = challenges.remove(challenge_id)
            .ok_or_else(|| "Challenge not found or expired".to_string())?;
        
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        if challenge.expires_at <= now {
            return Err("Challenge expired".to_string());
        }
        
        let passkey = self.webauthn
            .finish_passkey_registration(&response, &challenge.state)
            .map_err(|e| format!("Registration failed: {}", e))?;
        
        let cred_id = uuid::Uuid::new_v4().to_string();
        let credential_id_bytes = passkey.cred_id().to_vec();
        let credential_json = serde_json::to_string(&passkey)
            .map_err(|e| format!("Serialization failed: {}", e))?;
        
        // Store credential
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute(
            "INSERT INTO webauthn_credentials (id, identity_id, credential_id, credential_json, created_at, name) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                cred_id,
                challenge.identity_id,
                credential_id_bytes,
                credential_json,
                now,
                credential_name,
            ],
        ).map_err(|e| e.to_string())?;
        
        Ok(StoredCredential {
            id: cred_id,
            identity_id: challenge.identity_id,
            credential_id: credential_id_bytes,
            credential: credential_json,
            created_at: now,
            last_used_at: None,
            name: credential_name,
        })
    }

    /// Begin WebAuthn authentication
    pub async fn begin_authentication(&self, identity_id: &str) -> Result<RequestChallengeResponse, String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let credentials = self.get_credentials_for_identity(identity_id).await?;
        if credentials.is_empty() {
            return Err("No credentials registered".to_string());
        }
        
        let passkeys: Vec<Passkey> = credentials.iter()
            .filter_map(|c| serde_json::from_str(&c.credential).ok())
            .collect();
        
        if passkeys.is_empty() {
            return Err("No valid credentials".to_string());
        }
        
        let (rcr, auth_state) = self.webauthn
            .start_passkey_authentication(&passkeys)
            .map_err(|e| format!("Auth start failed: {}", e))?;
        
        let challenge_id = uuid::Uuid::new_v4().to_string();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        
        let mut challenges = self.auth_challenges.write().await;
        challenges.insert(challenge_id.clone(), AuthenticationChallenge {
            identity_id: identity_id.to_string(),
            state: auth_state,
            expires_at: now + 300,
        });
        
        Ok(rcr)
    }

    /// Complete WebAuthn authentication
    pub async fn complete_authentication(
        &self,
        challenge_id: &str,
        response: PublicKeyCredential,
    ) -> Result<String, String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let mut challenges = self.auth_challenges.write().await;
        let challenge = challenges.remove(challenge_id)
            .ok_or_else(|| "Challenge not found or expired".to_string())?;
        
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        if challenge.expires_at <= now {
            return Err("Challenge expired".to_string());
        }
        
        let auth_result = self.webauthn
            .finish_passkey_authentication(&response, &challenge.state)
            .map_err(|e| format!("Authentication failed: {}", e))?;
        
        // Update last used timestamp
        let cred_id_bytes = auth_result.cred_id().to_vec();
        let conn = self.db.connection();
        let conn = conn.lock();
        let _ = conn.execute(
            "UPDATE webauthn_credentials SET last_used_at = ?1 WHERE credential_id = ?2",
            rusqlite::params![now, cred_id_bytes],
        );
        
        Ok(challenge.identity_id)
    }

    /// Get all credentials for an identity
    pub async fn get_credentials_for_identity(&self, identity_id: &str) -> Result<Vec<StoredCredential>, String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        
        let mut stmt = conn.prepare(
            "SELECT id, identity_id, credential_id, credential_json, created_at, last_used_at, name FROM webauthn_credentials WHERE identity_id = ?1"
        ).map_err(|e| e.to_string())?;
        
        let rows = stmt.query_map(rusqlite::params![identity_id], |row| {
            Ok(StoredCredential {
                id: row.get(0)?,
                identity_id: row.get(1)?,
                credential_id: row.get(2)?,
                credential: row.get(3)?,
                created_at: row.get(4)?,
                last_used_at: row.get(5)?,
                name: row.get(6)?,
            })
        }).map_err(|e| e.to_string())?;
        
        let mut creds = Vec::new();
        for row in rows {
            creds.push(row.map_err(|e| e.to_string())?);
        }
        Ok(creds)
    }

    /// Count credentials for an identity
    pub async fn credential_count(&self, identity_id: &str) -> Result<usize, String> {
        let creds = self.get_credentials_for_identity(identity_id).await?;
        Ok(creds.len())
    }

    /// Delete a credential
    pub async fn delete_credential(&self, credential_id: &str, identity_id: &str) -> Result<(), String> {
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute(
            "DELETE FROM webauthn_credentials WHERE id = ?1 AND identity_id = ?2",
            rusqlite::params![credential_id, identity_id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[async_trait]
impl AuthProvider for WebAuthnProvider {
    fn provider_type(&self) -> AuthProviderType {
        AuthProviderType::Local
    }
    
    async fn can_handle(&self, _identifier: &str) -> bool {
        // WebAuthn can handle any local user
        true
    }
    
    async fn begin_auth(&self, request: &LoginRequest) -> Result<AuthResult, String> {
        // This is handled separately via begin_authentication
        Err("Use WebAuthn-specific endpoints".to_string())
    }
    
    async fn complete_mfa(&self, _challenge_id: &str, _response: serde_json::Value) -> Result<AuthResult, String> {
        Err("Use WebAuthn-specific endpoints".to_string())
    }
    
    async fn register(&self, _request: &RegistrationRequest) -> Result<Identity, String> {
        Err("Use local provider for registration, then add WebAuthn credentials".to_string())
    }
    
    async fn get_identity(&self, _id: &str) -> Result<Option<Identity>, String> {
        // Delegate to main identity store
        Ok(None)
    }
    
    async fn get_identity_by_identifier(&self, _identifier: &str) -> Result<Option<Identity>, String> {
        Ok(None)
    }
}
