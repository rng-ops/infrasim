//! Authentication provider abstraction.
//!
//! Supports pluggable backends for different authentication methods.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::types::*;
use super::rbac::PolicyEngine;
use infrasim_common::Database;

/// Configuration for authentication providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthProviderConfig {
    /// Enable local TOTP authentication
    #[serde(default = "default_true")]
    pub local_totp_enabled: bool,
    
    /// Enable WebAuthn (FIDO2) authentication
    #[serde(default = "default_true")]
    pub webauthn_enabled: bool,
    
    /// WebAuthn relying party ID (usually the domain)
    #[serde(default = "default_rp_id")]
    pub webauthn_rp_id: String,
    
    /// WebAuthn relying party name
    #[serde(default = "default_rp_name")]
    pub webauthn_rp_name: String,
    
    /// WebAuthn origin (e.g., "https://example.com")
    pub webauthn_origin: Option<String>,
    
    /// Enable OIDC authentication
    #[serde(default)]
    pub oidc_enabled: bool,
    
    /// OIDC provider configuration
    pub oidc: Option<OidcConfig>,
    
    /// Session TTL in seconds
    #[serde(default = "default_session_ttl")]
    pub session_ttl_secs: i64,
    
    /// Require MFA for all users
    #[serde(default)]
    pub require_mfa: bool,
    
    /// Default role for new users
    #[serde(default = "default_role")]
    pub default_role: String,
}

fn default_true() -> bool { true }
fn default_rp_id() -> String { "localhost".to_string() }
fn default_rp_name() -> String { "InfraSim".to_string() }
fn default_session_ttl() -> i64 { 12 * 60 * 60 } // 12 hours
fn default_role() -> String { "viewer".to_string() }

impl Default for AuthProviderConfig {
    fn default() -> Self {
        Self {
            local_totp_enabled: true,
            webauthn_enabled: true,
            webauthn_rp_id: default_rp_id(),
            webauthn_rp_name: default_rp_name(),
            webauthn_origin: None,
            oidc_enabled: false,
            oidc: None,
            session_ttl_secs: default_session_ttl(),
            require_mfa: false,
            default_role: default_role(),
        }
    }
}

/// OIDC provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// Provider name (e.g., "keycloak", "auth0")
    pub provider: String,
    /// Issuer URL
    pub issuer: String,
    /// Client ID
    pub client_id: String,
    /// Client secret
    pub client_secret: String,
    /// Redirect URI
    pub redirect_uri: String,
    /// Scopes to request
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
    /// Claim to use as display name
    #[serde(default = "default_name_claim")]
    pub name_claim: String,
    /// Claim to use for role mapping
    pub roles_claim: Option<String>,
}

fn default_scopes() -> Vec<String> {
    vec!["openid".to_string(), "profile".to_string(), "email".to_string()]
}

fn default_name_claim() -> String { "preferred_username".to_string() }

/// Trait for authentication providers
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Get the provider type
    fn provider_type(&self) -> AuthProviderType;
    
    /// Check if this provider can handle the given identifier
    async fn can_handle(&self, identifier: &str) -> bool;
    
    /// Begin authentication (returns challenge if MFA required)
    async fn begin_auth(&self, request: &LoginRequest) -> Result<AuthResult, String>;
    
    /// Complete MFA challenge
    async fn complete_mfa(&self, challenge_id: &str, response: serde_json::Value) -> Result<AuthResult, String>;
    
    /// Register a new identity
    async fn register(&self, request: &RegistrationRequest) -> Result<Identity, String>;
    
    /// Get identity by ID
    async fn get_identity(&self, id: &str) -> Result<Option<Identity>, String>;
    
    /// Get identity by identifier (email or display name)
    async fn get_identity_by_identifier(&self, identifier: &str) -> Result<Option<Identity>, String>;
}

/// Central authentication manager
pub struct AuthManager {
    pub config: AuthProviderConfig,
    pub policy_engine: Arc<RwLock<PolicyEngine>>,
    pub db: Database,
    providers: Vec<Arc<dyn AuthProvider>>,
}

impl AuthManager {
    pub fn new(config: AuthProviderConfig, db: Database) -> Self {
        Self {
            config,
            policy_engine: Arc::new(RwLock::new(PolicyEngine::new())),
            db,
            providers: Vec::new(),
        }
    }

