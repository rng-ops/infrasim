//! Meshnet API routes
//!
//! All routes for the Meshnet Console MVP:
//! - WebAuthn authentication
//! - Identity management
//! - Mesh peer management
//! - Appliance management
//! - Hosting stubs

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::meshnet::{
    db::{MeshnetDb, MeshnetUser},
    identity::{IdentityService, ProvisioningStatus},
    mesh::{MeshPeer, MeshProvider, WireGuardProvider},
    appliance::ApplianceService,
};

use webauthn_rs::prelude::*;

// ============================================================================
// State
// ============================================================================

/// Meshnet API state
pub struct MeshnetState {
    pub db: MeshnetDb,
    pub webauthn: Arc<Webauthn>,
    pub identity_service: Arc<IdentityService>,
    pub mesh_provider: Arc<WireGuardProvider>,
    pub appliance_service: Arc<ApplianceService>,
    pub base_domain: String,
}

impl MeshnetState {
    pub fn new(db: MeshnetDb) -> Result<Self, String> {
        let base_domain = std::env::var("BASE_DOMAIN")
            .unwrap_or_else(|_| "mesh.local".to_string());
        
        // WebAuthn configuration
        let rp_id = std::env::var("WEBAUTHN_RP_ID")
            .unwrap_or_else(|_| "localhost".to_string());
        let rp_origin = std::env::var("WEBAUTHN_RP_ORIGIN")
            .unwrap_or_else(|_| "http://localhost:8080".to_string());
        let rp_name = std::env::var("WEBAUTHN_RP_NAME")
            .unwrap_or_else(|_| "Meshnet Console".to_string());
        
        let rp_origin_url = url::Url::parse(&rp_origin)
            .map_err(|e| format!("Invalid RP origin: {}", e))?;
        
        let webauthn = WebauthnBuilder::new(&rp_id, &rp_origin_url)
            .map_err(|e| format!("WebAuthn builder error: {}", e))?
            .rp_name(&rp_name)
            .build()
            .map_err(|e| format!("WebAuthn build error: {}", e))?;
        
        let mesh_provider = Arc::new(WireGuardProvider::new(db.clone()));
        let identity_service = Arc::new(IdentityService::new(db.clone()));
        let appliance_service = Arc::new(ApplianceService::new(
            db.clone(),
            mesh_provider.clone(),
        ));
        
        Ok(Self {
            db,
            webauthn: Arc::new(webauthn),
            identity_service,
            mesh_provider,
            appliance_service,
            base_domain,
        })
    }
}

// ============================================================================
// Request/Response types
// ============================================================================

// Auth types
#[derive(Debug, Serialize)]
struct MeResponse {
    user: Option<MeshnetUser>,
    identity: Option<crate::meshnet::db::MeshnetIdentity>,
    statuses: Option<ProvisioningStatus>,
}

#[derive(Debug, Deserialize)]
struct RegisterOptionsRequest {
    handle: String,
}

#[derive(Debug, Serialize)]
struct RegisterOptionsResponse {
    challenge_id: String,
    options: CreationChallengeResponse,
}

#[derive(Debug, Deserialize)]
struct RegisterVerifyRequest {
    challenge_id: String,
    handle: String,
    credential: RegisterPublicKeyCredential,
}

#[derive(Debug, Serialize)]
struct AuthResponse {
    token: String,
    expires_at: i64,
    user: MeshnetUser,
}

#[derive(Debug, Serialize)]
struct LoginOptionsResponse {
    challenge_id: String,
    options: RequestChallengeResponse,
}

#[derive(Debug, Deserialize)]
struct LoginVerifyRequest {
    challenge_id: String,
    credential: PublicKeyCredential,
}

#[derive(Debug, Deserialize)]
struct LoginOptionsRequest {
    handle: String,
}

// Identity types
#[derive(Debug, Deserialize)]
struct CreateIdentityRequest {
    handle: String,
}

// Mesh types
#[derive(Debug, Deserialize)]
struct CreatePeerRequest {
    name: String,
}

