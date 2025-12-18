//! Core types for the authentication system.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Unique identifier for an authenticated identity
pub type IdentityId = String;

/// Session token
pub type SessionToken = String;

/// An authenticated identity (user/service account)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: IdentityId,
    pub display_name: String,
    pub email: Option<String>,
    pub roles: Vec<String>,
    pub provider: AuthProviderType,
    pub created_at: i64,
    pub last_login_at: Option<i64>,
    /// Enabled auth methods for this identity
    pub auth_methods: Vec<AuthMethod>,
}

impl Identity {
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role || r == "admin")
    }

    pub fn effective_permissions(&self, engine: &super::PolicyEngine) -> HashSet<String> {
        engine.permissions_for_roles(&self.roles)
    }
}

/// Type of authentication provider
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthProviderType {
    Local,
    Oidc,
    Ldap,
}

/// Authentication method available for an identity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    Totp { enabled: bool, enrolled_at: Option<i64> },
    WebAuthn { credential_count: usize },
    Password { has_password: bool },
    Oidc { provider: String },
}

/// Session data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub token: SessionToken,
    pub identity_id: IdentityId,
    pub created_at: i64,
    pub expires_at: i64,
    pub last_seen_at: i64,
    /// Auth method used for this session
    pub auth_method: String,
    /// IP address of the client
    pub ip_address: Option<String>,
    /// User agent
    pub user_agent: Option<String>,
}

/// Result of an authentication attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    pub success: bool,
    pub identity: Option<Identity>,
    pub session: Option<Session>,
    pub error: Option<String>,
    /// Requires additional factor (MFA)
    pub requires_mfa: Option<MfaChallenge>,
}

/// MFA challenge for multi-factor authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfaChallenge {
    pub challenge_id: String,
    pub methods_available: Vec<String>,
    pub expires_at: i64,
}

/// Registration request for a new identity
#[derive(Debug, Clone, Deserialize)]
pub struct RegistrationRequest {
    pub display_name: String,
    pub email: Option<String>,
    pub initial_role: Option<String>,
}

/// Login request (initial step)
#[derive(Debug, Clone, Deserialize)]
pub struct LoginRequest {
    pub identifier: String, // email or display_name
    #[serde(default)]
    pub totp_code: Option<String>,
    #[serde(default)]
    pub webauthn_response: Option<serde_json::Value>,
}

/// TOTP enrollment response
#[derive(Debug, Clone, Serialize)]
pub struct TotpEnrollment {
    pub secret_b32: String,
    pub otpauth_uri: String,
    pub qr_svg: String,
    pub issuer: String,
    pub label: String,
}

/// WebAuthn registration options
#[derive(Debug, Clone, Serialize)]
pub struct WebAuthnRegistrationOptions {
    pub challenge: String,
    pub rp_id: String,
    pub rp_name: String,
    pub user_id: String,
    pub user_name: String,
    pub user_display_name: String,
    pub attestation: String,
    pub authenticator_selection: serde_json::Value,
    pub pub_key_cred_params: Vec<serde_json::Value>,
    pub timeout: u64,
}

/// WebAuthn login options
#[derive(Debug, Clone, Serialize)]
pub struct WebAuthnLoginOptions {
    pub challenge: String,
    pub rp_id: String,
    pub timeout: u64,
    pub allow_credentials: Vec<serde_json::Value>,
    pub user_verification: String,
}

/// Audit event for authentication actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthAuditEvent {
    pub id: String,
    pub timestamp: i64,
    pub event_type: AuthEventType,
    pub identity_id: Option<String>,
    pub identity_name: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub success: bool,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthEventType {
    Login,
    Logout,
    Registration,
    TotpEnroll,
    TotpVerify,
    WebAuthnRegister,
    WebAuthnLogin,
    PasswordChange,
    RoleChange,
    SessionExpired,
    AccessDenied,
}