    /// Register a provider
    pub fn register_provider(&mut self, provider: Arc<dyn AuthProvider>) {
        self.providers.push(provider);
    }

    /// Get all registered providers
    pub fn providers(&self) -> &[Arc<dyn AuthProvider>] {
        &self.providers
    }

    /// Find provider for identifier
    pub async fn find_provider(&self, identifier: &str) -> Option<Arc<dyn AuthProvider>> {
        for provider in &self.providers {
            if provider.can_handle(identifier).await {
                return Some(provider.clone());
            }
        }
        None
    }

    /// Authenticate a user
    pub async fn authenticate(&self, request: &LoginRequest) -> Result<AuthResult, String> {
        // Try each provider in order
        for provider in &self.providers {
            if provider.can_handle(&request.identifier).await {
                return provider.begin_auth(request).await;
            }
        }
        Err("No suitable authentication provider found".to_string())
    }

    /// Register a new identity
    pub async fn register(&self, request: &RegistrationRequest) -> Result<Identity, String> {
        // Use the first local provider for registration
        for provider in &self.providers {
            if provider.provider_type() == AuthProviderType::Local {
                return provider.register(request).await;
            }
        }
        Err("No local provider available for registration".to_string())
    }

    /// Create a session for an identity
    pub async fn create_session(&self, identity: &Identity, auth_method: &str) -> Result<Session, String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        
        let token = uuid::Uuid::new_v4().to_string();
        let expires_at = now + self.config.session_ttl_secs;
        
        let session = Session {
            token: token.clone(),
            identity_id: identity.id.clone(),
            created_at: now,
            expires_at,
            last_seen_at: now,
            auth_method: auth_method.to_string(),
            ip_address: None,
            user_agent: None,
        };
        
        // Store session in database
        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute(
            "INSERT INTO auth_sessions (token, identity_id, created_at, expires_at, last_seen_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![token, identity.id, now, expires_at, now],
        ).map_err(|e| e.to_string())?;
        
        Ok(session)
    }

    /// Validate a session token
    pub async fn validate_session(&self, token: &str) -> Result<Option<(Session, Identity)>, String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        
        let conn = self.db.connection();
        let conn = conn.lock();
        
        let row: Option<(String, i64, i64, i64)> = conn
            .query_row(
                "SELECT identity_id, created_at, expires_at, last_seen_at FROM auth_sessions WHERE token = ?1",
                rusqlite::params![token],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        
        let (identity_id, created_at, expires_at, _last_seen) = match row {
            Some(r) => r,
            None => return Ok(None),
        };
        
        if expires_at <= now {
            // Session expired, clean up
            let _ = conn.execute("DELETE FROM auth_sessions WHERE token = ?1", rusqlite::params![token]);
            return Ok(None);
        }
        
        // Update last seen
        let _ = conn.execute(
            "UPDATE auth_sessions SET last_seen_at = ?1 WHERE token = ?2",
            rusqlite::params![now, token],
        );
        
        // Get identity
        let identity_row: Option<(String, String, String, i64, i64)> = conn
            .query_row(
                "SELECT id, display_name, role, created_at, totp_enabled FROM auth_identities WHERE id = ?1",
                rusqlite::params![identity_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        
        let (id, display_name, role, id_created_at, totp_enabled) = match identity_row {
            Some(r) => r,
            None => return Ok(None),
        };
        
        let session = Session {
            token: token.to_string(),
            identity_id: id.clone(),
            created_at,
            expires_at,
            last_seen_at: now,
            auth_method: "unknown".to_string(),
            ip_address: None,
            user_agent: None,
        };
        
        let identity = Identity {
            id,
            display_name,
            email: None,
            roles: vec![role],
            provider: AuthProviderType::Local,
            created_at: id_created_at,
            last_login_at: None,
            auth_methods: vec![
                AuthMethod::Totp { enabled: totp_enabled != 0, enrolled_at: None },
            ],
        };
        
        Ok(Some((session, identity)))
    }

    /// Check if identity has permission
    pub async fn has_permission(&self, identity: &Identity, permission: &str) -> bool {
        let engine = self.policy_engine.read().await;
        engine.has_permission(&identity.roles, permission)
    }

    /// Export RBAC policy as Terraform
    pub async fn export_terraform(&self) -> String {
        let engine = self.policy_engine.read().await;
        engine.export_terraform()
    }
}

use rusqlite::OptionalExtension;