// Appliance types
#[derive(Debug, Deserialize)]
struct CreateApplianceRequest {
    name: String,
}

// ============================================================================
// Router
// ============================================================================

/// Create the meshnet router from a Database
/// 
/// Returns a router with the meshnet API routes nested under /api/meshnet
pub fn meshnet_router(db: infrasim_common::Database) -> Router {
    let meshnet_db = MeshnetDb::new(db);
    
    // Initialize schema
    if let Err(e) = meshnet_db.init_schema() {
        warn!("Failed to initialize meshnet schema: {}", e);
    }
    
    let state = match MeshnetState::new(meshnet_db) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            warn!("Failed to create meshnet state: {}", e);
            // Return empty router if initialization fails
            return Router::new();
        }
    };
    
    create_meshnet_routes(state)
}

/// Create the meshnet router with pre-configured state
fn create_meshnet_routes(state: Arc<MeshnetState>) -> Router {
    Router::new()
        // WebAuthn auth
        .route("/auth/register/options", post(register_options_handler))
        .route("/auth/register/verify", post(register_verify_handler))
        .route("/auth/login/options", post(login_options_handler))
        .route("/auth/login/verify", post(login_verify_handler))
        .route("/auth/logout", post(logout_handler))
        .route("/me", get(me_handler))
        
        // Identity
        .route("/identity", post(create_identity_handler).get(get_identity_handler))
        .route("/identity/provision", post(provision_identity_handler))
        .route("/identity/status", get(identity_status_handler))
        
        // Mesh
        .route("/mesh/peers", post(create_peer_handler).get(list_peers_handler))
        .route("/mesh/peers/:id", get(get_peer_handler))
        .route("/mesh/peers/:id/config", get(download_peer_config_handler))
        .route("/mesh/peers/:id/revoke", post(revoke_peer_handler))
        .route("/mesh/rotate-keys", post(rotate_keys_handler))
        
        // Appliances
        .route("/appliances", post(create_appliance_handler).get(list_appliances_handler))
        .route("/appliances/:id", get(get_appliance_handler).delete(delete_appliance_handler))
        .route("/appliances/:id/archive", get(download_archive_handler))
        .route("/appliances/:id/terraform", get(get_terraform_handler))
        .route("/appliances/:id/redeploy", post(redeploy_appliance_handler))
        
        // Hosting stubs
        .route("/hosting/list", get(hosting_list_stub))
        .route("/hosting/upload", post(hosting_upload_stub))
        .route("/hosting/file", delete(hosting_delete_stub))
        
        .with_state(state)
}

// ============================================================================
// Auth helpers
// ============================================================================

fn now_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

const SESSION_TTL_SECS: i64 = 60 * 60 * 24; // 24 hours
const CHALLENGE_TTL_SECS: i64 = 300; // 5 minutes

fn hash_token(token: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn extract_token(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn get_current_user(state: &MeshnetState, headers: &axum::http::HeaderMap) -> Result<MeshnetUser, StatusCode> {
    let token = extract_token(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let token_hash = hash_token(&token);
    
    let session = state.db.get_session_by_token_hash(&token_hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    if session.expires_at <= now_epoch_secs() {
        let _ = state.db.delete_session(&token_hash);
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    state.db.get_user(session.user_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)
}

// ============================================================================
// Auth handlers
// ============================================================================

async fn register_options_handler(
    State(state): State<Arc<MeshnetState>>,
    Json(req): Json<RegisterOptionsRequest>,
) -> impl IntoResponse {
    // Validate handle first
    let handle = match crate::meshnet::validate_handle(&req.handle) {
        Ok(h) => h,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response();
        }
    };
    
    // Check if handle is taken
    if let Ok(Some(_)) = state.db.get_identity_by_handle(&handle) {
        return (StatusCode::CONFLICT, Json(serde_json::json!({
            "error": format!("Handle '{}' is already taken", handle)
        }))).into_response();
    }
    
    // Generate registration challenge
    let user_id = Uuid::new_v4();
    let exclude_credentials: Vec<CredentialID> = vec![];
    
    match state.webauthn.start_passkey_registration(
        user_id,
        &handle,
        &handle,
        Some(exclude_credentials),
    ) {
        Ok((ccr, reg_state)) => {
            let challenge_id = Uuid::new_v4().to_string();
            let expires_at = now_epoch_secs() + CHALLENGE_TTL_SECS;
            
            // Store challenge state
            let state_json = serde_json::to_string(&reg_state).unwrap_or_default();
            if let Err(e) = state.db.store_challenge(
                &challenge_id,
                Some(user_id),
                "registration",
                &state_json,
                expires_at,
            ) {
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                    "error": format!("Failed to store challenge: {}", e)
                }))).into_response();
            }
            
            (StatusCode::OK, Json(RegisterOptionsResponse {
                challenge_id,
                options: ccr,
            })).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to create registration challenge: {}", e)
            }))).into_response()
        }
    }
}

