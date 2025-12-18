//! Authentication middleware for Axum.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::types::Identity;
use super::provider::AuthManager;

/// Extension that holds the authenticated identity
#[derive(Clone)]
pub struct AuthenticatedIdentity(pub Identity);

/// Check if the request has a valid session and extract identity
pub async fn auth_extractor(
    auth_manager: &AuthManager,
    auth_header: Option<&str>,
) -> Result<Identity, (StatusCode, &'static str)> {
    let token = auth_header
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or((StatusCode::UNAUTHORIZED, "Missing or invalid authorization header"))?;
    
    let result = auth_manager.validate_session(token).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Session validation failed"))?;
    
    let (_session, identity) = result
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid or expired session"))?;
    
    Ok(identity)
}

/// Middleware that requires authentication
pub async fn require_auth<S>(
    State(auth_manager): State<Arc<AuthManager>>,
    mut request: Request,
    next: Next,
) -> Response {
    let auth_header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    
    match auth_extractor(&auth_manager, auth_header).await {
        Ok(identity) => {
            request.extensions_mut().insert(AuthenticatedIdentity(identity));
            next.run(request).await
        }
        Err((status, msg)) => {
            (status, Json(serde_json::json!({"error": msg}))).into_response()
        }
    }
}

/// Middleware factory that requires a specific permission
pub fn require_permission(permission: &'static str) -> impl Fn(
    State<Arc<AuthManager>>,
    Request,
    Next,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> + Clone {
    move |State(auth_manager): State<Arc<AuthManager>>, request: Request, next: Next| {
        let perm = permission;
        Box::pin(async move {
            let auth_header = request
                .headers()
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok());
            
            let identity = match auth_extractor(&auth_manager, auth_header).await {
                Ok(id) => id,
                Err((status, msg)) => {
                    return (status, Json(serde_json::json!({"error": msg}))).into_response();
                }
            };
            
            // Check permission
            if !auth_manager.has_permission(&identity, perm).await {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({
                        "error": "access_denied",
                        "required_permission": perm,
                        "user_roles": identity.roles,
                    })),
                ).into_response();
            }
            
            // Continue with the identity attached
            let mut request = request;
            request.extensions_mut().insert(AuthenticatedIdentity(identity));
            next.run(request).await
        })
    }
}

/// Helper to get the authenticated identity from request extensions
pub fn get_identity(request: &Request) -> Option<&Identity> {
    request.extensions().get::<AuthenticatedIdentity>().map(|a| &a.0)
}

/// Audit log helper
pub async fn log_auth_event(
    auth_manager: &AuthManager,
    event_type: super::types::AuthEventType,
    identity_id: Option<&str>,
    identity_name: Option<&str>,
    success: bool,
    details: serde_json::Value,
) {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let event = super::types::AuthAuditEvent {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64,
        event_type,
        identity_id: identity_id.map(String::from),
        identity_name: identity_name.map(String::from),
        ip_address: None,
        user_agent: None,
        success,
        details,
    };
    
    // Store audit event (best effort)
    let conn = auth_manager.db.connection();
    let conn = conn.lock();
    let _ = conn.execute(
        "INSERT INTO auth_audit_log (id, timestamp, event_type, identity_id, identity_name, success, details_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            event.id,
            event.timestamp,
            serde_json::to_string(&event.event_type).unwrap_or_default(),
            event.identity_id,
            event.identity_name,
            event.success,
            serde_json::to_string(&event.details).unwrap_or_default(),
        ],
    );
}

/// Initialize audit log table
pub fn init_audit_schema(db: &infrasim_common::Database) {
    let conn = db.connection();
    let conn = conn.lock();
    let _ = conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS auth_audit_log (
            id TEXT PRIMARY KEY,
            timestamp INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            identity_id TEXT,
            identity_name TEXT,
            ip_address TEXT,
            user_agent TEXT,
            success INTEGER NOT NULL,
            details_json TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON auth_audit_log(timestamp);
        CREATE INDEX IF NOT EXISTS idx_audit_identity ON auth_audit_log(identity_id);
        CREATE INDEX IF NOT EXISTS idx_audit_type ON auth_audit_log(event_type);
        "#,
    );
}
