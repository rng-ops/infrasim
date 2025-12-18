//! OIDC authentication provider for Keycloak, Auth0, etc.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::types::*;
use super::provider::{AuthProvider, OidcConfig};

/// OIDC token response
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    id_token: Option<String>,
}

/// OIDC userinfo response
#[derive(Debug, Deserialize)]
struct UserInfo {
    sub: String,
    #[serde(alias = "preferred_username")]
    name: Option<String>,
    email: Option<String>,
    #[serde(default)]
    groups: Vec<String>,
    #[serde(default)]
    roles: Vec<String>,
}

/// OIDC provider state
#[derive(Debug)]
pub struct OidcProviderState {
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
    pub redirect_uri: String,
    pub expires_at: i64,
}

/// OIDC authentication provider
pub struct OidcProvider {
    config: OidcConfig,
    http_client: reqwest::Client,
    /// Pending auth states
    pending_states: RwLock<std::collections::HashMap<String, OidcProviderState>>,
    /// Discovered endpoints (cached)
    discovery: RwLock<Option<OidcDiscovery>>,
}

/// OIDC discovery document
#[derive(Debug, Clone, Deserialize)]
struct OidcDiscovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    jwks_uri: String,
    #[serde(default)]
    end_session_endpoint: Option<String>,
}

impl OidcProvider {
    pub fn new(config: OidcConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            pending_states: RwLock::new(std::collections::HashMap::new()),
            discovery: RwLock::new(None),
        }
    }

    /// Discover OIDC endpoints
    async fn discover(&self) -> Result<OidcDiscovery, String> {
        // Check cache
        {
            let cached = self.discovery.read().await;
            if let Some(disc) = cached.as_ref() {
                return Ok(disc.clone());
            }
        }

        // Fetch discovery document
        let discovery_url = format!("{}/.well-known/openid-configuration", self.config.issuer);
        let resp = self.http_client
            .get(&discovery_url)
            .send()
            .await
            .map_err(|e| format!("Discovery failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Discovery failed: {}", resp.status()));
        }

        let disc: OidcDiscovery = resp.json().await
            .map_err(|e| format!("Discovery parse failed: {}", e))?;

        // Cache it
        {
            let mut cached = self.discovery.write().await;
            *cached = Some(disc.clone());
        }

        Ok(disc)
    }

    /// Generate authorization URL for login redirect
    pub async fn authorization_url(&self) -> Result<(String, String), String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        use data_encoding::BASE64URL_NOPAD;
        use sha2::{Sha256, Digest};

        let disc = self.discover().await?;

        let state = uuid::Uuid::new_v4().to_string();
        let nonce = uuid::Uuid::new_v4().to_string();
        
        // Generate PKCE
        let verifier_bytes: [u8; 32] = rand::random();
        let pkce_verifier = BASE64URL_NOPAD.encode(&verifier_bytes);
        let mut hasher = Sha256::new();
        hasher.update(pkce_verifier.as_bytes());
        let challenge = BASE64URL_NOPAD.encode(&hasher.finalize());

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;

        // Store state
        {
            let mut states = self.pending_states.write().await;
            states.insert(state.clone(), OidcProviderState {
                state: state.clone(),
                nonce: nonce.clone(),
                pkce_verifier: pkce_verifier.clone(),
                redirect_uri: self.config.redirect_uri.clone(),
                expires_at: now + 600, // 10 minutes
            });
        }

        let scopes = self.config.scopes.join(" ");
        let url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&nonce={}&code_challenge={}&code_challenge_method=S256",
            disc.authorization_endpoint,
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(&scopes),
            urlencoding::encode(&state),
            urlencoding::encode(&nonce),
            urlencoding::encode(&challenge),
        );

        Ok((url, state))
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(&self, code: &str, state: &str) -> Result<UserInfo, String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let disc = self.discover().await?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;

        // Get and remove pending state
        let pending = {
            let mut states = self.pending_states.write().await;
            states.remove(state)
        }.ok_or_else(|| "Invalid state parameter".to_string())?;

        if pending.expires_at <= now {
            return Err("Authorization expired".to_string());
        }

        // Exchange code for tokens
        let token_resp = self.http_client
            .post(&disc.token_endpoint)
            .form(&[
                ("grant_type", "authorization_code"),
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("code", code),
                ("redirect_uri", &pending.redirect_uri),
                ("code_verifier", &pending.pkce_verifier),
            ])
            .send()
            .await
            .map_err(|e| format!("Token exchange failed: {}", e))?;

        if !token_resp.status().is_success() {
            let err = token_resp.text().await.unwrap_or_default();
            return Err(format!("Token exchange failed: {}", err));
        }

        let tokens: TokenResponse = token_resp.json().await
            .map_err(|e| format!("Token parse failed: {}", e))?;

        // Get user info
        let userinfo_resp = self.http_client
            .get(&disc.userinfo_endpoint)
            .bearer_auth(&tokens.access_token)
            .send()
            .await
            .map_err(|e| format!("Userinfo failed: {}", e))?;

        if !userinfo_resp.status().is_success() {
            return Err("Failed to get user info".to_string());
        }

        let userinfo: UserInfo = userinfo_resp.json().await
            .map_err(|e| format!("Userinfo parse failed: {}", e))?;

        Ok(userinfo)
    }

    /// Map OIDC user to local identity
    pub fn map_to_identity(&self, userinfo: &UserInfo) -> Identity {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        
        // Map roles from OIDC claims
        let mut roles = userinfo.roles.clone();
        if roles.is_empty() {
            roles = userinfo.groups.clone();
        }
        if roles.is_empty() {
            roles = vec!["viewer".to_string()];
        }

        let display_name = userinfo.name.clone()
            .or(userinfo.email.clone())
            .unwrap_or_else(|| userinfo.sub.clone());

        Identity {
            id: format!("oidc:{}", userinfo.sub),
            display_name,
            email: userinfo.email.clone(),
            roles,
            provider: AuthProviderType::Oidc,
            created_at: now,
            last_login_at: Some(now),
            auth_methods: vec![
                AuthMethod::Oidc { provider: self.config.provider.clone() },
            ],
        }
    }
}