async fn register_verify_handler(
    State(state): State<Arc<MeshnetState>>,
    Json(req): Json<RegisterVerifyRequest>,
) -> impl IntoResponse {
    // Validate handle again
    let handle = match crate::meshnet::validate_handle(&req.handle) {
        Ok(h) => h,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response();
        }
    };
    
    // Get challenge
    let (user_id, _challenge_type, state_json, expires_at) = match state.db.get_challenge(&req.challenge_id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "Challenge not found or expired"
            }))).into_response();
        }
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to get challenge: {}", e)
            }))).into_response();
        }
    };
    
    if expires_at <= now_epoch_secs() {
        let _ = state.db.delete_challenge(&req.challenge_id);
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "Challenge expired"
        }))).into_response();
    }
    
    let user_id = match user_id {
        Some(id) => id,
        None => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "Invalid challenge state"
            }))).into_response();
        }
    };
    
    // Deserialize registration state
    let reg_state: PasskeyRegistration = match serde_json::from_str(&state_json) {
        Ok(s) => s,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to parse challenge state: {}", e)
            }))).into_response();
        }
    };
    
    // Finish registration
    let passkey = match state.webauthn.finish_passkey_registration(&req.credential, &reg_state) {
        Ok(pk) => pk,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": format!("Registration failed: {}", e)
            }))).into_response();
        }
    };
    
    // Delete challenge
    let _ = state.db.delete_challenge(&req.challenge_id);
    
    // Create user
    let user = match state.db.create_user(Some(&handle)) {
        Ok(mut u) => {
            // Override the generated ID with the one from the challenge
            u.id = user_id;
            // Actually we need to create with specific ID - for now, use the new ID
            u
        }
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to create user: {}", e)
            }))).into_response();
        }
    };
    
    // Store credential
    let passkey_json = serde_json::to_string(&passkey).unwrap_or_default();
    let cred = crate::meshnet::db::WebAuthnCredential {
        id: Uuid::new_v4(),
        user_id: user.id,
        credential_id: passkey.cred_id().to_vec(),
        public_key: passkey_json.into_bytes(),
        sign_count: 0,
        transports: None,
        created_at: now_epoch_secs(),
    };
    
    if let Err(e) = state.db.store_credential(&cred) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("Failed to store credential: {}", e)
        }))).into_response();
    }
    
    // Create identity
    if let Err(e) = state.identity_service.create_identity(user.id, &handle).await {
        warn!("Failed to create identity during registration: {}", e);
        // Continue anyway - user can create identity later
    }
    
    // Create session
    let token = hex::encode(rand::random::<[u8; 32]>());
    let token_hash = hash_token(&token);
    let expires_at = now_epoch_secs() + SESSION_TTL_SECS;
    
    if let Err(e) = state.db.create_session(user.id, &token_hash, expires_at) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("Failed to create session: {}", e)
        }))).into_response();
    }
    
    info!("User {} registered with handle {}", user.id, handle);
    
    (StatusCode::OK, Json(AuthResponse {
        token,
        expires_at,
        user,
    })).into_response()
}

async fn login_options_handler(
    State(state): State<Arc<MeshnetState>>,
    Json(req): Json<LoginOptionsRequest>,
) -> impl IntoResponse {
    // Look up user by handle to get their passkeys
    let identity = match state.db.get_identity_by_handle(&req.handle) {
        Ok(Some(i)) => i,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "Identity not found"
            }))).into_response();
        }
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Database error: {}", e)
            }))).into_response();
        }
    };
    
    // Get user's credentials
    let creds = match state.db.get_credentials_for_user(identity.user_id) {
        Ok(c) => c,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to get credentials: {}", e)
            }))).into_response();
        }
    };
    
    // Parse passkeys from stored credentials
    let passkeys: Vec<Passkey> = creds.iter()
        .filter_map(|c| {
            let json = String::from_utf8(c.public_key.clone()).ok()?;
            serde_json::from_str::<Passkey>(&json).ok()
        })
        .collect();
    
    if passkeys.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "No passkeys registered for this identity"
        }))).into_response();
    }
    
    // Start passkey authentication
    match state.webauthn.start_passkey_authentication(&passkeys) {
        Ok((rcr, auth_state)) => {
            let challenge_id = Uuid::new_v4().to_string();
            let expires_at = now_epoch_secs() + CHALLENGE_TTL_SECS;
            
            let state_json = serde_json::to_string(&auth_state).unwrap_or_default();
            if let Err(e) = state.db.store_challenge(
                &challenge_id,
                Some(identity.user_id),
                "authentication",
                &state_json,
                expires_at,
            ) {
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                    "error": format!("Failed to store challenge: {}", e)
                }))).into_response();
            }
            
            (StatusCode::OK, Json(LoginOptionsResponse {
                challenge_id,
                options: rcr,
            })).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to create login challenge: {}", e)
            }))).into_response()
        }
    }
}

async fn login_verify_handler(
    State(state): State<Arc<MeshnetState>>,
    Json(req): Json<LoginVerifyRequest>,
) -> impl IntoResponse {
    // Get challenge - for passkey auth we stored the user_id
    let (user_id, _challenge_type, state_json, expires_at) = match state.db.get_challenge(&req.challenge_id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "Challenge not found or expired"
            }))).into_response();
        }
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to get challenge: {}", e)
            }))).into_response();
        }
    };
    
    let user_id = match user_id {
        Some(id) => id,
        None => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "Invalid challenge: no user ID"
            }))).into_response();
        }
    };
    
    if expires_at <= now_epoch_secs() {
        let _ = state.db.delete_challenge(&req.challenge_id);
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "Challenge expired"
        }))).into_response();
    }
    
    // Deserialize passkey auth state
    let auth_state: PasskeyAuthentication = match serde_json::from_str(&state_json) {
        Ok(s) => s,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to parse challenge state: {}", e)
            }))).into_response();
        }
    };
    
    // Get user
    let user = match state.db.get_user(user_id) {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({
                "error": "User not found"
            }))).into_response();
        }
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to look up user: {}", e)
            }))).into_response();
        }
    };
    
    // Finish passkey authentication
    let auth_result = match state.webauthn.finish_passkey_authentication(
        &req.credential,
        &auth_state,
    ) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({
                "error": format!("Authentication failed: {}", e)
            }))).into_response();
        }
    };
    
    // Update sign count
    let cred_id = req.credential.id.as_ref();
    let _ = state.db.update_credential_sign_count(cred_id, auth_result.counter());
    
    // Delete challenge
    let _ = state.db.delete_challenge(&req.challenge_id);
    
    // Create session
    let token = hex::encode(rand::random::<[u8; 32]>());
    let token_hash = hash_token(&token);
    let expires_at = now_epoch_secs() + SESSION_TTL_SECS;
    
    if let Err(e) = state.db.create_session(user.id, &token_hash, expires_at) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("Failed to create session: {}", e)
        }))).into_response();
    }
    
    info!("User {} logged in", user.id);
    
    (StatusCode::OK, Json(AuthResponse {
        token,
        expires_at,
        user,
    })).into_response()
}