#[async_trait]
impl AuthProvider for OidcProvider {
    fn provider_type(&self) -> AuthProviderType {
        AuthProviderType::Oidc
    }

    async fn can_handle(&self, identifier: &str) -> bool {
        // OIDC handles email-like identifiers or identifiers starting with the provider name
        identifier.contains('@') || identifier.starts_with(&format!("{}:", self.config.provider))
    }

    async fn begin_auth(&self, _request: &LoginRequest) -> Result<AuthResult, String> {
        // OIDC uses redirect-based flow, return the URL to redirect to
        let (url, _state) = self.authorization_url().await?;
        Ok(AuthResult {
            success: false,
            identity: None,
            session: None,
            error: None,
            requires_mfa: Some(MfaChallenge {
                challenge_id: url,
                methods_available: vec!["oidc_redirect".to_string()],
                expires_at: 0,
            }),
        })
    }

    async fn complete_mfa(&self, challenge_id: &str, response: serde_json::Value) -> Result<AuthResult, String> {
        // challenge_id is the authorization code, response contains the state
        let code = challenge_id;
        let state = response.get("state")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing state".to_string())?;

        let userinfo = self.exchange_code(code, state).await?;
        let identity = self.map_to_identity(&userinfo);

        Ok(AuthResult {
            success: true,
            identity: Some(identity),
            session: None, // Session is created by AuthManager
            error: None,
            requires_mfa: None,
        })
    }

    async fn register(&self, _request: &RegistrationRequest) -> Result<Identity, String> {
        Err("OIDC registration is handled by the identity provider".to_string())
    }

    async fn get_identity(&self, _id: &str) -> Result<Option<Identity>, String> {
        // OIDC identities are transient, looked up during auth
        Ok(None)
    }

    async fn get_identity_by_identifier(&self, _identifier: &str) -> Result<Option<Identity>, String> {
        Ok(None)
    }
}