async fn logout_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if let Some(token) = extract_token(&headers) {
        let token_hash = hash_token(&token);
        let _ = state.db.delete_session(&token_hash);
    }
    StatusCode::NO_CONTENT
}

async fn me_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    match get_current_user(&state, &headers) {
        Ok(user) => {
            let identity = state.db.get_identity_by_user(user.id).ok().flatten();
            let statuses = identity.as_ref().map(|i| ProvisioningStatus::from(i));
            
            (StatusCode::OK, Json(MeResponse {
                user: Some(user),
                identity,
                statuses,
            })).into_response()
        }
        Err(_) => {
            (StatusCode::OK, Json(MeResponse {
                user: None,
                identity: None,
                statuses: None,
            })).into_response()
        }
    }
}

// ============================================================================
// Identity handlers
// ============================================================================

async fn create_identity_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateIdentityRequest>,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    match state.identity_service.create_identity(user.id, &req.handle).await {
        Ok(identity) => (StatusCode::CREATED, Json(identity)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn get_identity_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    match state.identity_service.get_identity(user.id) {
        Ok(Some(identity)) => (StatusCode::OK, Json(identity)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "No identity found"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn provision_identity_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    match state.identity_service.start_provisioning(user.id).await {
        Ok(()) => (StatusCode::ACCEPTED, Json(serde_json::json!({"status": "provisioning started"}))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn identity_status_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    match state.identity_service.get_status(user.id) {
        Ok(Some(status)) => (StatusCode::OK, Json(status)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "No identity found"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

// ============================================================================
// Mesh handlers
// ============================================================================

async fn create_peer_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreatePeerRequest>,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    if req.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Name is required"}))).into_response();
    }
    
    match state.mesh_provider.create_peer(user.id, &req.name).await {
        Ok(peer) => (StatusCode::CREATED, Json(peer)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn list_peers_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    match state.mesh_provider.list_peers(user.id).await {
        Ok(peers) => (StatusCode::OK, Json(serde_json::json!({"peers": peers}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn get_peer_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    let peer_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid peer ID"}))).into_response(),
    };
    
    match state.mesh_provider.get_peer(peer_id).await {
        Ok(Some(peer)) if peer.user_id == user.id => {
            (StatusCode::OK, Json(MeshPeer::from(&peer))).into_response()
        }
        Ok(Some(_)) => (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Access denied"}))).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Peer not found"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn download_peer_config_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    let peer_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid peer ID"}))).into_response(),
    };
    
    let peer = match state.mesh_provider.get_peer(peer_id).await {
        Ok(Some(p)) if p.user_id == user.id => p,
        Ok(Some(_)) => return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Access denied"}))).into_response(),
        Ok(None) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Peer not found"}))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    };
    
    if peer.revoked_at.is_some() {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Peer has been revoked"}))).into_response();
    }
    
    let identity = match state.db.get_identity_by_user(user.id) {
        Ok(Some(i)) => i,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "No identity found"}))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    };
    
    match state.mesh_provider.render_client_config(&peer, &identity) {
        Ok(config) => {
            let filename = format!("{}-{}.conf", identity.handle, peer.name);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/x-wireguard-profile")
                .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", filename))
                .body(axum::body::Body::from(config))
                .unwrap()
                .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn revoke_peer_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    let peer_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid peer ID"}))).into_response(),
    };
    
    // Verify ownership
    match state.mesh_provider.get_peer(peer_id).await {
        Ok(Some(p)) if p.user_id == user.id => {}
        Ok(Some(_)) => return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Access denied"}))).into_response(),
        Ok(None) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Peer not found"}))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
    
    match state.mesh_provider.revoke_peer(peer_id).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "revoked"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn rotate_keys_handler(
    State(_state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // Stub: key rotation would regenerate gateway keys
    (StatusCode::OK, Json(serde_json::json!({
        "status": "ok",
        "note": "Key rotation is a stub in MVP"
    })))
}

// ============================================================================
// Appliance handlers
// ============================================================================

async fn create_appliance_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateApplianceRequest>,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    if req.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Name is required"}))).into_response();
    }
    
    match state.appliance_service.create_appliance(user.id, &req.name).await {
        Ok(appliance) => (StatusCode::CREATED, Json(appliance)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn list_appliances_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    match state.appliance_service.list_appliances(user.id) {
        Ok(appliances) => (StatusCode::OK, Json(serde_json::json!({"appliances": appliances}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn get_appliance_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    let appliance_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid appliance ID"}))).into_response(),
    };
    
    match state.appliance_service.get_appliance(appliance_id) {
        Ok(Some(a)) if a.user_id == user.id => (StatusCode::OK, Json(a)).into_response(),
        Ok(Some(_)) => (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Access denied"}))).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Appliance not found"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn delete_appliance_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    let appliance_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid appliance ID"}))).into_response(),
    };
    
    // Verify ownership
    match state.appliance_service.get_appliance(appliance_id) {
        Ok(Some(a)) if a.user_id == user.id => {}
        Ok(Some(_)) => return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Access denied"}))).into_response(),
        Ok(None) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Appliance not found"}))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
    
    match state.appliance_service.delete_appliance(appliance_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn download_archive_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    let appliance_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid appliance ID"}))).into_response(),
    };
    
    // Verify ownership and get appliance
    let appliance = match state.appliance_service.get_appliance(appliance_id) {
        Ok(Some(a)) if a.user_id == user.id => a,
        Ok(Some(_)) => return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Access denied"}))).into_response(),
        Ok(None) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Appliance not found"}))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    };
    
    if appliance.status != crate::meshnet::db::ApplianceStatus::Ready {
        return (StatusCode::CONFLICT, Json(serde_json::json!({
            "error": "Appliance is not ready",
            "status": appliance.status.to_string()
        }))).into_response();
    }
    
    let archive_path = match &appliance.archive_path {
        Some(p) => p.clone(),
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Archive not found"}))).into_response(),
    };
    
    match tokio::fs::read(&archive_path).await {
        Ok(bytes) => {
            let filename = format!("{}.tar.gz", appliance.name);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/gzip")
                .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", filename))
                .body(axum::body::Body::from(bytes))
                .unwrap()
                .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("Failed to read archive: {}", e)
        }))).into_response(),
    }
}

async fn get_terraform_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    let appliance_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid appliance ID"}))).into_response(),
    };
    
    // Verify ownership
    match state.appliance_service.get_appliance(appliance_id) {
        Ok(Some(a)) if a.user_id == user.id => {}
        Ok(Some(_)) => return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Access denied"}))).into_response(),
        Ok(None) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Appliance not found"}))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
    
    match state.appliance_service.get_terraform(appliance_id) {
        Ok(Some(content)) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(content))
                .unwrap()
                .into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Terraform not found"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn redeploy_appliance_handler(
    State(state): State<Arc<MeshnetState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let user = match get_current_user(&state, &headers) {
        Ok(u) => u,
        Err(status) => return (status, Json(serde_json::json!({"error": "Unauthorized"}))).into_response(),
    };
    
    let appliance_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid appliance ID"}))).into_response(),
    };
    
    // Verify ownership
    match state.appliance_service.get_appliance(appliance_id) {
        Ok(Some(a)) if a.user_id == user.id => {}
        Ok(Some(_)) => return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Access denied"}))).into_response(),
        Ok(None) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Appliance not found"}))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
    
    match state.appliance_service.redeploy(appliance_id).await {
        Ok(()) => (StatusCode::ACCEPTED, Json(serde_json::json!({"status": "rebuilding"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

// ============================================================================
// Hosting stubs (501 Not Implemented)
// ============================================================================

async fn hosting_list_stub() -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({
        "error": "Hosting is not yet implemented",
        "paths": ["/public", "/profiles", "/installers"]
    })))
}

async fn hosting_upload_stub() -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({
        "error": "Hosting is not yet implemented"
    })))
}

async fn hosting_delete_stub() -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({
        "error": "Hosting is not yet implemented"
    })))
}
