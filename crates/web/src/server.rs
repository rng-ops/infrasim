//! Web server implementation

use crate::static_files::StaticFiles;
use crate::vnc_proxy::VncProxy;
use axum::{
    extract::Request,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    middleware,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post, put, delete},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::process;
use std::sync::Arc;
use std::path::PathBuf;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
struct LocalControl {
    /// If set, admin endpoints require this header: `x-infrasim-admin-token`.
    /// This is distinct from normal Web UI auth.
    admin_token: Option<String>,
    /// Path to a daemon pidfile (best-effort). Used for stop/restart.
    daemon_pidfile: Option<String>,
}

impl LocalControl {
    fn from_env() -> Option<Self> {
        let enabled = std::env::var("INFRASIM_WEB_CONTROL_ENABLED")
            .ok()
            .unwrap_or_else(|| "0".to_string());
        if enabled != "1" {
            return None;
        }

        let admin_token = std::env::var("INFRASIM_WEB_ADMIN_TOKEN")
            .ok()
            .and_then(|v| if v.trim().is_empty() { None } else { Some(v) });

        let daemon_pidfile = std::env::var("INFRASIM_DAEMON_PIDFILE")
            .ok()
            .and_then(|v| if v.trim().is_empty() { None } else { Some(v) });

        Some(Self {
            admin_token,
            daemon_pidfile,
        })
    }

    fn check_admin_token(&self, headers: &axum::http::HeaderMap) -> bool {
        match &self.admin_token {
            None => true,
            Some(expected) => headers
                .get("x-infrasim-admin-token")
                .and_then(|v| v.to_str().ok())
                .map(|v| v == expected)
                .unwrap_or(false),
        }
    }
}

#[derive(Clone, Debug)]
struct UiStatic {
    dir: Option<PathBuf>,
}

impl UiStatic {
    fn from_env() -> Self {
        let dir = std::env::var("INFRASIM_WEB_STATIC_DIR")
            .ok()
            .and_then(|v| {
                let v = v.trim();
                if v.is_empty() { None } else { Some(PathBuf::from(v)) }
            });
        Self { dir }
    }
}

use infrasim_common::crypto::KeyPair;
use infrasim_common::Signer;
use infrasim_common::Database;
use jsonwebtoken::{decode, Algorithm, DecodingKey, TokenData, Validation};
use once_cell::sync::OnceCell;
use data_encoding::BASE32_NOPAD;
use qrcode::QrCode;
use qrcode::render::svg;
use totp_rs::{Algorithm as TotpAlgorithm, Secret, TOTP};
use rusqlite::OptionalExtension;

/// Web server state
#[derive(Clone)]
pub struct WebServer {
    state: Arc<WebServerState>,
}

struct WebServerState {
    /// VNC target registry: vm_id -> (host, port)
    vnc_targets: RwLock<HashMap<String, (String, u16)>>,
    /// Auth tokens
    tokens: RwLock<HashMap<String, String>>,
    /// Static file handler
    static_files: StaticFiles,

    /// Optional SPA static directory
    ui_static: UiStatic,

    cfg: WebServerConfig,
    daemon: DaemonProxy,
    projects: RwLock<HashMap<String, Project>>,

    appliances: RwLock<HashMap<String, ApplianceInstance>>,

    /// Virtual filesystem registry for resource-centric management
    filesystems: RwLock<HashMap<String, Filesystem>>,

    db: Database,

    control: Option<LocalControl>,

    /// MDM mobileconfig manager
    mdm: crate::mdm::MdmManager,
}

// ============================================================================
// TOTP auth (minimal, local)
// ============================================================================

const AUTH_SESSION_TTL_SECS: i64 = 60 * 60 * 12; // 12h
const AUTH_MAX_FAILED_ATTEMPTS: i64 = 10;
const AUTH_LOCKOUT_SECS: i64 = 5 * 60;

fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn init_auth_schema(db: &Database) {
    // Best-effort: if this fails we still want the server to boot.
    // The endpoints will surface errors.
    let conn_arc = db.connection();
    let conn = conn_arc.lock();
    let _ = conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS auth_identities (
            id TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            role TEXT NOT NULL,
            totp_secret_b32 TEXT,
            totp_enabled INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_auth_identities_display_name ON auth_identities(display_name);

        CREATE TABLE IF NOT EXISTS auth_sessions (
            token TEXT PRIMARY KEY,
            identity_id TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            last_seen_at INTEGER NOT NULL,
            FOREIGN KEY(identity_id) REFERENCES auth_identities(id)
        );
        CREATE INDEX IF NOT EXISTS idx_auth_sessions_identity ON auth_sessions(identity_id);
        CREATE INDEX IF NOT EXISTS idx_auth_sessions_expires ON auth_sessions(expires_at);

        CREATE TABLE IF NOT EXISTS auth_attempts (
            identity_id TEXT PRIMARY KEY,
            failed_count INTEGER NOT NULL DEFAULT 0,
            locked_until INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL
        );
        "#,
    );
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthIdentity {
    id: String,
    display_name: String,
    role: String,
    totp_enabled: bool,
    created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateIdentityRequest {
    display_name: String,
    #[serde(default)]
    role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateIdentityResponse {
    identity: AuthIdentity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BeginTotpEnrollRequest {
    display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BeginTotpEnrollResponse {
    identity: AuthIdentity,
    issuer: String,
    label: String,
    secret_b32: String,
    otpauth_uri: String,
    qr_svg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfirmTotpEnrollRequest {
    display_name: String,
    code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoginTotpRequest {
    display_name: String,
    code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoginResponse {
    token: String,
    expires_at: i64,
    identity: AuthIdentity,
}

#[derive(Clone, Debug)]
pub struct WebServerConfig {
    /// InfraSim daemon address, e.g. http://127.0.0.1:50051
    pub daemon_addr: String,
    /// Authentication policy for the Web UI.
    pub auth: WebUiAuth,
}

#[derive(Clone, Debug)]
pub enum WebUiAuth {
    /// Require a bearer token (recommended even on localhost).
    Token(String),
    /// Validate signed JWTs and enforce an issuer allowlist.
    Jwt(JwtAuthConfig),
    /// Generate a random ephemeral token at startup and print it once.
    DevRandom,
    /// No auth (not recommended).
    None,
}

#[derive(Clone, Debug)]
pub struct JwtAuthConfig {
    /// Allowed issuer strings.
    pub allowed_issuers: Vec<String>,
    /// Required audience.
    pub audience: String,
    /// Path to a local JWKS file (JSON).
    pub local_jwks_path: String,
}

impl WebServerConfig {
    fn bearer_token(&self) -> Option<String> {
        match &self.auth {
            WebUiAuth::Token(t) => Some(t.clone()),
            WebUiAuth::Jwt(_) => None,
            WebUiAuth::DevRandom => None,
            WebUiAuth::None => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct JwtRegisteredClaims {
    iss: Option<String>,
    aud: Option<serde_json::Value>,
    exp: Option<i64>,
    nbf: Option<i64>,
    iat: Option<i64>,
    sub: Option<String>,
}

static LOCAL_JWKS_CACHE: OnceCell<Jwks> = OnceCell::new();

#[derive(Debug, Clone, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Debug, Clone, Deserialize)]
struct Jwk {
    kty: String,
    kid: Option<String>,
    alg: Option<String>,
    #[serde(rename = "use")]
    use_: Option<String>,

    // RSA
    n: Option<String>,
    e: Option<String>,

    // EC
    crv: Option<String>,
    x: Option<String>,
    y: Option<String>,
}

fn parse_allowed_issuers(s: &str) -> Vec<String> {
    s.split(',')
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

fn load_local_jwks(path: &str) -> anyhow::Result<Jwks> {
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn audience_matches(aud: &Option<serde_json::Value>, required: &str) -> bool {
    match aud {
        None => false,
        Some(serde_json::Value::String(s)) => s == required,
        Some(serde_json::Value::Array(list)) => list.iter().any(|v| v.as_str() == Some(required)),
        _ => false,
    }
}

fn decoding_key_for_jwk(jwk: &Jwk) -> anyhow::Result<DecodingKey> {
    match jwk.kty.as_str() {
        "RSA" => {
            let n = jwk.n.as_ref().ok_or_else(|| anyhow::anyhow!("RSA jwk missing n"))?;
            let e = jwk.e.as_ref().ok_or_else(|| anyhow::anyhow!("RSA jwk missing e"))?;
            Ok(DecodingKey::from_rsa_components(n, e)?)
        }
        "EC" => {
            let x = jwk.x.as_ref().ok_or_else(|| anyhow::anyhow!("EC jwk missing x"))?;
            let y = jwk.y.as_ref().ok_or_else(|| anyhow::anyhow!("EC jwk missing y"))?;
            Ok(DecodingKey::from_ec_components(x, y)?)
        }
        other => Err(anyhow::anyhow!("unsupported jwk kty: {other}")),
    }
}

fn algorithm_for_jwk(jwk: &Jwk) -> anyhow::Result<Algorithm> {
    // Prefer explicit alg if present, otherwise infer from kty.
    if let Some(alg) = jwk.alg.as_deref() {
        return match alg {
            "RS256" => Ok(Algorithm::RS256),
            "RS384" => Ok(Algorithm::RS384),
            "RS512" => Ok(Algorithm::RS512),
            "ES256" => Ok(Algorithm::ES256),
            "ES384" => Ok(Algorithm::ES384),
            // jsonwebtoken (v9) does not expose ES512; if needed we can upgrade or reject.
            "ES512" => Err(anyhow::anyhow!("unsupported jwt alg ES512 (not supported by current verifier)")),
            other => Err(anyhow::anyhow!("unsupported jwk alg: {other}")),
        };
    }

    match jwk.kty.as_str() {
        "RSA" => Ok(Algorithm::RS256),
        "EC" => Ok(Algorithm::ES256),
        other => Err(anyhow::anyhow!("unsupported jwk kty: {other}")),
    }
}

fn verify_jwt_with_local_jwks(token: &str, cfg: &JwtAuthConfig) -> anyhow::Result<TokenData<JwtRegisteredClaims>> {
    let jwks = LOCAL_JWKS_CACHE.get_or_try_init(|| load_local_jwks(&cfg.local_jwks_path))?;

    // Pull header kid by decoding header only.
    let header = jsonwebtoken::decode_header(token)?;
    let kid = header.kid.clone();

    // Choose key by kid if present, else try all keys.
    let candidates: Vec<&Jwk> = match kid.as_deref() {
        Some(k) => jwks.keys.iter().filter(|j| j.kid.as_deref() == Some(k)).collect(),
        None => jwks.keys.iter().collect(),
    };
    if candidates.is_empty() {
        return Err(anyhow::anyhow!("no jwk found for kid"));
    }

    let mut last_err: Option<anyhow::Error> = None;
    for jwk in candidates {
        let alg = algorithm_for_jwk(jwk)?;
        let mut validation = Validation::new(alg);
        validation.set_audience(&[cfg.audience.clone()]);
        // We verify issuer manually to allow multiple issuers.
        validation.validate_exp = true;
        validation.validate_nbf = true;

        let key = decoding_key_for_jwk(jwk)?;
        match decode::<JwtRegisteredClaims>(token, &key, &validation) {
            Ok(td) => {
                let iss = td.claims.iss.clone().unwrap_or_default();
                if !cfg.allowed_issuers.iter().any(|i| i == &iss) {
                    return Err(anyhow::anyhow!("issuer not allowed"));
                }
                if !audience_matches(&td.claims.aud, &cfg.audience) {
                    return Err(anyhow::anyhow!("audience mismatch"));
                }
                return Ok(td);
            }
            Err(e) => last_err = Some(anyhow::anyhow!(e)),
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("jwt verification failed")))
}

// ============================================================================
// Daemon gRPC Client
// ============================================================================

use crate::generated::infrasim::{
    infra_sim_daemon_client::InfraSimDaemonClient,
    CreateVmRequest, VmSpec, NetworkMode, GetHealthRequest,
    StartVmRequest, StopVmRequest, CreateNetworkRequest, NetworkSpec,
    CreateVolumeRequest, VolumeSpec, VolumeKind,
    CreateConsoleRequest, ConsoleSpec,
    CreateSnapshotRequest, SnapshotSpec,
    // List/Get operations (note: tonic generates snake_case method names)
    ListVMsRequest, GetVmRequest,
    ListVolumesRequest, GetVolumeRequest,
    ListSnapshotsRequest,
    ListNetworksRequest,
    GetAttestationRequest, GetDaemonStatusRequest,
};

#[derive(Clone)]
struct DaemonProxy {
    endpoint: String,
}

impl DaemonProxy {
    fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    async fn connect(&self) -> Result<InfraSimDaemonClient<tonic::transport::Channel>, anyhow::Error> {
        let client = InfraSimDaemonClient::connect(self.endpoint.clone()).await?;
        Ok(client)
    }

    async fn health(&self) -> Result<serde_json::Value, anyhow::Error> {
        match self.connect().await {
            Ok(mut client) => {
                match client.get_health(GetHealthRequest {}).await {
                    Ok(resp) => {
                        let h = resp.into_inner();
                        Ok(serde_json::json!({
                            "ok": h.healthy,
                            "version": h.version,
                            "uptime_seconds": h.uptime_seconds,
                        }))
                    }
                    Err(e) => Ok(serde_json::json!({"ok": false, "error": e.to_string()})),
                }
            }
            Err(e) => Ok(serde_json::json!({"ok": false, "error": e.to_string()})),
        }
    }

    /// Create a VM from an appliance template.
    async fn create_vm(&self, name: &str, template: &ApplianceTemplate) -> Result<String, anyhow::Error> {
        let mut client = self.connect().await?;
        let req = CreateVmRequest {
            name: name.to_string(),
            spec: Some(VmSpec {
                arch: template.arch.clone(),
                machine: template.machine.clone(),
                cpu_cores: template.cpu_cores,
                memory_mb: template.memory_mb,
                compatibility_mode: template.compatibility_mode,
                volume_ids: vec![],
                network_ids: vec![],
                qos_profile_id: String::new(),
                enable_tpm: false,
                boot_disk_id: String::new(),
                extra_args: std::collections::HashMap::new(),
            }),
            labels: std::collections::HashMap::new(),
        };
        let resp = client.create_vm(req).await?;
        let vm = resp.into_inner().vm.ok_or_else(|| anyhow::anyhow!("no vm in response"))?;
        let meta = vm.meta.ok_or_else(|| anyhow::anyhow!("no meta"))?;
        Ok(meta.id)
    }

    /// Start a VM.
    async fn start_vm(&self, vm_id: &str) -> Result<(), anyhow::Error> {
        let mut client = self.connect().await?;
        client.start_vm(StartVmRequest { id: vm_id.to_string() }).await?;
        Ok(())
    }

    /// Stop a VM.
    async fn stop_vm(&self, vm_id: &str, force: bool) -> Result<(), anyhow::Error> {
        let mut client = self.connect().await?;
        client.stop_vm(StopVmRequest { id: vm_id.to_string(), force }).await?;
        Ok(())
    }

    /// Create a network.
    async fn create_network(&self, name: &str, def: &NetworkDef) -> Result<String, anyhow::Error> {
        let mut client = self.connect().await?;
        let mode = match def.mode.as_str() {
            "vmnet_bridged" => NetworkMode::VmnetBridged,
            "vmnet_shared" => NetworkMode::VmnetShared,
            _ => NetworkMode::User,
        };
        let req = CreateNetworkRequest {
            name: name.to_string(),
            spec: Some(NetworkSpec {
                mode: mode.into(),
                cidr: def.cidr.clone().unwrap_or_default(),
                gateway: def.gateway.clone().unwrap_or_default(),
                dns: String::new(),
                dhcp_enabled: def.dhcp,
                mtu: 1500,
            }),
            labels: std::collections::HashMap::new(),
        };
        let resp = client.create_network(req).await?;
        let net = resp.into_inner().network.ok_or_else(|| anyhow::anyhow!("no network in response"))?;
        let meta = net.meta.ok_or_else(|| anyhow::anyhow!("no meta"))?;
        Ok(meta.id)
    }

    /// Create a volume.
    async fn create_volume(&self, name: &str, def: &VolumeDef) -> Result<String, anyhow::Error> {
        let mut client = self.connect().await?;
        let req = CreateVolumeRequest {
            name: name.to_string(),
            spec: Some(VolumeSpec {
                kind: VolumeKind::Disk.into(),
                source: String::new(),
                integrity: None,
                read_only: false,
                size_bytes: (def.size_mb as i64) * 1024 * 1024,
                format: "qcow2".to_string(),
                overlay: true,
            }),
            labels: std::collections::HashMap::new(),
        };
        let resp = client.create_volume(req).await?;
        let vol = resp.into_inner().volume.ok_or_else(|| anyhow::anyhow!("no volume in response"))?;
        let meta = vol.meta.ok_or_else(|| anyhow::anyhow!("no meta"))?;
        Ok(meta.id)
    }

    /// Create a console for a VM.
    async fn create_console(&self, vm_id: &str, vnc_port: i32, web_port: i32) -> Result<String, anyhow::Error> {
        let mut client = self.connect().await?;
        let req = CreateConsoleRequest {
            name: format!("console-{}", vm_id),
            spec: Some(ConsoleSpec {
                vm_id: vm_id.to_string(),
                enable_vnc: true,
                vnc_port,
                enable_web: true,
                web_port,
                auth_token: uuid::Uuid::new_v4().to_string(),
            }),
        };
        let resp = client.create_console(req).await?;
        let console = resp.into_inner().console.ok_or_else(|| anyhow::anyhow!("no console in response"))?;
        let meta = console.meta.ok_or_else(|| anyhow::anyhow!("no meta"))?;
        Ok(meta.id)
    }

    /// Create a snapshot of a VM.
    async fn create_snapshot(&self, vm_id: &str, name: &str, include_memory: bool) -> Result<String, anyhow::Error> {
        let mut client = self.connect().await?;
        let req = CreateSnapshotRequest {
            name: name.to_string(),
            spec: Some(SnapshotSpec {
                vm_id: vm_id.to_string(),
                include_memory,
                include_disk: true,
                description: format!("Snapshot of VM {}", vm_id),
            }),
            labels: std::collections::HashMap::new(),
        };
        let resp = client.create_snapshot(req).await?;
        let snap = resp.into_inner().snapshot.ok_or_else(|| anyhow::anyhow!("no snapshot in response"))?;
        let meta = snap.meta.ok_or_else(|| anyhow::anyhow!("no meta"))?;
        Ok(meta.id)
    }

    // ========================================================================
    // List/Get operations for inventory view
    // ========================================================================

    /// List all VMs from daemon.
    async fn list_vms(&self) -> Result<Vec<VmInfo>, anyhow::Error> {
        let mut client = self.connect().await?;
        let resp = client.list_v_ms(ListVMsRequest { label_selector: std::collections::HashMap::new() }).await?;
        let vms = resp.into_inner().vms;
        Ok(vms.into_iter().map(|vm| {
            let meta = vm.meta.unwrap_or_default();
            let spec = vm.spec.unwrap_or_default();
            let status = vm.status.unwrap_or_default();
            VmInfo {
                id: meta.id,
                name: meta.name,
                arch: spec.arch,
                machine: spec.machine,
                cpu_cores: spec.cpu_cores,
                memory_mb: spec.memory_mb,
                state: vm_state_to_string(status.state),
                vnc_display: status.vnc_display,
                uptime_seconds: status.uptime_seconds,
                volume_ids: spec.volume_ids,
                network_ids: spec.network_ids,
                created_at: meta.created_at,
                labels: meta.labels,
            }
        }).collect())
    }

    /// Get a single VM by ID.
    async fn get_vm(&self, vm_id: &str) -> Result<VmInfo, anyhow::Error> {
        let mut client = self.connect().await?;
        let resp = client.get_vm(GetVmRequest { id: vm_id.to_string() }).await?;
        let vm = resp.into_inner().vm.ok_or_else(|| anyhow::anyhow!("VM not found"))?;
        let meta = vm.meta.unwrap_or_default();
        let spec = vm.spec.unwrap_or_default();
        let status = vm.status.unwrap_or_default();
        Ok(VmInfo {
            id: meta.id,
            name: meta.name,
            arch: spec.arch,
            machine: spec.machine,
            cpu_cores: spec.cpu_cores,
            memory_mb: spec.memory_mb,
            state: vm_state_to_string(status.state),
            vnc_display: status.vnc_display,
            uptime_seconds: status.uptime_seconds,
            volume_ids: spec.volume_ids,
            network_ids: spec.network_ids,
            created_at: meta.created_at,
            labels: meta.labels,
        })
    }

    /// List all volumes (images) from daemon.
    async fn list_volumes(&self) -> Result<Vec<VolumeInfo>, anyhow::Error> {
        let mut client = self.connect().await?;
        let resp = client.list_volumes(ListVolumesRequest {
            label_selector: std::collections::HashMap::new(),
            kind_filter: 0,
        }).await?;
        let volumes = resp.into_inner().volumes;
        Ok(volumes.into_iter().map(|vol| {
            let meta = vol.meta.unwrap_or_default();
            let spec = vol.spec.unwrap_or_default();
            let status = vol.status.unwrap_or_default();
            VolumeInfo {
                id: meta.id,
                name: meta.name,
                kind: volume_kind_to_string(spec.kind),
                format: spec.format,
                size_bytes: spec.size_bytes,
                actual_size: status.actual_size,
                local_path: status.local_path,
                digest: status.digest,
                ready: status.ready,
                verified: status.verified,
                source: spec.source,
                created_at: meta.created_at,
                labels: meta.labels,
            }
        }).collect())
    }

    /// Get a single volume by ID.
    async fn get_volume(&self, vol_id: &str) -> Result<VolumeInfo, anyhow::Error> {
        let mut client = self.connect().await?;
        let resp = client.get_volume(GetVolumeRequest { id: vol_id.to_string() }).await?;
        let vol = resp.into_inner().volume.ok_or_else(|| anyhow::anyhow!("Volume not found"))?;
        let meta = vol.meta.unwrap_or_default();
        let spec = vol.spec.unwrap_or_default();
        let status = vol.status.unwrap_or_default();
        Ok(VolumeInfo {
            id: meta.id,
            name: meta.name,
            kind: volume_kind_to_string(spec.kind),
            format: spec.format,
            size_bytes: spec.size_bytes,
            actual_size: status.actual_size,
            local_path: status.local_path,
            digest: status.digest,
            ready: status.ready,
            verified: status.verified,
            source: spec.source,
            created_at: meta.created_at,
            labels: meta.labels,
        })
    }

    /// List all snapshots from daemon.
    async fn list_snapshots(&self, vm_id: Option<&str>) -> Result<Vec<SnapshotInfo>, anyhow::Error> {
        let mut client = self.connect().await?;
        let resp = client.list_snapshots(ListSnapshotsRequest {
            vm_id: vm_id.unwrap_or_default().to_string(),
            label_selector: std::collections::HashMap::new(),
        }).await?;
        let snapshots = resp.into_inner().snapshots;
        Ok(snapshots.into_iter().map(|snap| {
            let meta = snap.meta.unwrap_or_default();
            let spec = snap.spec.unwrap_or_default();
            let status = snap.status.unwrap_or_default();
            SnapshotInfo {
                id: meta.id,
                name: meta.name,
                vm_id: spec.vm_id,
                include_memory: spec.include_memory,
                include_disk: spec.include_disk,
                description: spec.description,
                complete: status.complete,
                disk_snapshot_path: status.disk_snapshot_path,
                memory_snapshot_path: status.memory_snapshot_path,
                digest: status.digest,
                size_bytes: status.size_bytes,
                encrypted: status.encrypted,
                created_at: meta.created_at,
                labels: meta.labels,
            }
        }).collect())
    }

    /// List all networks from daemon.
    async fn list_networks(&self) -> Result<Vec<NetworkInfo>, anyhow::Error> {
        let mut client = self.connect().await?;
        let resp = client.list_networks(ListNetworksRequest {
            label_selector: std::collections::HashMap::new(),
        }).await?;
        let networks = resp.into_inner().networks;
        Ok(networks.into_iter().map(|net| {
            let meta = net.meta.unwrap_or_default();
            let spec = net.spec.unwrap_or_default();
            let status = net.status.unwrap_or_default();
            NetworkInfo {
                id: meta.id,
                name: meta.name,
                mode: network_mode_to_string(spec.mode),
                cidr: spec.cidr,
                gateway: spec.gateway,
                dns: spec.dns,
                dhcp_enabled: spec.dhcp_enabled,
                mtu: spec.mtu,
                active: status.active,
                bridge_interface: status.bridge_interface,
                connected_vms: status.connected_vms,
                created_at: meta.created_at,
                labels: meta.labels,
            }
        }).collect())
    }

    /// Get daemon status.
    async fn get_daemon_status(&self) -> Result<DaemonStatus, anyhow::Error> {
        let mut client = self.connect().await?;
        let resp = client.get_daemon_status(GetDaemonStatusRequest {}).await?;
        let s = resp.into_inner();
        Ok(DaemonStatus {
            running_vms: s.running_vms,
            total_vms: s.total_vms,
            memory_used_bytes: s.memory_used_bytes,
            disk_used_bytes: s.disk_used_bytes,
            store_path: s.store_path,
            qemu_available: s.qemu_available,
            qemu_version: s.qemu_version,
            hvf_available: s.hvf_available,
        })
    }

    /// Get attestation report for a VM.
    async fn get_attestation(&self, vm_id: &str) -> Result<serde_json::Value, anyhow::Error> {
        let mut client = self.connect().await?;
        let resp = client.get_attestation(GetAttestationRequest { vm_id: vm_id.to_string() }).await?;
        let report = resp.into_inner().report;
        match report {
            Some(r) => Ok(serde_json::json!({
                "id": r.id,
                "vm_id": r.vm_id,
                "digest": r.digest,
                "signature": hex::encode(&r.signature),
                "created_at": r.created_at,
                "attestation_type": r.attestation_type,
                "host_provenance": r.host_provenance.map(|hp| serde_json::json!({
                    "qemu_version": hp.qemu_version,
                    "qemu_args": hp.qemu_args,
                    "base_image_hash": hp.base_image_hash,
                    "volume_hashes": hp.volume_hashes,
                    "macos_version": hp.macos_version,
                    "cpu_model": hp.cpu_model,
                    "hvf_enabled": hp.hvf_enabled,
                    "hostname": hp.hostname,
                    "timestamp": hp.timestamp,
                })),
            })),
            None => Ok(serde_json::json!({"error": "no attestation report"})),
        }
    }
}

// Helper functions for enum conversion
fn vm_state_to_string(state: i32) -> String {
    match state {
        1 => "pending".to_string(),
        2 => "running".to_string(),
        3 => "stopped".to_string(),
        4 => "paused".to_string(),
        5 => "error".to_string(),
        _ => "unknown".to_string(),
    }
}

fn volume_kind_to_string(kind: i32) -> String {
    match kind {
        1 => "disk".to_string(),
        2 => "weights".to_string(),
        _ => "unknown".to_string(),
    }
}

fn network_mode_to_string(mode: i32) -> String {
    match mode {
        1 => "user".to_string(),
        2 => "vmnet_shared".to_string(),
        3 => "vmnet_bridged".to_string(),
        _ => "unknown".to_string(),
    }
}

// ============================================================================
// Inventory Info Types (JSON-serializable)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VmInfo {
    id: String,
    name: String,
    arch: String,
    machine: String,
    cpu_cores: i32,
    memory_mb: i64,
    state: String,
    vnc_display: String,
    uptime_seconds: i64,
    volume_ids: Vec<String>,
    network_ids: Vec<String>,
    created_at: i64,
    labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VolumeInfo {
    id: String,
    name: String,
    kind: String,
    format: String,
    size_bytes: i64,
    actual_size: i64,
    local_path: String,
    digest: String,
    ready: bool,
    verified: bool,
    source: String,
    created_at: i64,
    labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotInfo {
    id: String,
    name: String,
    vm_id: String,
    include_memory: bool,
    include_disk: bool,
    description: String,
    complete: bool,
    disk_snapshot_path: String,
    memory_snapshot_path: String,
    digest: String,
    size_bytes: i64,
    encrypted: bool,
    created_at: i64,
    labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkInfo {
    id: String,
    name: String,
    mode: String,
    cidr: String,
    gateway: String,
    dns: String,
    dhcp_enabled: bool,
    mtu: i32,
    active: bool,
    bridge_interface: String,
    connected_vms: i32,
    created_at: i64,
    labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DaemonStatus {
    running_vms: i32,
    total_vms: i32,
    memory_used_bytes: i64,
    disk_used_bytes: i64,
    store_path: String,
    qemu_available: bool,
    qemu_version: String,
    hvf_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Project {
    id: String,
    name: String,
    created_at: i64,
    prompts: Vec<Prompt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Prompt {
    id: String,
    title: String,
    body: String,
    created_at: i64,
    llm_provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateProjectRequest {
    name: String,
}

// ============================================================================
// Appliance (VM Template) MVP
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApplianceTemplate {
    id: String,
    title: String,
    description: String,
    arch: String,
    machine: String,
    cpu_cores: i32,
    memory_mb: i64,
    compatibility_mode: bool,
    tags: Vec<String>,
    /// Optional container/image reference (e.g. quay.io/keycloak/keycloak:26)
    #[serde(default)]
    image: Option<String>,
    /// Environment variables for the appliance runtime
    #[serde(default)]
    env: HashMap<String, String>,
    /// Exposed ports
    #[serde(default)]
    ports: Vec<AppliancePort>,
    /// Boot plan steps (ordered)
    #[serde(default)]
    boot_plan: Vec<BootStep>,
    /// Network configuration hints
    #[serde(default)]
    networks: Vec<NetworkDef>,
    /// Storage volumes
    #[serde(default)]
    volumes: Vec<VolumeDef>,
    /// Software tooling installed in the image
    #[serde(default)]
    tools: Vec<ToolDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppliancePort {
    container_port: u16,
    #[serde(default)]
    host_port: Option<u16>,
    #[serde(default = "default_tcp")]
    protocol: String,
    #[serde(default)]
    description: String,
}

fn default_tcp() -> String { "tcp".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BootStep {
    order: u32,
    action: String,
    description: String,
    #[serde(default)]
    args: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkDef {
    id: String,
    mode: String,
    #[serde(default)]
    cidr: Option<String>,
    #[serde(default)]
    gateway: Option<String>,
    #[serde(default)]
    dhcp: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VolumeDef {
    id: String,
    size_mb: u64,
    mount_path: String,
    #[serde(default = "default_disk_kind")]
    kind: String,
}

fn default_disk_kind() -> String { "disk".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolDef {
    name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApplianceInstance {
    id: String,
    name: String,
    template_id: String,
    created_at: i64,
    vm_id: Option<String>,
    status: String,
    /// IDs of networks created for this appliance
    #[serde(default)]
    network_ids: Vec<String>,
    /// IDs of volumes created for this appliance
    #[serde(default)]
    volume_ids: Vec<String>,
    /// Console ID (if created)
    #[serde(default)]
    console_id: Option<String>,
    /// Snapshot IDs associated with this appliance
    #[serde(default)]
    snapshot_ids: Vec<String>,
    /// Last updated timestamp
    #[serde(default)]
    updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApplianceCatalogSpec {
    /// Mirrors `ApplianceInstance` fields we care about persisting.
    template_id: String,
    vm_id: Option<String>,
    network_ids: Vec<String>,
    volume_ids: Vec<String>,
    console_id: Option<String>,
    snapshot_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApplianceCatalogStatus {
    status: String,
    error: Option<String>,
}

async fn load_appliance_catalog_into_memory(state: Arc<WebServerState>) -> anyhow::Result<()> {
    // Use blocking DB access in a spawn_blocking to avoid holding up the reactor.
    let db = state.db.clone();
    let rows = tokio::task::spawn_blocking(move || {
        db.list::<ApplianceCatalogSpec, ApplianceCatalogStatus>("appliance_catalog")
    })
    .await??;

    let mut appliances = state.appliances.write().await;
    for row in rows {
        let instance = ApplianceInstance {
            id: row.id,
            name: row.name,
            template_id: row.spec.template_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
            status: row.status.status,
            vm_id: row.spec.vm_id,
            network_ids: row.spec.network_ids,
            volume_ids: row.spec.volume_ids,
            console_id: row.spec.console_id,
            snapshot_ids: row.spec.snapshot_ids,
        };

        appliances.insert(instance.id.clone(), instance);
    }
    Ok(())
}

async fn persist_catalog_instance(state: &WebServerState, instance: &ApplianceInstance) -> anyhow::Result<()> {
    let db = state.db.clone();
    let id = instance.id.clone();
    let name = instance.name.clone();

    let spec = ApplianceCatalogSpec {
        template_id: instance.template_id.clone(),
        vm_id: instance.vm_id.clone(),
        network_ids: instance.network_ids.clone(),
        volume_ids: instance.volume_ids.clone(),
        console_id: instance.console_id.clone(),
        snapshot_ids: instance.snapshot_ids.clone(),
    };
    let status = ApplianceCatalogStatus {
        status: instance.status.clone(),
        error: None,
    };

    tokio::task::spawn_blocking(move || {
        let mut labels = std::collections::HashMap::new();
        labels.insert("kind".to_string(), "appliance".to_string());

        // Upsert: insert if missing, otherwise update.
        match db.exists("appliance_catalog", &id) {
            Ok(true) => db.update("appliance_catalog", &id, Some(&spec), Some(&status)),
            Ok(false) => db.insert("appliance_catalog", &id, &name, &spec, &status, &labels),
            Err(e) => Err(e),
        }
    })
    .await??;

    Ok(())
}

/// Detailed appliance view with resolved resources
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApplianceDetail {
    instance: ApplianceInstance,
    template: Option<ApplianceTemplate>,
    vm: Option<VmInfo>,
    networks: Vec<NetworkInfo>,
    volumes: Vec<VolumeInfo>,
    snapshots: Vec<SnapshotInfo>,
    terraform_hcl: String,
    /// Serialized export bundle (JSON)
    export_bundle: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateApplianceRequest {
    name: String,
    template_id: String,
    /// Whether to automatically start the VM after creation. Defaults to true.
    #[serde(default)]
    auto_start: Option<bool>,
}

/// Request to import an appliance from an export bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImportApplianceRequest {
    /// The export bundle (from ApplianceDetail.export_bundle or /api/appliances/:id/export)
    bundle: serde_json::Value,
    /// New name for the imported appliance (optional, defaults to bundle name + "-imported")
    #[serde(default)]
    new_name: Option<String>,
}

/// Request to archive an appliance (backup to a persistent store)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArchiveApplianceRequest {
    /// Archive format: "tar.gz", "zip", "json"
    #[serde(default = "default_archive_format")]
    format: String,
    /// Include memory snapshots in archive
    #[serde(default)]
    include_memory: bool,
    /// Include all historical snapshots
    #[serde(default)]
    include_all_snapshots: bool,
}

fn default_archive_format() -> String {
    "json".to_string()
}

// ============================================================================
// AI / LangChain-style prompt bridge
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AiDefineRequest {
    /// Natural-language prompt describing the desired appliance/tool/network.
    prompt: String,
    /// Optional context (e.g. existing appliance id to extend).
    #[serde(default)]
    context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AiDefineResponse {
    /// Interpreted intent.
    intent: String,
    /// Generated appliance template (if applicable).
    appliance_template: Option<ApplianceTemplate>,
    /// Generated network definitions.
    networks: Vec<NetworkDef>,
    /// Generated volume definitions.
    volumes: Vec<VolumeDef>,
    /// Generated tool definitions.
    tools: Vec<ToolDef>,
    /// Terraform HCL snippet for the above.
    terraform_hcl: String,
    /// Notes / reasoning.
    notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreatePromptRequest {
    title: String,
    body: String,
    llm_provider: Option<String>,
}

// ============================================================================
// Terraform-Addressable Virtual Filesystem Model
// ============================================================================
// Each appliance may reference multiple filesystem types:
// - fs.local: Host-bound filesystem
// - fs.snapshot: Immutable point-in-time snapshot
// - fs.ephemeral: RAM / temp filesystem
// - fs.network: NFS / iSCSI / object storage
// - fs.physical: Bound to a physical device
// - fs.geobound: Tied to a geographic location / jurisdiction
// ============================================================================

/// Filesystem type enumeration (Terraform-compatible)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FilesystemType {
    /// Host-bound local filesystem
    Local,
    /// Immutable point-in-time snapshot
    Snapshot,
    /// RAM / temporary filesystem (not persisted)
    Ephemeral,
    /// Network-attached storage (NFS, iSCSI, S3, etc.)
    Network,
    /// Bound to a physical device (USB, NVMe passthrough)
    Physical,
    /// Geographically bound (jurisdiction-restricted)
    Geobound,
}

impl Default for FilesystemType {
    fn default() -> Self {
        FilesystemType::Local
    }
}

/// Mutability of a filesystem
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FilesystemMutability {
    /// Read-write
    ReadWrite,
    /// Read-only
    ReadOnly,
    /// Copy-on-write (immutable base with overlay)
    CopyOnWrite,
}

impl Default for FilesystemMutability {
    fn default() -> Self {
        FilesystemMutability::ReadWrite
    }
}

/// Geographic bounds for geobound filesystems
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeographicBounds {
    /// ISO 3166-1 alpha-2 country codes allowed
    pub allowed_countries: Vec<String>,
    /// ISO 3166-2 region codes allowed
    pub allowed_regions: Vec<String>,
    /// Data residency requirement description
    pub residency_policy: String,
    /// Compliance framework (e.g., "GDPR", "HIPAA", "FedRAMP")
    pub compliance_framework: Option<String>,
}

/// Lifecycle rules for a filesystem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemLifecycle {
    /// Auto-delete after this many seconds (0 = never)
    pub ttl_seconds: u64,
    /// Maximum number of snapshots to retain
    pub max_snapshots: u32,
    /// Whether to auto-snapshot on detach
    pub snapshot_on_detach: bool,
    /// Whether to auto-archive when idle
    pub archive_when_idle: bool,
    /// Idle threshold in seconds before archive
    pub idle_threshold_seconds: u64,
}

impl Default for FilesystemLifecycle {
    fn default() -> Self {
        FilesystemLifecycle {
            ttl_seconds: 0,
            max_snapshots: 10,
            snapshot_on_detach: false,
            archive_when_idle: false,
            idle_threshold_seconds: 3600,
        }
    }
}

/// Provenance metadata for a filesystem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemProvenance {
    /// SHA256 digest of the filesystem content
    pub digest: String,
    /// Signature over the digest
    pub signature: Option<String>,
    /// Public key used for signing
    pub signer_public_key: Option<String>,
    /// When the provenance was computed
    pub computed_at: i64,
    /// Source of the filesystem content
    pub source_uri: Option<String>,
    /// Parent filesystem ID (for snapshots/forks)
    pub parent_id: Option<String>,
    /// Chain of custody attestations
    pub attestations: Vec<String>,
}

/// A Terraform-addressable virtual filesystem resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filesystem {
    /// Unique filesystem ID
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Filesystem type
    #[serde(rename = "type")]
    pub fs_type: FilesystemType,
    /// Backing store URI (e.g., file://, nfs://, s3://)
    pub backing_store: String,
    /// Size in bytes
    pub size_bytes: i64,
    /// Actual used bytes
    pub used_bytes: i64,
    /// Mutability
    pub mutability: FilesystemMutability,
    /// Geographic bounds (for geobound type)
    pub geographic_bounds: Option<GeographicBounds>,
    /// Lifecycle rules
    pub lifecycle: FilesystemLifecycle,
    /// Provenance metadata
    pub provenance: Option<FilesystemProvenance>,
    /// Appliance IDs this filesystem is attached to
    pub attached_to: Vec<String>,
    /// Mount point within appliances
    pub mount_path: String,
    /// Format (ext4, xfs, qcow2, raw, etc.)
    pub format: String,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
    /// Labels for filtering
    pub labels: HashMap<String, String>,
}

/// Request to create a new filesystem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFilesystemRequest {
    pub name: String,
    #[serde(rename = "type", default)]
    pub fs_type: FilesystemType,
    #[serde(default)]
    pub size_bytes: i64,
    #[serde(default)]
    pub backing_store: Option<String>,
    #[serde(default)]
    pub mutability: FilesystemMutability,
    #[serde(default)]
    pub mount_path: String,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default)]
    pub geographic_bounds: Option<GeographicBounds>,
    #[serde(default)]
    pub lifecycle: Option<FilesystemLifecycle>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

fn default_format() -> String {
    "qcow2".to_string()
}

/// Request to attach a filesystem to an appliance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachFilesystemRequest {
    pub appliance_id: String,
    #[serde(default)]
    pub mount_path: Option<String>,
    #[serde(default)]
    pub read_only: bool,
}

/// Request to detach a filesystem from an appliance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetachFilesystemRequest {
    pub appliance_id: String,
    #[serde(default)]
    pub create_snapshot: bool,
}

/// Request to snapshot a filesystem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFilesystemRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

// ============================================================================
// Resource Graph Model
// ============================================================================
// The resource graph represents all appliance-associated resources:
// - Compute: VM definitions
// - Filesystems: Root, data, ephemeral overlays
// - Devices: NICs, USB, PCI passthrough
// - Networks: Virtual and physical segments
// - Provenance: Snapshots, attestations
// ============================================================================

/// A node in the resource graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub name: String,
    pub data: serde_json::Value,
    #[serde(default)]
    pub position: Option<NodePosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

/// An edge in the resource graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub edge_type: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

/// The complete resource graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceGraph {
    pub nodes: Vec<ResourceNode>,
    pub edges: Vec<ResourceEdge>,
    pub version: String,
    pub computed_at: i64,
}

/// Plan result for graph changes (Terraform-style)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphPlanResult {
    pub adds: Vec<PlanChange>,
    pub updates: Vec<PlanChange>,
    pub deletes: Vec<PlanChange>,
    pub warnings: Vec<String>,
    pub valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanChange {
    pub resource_type: String,
    pub resource_id: String,
    pub name: String,
    #[serde(default)]
    pub changes: Vec<String>,
}

/// Request to plan graph changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanGraphRequest {
    pub draft: ResourceGraph,
}

/// Request to apply graph changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyGraphRequest {
    pub draft: ResourceGraph,
    #[serde(default)]
    pub dry_run: bool,
}

/// Request to validate a graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateGraphRequest {
    pub graph: ResourceGraph,
}

/// UI manifest for provenance and versioning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiManifest {
    pub schema_version: String,
    pub ui_version: String,
    pub git_commit: String,
    pub git_branch: String,
    pub build_timestamp: String,
    pub total_size_bytes: u64,
    pub asset_count: usize,
    pub api_schema_version: String,
    pub declared_resource_kinds: Vec<String>,
    pub mount_point: String,
    #[serde(default)]
    pub assets: Vec<UiManifestAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiManifestAsset {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

pub async fn serve(addr: SocketAddr, cfg: WebServerConfig) -> anyhow::Result<()> {
    let server = WebServer::new(cfg);
    server.serve(addr).await
}

static UI_DIST_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../ui/apps/console/dist");

impl WebServer {
    /// Create a new web server
    pub fn new(cfg: WebServerConfig) -> Self {
        let auth = match &cfg.auth {
            WebUiAuth::Token(_) => None,
            WebUiAuth::Jwt(_) => None,
            WebUiAuth::DevRandom => {
                let token = hex::encode(rand::random::<[u8; 16]>());
                eprintln!("INFRASIM_WEB_AUTH_TOKEN (dev): {}", token);
                Some(token)
            }
            WebUiAuth::None => None,
        };

        let db = Database::open(infrasim_common::default_db_path())
            .expect("failed to open infrasim state.db");

        // Best-effort schema init for local auth tables.
        init_auth_schema(&db);

        // MDM config manager
        let mdm_config = crate::mdm::MdmConfig {
            org_name: std::env::var("INFRASIM_MDM_ORG").unwrap_or_else(|_| "InfraSim".to_string()),
            domain: std::env::var("INFRASIM_MDM_DOMAIN").unwrap_or_else(|_| "infrasim.local".to_string()),
            cert_store_path: std::path::PathBuf::from(
                std::env::var("INFRASIM_MDM_CERT_PATH").unwrap_or_else(|_| {
                    dirs::home_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                        .join(".infrasim/mdm")
                        .to_string_lossy()
                        .to_string()
                })
            ),
        };
        let mdm = crate::mdm::MdmManager::new(mdm_config);

        Self {
            state: Arc::new(WebServerState {
                vnc_targets: RwLock::new(HashMap::new()),
                tokens: RwLock::new(HashMap::new()),
                static_files: StaticFiles::new(),
                ui_static: UiStatic::from_env(),
                daemon: DaemonProxy::new(cfg.daemon_addr.clone()),
                cfg,
                projects: RwLock::new(HashMap::new()),
                appliances: RwLock::new(HashMap::new()),
                filesystems: RwLock::new(HashMap::new()),
                db,
                control: LocalControl::from_env(),
                mdm,
            }),
        }
        .with_dev_token(auth)
    }

    fn with_dev_token(self, token: Option<String>) -> Self {
        if let Some(token) = token {
            let state = self.state.clone();
            tokio::spawn(async move {
                let mut tokens = state.tokens.write().await;
                tokens.insert("dev".to_string(), token);
            });
        }
        // Load persisted appliance catalog into memory.
        let state = self.state.clone();
        tokio::spawn(async move {
            if let Err(e) = load_appliance_catalog_into_memory(state.clone()).await {
                warn!("failed to load appliance catalog: {}", e);
            }
        });

        self
    }

    /// Register a VNC target for a VM
    pub async fn register_vnc(&self, vm_id: &str, host: &str, port: u16) {
        let mut targets = self.state.vnc_targets.write().await;
        targets.insert(vm_id.to_string(), (host.to_string(), port));
        debug!("Registered VNC target for {}: {}:{}", vm_id, host, port);
    }

    /// Unregister a VNC target
    pub async fn unregister_vnc(&self, vm_id: &str) {
        let mut targets = self.state.vnc_targets.write().await;
        targets.remove(vm_id);
    }

    /// Get a VNC target
    pub async fn get_vnc_target(&self, vm_id: &str) -> Option<(String, u16)> {
        let targets = self.state.vnc_targets.read().await;
        targets.get(vm_id).cloned()
    }

    /// Create router
    pub fn router(&self) -> Router {
        let state = self.state.clone();
        let meshnet_db = state.db.clone(); // Clone db for meshnet before state is moved
        let auth_layer = middleware::from_fn(move |req, next| {
            let state = state.clone();
            async move { auth_middleware_inner(state, req, next).await }
        });

        // Protected routes (require main app auth)
        let protected_routes = Router::new()
            // Filesystem Resource API (Terraform-addressable)
            .route("/api/filesystems", get(list_filesystems_handler).post(create_filesystem_handler))
            .route("/api/filesystems/:fs_id", get(get_filesystem_handler).delete(delete_filesystem_handler))
            .route(
                "/api/filesystems/:fs_id/snapshot",
                post(create_filesystem_snapshot_handler),
            )
            .route("/api/filesystems/:fs_id/attach", post(attach_filesystem_handler))
            .route("/api/filesystems/:fs_id/detach", post(detach_filesystem_handler))

            // Resource Graph API
            .route("/api/graph", get(get_resource_graph_handler))
            .route("/api/graph/plan", post(plan_graph_changes_handler))
            .route("/api/graph/apply", post(apply_graph_changes_handler))
            .route("/api/graph/validate", post(validate_graph_handler))

            // Local admin controls (requires normal auth; requires control enabled)
            .route("/api/admin/status", get(admin_status_handler))
            .route("/api/admin/restart-web", post(admin_restart_web_handler))
            .route("/api/admin/restart-daemon", post(admin_restart_daemon_handler))
            .route("/api/admin/stop-daemon", post(admin_stop_daemon_handler))

            // Inventory: Images (qcow2 volumes/snapshots)
            .route("/api/images", get(list_images_handler))
            .route("/api/images/:image_id", get(get_image_handler))

            // Inventory: Volumes
            .route("/api/volumes", get(list_volumes_handler))
            .route("/api/volumes/:volume_id", get(get_volume_handler))

            // Inventory: Snapshots
            .route("/api/snapshots", get(list_snapshots_handler))
            .route("/api/snapshots/:snapshot_id", get(get_snapshot_handler))

            // Inventory: Networks
            .route("/api/networks", get(list_networks_handler))
            .route("/api/networks/:network_id", get(get_network_handler))

            // Project + prompt workspace (local, persisted in-memory for MVP)
            .route("/api/projects", get(list_projects_handler).post(create_project_handler))
            .route(
                "/api/projects/:project_id/prompts",
                get(list_prompts_handler).post(create_prompt_handler),
            )

            // Terraform helpers
            .route("/api/terraform/generate", post(terraform_generate_handler))
            .route("/api/terraform/audit", post(terraform_audit_handler))

            // Provenance helpers
            .route("/api/provenance/attest", post(attest_project_handler))
            .route("/api/provenance/evidence", post(provenance_evidence_handler))

            // Appliance (VM template) MVP
            .route("/api/appliances/templates", get(list_appliance_templates_handler))
            .route("/api/appliances", get(list_appliances_handler).post(create_appliance_handler))
            .route("/api/appliances/seed", post(seed_appliances_handler))
            .route("/api/appliances/import", post(import_appliance_handler))
            .route("/api/appliances/:appliance_id", get(get_appliance_detail_handler))
            .route("/api/appliances/:appliance_id/terraform", get(appliance_terraform_handler))
            .route("/api/appliances/:appliance_id/boot", post(appliance_boot_handler))
            .route("/api/appliances/:appliance_id/stop", post(appliance_stop_handler))
            .route("/api/appliances/:appliance_id/snapshot", post(appliance_snapshot_handler))
            .route("/api/appliances/:appliance_id/export", get(export_appliance_handler))
            .route("/api/appliances/:appliance_id/archive", post(archive_appliance_handler))
            .route("/api/appliances/:appliance_id/attestation", get(appliance_attestation_handler))

            // AI prompt bridge (LangChain-style)
            .route("/api/ai/define", post(ai_define_handler))

            // Auth (local TOTP / Google Authenticator compatible)
            .route("/api/auth/status", get(auth_status_handler))
            .route("/api/auth/identities", post(auth_create_identity_handler))
            .route("/api/auth/totp/begin", post(auth_totp_begin_handler))
            .route("/api/auth/totp/confirm", post(auth_totp_confirm_handler))
            .route("/api/auth/totp/login", post(auth_totp_login_handler))
            .route("/api/auth/whoami", get(auth_whoami_handler))

            // MDM / mobileconfig endpoints
            .route("/api/mdm/status", get(mdm_status_handler))
            .route("/api/mdm/root-ca", get(mdm_root_ca_handler))
            .route("/api/mdm/bridges", get(mdm_list_bridges_handler).post(mdm_add_bridge_handler))
            .route("/api/mdm/vpns", get(mdm_list_vpns_handler).post(mdm_add_vpn_handler))
            .route("/api/mdm/profile", post(mdm_generate_profile_handler))
            .route("/api/mdm/profile/:name", get(mdm_download_profile_handler))
            // Webhook for device config delivery (signed mobileconfig)
            .route("/webhook/config/:token", get(webhook_config_handler))

            // Docker/Container image browser and appliance builder
            .route("/api/docker/status", get(docker_status_handler))
            .route("/api/docker/images", get(docker_list_images_handler))
            .route("/api/docker/images/:image_ref/inspect", get(docker_inspect_image_handler))
            .route("/api/docker/images/:image_ref/history", get(docker_image_history_handler))
            .route("/api/docker/images/pull", post(docker_pull_image_handler))
            .route("/api/docker/search", get(docker_search_handler))
            .route("/api/docker/build", post(docker_build_appliance_handler))

            // RBAC / Policy export
            .route("/api/rbac/roles", get(rbac_list_roles_handler))
            .route("/api/rbac/policies", get(rbac_list_policies_handler))
            .route("/api/rbac/terraform", get(rbac_terraform_export_handler))

            .route("/api/vms", get(list_vms_api_handler))
            .route("/api/vms/:vm_id", get(get_vm_handler))
            .route("/api/vms/:vm_id/vnc", get(vnc_info_handler))
            // VNC WebSocket proxy
            .route("/websockify/:vm_id", get(websocket_handler))
            .layer(auth_layer)
            .with_state(self.state.clone());

        // Public routes (no auth required)
        Router::new()
            // New Console UI (SPA) served at the root (public)
            .route("/", get(ui_root_index_handler))
            .route("/favicon.ico", get(ui_favicon_handler))
            .route("/assets/*path", get(ui_root_static_handler))
            // Provide backward-compatible /ui paths (public)
            .route("/ui", get(ui_root_index_handler))
            .route("/ui/", get(ui_root_index_handler))
            .route("/ui/assets/*path", get(ui_ui_assets_handler))
            // API endpoints (public health checks)
            .route("/api/health", get(health_handler))
            .route("/api/daemon", get(daemon_health_handler))
            .route("/api/daemon/status", get(daemon_status_handler))

            // UI Manifest endpoint (public, for provenance)
            .route("/api/ui/manifest", get(ui_manifest_handler))

            // Legacy noVNC/static console endpoints (kept for now, but no longer the root UI)
            .route("/vnc.html", get(vnc_html_handler))
            .route("/vnc_lite.html", get(vnc_lite_handler))
            .route("/app/*path", get(static_handler))
            .route("/core/*path", get(static_handler))
            .route("/vendor/*path", get(static_handler))

            // Meshnet Console MVP (Identity, Mesh, Appliances)
            // Has its own WebAuthn auth - NOT protected by main app auth
            .nest_service("/api/meshnet", crate::meshnet::routes::meshnet_router(meshnet_db))

            // Build Pipeline Analysis (dependency graphs, timing probes)
            .nest_service("/api/analysis", crate::build_analysis::analysis_routes(
                std::sync::Arc::new(crate::build_analysis::AnalysisCache::default())
            ))

            // Snapshot Browser with provenance and memory pinning
            .nest_service("/api/snapshot-browser", crate::snapshot_browser::snapshot_browser_routes(
                std::sync::Arc::new(crate::snapshot_browser::SnapshotBrowserState::default())
            ))

            // Static pipeline analyzer HTML
            .route("/pipeline-analyzer", get(pipeline_analyzer_handler))

            // Merge protected routes
            .merge(protected_routes)

            // Fallback
            .fallback(not_found_handler)
            .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any))
            .layer(TraceLayer::new_for_http())
            .with_state(self.state.clone())
    }

    /// Start the web server
    pub async fn serve(self, addr: SocketAddr) -> anyhow::Result<()> {
        info!("Web console starting on http://{}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, self.router()).await?;

        Ok(())
    }
}

impl Default for WebServer {
    fn default() -> Self {
        Self::new(WebServerConfig {
            daemon_addr: "http://127.0.0.1:50051".to_string(),
            auth: WebUiAuth::DevRandom,
        })
    }
}

// ============================================================================
// Handlers
// ============================================================================

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "infrasim-web"
    }))
}

async fn daemon_health_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    match state.daemon.health().await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": format!("{}", e)})),
        )
            .into_response(),
    }
}

async fn daemon_status_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    match state.daemon.get_daemon_status().await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": format!("{}", e)})),
        )
            .into_response(),
    }
}

// ============================================================================
// Root UI handlers (Vite build)
// ============================================================================

async fn ui_root_index_handler() -> impl IntoResponse {
    match tokio::fs::read_to_string(format!("{}/index.html", UI_DIST_DIR)).await {
        Ok(html) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
            html,
        )
            .into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            "UI build not found. Run: (cd infrasim/ui && pnpm -r build)",
        )
            .into_response(),
    }
}

async fn ui_favicon_handler() -> impl IntoResponse {
    // Prefer dist/favicon.ico if it exists; otherwise return empty 204.
    let full_path = format!("{}/favicon.ico", UI_DIST_DIR);
    match tokio::fs::read(&full_path).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "image/x-icon")],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn ui_root_static_handler(Path(path): Path<String>) -> impl IntoResponse {
    // Serve from dist/assets/*
    let full_path = format!("{}/assets/{}", UI_DIST_DIR, path);
    match tokio::fs::read(&full_path).await {
        Ok(bytes) => {
            let mime = mime_guess::from_path(&full_path).first_or_octet_stream();
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, mime.as_ref())],
                bytes,
            )
                .into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

async fn ui_ui_assets_handler(Path(path): Path<String>) -> impl IntoResponse {
    // Compatibility: /ui/assets/* should map to the same dist/assets/* files.
    ui_root_static_handler(Path(path)).await
}

// ============================================================================
// Auth handlers
// ============================================================================

fn normalize_display_name(s: &str) -> String {
    s.trim().to_lowercase()
}

fn default_issuer() -> String {
    std::env::var("INFRASIM_AUTH_ISSUER").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| "InfraSim".to_string())
}

fn totp_for_secret_b32(issuer: &str, label: &str, secret_b32: &str) -> anyhow::Result<TOTP> {
    // totp-rs expects base32 secret; we store NOPAD base32.
    let secret = Secret::Encoded(secret_b32.to_string());
    Ok(TOTP::new(
        TotpAlgorithm::SHA1,
        6,
        1,
        30,
        secret.to_bytes()?,
        Some(issuer.to_string()),
        label.to_string(),
    )?)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut v: u8 = 0;
    for (x, y) in a.as_bytes().iter().zip(b.as_bytes().iter()) {
        v |= x ^ y;
    }
    v == 0
}

fn verify_totp_code(totp: &TOTP, code: &str) -> bool {
    // Accept current +/- one time-step for clock skew.
    let code = code.trim();
    if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let now = now_epoch_secs();
    // totp-rs can verify with timestamp using check_current; but we need window.
    for offset in [-30i64, 0i64, 30i64] {
        let gen = totp.generate((now + offset) as u64);
        if constant_time_eq(&gen, code) {
            return true;
        }
    }
    false
}

async fn auth_create_identity_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<CreateIdentityRequest>,
) -> impl IntoResponse {
    let display_name = normalize_display_name(&req.display_name);
    if display_name.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"display_name required"}))).into_response();
    }
    let role = req.role.unwrap_or_else(|| "admin".to_string());
    let id = Uuid::new_v4().to_string();
    let created_at = now_epoch_secs();

    let conn = state.db.connection();
    let conn = conn.lock();
    let res = conn.execute(
        "INSERT INTO auth_identities (id, display_name, role, totp_secret_b32, totp_enabled, created_at) VALUES (?1, ?2, ?3, NULL, 0, ?4)",
        rusqlite::params![id, display_name, role, created_at],
    );
    match res {
        Ok(_) => {
            let identity = AuthIdentity { id, display_name, role, totp_enabled: false, created_at };
            (StatusCode::OK, Json(CreateIdentityResponse { identity })).into_response()
        }
        Err(e) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": format!("{}", e)})),
        )
            .into_response(),
    }
}

async fn auth_totp_begin_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<BeginTotpEnrollRequest>,
) -> impl IntoResponse {
    let display_name = normalize_display_name(&req.display_name);
    if display_name.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"display_name required"}))).into_response();
    }

    let issuer = default_issuer();
    let label = display_name.clone();

    // Generate 20 random bytes -> base32 nopad.
    let mut raw = [0u8; 20];
    raw.copy_from_slice(&rand::random::<[u8; 32]>()[0..20]);
    let secret_b32 = BASE32_NOPAD.encode(&raw);

    let totp = match totp_for_secret_b32(&issuer, &label, &secret_b32) {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    let otpauth_uri = totp.get_url();
    let qr_svg = match QrCode::new(otpauth_uri.as_bytes()) {
        Ok(code) => code.render::<svg::Color>().min_dimensions(220, 220).build(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };

    let conn = state.db.connection();
    let conn = conn.lock();
    // Ensure identity exists; if not, create it on the fly.
    let existing: Option<(String, String, String, i64)> = conn
        .query_row(
            "SELECT id, display_name, role, created_at FROM auth_identities WHERE display_name = ?1",
            rusqlite::params![display_name],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .ok()
        .flatten();

    let (id, role, created_at) = match existing {
        Some((id, _dn, role, created_at)) => (id, role, created_at),
        None => {
            let id = Uuid::new_v4().to_string();
            let role = "admin".to_string();
            let created_at = now_epoch_secs();
            let _ = conn.execute(
                "INSERT INTO auth_identities (id, display_name, role, totp_secret_b32, totp_enabled, created_at) VALUES (?1, ?2, ?3, NULL, 0, ?4)",
                rusqlite::params![id, display_name, role, created_at],
            );
            (id, role, created_at)
        }
    };

    let _ = conn.execute(
        "UPDATE auth_identities SET totp_secret_b32 = ?1, totp_enabled = 0 WHERE id = ?2",
        rusqlite::params![secret_b32, id],
    );

    let identity = AuthIdentity { id, display_name: label.clone(), role, totp_enabled: false, created_at };
    (StatusCode::OK, Json(BeginTotpEnrollResponse { identity, issuer, label, secret_b32, otpauth_uri, qr_svg })).into_response()
}

async fn auth_totp_confirm_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<ConfirmTotpEnrollRequest>,
) -> impl IntoResponse {
    let display_name = normalize_display_name(&req.display_name);
    let code = req.code.trim().to_string();

    let conn = state.db.connection();
    let conn = conn.lock();
    let row: Option<(String, String, i64, Option<String>, i64)> = conn
        .query_row(
            "SELECT id, role, created_at, totp_secret_b32, totp_enabled FROM auth_identities WHERE display_name = ?1",
            rusqlite::params![display_name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .ok()
        .flatten();
    let (id, role, created_at, secret_opt, _enabled) = match row {
        Some(v) => v,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"unknown identity"}))).into_response(),
    };
    let secret_b32 = match secret_opt {
        Some(v) => v.to_string(),
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"enrollment not started"}))).into_response(),
    };

    let issuer = default_issuer();
    let totp = match totp_for_secret_b32(&issuer, &display_name, &secret_b32) {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    if !verify_totp_code(&totp, &code) {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"invalid code"}))).into_response();
    }

    let _ = conn.execute(
        "UPDATE auth_identities SET totp_enabled = 1 WHERE id = ?1",
        rusqlite::params![id],
    );
    let identity = AuthIdentity { id, display_name, role, totp_enabled: true, created_at };
    (StatusCode::OK, Json(serde_json::json!({"ok": true, "identity": identity}))).into_response()
}

async fn auth_totp_login_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<LoginTotpRequest>,
) -> impl IntoResponse {
    let display_name = normalize_display_name(&req.display_name);
    let code = req.code.trim().to_string();
    if display_name.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"display_name required"}))).into_response();
    }

    let now = now_epoch_secs();
    let conn_arc = state.db.connection();
    let conn = conn_arc.lock();

    let row: Option<(String, String, i64, Option<String>, i64)> = conn
        .query_row(
            "SELECT id, role, created_at, totp_secret_b32, totp_enabled FROM auth_identities WHERE display_name = ?1",
            rusqlite::params![display_name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .ok()
        .flatten();

    let (id, role, created_at, secret_opt, enabled) = match row {
        Some(v) => v,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"unknown identity"}))).into_response(),
    };

    // Check lockout.
    let attempt: Option<(i64, i64)> = conn
        .query_row(
            "SELECT failed_count, locked_until FROM auth_attempts WHERE identity_id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .ok()
        .flatten();
    if let Some((_failed, locked_until)) = attempt {
        if locked_until > now {
            return (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({"error":"locked" , "locked_until": locked_until}))).into_response();
        }
    }

    if enabled == 0 {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"totp not enabled"}))).into_response();
    }
    let secret_b32 = match secret_opt {
        Some(v) => v.to_string(),
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"totp not configured"}))).into_response(),
    };

    let issuer = default_issuer();
    let totp = match totp_for_secret_b32(&issuer, &display_name, &secret_b32) {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };

    if !verify_totp_code(&totp, &code) {
        // Update attempts.
        let failed = attempt.map(|(f, _)| f).unwrap_or(0) + 1;
        let mut locked_until = 0i64;
        if failed >= AUTH_MAX_FAILED_ATTEMPTS {
            locked_until = now + AUTH_LOCKOUT_SECS;
        }
        let _ = conn.execute(
            "INSERT INTO auth_attempts (identity_id, failed_count, locked_until, updated_at) VALUES (?1, ?2, ?3, ?4)\
             ON CONFLICT(identity_id) DO UPDATE SET failed_count=?2, locked_until=?3, updated_at=?4",
            rusqlite::params![id, failed, locked_until, now],
        );
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"invalid code"}))).into_response();
    }

    // Reset attempts on success.
    let _ = conn.execute(
        "INSERT INTO auth_attempts (identity_id, failed_count, locked_until, updated_at) VALUES (?1, 0, 0, ?2)\
         ON CONFLICT(identity_id) DO UPDATE SET failed_count=0, locked_until=0, updated_at=?2",
        rusqlite::params![id, now],
    );

    let token = hex::encode(rand::random::<[u8; 32]>());
    let expires_at = now + AUTH_SESSION_TTL_SECS;
    let _ = conn.execute(
        "INSERT INTO auth_sessions (token, identity_id, created_at, expires_at, last_seen_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![token, id, now, expires_at, now],
    );

    let identity = AuthIdentity { id, display_name, role, totp_enabled: true, created_at };
    (StatusCode::OK, Json(LoginResponse { token, expires_at, identity })).into_response()
}

async fn auth_whoami_handler(State(state): State<Arc<WebServerState>>, headers: axum::http::HeaderMap) -> impl IntoResponse {
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = auth_header.strip_prefix("Bearer ").unwrap_or("");
    if token.is_empty() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"missing bearer token"}))).into_response();
    }
    let now = now_epoch_secs();
    let conn = state.db.connection();
    let conn = conn.lock();
    let row: Option<(String, i64, String, String, i64, i64)> = conn
        .query_row(
            "SELECT s.identity_id, s.expires_at, i.display_name, i.role, i.created_at, i.totp_enabled \
             FROM auth_sessions s JOIN auth_identities i ON i.id = s.identity_id WHERE s.token = ?1",
            rusqlite::params![token],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()
        .ok()
        .flatten();
    let (identity_id, expires_at, display_name, role, created_at, enabled) = match row {
        Some(v) => v,
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"invalid token"}))).into_response(),
    };
    if expires_at <= now {
        let _ = conn.execute("DELETE FROM auth_sessions WHERE token = ?1", rusqlite::params![token]);
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"expired"}))).into_response();
    }
    let _ = conn.execute(
        "UPDATE auth_sessions SET last_seen_at = ?1 WHERE token = ?2",
        rusqlite::params![now, token],
    );
    let identity = AuthIdentity {
        id: identity_id,
        display_name,
        role,
        totp_enabled: enabled != 0,
        created_at,
    };
    (StatusCode::OK, Json(serde_json::json!({"identity": identity, "expires_at": expires_at}))).into_response()
}

// ============================================================================
// Auth status (for first-time setup detection)
// ============================================================================

/// Response for /api/auth/status - tells the UI whether this is first-time setup
#[derive(Debug, Clone, Serialize)]
struct AuthStatusResponse {
    /// True if no identities exist (first-time setup needed)
    needs_setup: bool,
    /// Number of registered identities
    identity_count: i64,
    /// True if any identity has TOTP enabled
    has_totp_enabled: bool,
}

async fn auth_status_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    let conn = state.db.connection();
    let conn = conn.lock();
    
    let identity_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM auth_identities", [], |r| r.get(0))
        .unwrap_or(0);
    
    let totp_enabled_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM auth_identities WHERE totp_enabled = 1", [], |r| r.get(0))
        .unwrap_or(0);
    
    Json(AuthStatusResponse {
        needs_setup: identity_count == 0,
        identity_count,
        has_totp_enabled: totp_enabled_count > 0,
    })
}

// ============================================================================
// MDM / mobileconfig handlers
// ============================================================================

use crate::mdm::{BridgeConfig, VpnConfig, VpnType, PeerEndpoint, ProfileRequest};

async fn mdm_status_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    // Initialize MDM if not already done
    if state.mdm.chain.read().await.is_none() {
        if let Err(e) = state.mdm.init().await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to init MDM: {}", e)
            }))).into_response();
        }
    }
    
    let bridges = state.mdm.list_bridges().await;
    let vpns = state.mdm.list_vpns().await;
    let has_root_ca = state.mdm.get_root_ca_pem().await.is_some();
    
    Json(serde_json::json!({
        "initialized": has_root_ca,
        "org_name": state.mdm.config.org_name,
        "domain": state.mdm.config.domain,
        "bridge_count": bridges.len(),
        "vpn_count": vpns.len(),
        "cert_store_path": state.mdm.config.cert_store_path.display().to_string(),
    })).into_response()
}

async fn mdm_root_ca_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    // Initialize MDM if not already done
    if state.mdm.chain.read().await.is_none() {
        if let Err(e) = state.mdm.init().await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to init MDM: {}", e)
            }))).into_response();
        }
    }
    
    match state.mdm.get_root_ca_pem().await {
        Some(pem) => {
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/x-pem-file")
                .header("content-disposition", "attachment; filename=\"infrasim-root-ca.crt\"")
                .body(axum::body::Body::from(pem))
                .unwrap()
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "Root CA not initialized"
        }))).into_response()
    }
}

#[derive(Debug, Deserialize)]
struct AddBridgeRequest {
    name: String,
    subnet: String,
    gateway: String,
    #[serde(default)]
    dns_servers: Vec<String>,
    #[serde(default)]
    peers: Vec<PeerEndpoint>,
}

async fn mdm_list_bridges_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    let bridges = state.mdm.list_bridges().await;
    Json(serde_json::json!({ "bridges": bridges }))
}

async fn mdm_add_bridge_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<AddBridgeRequest>,
) -> impl IntoResponse {
    let bridge = BridgeConfig {
        name: req.name,
        subnet: req.subnet,
        gateway: req.gateway,
        dns_servers: if req.dns_servers.is_empty() { 
            vec!["8.8.8.8".into(), "8.8.4.4".into()] 
        } else { 
            req.dns_servers 
        },
        peers: req.peers,
    };
    state.mdm.add_bridge(bridge.clone()).await;
    (StatusCode::CREATED, Json(serde_json::json!({ "bridge": bridge })))
}

#[derive(Debug, Deserialize)]
struct AddVpnRequest {
    display_name: String,
    server: String,
    #[serde(default = "default_vpn_type")]
    vpn_type: String,
    shared_secret: Option<String>,
    username: Option<String>,
    #[serde(default)]
    on_demand: bool,
    #[serde(default)]
    trusted_ssids: Vec<String>,
}

fn default_vpn_type() -> String { "ikev2".into() }

async fn mdm_list_vpns_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    let vpns = state.mdm.list_vpns().await;
    Json(serde_json::json!({ "vpns": vpns }))
}

async fn mdm_add_vpn_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<AddVpnRequest>,
) -> impl IntoResponse {
    let vpn_type = match req.vpn_type.to_lowercase().as_str() {
        "ikev2" => VpnType::IKEv2,
        "wireguard" => VpnType::WireGuard,
        "ipsec" => VpnType::IPSec,
        _ => VpnType::IKEv2,
    };
    let vpn = VpnConfig {
        display_name: req.display_name,
        server: req.server,
        vpn_type,
        shared_secret: req.shared_secret,
        username: req.username,
        on_demand: req.on_demand,
        trusted_ssids: req.trusted_ssids,
    };
    state.mdm.add_vpn(vpn.clone()).await;
    (StatusCode::CREATED, Json(serde_json::json!({ "vpn": vpn })))
}

#[derive(Debug, Deserialize)]
struct GenerateProfileRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

async fn mdm_generate_profile_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<GenerateProfileRequest>,
) -> impl IntoResponse {
    // Initialize MDM if not already done
    if state.mdm.chain.read().await.is_none() {
        if let Err(e) = state.mdm.init().await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to init MDM: {}", e)
            }))).into_response();
        }
    }
    
    match state.mdm.generate_profile(&req.name).await {
        Ok(xml) => {
            // Return info about the generated profile
            let (cert_path, key_path, chain_path) = state.mdm.signing_paths();
            Json(serde_json::json!({
                "name": req.name,
                "size_bytes": xml.len(),
                "unsigned_xml": String::from_utf8_lossy(&xml),
                "signing_hint": format!(
                    "To sign: openssl smime -sign -signer {} -inkey {} -certfile {} -nodetach -outform der -in profile.mobileconfig -out profile.signed.mobileconfig",
                    cert_path.display(), key_path.display(), chain_path.display()
                ),
                "download_url": format!("/api/mdm/profile/{}", req.name.to_lowercase().replace(' ', "-")),
            })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("Failed to generate profile: {}", e)
        }))).into_response()
    }
}

async fn mdm_download_profile_handler(
    State(state): State<Arc<WebServerState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    // Initialize MDM if not already done
    if state.mdm.chain.read().await.is_none() {
        if let Err(e) = state.mdm.init().await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to init MDM: {}", e)
            }))).into_response();
        }
    }
    
    match state.mdm.generate_profile(&name).await {
        Ok(xml) => {
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/x-apple-aspen-config")
                .header("content-disposition", format!("attachment; filename=\"{}.mobileconfig\"", name))
                .body(axum::body::Body::from(xml))
                .unwrap()
                .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("Failed to generate profile: {}", e)
        }))).into_response()
    }
}

/// Webhook for config delivery - simple token-based access for devices
async fn webhook_config_handler(
    State(state): State<Arc<WebServerState>>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    // For MVP, accept any non-empty token and return the default profile
    // In production, you'd validate the token against a database
    if token.is_empty() || token.len() < 8 {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({
            "error": "Invalid config token"
        }))).into_response();
    }
    
    // Initialize MDM if not already done
    if state.mdm.chain.read().await.is_none() {
        if let Err(e) = state.mdm.init().await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to init MDM: {}", e)
            }))).into_response();
        }
    }
    
    // Generate a profile named after the token (or use a default)
    let profile_name = format!("device-{}", &token[..8]);
    match state.mdm.generate_profile(&profile_name).await {
        Ok(xml) => {
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/x-apple-aspen-config")
                .header("content-disposition", format!("attachment; filename=\"{}.mobileconfig\"", profile_name))
                .body(axum::body::Body::from(xml))
                .unwrap()
                .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("Failed to generate profile: {}", e)
        }))).into_response()
    }
}

// ============================================================================
// Docker / Container Image Browser handlers
// ============================================================================

use crate::docker::{ContainerManager, ApplianceBuildSpec, NetworkInterface, ImageOverlay, NetworkInterfaceType, OverlayType, OutputFormat, CloudInitConfig};

async fn docker_status_handler() -> impl IntoResponse {
    let manager = ContainerManager::new();
    let runtime = manager.runtime;
    
    Json(serde_json::json!({
        "available": runtime.is_some(),
        "runtime": runtime.map(|r| match r {
            crate::docker::ContainerRuntime::Docker => "docker",
            crate::docker::ContainerRuntime::Podman => "podman",
        }),
        "features": {
            "image_browser": true,
            "image_pull": true,
            "appliance_builder": true,
            "network_config": true,
            "overlay_support": true,
        }
    }))
}

async fn docker_list_images_handler() -> impl IntoResponse {
    let manager = ContainerManager::new();
    
    match manager.list_local_images().await {
        Ok(images) => Json(serde_json::json!({
            "images": images,
            "count": images.len(),
        })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": e,
            "hint": "Ensure Docker or Podman is installed and running"
        }))).into_response()
    }
}

async fn docker_inspect_image_handler(
    Path(image_ref): Path<String>,
) -> impl IntoResponse {
    let manager = ContainerManager::new();
    let image_ref = urlencoding::decode(&image_ref).unwrap_or_default().to_string();
    
    match manager.inspect_image(&image_ref).await {
        Ok(info) => Json(info).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": e,
            "image": image_ref,
        }))).into_response()
    }
}

async fn docker_image_history_handler(
    Path(image_ref): Path<String>,
) -> impl IntoResponse {
    let manager = ContainerManager::new();
    let image_ref = urlencoding::decode(&image_ref).unwrap_or_default().to_string();
    
    match manager.get_image_history(&image_ref).await {
        Ok(layers) => Json(serde_json::json!({
            "image": image_ref,
            "layers": layers,
            "layer_count": layers.len(),
        })).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": e,
            "image": image_ref,
        }))).into_response()
    }
}

#[derive(Debug, Deserialize)]
struct DockerPullRequest {
    image: String,
}

async fn docker_pull_image_handler(
    Json(req): Json<DockerPullRequest>,
) -> impl IntoResponse {
    let manager = ContainerManager::new();
    
    match manager.pull_image(&req.image).await {
        Ok(output) => Json(serde_json::json!({
            "success": true,
            "image": req.image,
            "output": output,
        })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false,
            "error": e,
            "image": req.image,
        }))).into_response()
    }
}

#[derive(Debug, Deserialize)]
struct DockerSearchQuery {
    q: String,
}

async fn docker_search_handler(
    Query(params): Query<DockerSearchQuery>,
) -> impl IntoResponse {
    let manager = ContainerManager::new();
    
    match manager.search_registry(&params.q).await {
        Ok(results) => Json(serde_json::json!({
            "query": params.q,
            "results": results,
            "count": results.len(),
        })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": e,
            "query": params.q,
        }))).into_response()
    }
}

#[derive(Debug, Deserialize)]
struct ApplianceBuildRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    base_image: String,
    #[serde(default = "default_arch")]
    arch: String,
    #[serde(default = "default_memory")]
    memory_mb: i64,
    #[serde(default = "default_cpu")]
    cpu_cores: i32,
    #[serde(default)]
    interfaces: Vec<NetworkInterface>,
    #[serde(default)]
    overlays: Vec<ImageOverlay>,
    #[serde(default)]
    output_format: Option<String>,
    #[serde(default)]
    cloud_init: Option<CloudInitConfig>,
}

fn default_arch() -> String { "aarch64".to_string() }
fn default_memory() -> i64 { 2048 }
fn default_cpu() -> i32 { 2 }

async fn docker_build_appliance_handler(
    Json(req): Json<ApplianceBuildRequest>,
) -> impl IntoResponse {
    // Generate default interfaces if none provided
    let interfaces = if req.interfaces.is_empty() {
        ContainerManager::default_interfaces()
    } else {
        req.interfaces
    };

    let output_format = req.output_format.as_deref().map(|f| match f.to_lowercase().as_str() {
        "raw" => OutputFormat::Raw,
        "container" => OutputFormat::Container,
        _ => OutputFormat::Qcow2,
    }).unwrap_or(OutputFormat::Qcow2);

    let spec = ApplianceBuildSpec {
        name: req.name.clone(),
        description: req.description,
        base_image: req.base_image.clone(),
        arch: req.arch,
        memory_mb: req.memory_mb,
        cpu_cores: req.cpu_cores,
        interfaces: interfaces.clone(),
        overlays: req.overlays,
        output_format,
        cloud_init: req.cloud_init,
    };

    // Generate Terraform HCL for the spec
    let terraform_hcl = generate_build_spec_terraform(&spec);
    
    // Generate network interface HCL
    let network_hcl = ContainerManager::interfaces_to_terraform(&interfaces);

    Json(serde_json::json!({
        "status": "planned",
        "spec": spec,
        "terraform_hcl": terraform_hcl,
        "network_hcl": network_hcl,
        "next_steps": [
            format!("Pull base image: docker pull {}", req.base_image),
            "Apply overlays (files, packages, commands)",
            "Generate qcow2 from container filesystem",
            "Create VM with specified network interfaces",
        ],
        "hint": "Submit to /api/appliances to create and boot the appliance"
    }))
}

fn generate_build_spec_terraform(spec: &ApplianceBuildSpec) -> String {
    let id = spec.name.to_lowercase().replace(' ', "_").replace('-', "_");
    format!(
        r#"# Appliance: {}
# Generated by InfraSim Docker Appliance Builder

resource "infrasim_appliance" "{}" {{
  name        = "{}"
  description = {}
  
  base_image = "{}"
  arch       = "{}"
  memory_mb  = {}
  cpu_cores  = {}
  
  output_format = "{:?}"

  # Network interfaces
  {}

  # Overlays (customizations)
  {}
}}
"#,
        spec.name,
        id,
        spec.name,
        spec.description.as_ref().map(|d| format!("\"{}\"", d)).unwrap_or("null".to_string()),
        spec.base_image,
        spec.arch,
        spec.memory_mb,
        spec.cpu_cores,
        spec.output_format,
        spec.interfaces.iter().enumerate().map(|(i, iface)| {
            format!(
                r#"network_interface {{
    name = "{}"
    type = "{:?}"
    {}
  }}"#,
                iface.name,
                iface.nic_type,
                iface.mac_address.as_ref().map(|m| format!("mac_address = \"{}\"", m)).unwrap_or_default()
            )
        }).collect::<Vec<_>>().join("\n\n  "),
        spec.overlays.iter().enumerate().map(|(i, overlay)| {
            format!(
                r#"overlay {{
    type = "{:?}"
    name = "{}"
    {}
  }}"#,
                overlay.overlay_type,
                overlay.name,
                match overlay.overlay_type {
                    OverlayType::Files => format!("source = {:?}\n    dest = {:?}", overlay.source_path, overlay.dest_path),
                    OverlayType::Shell => format!("commands = {:?}", overlay.commands),
                    OverlayType::Packages => format!("packages = {:?}", overlay.packages),
                    OverlayType::Environment => format!("env = {:?}", overlay.env_vars),
                    OverlayType::CloudInit => "# cloud-init configured separately".to_string(),
                }
            )
        }).collect::<Vec<_>>().join("\n\n  ")
    )
}

// ============================================================================
// RBAC / Policy handlers
// ============================================================================

async fn rbac_list_roles_handler() -> impl IntoResponse {
    let engine = crate::auth::PolicyEngine::new();
    let roles = engine.roles();
    Json(serde_json::json!({
        "roles": roles,
        "count": roles.len(),
    }))
}

async fn rbac_list_policies_handler() -> impl IntoResponse {
    let engine = crate::auth::PolicyEngine::new();
    let permissions = engine.permissions();
    Json(serde_json::json!({
        "permissions": permissions,
        "count": permissions.len(),
        "built_in_roles": ["admin", "operator", "viewer", "builder"],
    }))
}

async fn rbac_terraform_export_handler() -> impl IntoResponse {
    let engine = crate::auth::PolicyEngine::new();
    let hcl = engine.export_terraform();
    
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/plain; charset=utf-8")
        .header("content-disposition", "attachment; filename=\"rbac-policy.tf\"")
        .body(axum::body::Body::from(hcl))
        .unwrap()
        .into_response()
}

// ============================================================================
// Admin controls
// ============================================================================

async fn admin_status_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    let enabled = state.control.is_some();
    let requires_admin_token = state
        .control
        .as_ref()
        .and_then(|c| c.admin_token.as_ref())
        .is_some();

    Json(serde_json::json!({
        "control_enabled": enabled,
        "requires_admin_token": requires_admin_token,
        "daemon_pidfile": state.control.as_ref().and_then(|c| c.daemon_pidfile.as_ref()).cloned(),
        "note": if enabled {
            "Admin controls are enabled. Use x-infrasim-admin-token if configured."
        } else {
            "Admin controls are disabled. Set INFRASIM_WEB_CONTROL_ENABLED=1. For safe restart, run under a supervisor that restarts on exit."
        }
    }))
}

async fn admin_restart_web_handler(
    State(state): State<Arc<WebServerState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let Some(control) = state.control.as_ref() else {
        return (
            StatusCode::PRECONDITION_FAILED,
            Json(serde_json::json!({
                "error": "web-control-disabled",
                "hint": "Set INFRASIM_WEB_CONTROL_ENABLED=1 and run infrasim-web under a supervisor (launchd/systemd/foreman) that restarts it on exit."
            })),
        )
            .into_response();
    };

    if !control.check_admin_token(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "missing-or-invalid-admin-token"})),
        )
            .into_response();
    }

    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        // Exit code 75 (EX_TEMPFAIL) hints a supervisor to restart.
        process::exit(75);
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "status": "restarting",
            "note": "Process exiting now; supervisor should restart it."
        })),
    )
        .into_response()
}

async fn admin_restart_daemon_handler(
    State(state): State<Arc<WebServerState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let Some(control) = state.control.as_ref() else {
        return (
            StatusCode::PRECONDITION_FAILED,
            Json(serde_json::json!({
                "error": "web-control-disabled",
                "hint": "Enable INFRASIM_WEB_CONTROL_ENABLED=1 and provide INFRASIM_DAEMON_PIDFILE; run the daemon under a supervisor to restart it."
            })),
        )
            .into_response();
    };

    if !control.check_admin_token(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "missing-or-invalid-admin-token"})),
        )
            .into_response();
    }

    let Some(pidfile) = control.daemon_pidfile.as_ref() else {
        return (
            StatusCode::PRECONDITION_FAILED,
            Json(serde_json::json!({
                "error": "no-daemon-pidfile",
                "hint": "Set INFRASIM_DAEMON_PIDFILE to a pidfile path, and have the daemon write it (or manage it via a supervisor)."
            })),
        )
            .into_response();
    };

    match read_pidfile(pidfile).and_then(|pid| send_sigterm(pid)) {
        Ok(pid) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({
                "status": "signaled",
                "signal": "SIGTERM",
                "pid": pid,
                "note": "Daemon should exit; supervisor should restart it."
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": format!("{}", e)})),
        )
            .into_response(),
    }
}

async fn admin_stop_daemon_handler(
    State(state): State<Arc<WebServerState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let Some(control) = state.control.as_ref() else {
        return (
            StatusCode::PRECONDITION_FAILED,
            Json(serde_json::json!({
                "error": "web-control-disabled",
                "hint": "Enable INFRASIM_WEB_CONTROL_ENABLED=1 and provide INFRASIM_DAEMON_PIDFILE for stop controls."
            })),
        )
            .into_response();
    };

    if !control.check_admin_token(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "missing-or-invalid-admin-token"})),
        )
            .into_response();
    }

    let Some(pidfile) = control.daemon_pidfile.as_ref() else {
        return (
            StatusCode::PRECONDITION_FAILED,
            Json(serde_json::json!({
                "error": "no-daemon-pidfile",
                "hint": "Set INFRASIM_DAEMON_PIDFILE to a pidfile path."
            })),
        )
            .into_response();
    };

    match read_pidfile(pidfile).and_then(|pid| send_sigterm(pid)) {
        Ok(pid) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({"status": "signaled", "signal": "SIGTERM", "pid": pid})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": format!("{}", e)})),
        )
            .into_response(),
    }
}

fn read_pidfile(path: &str) -> anyhow::Result<i32> {
    let raw = std::fs::read_to_string(path)?;
    Ok(raw.trim().parse()?)
}

fn send_sigterm(pid: i32) -> anyhow::Result<i32> {
    #[cfg(unix)]
    {
        let res = unsafe { libc::kill(pid, libc::SIGTERM) };
        if res != 0 {
            return Err(anyhow::anyhow!("failed to signal pid {}", pid));
        }
        Ok(pid)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Err(anyhow::anyhow!("signals not supported on this platform"))
    }
}

async fn admin_page_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    let enabled = state.control.is_some();
    let needs_token = state
        .control
        .as_ref()
        .and_then(|c| c.admin_token.as_ref())
        .is_some();

    let body = format!(
        r#"<!doctype html>
<html>
  <head>
    <meta charset=\"utf-8\" />
    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />
    <title>InfraSim Admin</title>
    <style>
      body {{ font-family: ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial; padding: 18px; max-width: 920px; margin: 0 auto; }}
      .card {{ border: 1px solid #e5e7eb; border-radius: 10px; padding: 14px 16px; margin: 12px 0; }}
      button {{ padding: 10px 12px; border-radius: 8px; border: 1px solid #d1d5db; background:#111827; color:#fff; cursor:pointer; margin-right: 8px; }}
      button.secondary {{ background:#374151; }}
      input {{ padding: 10px 12px; border-radius: 8px; border: 1px solid #d1d5db; width: 360px; max-width: 100%; }}
      pre {{ background:#0b1020; color:#e5e7eb; padding:12px; border-radius:10px; overflow:auto; }}
      .hint {{ color:#6b7280; }}
      code {{ background: #f3f4f6; padding: 2px 6px; border-radius: 6px; }}
    </style>
  </head>
  <body>
    <h1>InfraSim Admin</h1>
    <p class=\"hint\">Control enabled: <b>{enabled}</b>. Admin token required: <b>{needs_token}</b>.</p>

    <div class=\"card\">
      <h3>Admin token (optional)</h3>
      <p class=\"hint\">Sent as <code>x-infrasim-admin-token</code> (only if configured).</p>
      <input id=\"tok\" placeholder=\"x-infrasim-admin-token\" />
    </div>

    <div class=\"card\">
      <h3>Actions</h3>
      <button onclick=\"post('/api/admin/restart-web')\">Restart Web (exit)</button>
      <button class=\"secondary\" onclick=\"post('/api/admin/restart-daemon')\">Restart Daemon (SIGTERM)</button>
      <button class=\"secondary\" onclick=\"post('/api/admin/stop-daemon')\">Stop Daemon (SIGTERM)</button>
      <p class=\"hint\">To actually restart after exit, run via launchd/systemd (or another supervisor) that restarts processes.</p>
    </div>

    <div class=\"card\">
      <h3>Status</h3>
      <button class=\"secondary\" onclick=\"getStatus()\">Refresh</button>
      <pre id=\"out\">(no output)</pre>
    </div>

        <script>
            function headers() {{
        const token = document.getElementById('tok').value;
                const h = {{ 'content-type': 'application/json' }};
        if (token) h['x-infrasim-admin-token'] = token;
        return h;
            }}
            async function post(path) {{
                const r = await fetch(path, {{ method: 'POST', headers: headers() }});
        const t = await r.text();
        document.getElementById('out').textContent = r.status + "\n" + t;
            }}
            async function getStatus() {{
                const r = await fetch('/api/admin/status', {{ headers: headers() }});
        const t = await r.text();
        document.getElementById('out').textContent = r.status + "\n" + t;
            }}
      getStatus();
    </script>
  </body>
</html>"#
    );

    Html(body)
}

// ============================================================================
// Inventory Handlers: Images (qcow2 volumes that are disk images)
// ============================================================================

async fn list_images_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    // Images are volumes with format=qcow2 or raw, typically used as boot disks
    match state.daemon.list_volumes().await {
        Ok(volumes) => {
            let images: Vec<_> = volumes.into_iter()
                .filter(|v| v.kind == "disk" && (v.format == "qcow2" || v.format == "raw"))
                .collect();
            (StatusCode::OK, Json(serde_json::json!({
                "images": images,
                "count": images.len(),
            }))).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn get_image_handler(
    State(state): State<Arc<WebServerState>>,
    Path(image_id): Path<String>,
) -> impl IntoResponse {
    match state.daemon.get_volume(&image_id).await {
        Ok(vol) => (StatusCode::OK, Json(vol)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

// ============================================================================
// Inventory Handlers: Volumes
// ============================================================================

async fn list_volumes_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    match state.daemon.list_volumes().await {
        Ok(volumes) => (StatusCode::OK, Json(serde_json::json!({
            "volumes": volumes,
            "count": volumes.len(),
        }))).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn get_volume_handler(
    State(state): State<Arc<WebServerState>>,
    Path(volume_id): Path<String>,
) -> impl IntoResponse {
    match state.daemon.get_volume(&volume_id).await {
        Ok(vol) => (StatusCode::OK, Json(vol)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

// ============================================================================
// Inventory Handlers: Snapshots
// ============================================================================

async fn list_snapshots_handler(
    State(state): State<Arc<WebServerState>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let vm_id = params.get("vm_id").map(|s| s.as_str());
    match state.daemon.list_snapshots(vm_id).await {
        Ok(snapshots) => (StatusCode::OK, Json(serde_json::json!({
            "snapshots": snapshots,
            "count": snapshots.len(),
        }))).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn get_snapshot_handler(
    State(state): State<Arc<WebServerState>>,
    Path(snapshot_id): Path<String>,
) -> impl IntoResponse {
    // We need to list and filter since there's no get_snapshot by ID
    match state.daemon.list_snapshots(None).await {
        Ok(snapshots) => {
            match snapshots.into_iter().find(|s| s.id == snapshot_id) {
                Some(snap) => (StatusCode::OK, Json(snap)).into_response(),
                None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "snapshot not found"}))).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

// ============================================================================
// Inventory Handlers: Networks
// ============================================================================

async fn list_networks_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    match state.daemon.list_networks().await {
        Ok(networks) => (StatusCode::OK, Json(serde_json::json!({
            "networks": networks,
            "count": networks.len(),
        }))).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn get_network_handler(
    State(state): State<Arc<WebServerState>>,
    Path(network_id): Path<String>,
) -> impl IntoResponse {
    match state.daemon.list_networks().await {
        Ok(networks) => {
            match networks.into_iter().find(|n| n.id == network_id) {
                Some(net) => (StatusCode::OK, Json(net)).into_response(),
                None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "network not found"}))).into_response(),
            }
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

// ============================================================================
// Inventory Handlers: VMs
// ============================================================================

async fn list_vms_api_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    match state.daemon.list_vms().await {
        Ok(vms) => (StatusCode::OK, Json(serde_json::json!({
            "vms": vms,
            "count": vms.len(),
        }))).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn get_vm_handler(
    State(state): State<Arc<WebServerState>>,
    Path(vm_id): Path<String>,
) -> impl IntoResponse {
    match state.daemon.get_vm(&vm_id).await {
        Ok(vm) => (StatusCode::OK, Json(vm)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn auth_middleware_inner(
    state: Arc<WebServerState>,
    req: Request,
    next: middleware::Next,
) -> Response {
    let path = req.uri().path();
    
    // =========================================================================
    // Static Asset Policy (Non-Negotiable)
    // =========================================================================
    // /ui/* must be publicly readable (JS, CSS, HTML, fonts, images)
    // /api/* remains authenticated
    // /api/admin/* remains admin-token gated
    // /api/health and /api/ui/manifest are public for monitoring/provenance
    // =========================================================================
    
    // Dev bypass must be explicitly enabled.
    let dev_bypass_enabled = std::env::var("INFRASIM_WEB_DEV_BYPASS_AUTH")
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);

    let dev_header_ok = req
        .headers()
        .get("x-infrasim-dev")
        .and_then(|v| v.to_str().ok())
        == Some("1");

    // Public paths - no authentication required
    let is_public_path = 
        // Root and legacy static assets
        path == "/" 
        || path == "/favicon.ico"
        || path.starts_with("/assets/")
        || path.starts_with("/app/") 
        || path.starts_with("/core/") 
        || path.starts_with("/vendor/")
        // UI static assets (SPA bundle) - MUST be public
        || path.starts_with("/ui/")
        || path == "/ui"
        // VNC HTML pages (legacy)
        || path == "/vnc.html"
        || path == "/vnc_lite.html"
        // Auth endpoints (TOTP login/enrollment)
        || path.starts_with("/api/auth/")
        // Public API endpoints
        || path == "/api/health"
        || path == "/api/ui/manifest"
        // Dev convenience: allow API in local/dev UI mode.
        || (path.starts_with("/api/")
            && (dev_bypass_enabled && dev_header_ok));
    
    // WebSocket paths - auth handled at connection time
    let is_websocket_path = path.starts_with("/websockify/");
    
    if is_public_path || is_websocket_path {
        return next.run(req).await;
    }

    // If auth is disabled, allow.
    if matches!(state.cfg.auth, WebUiAuth::None) {
        return next.run(req).await;
    }

    // JWT mode: validate and allow.
    if let WebUiAuth::Jwt(cfg) = &state.cfg.auth {
        let auth_header = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let token = auth_header.strip_prefix("Bearer ").unwrap_or("");
        if token.is_empty() {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "missing bearer token"})),
            )
                .into_response();
        }

        match verify_jwt_with_local_jwks(token, cfg) {
            Ok(_td) => {
                // TODO: attach claims into request extensions for RBAC.
                return next.run(req).await;
            }
            Err(e) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({"error": "invalid jwt", "detail": format!("{e}")})),
                )
                    .into_response();
            }
        }
    }

    // Token can be configured statically or generated (dev) and stored under "dev".
    let expected = match &state.cfg.auth {
        WebUiAuth::Token(t) => Some(t.clone()),
        WebUiAuth::Jwt(_) => None,
        WebUiAuth::DevRandom => {
            let tokens = state.tokens.read().await;
            tokens.get("dev").cloned()
        }
        WebUiAuth::None => None,
    };

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let provided = auth_header.strip_prefix("Bearer ").unwrap_or("");

    if provided.is_empty() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "missing bearer token"})),
        )
            .into_response();
    }

    if let Some(expected) = expected {
        if provided == expected {
            return next.run(req).await;
        }
    }

    // If not the configured token, check if it's an issued auth session.
    let now = now_epoch_secs();

    // IMPORTANT: don't hold the sqlite lock across await.
    let (allowed, error_response) = {
        let conn_arc = state.db.connection();
        let conn = conn_arc.lock();

        let session: Option<i64> = conn
            .query_row(
                "SELECT expires_at FROM auth_sessions WHERE token = ?1",
                rusqlite::params![provided],
                |r| Ok(r.get(0)?),
            )
            .optional()
            .ok()
            .flatten();

        match session {
            Some(expires_at) if expires_at > now => {
                let _ = conn.execute(
                    "UPDATE auth_sessions SET last_seen_at = ?1 WHERE token = ?2",
                    rusqlite::params![now, provided],
                );
                (true, None)
            }
            Some(_) => {
                let _ = conn.execute("DELETE FROM auth_sessions WHERE token = ?1", rusqlite::params![provided]);
                (false, Some((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "expired"}))).into_response()))
            }
            None => {
                (false, Some((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "missing or invalid bearer token"}))).into_response()))
            }
        }
    };

    if !allowed {
        return error_response.unwrap_or_else(|| {
            (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "unauthorized"}))).into_response()
        });
    }

    next.run(req).await
}

async fn list_projects_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    let projects = state.projects.read().await;
    let list: Vec<_> = projects.values().cloned().collect();
    Json(serde_json::json!({"projects": list}))
}

async fn create_project_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<CreateProjectRequest>,
) -> Response {
    if req.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "name must not be empty"})),
        )
            .into_response();
    }

    let id = uuid::Uuid::new_v4().to_string();
    let project = Project {
        id: id.clone(),
        name: req.name,
        created_at: chrono::Utc::now().timestamp(),
        prompts: vec![],
    };

    let mut projects = state.projects.write().await;
    projects.insert(id.clone(), project.clone());

    (StatusCode::CREATED, Json(project)).into_response()
}

fn builtin_appliance_templates() -> Vec<ApplianceTemplate> {
    vec![
        // Pi-like desktop template
        ApplianceTemplate {
            id: "pi-like-aarch64-desktop".to_string(),
            title: "Pi-like AArch64 Desktop".to_string(),
            description: "A Raspberry-Pi-like (AArch64) VM profile intended for interactive desktop-style workloads (e.g. Kali + browser + CLI).".to_string(),
            arch: "aarch64".to_string(),
            machine: "virt".to_string(),
            cpu_cores: 4,
            memory_mb: 4096,
            compatibility_mode: true,
            tags: vec!["aarch64".to_string(), "pi-like".to_string(), "desktop".to_string()],
            image: None,
            env: HashMap::new(),
            ports: vec![],
            boot_plan: vec![
                BootStep { order: 1, action: "create_vm".to_string(), description: "Provision VM via daemon".to_string(), args: HashMap::new() },
                BootStep { order: 2, action: "start_vm".to_string(), description: "Start the VM".to_string(), args: HashMap::new() },
                BootStep { order: 3, action: "wait_ssh".to_string(), description: "Wait for SSH readiness".to_string(), args: HashMap::new() },
            ],
            networks: vec![
                NetworkDef { id: "default".to_string(), mode: "user".to_string(), cidr: Some("10.0.2.0/24".to_string()), gateway: Some("10.0.2.2".to_string()), dhcp: true },
            ],
            volumes: vec![
                VolumeDef { id: "root".to_string(), size_mb: 8192, mount_path: "/".to_string(), kind: "disk".to_string() },
            ],
            tools: vec![],
        },
        // Alpine Linux on Raspberry Pi architecture
        ApplianceTemplate {
            id: "alpine-rpi-aarch64".to_string(),
            title: "Alpine Linux on Raspberry Pi".to_string(),
            description: "Minimal Alpine Linux appliance running on emulated Raspberry Pi architecture (AArch64). Includes basic setup and SSH access.".to_string(),
            arch: "aarch64".to_string(),
            machine: "raspi3".to_string(),
            cpu_cores: 4,
            memory_mb: 1024,
            compatibility_mode: false,
            tags: vec!["aarch64".to_string(), "alpine".to_string(), "raspberry-pi".to_string(), "minimal".to_string()],
            image: Some("alpine:latest".to_string()),
            env: {
                let mut m = HashMap::new();
                m.insert("ALPINE_MIRROR".to_string(), "http://dl-cdn.alpinelinux.org/alpine".to_string());
                m
            },
            ports: vec![
                AppliancePort { container_port: 22, host_port: Some(2222), protocol: "tcp".to_string(), description: "SSH access".to_string() },
            ],
            boot_plan: vec![
                BootStep { order: 1, action: "create_vm".to_string(), description: "Provision AArch64 VM with Raspberry Pi machine".to_string(), args: HashMap::new() },
                BootStep { order: 2, action: "pull_image".to_string(), description: "Pull Alpine Linux image".to_string(), args: {
                    let mut m = HashMap::new();
                    m.insert("image".to_string(), "alpine:latest".to_string());
                    m
                }},
                BootStep { order: 3, action: "run_container".to_string(), description: "Start Alpine container".to_string(), args: {
                    let mut m = HashMap::new();
                    m.insert("cmd".to_string(), "/bin/sh".to_string());
                    m
                }},
                BootStep { order: 4, action: "wait_ssh".to_string(), description: "Wait for SSH readiness on port 2222".to_string(), args: {
                    let mut m = HashMap::new();
                    m.insert("port".to_string(), "2222".to_string());
                    m
                }},
            ],
            networks: vec![
                NetworkDef { id: "default".to_string(), mode: "user".to_string(), cidr: Some("10.0.2.0/24".to_string()), gateway: Some("10.0.2.2".to_string()), dhcp: true },
            ],
            volumes: vec![
                VolumeDef { id: "root".to_string(), size_mb: 2048, mount_path: "/".to_string(), kind: "disk".to_string() },
                VolumeDef { id: "data".to_string(), size_mb: 1024, mount_path: "/data".to_string(), kind: "disk".to_string() },
            ],
            tools: vec![
                ToolDef { name: "openssh".to_string(), version: Some("latest".to_string()), purpose: "SSH server for remote access".to_string() },
                ToolDef { name: "alpine-base".to_string(), version: Some("latest".to_string()), purpose: "Base Alpine Linux packages".to_string() },
            ],
        },
        // Keycloak IdP appliance
        ApplianceTemplate {
            id: "keycloak-aarch64".to_string(),
            title: "Keycloak Identity Provider".to_string(),
            description: "Keycloak (AArch64) appliance for identity federation and SSO. Runs in dev mode by default; configure TLS/proxy for production.".to_string(),
            arch: "aarch64".to_string(),
            machine: "virt".to_string(),
            cpu_cores: 2,
            memory_mb: 2048,
            compatibility_mode: false,
            tags: vec!["aarch64".to_string(), "identity".to_string(), "keycloak".to_string(), "sso".to_string()],
            image: Some("quay.io/keycloak/keycloak:26.0".to_string()),
            env: {
                let mut m = HashMap::new();
                m.insert("KC_BOOTSTRAP_ADMIN_USERNAME".to_string(), "admin".to_string());
                m.insert("KC_BOOTSTRAP_ADMIN_PASSWORD".to_string(), "changeme".to_string());
                m
            },
            ports: vec![
                AppliancePort { container_port: 8080, host_port: Some(8080), protocol: "tcp".to_string(), description: "Keycloak HTTP".to_string() },
                AppliancePort { container_port: 8443, host_port: Some(8443), protocol: "tcp".to_string(), description: "Keycloak HTTPS".to_string() },
            ],
            boot_plan: vec![
                BootStep { order: 1, action: "create_vm".to_string(), description: "Provision AArch64 VM".to_string(), args: HashMap::new() },
                BootStep { order: 2, action: "pull_image".to_string(), description: "Pull Keycloak container image".to_string(), args: {
                    let mut m = HashMap::new();
                    m.insert("image".to_string(), "quay.io/keycloak/keycloak:26.0".to_string());
                    m
                }},
                BootStep { order: 3, action: "run_container".to_string(), description: "Start Keycloak in dev mode".to_string(), args: {
                    let mut m = HashMap::new();
                    m.insert("cmd".to_string(), "start-dev".to_string());
                    m
                }},
                BootStep { order: 4, action: "wait_http".to_string(), description: "Wait for Keycloak /health/ready".to_string(), args: {
                    let mut m = HashMap::new();
                    m.insert("url".to_string(), "http://localhost:8080/health/ready".to_string());
                    m
                }},
            ],
            networks: vec![
                NetworkDef { id: "mgmt".to_string(), mode: "user".to_string(), cidr: Some("10.0.2.0/24".to_string()), gateway: Some("10.0.2.2".to_string()), dhcp: true },
            ],
            volumes: vec![
                VolumeDef { id: "kc-data".to_string(), size_mb: 1024, mount_path: "/opt/keycloak/data".to_string(), kind: "disk".to_string() },
            ],
            tools: vec![
                ToolDef { name: "keycloak".to_string(), version: Some("26.0".to_string()), purpose: "Identity and access management".to_string() },
            ],
        },
    ]
}

async fn list_appliance_templates_handler() -> impl IntoResponse {
    Json(serde_json::json!({"templates": builtin_appliance_templates()}))
}

async fn list_appliances_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    // Best-effort refresh from DB to ensure persistence is reflected.
    if let Err(e) = load_appliance_catalog_into_memory(state.clone()).await {
        warn!("failed to refresh appliance catalog: {}", e);
    }

    let appliances = state.appliances.read().await;
    let list: Vec<_> = appliances.values().cloned().collect();
    Json(serde_json::json!({"appliances": list}))
}

#[derive(Debug, Clone, Deserialize)]
struct SeedAppliancesRequest {
    /// Template IDs to seed. If omitted/empty, seeds all built-in templates.
    #[serde(default)]
    template_ids: Vec<String>,
    /// Optional name prefix for seeded instances.
    #[serde(default)]
    name_prefix: Option<String>,
}

/// "Migration" for MVP: seed launchable appliance entries into the web server's
/// catalog so they show up in the UI even before a user manually creates them.
///
/// Note: Today the web server stores appliance instances in-memory. This endpoint
/// makes the Keycloak template visible as a launchable item by creating an
/// ApplianceInstance with status "seeded".
async fn seed_appliances_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<SeedAppliancesRequest>,
) -> impl IntoResponse {
    let templates = builtin_appliance_templates();
    let selected: Vec<ApplianceTemplate> = if req.template_ids.is_empty() {
        templates
    } else {
        templates
            .into_iter()
            .filter(|t| req.template_ids.iter().any(|id| id == &t.id))
            .collect()
    };

    if selected.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "no matching templates to seed"})),
        );
    }

    let prefix = req.name_prefix.unwrap_or_else(|| "seed".to_string());
    let mut created: Vec<ApplianceInstance> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    let mut appliances = state.appliances.write().await;
    let now = chrono::Utc::now().timestamp();

    for t in selected {
        // Skip if already present (by template_id + name prefix heuristic).
        let already = appliances.values().any(|a| a.template_id == t.id && a.name.starts_with(&prefix));
        if already {
            skipped.push(t.id);
            continue;
        }

        let id = uuid::Uuid::new_v4().to_string();
        let instance = ApplianceInstance {
            id: id.clone(),
            name: format!("{}-{}", prefix, t.id),
            template_id: t.id,
            created_at: now,
            updated_at: now,
            status: "seeded".to_string(),
            vm_id: None,
            network_ids: vec![],
            volume_ids: vec![],
            console_id: None,
            snapshot_ids: vec![],
        };

        appliances.insert(id.clone(), instance.clone());
        // Persist to DB.
        if let Err(e) = persist_catalog_instance(&state, &instance).await {
            warn!("failed to persist catalog instance: {}", e);
        }
        created.push(instance);
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "created": created,
            "skipped_template_ids": skipped,
            "note": "Seeded appliances are launchable via POST /api/appliances/:id/boot"
        })),
    )
}

async fn create_appliance_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<CreateApplianceRequest>,
) -> Response {
    if req.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "name must not be empty"})),
        )
            .into_response();
    }

    let templates = builtin_appliance_templates();
    let Some(template) = templates.iter().find(|t| t.id == req.template_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "unknown template_id"})),
        )
            .into_response();
    };

    let id = uuid::Uuid::new_v4().to_string();
    let mut vm_id: Option<String> = None;
    let mut console_id: Option<String> = None;
    let mut network_ids: Vec<String> = vec![];
    let mut volume_ids: Vec<String> = vec![];
    let mut status = "created".to_string();
    let mut error_msg: Option<String> = None;

    // Wire to daemon: create networks, volumes, VM, and console.
    let daemon = &state.daemon;
    
    // 1. Create networks
    for net in &template.networks {
        match daemon.create_network(&format!("{}-{}", req.name, net.id), net).await {
            Ok(net_id) => {
                info!("Created network {} -> {}", net.id, net_id);
                network_ids.push(net_id);
            }
            Err(e) => warn!("Failed to create network {}: {}", net.id, e),
        }
    }

    // 2. Create volumes
    for vol in &template.volumes {
        match daemon.create_volume(&format!("{}-{}", req.name, vol.id), vol).await {
            Ok(vol_id) => {
                info!("Created volume {} -> {}", vol.id, vol_id);
                volume_ids.push(vol_id);
            }
            Err(e) => warn!("Failed to create volume {}: {}", vol.id, e),
        }
    }

    // 3. Create VM
    match daemon.create_vm(&req.name, template).await {
        Ok(created_vm_id) => {
            vm_id = Some(created_vm_id.clone());
            status = "vm_created".to_string();
            info!("Created VM {} -> {}", req.name, created_vm_id);

            // 4. Start VM if auto_start is enabled (default true)
            if req.auto_start.unwrap_or(true) {
                match daemon.start_vm(&created_vm_id).await {
                    Ok(_) => {
                        status = "running".to_string();
                        info!("Started VM {}", created_vm_id);

                        // 5. Create console
                        match daemon.create_console(&created_vm_id, 5900, 6080).await {
                            Ok(cid) => {
                                info!("Created console {} for VM {}", cid, created_vm_id);
                                console_id = Some(cid);
                            }
                            Err(e) => warn!("Failed to create console for {}: {}", created_vm_id, e),
                        }
                    }
                    Err(e) => {
                        status = "start_failed".to_string();
                        error_msg = Some(e.to_string());
                        warn!("Failed to start VM {}: {}", created_vm_id, e);
                    }
                }
            }
        }
        Err(e) => {
            status = "vm_creation_failed".to_string();
            error_msg = Some(e.to_string());
            warn!("Failed to create VM for appliance {}: {}", req.name, e);
        }
    }

    let now = chrono::Utc::now().timestamp();
    let instance = ApplianceInstance {
        id: id.clone(),
        name: req.name,
        template_id: req.template_id,
        created_at: now,
        vm_id,
        status,
        network_ids,
        volume_ids,
        console_id,
        snapshot_ids: vec![],
        updated_at: now,
    };

    let mut appliances = state.appliances.write().await;
    appliances.insert(id.clone(), instance.clone());

    let response = serde_json::json!({
        "appliance": instance,
        "error": error_msg,
    });

    (StatusCode::CREATED, Json(response)).into_response()
}

// Generate Terraform HCL for an appliance's networks + volumes.
async fn appliance_terraform_handler(
    State(state): State<Arc<WebServerState>>,
    Path(appliance_id): Path<String>,
) -> Response {
    let appliances = state.appliances.read().await;
    let Some(instance) = appliances.get(&appliance_id) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "appliance not found"}))).into_response();
    };

    let templates = builtin_appliance_templates();
    let Some(tpl) = templates.iter().find(|t| t.id == instance.template_id) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "template not found"}))).into_response();
    };

    // Build Terraform HCL for networks and volumes.
    let mut hcl = String::new();
    hcl.push_str(&format!(r#"# Terraform for appliance: {} (template: {})
terraform {{
  required_providers {{
    infrasim = {{
      source  = "infrasim/infrasim"
      version = ">= 0.1.0"
    }}
  }}
}}

provider "infrasim" {{
  endpoint = "{}"
}}

"#, instance.name, tpl.id, state.cfg.daemon_addr));

    for net in &tpl.networks {
        hcl.push_str(&format!(r#"resource "infrasim_network" "{}" {{
  name         = "{}"
  mode         = "{}"
  cidr         = "{}"
  gateway      = "{}"
  dhcp_enabled = {}
}}

"#,
            net.id,
            net.id,
            net.mode,
            net.cidr.as_deref().unwrap_or(""),
            net.gateway.as_deref().unwrap_or(""),
            net.dhcp,
        ));
    }

    for vol in &tpl.volumes {
        hcl.push_str(&format!(r#"resource "infrasim_volume" "{}" {{
  name      = "{}"
  size_mb   = {}
  kind      = "{}"
}}

"#,
            vol.id,
            vol.id,
            vol.size_mb,
            vol.kind,
        ));
    }

    // VM resource referencing networks + volumes.
    let net_ids: Vec<String> = tpl.networks.iter().map(|n| format!("infrasim_network.{}.id", n.id)).collect();
    let vol_ids: Vec<String> = tpl.volumes.iter().map(|v| format!("infrasim_volume.{}.id", v.id)).collect();
    hcl.push_str(&format!(r#"resource "infrasim_vm" "{}" {{
  name             = "{}"
  arch             = "{}"
  machine          = "{}"
  cpu_cores        = {}
  memory_mb        = {}
  compatibility_mode = {}
  network_ids      = [{}]
  volume_ids       = [{}]
}}
"#,
        instance.name,
        instance.name,
        tpl.arch,
        tpl.machine,
        tpl.cpu_cores,
        tpl.memory_mb,
        tpl.compatibility_mode,
        net_ids.join(", "),
        vol_ids.join(", "),
    ));

    (StatusCode::OK, Json(serde_json::json!({
        "appliance_id": appliance_id,
        "terraform_hcl": hcl,
    }))).into_response()
}

// Trigger the boot plan for an appliance instance (MVP stub).
async fn appliance_boot_handler(
    State(state): State<Arc<WebServerState>>,
    Path(appliance_id): Path<String>,
) -> Response {
    let mut appliances = state.appliances.write().await;
    let Some(instance) = appliances.get_mut(&appliance_id) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "appliance not found"}))).into_response();
    };

    let templates = builtin_appliance_templates();
    let Some(tpl) = templates.iter().find(|t| t.id == instance.template_id) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "template not found"}))).into_response();
    };

    // If we have a VM, start it via daemon.
    if let Some(vm_id) = &instance.vm_id {
        match state.daemon.start_vm(vm_id).await {
            Ok(_) => {
                instance.status = "running".to_string();
                info!("Started VM {} for appliance {}", vm_id, appliance_id);
            }
            Err(e) => {
                instance.status = "start_failed".to_string();
                warn!("Failed to start VM {}: {}", vm_id, e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                    "error": format!("failed to start VM: {}", e),
                }))).into_response();
            }
        }
    } else {
        instance.status = "booting".to_string();
    }

    (StatusCode::ACCEPTED, Json(serde_json::json!({
        "appliance_id": appliance_id,
        "status": instance.status,
        "boot_plan": tpl.boot_plan,
    }))).into_response()
}

// Stop an appliance instance (stop the VM).
async fn appliance_stop_handler(
    State(state): State<Arc<WebServerState>>,
    Path(appliance_id): Path<String>,
    Json(req): Json<ApplianceStopRequest>,
) -> Response {
    let mut appliances = state.appliances.write().await;
    let Some(instance) = appliances.get_mut(&appliance_id) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "appliance not found"}))).into_response();
    };

    let Some(vm_id) = &instance.vm_id else {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "no VM associated with appliance"}))).into_response();
    };

    match state.daemon.stop_vm(vm_id, req.force.unwrap_or(false)).await {
        Ok(_) => {
            instance.status = "stopped".to_string();
            info!("Stopped VM {} for appliance {}", vm_id, appliance_id);
            (StatusCode::OK, Json(serde_json::json!({
                "appliance_id": appliance_id,
                "status": instance.status,
            }))).into_response()
        }
        Err(e) => {
            warn!("Failed to stop VM {}: {}", vm_id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("failed to stop VM: {}", e),
            }))).into_response()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApplianceStopRequest {
    #[serde(default)]
    force: Option<bool>,
}

// Create a snapshot of an appliance VM with signed evidence bundle.
async fn appliance_snapshot_handler(
    State(state): State<Arc<WebServerState>>,
    Path(appliance_id): Path<String>,
    Json(req): Json<ApplianceSnapshotRequest>,
) -> Response {
    let appliances = state.appliances.read().await;
    let Some(instance) = appliances.get(&appliance_id) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "appliance not found"}))).into_response();
    };

    let Some(vm_id) = &instance.vm_id else {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "no VM associated with appliance"}))).into_response();
    };

    // Create snapshot via daemon
    let snapshot_name = req.name.unwrap_or_else(|| format!("snapshot-{}", chrono::Utc::now().timestamp()));
    match state.daemon.create_snapshot(vm_id, &snapshot_name, req.include_memory.unwrap_or(false)).await {
        Ok(snapshot_id) => {
            info!("Created snapshot {} for appliance {} (VM {})", snapshot_id, appliance_id, vm_id);

            // Create signed evidence bundle for the snapshot
            let key_pair = infrasim_common::crypto::KeyPair::generate();
            let evidence = serde_json::json!({
                "type": "snapshot",
                "snapshot_id": snapshot_id,
                "appliance_id": appliance_id,
                "vm_id": vm_id,
                "name": snapshot_name,
                "include_memory": req.include_memory.unwrap_or(false),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            let evidence_bytes = serde_json::to_vec(&evidence).unwrap_or_default();
            let signature = key_pair.sign(&evidence_bytes);

            (StatusCode::CREATED, Json(serde_json::json!({
                "snapshot_id": snapshot_id,
                "appliance_id": appliance_id,
                "vm_id": vm_id,
                "name": snapshot_name,
                "evidence": {
                    "data": evidence,
                    "signature": hex::encode(&signature),
                    "public_key": hex::encode(key_pair.public_key_bytes()),
                },
            }))).into_response()
        }
        Err(e) => {
            warn!("Failed to create snapshot for VM {}: {}", vm_id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("failed to create snapshot: {}", e),
            }))).into_response()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApplianceSnapshotRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    include_memory: Option<bool>,
}

// ============================================================================
// Detailed Appliance Handlers
// ============================================================================

/// Get detailed appliance view with all resolved resources.
async fn get_appliance_detail_handler(
    State(state): State<Arc<WebServerState>>,
    Path(appliance_id): Path<String>,
) -> Response {
    let appliances = state.appliances.read().await;
    let Some(instance) = appliances.get(&appliance_id) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "appliance not found"}))).into_response();
    };

    let templates = builtin_appliance_templates();
    let template = templates.iter().find(|t| t.id == instance.template_id).cloned();

    // Fetch VM details
    let vm = if let Some(vm_id) = &instance.vm_id {
        state.daemon.get_vm(vm_id).await.ok()
    } else {
        None
    };

    // Fetch network details
    let all_networks = state.daemon.list_networks().await.unwrap_or_default();
    let networks: Vec<_> = all_networks.into_iter()
        .filter(|n| instance.network_ids.contains(&n.id))
        .collect();

    // Fetch volume details
    let all_volumes = state.daemon.list_volumes().await.unwrap_or_default();
    let volumes: Vec<_> = all_volumes.into_iter()
        .filter(|v| instance.volume_ids.contains(&v.id))
        .collect();

    // Fetch snapshot details
    let all_snapshots = state.daemon.list_snapshots(instance.vm_id.as_deref()).await.unwrap_or_default();
    let snapshots: Vec<_> = all_snapshots.into_iter()
        .filter(|s| instance.snapshot_ids.contains(&s.id) || instance.vm_id.as_ref().map(|id| &s.vm_id == id).unwrap_or(false))
        .collect();

    // Generate Terraform HCL
    let terraform_hcl = generate_appliance_terraform(&instance, template.as_ref(), &state.cfg.daemon_addr);

    // Build export bundle
    let export_bundle = serde_json::json!({
        "version": "1.0",
        "type": "infrasim_appliance_export",
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "appliance": {
            "id": instance.id,
            "name": instance.name,
            "template_id": instance.template_id,
            "created_at": instance.created_at,
            "status": instance.status,
        },
        "template": template,
        "vm": vm,
        "networks": networks,
        "volumes": volumes,
        "snapshots": snapshots,
        "terraform_hcl": terraform_hcl,
    });

    let detail = ApplianceDetail {
        instance: instance.clone(),
        template,
        vm,
        networks,
        volumes,
        snapshots,
        terraform_hcl,
        export_bundle,
    };

    (StatusCode::OK, Json(detail)).into_response()
}

/// Export an appliance to a JSON bundle for backup/restore.
async fn export_appliance_handler(
    State(state): State<Arc<WebServerState>>,
    Path(appliance_id): Path<String>,
) -> Response {
    let appliances = state.appliances.read().await;
    let Some(instance) = appliances.get(&appliance_id) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "appliance not found"}))).into_response();
    };

    let templates = builtin_appliance_templates();
    let template = templates.iter().find(|t| t.id == instance.template_id).cloned();

    // Fetch all associated resources
    let vm = if let Some(vm_id) = &instance.vm_id {
        state.daemon.get_vm(vm_id).await.ok()
    } else {
        None
    };

    let all_networks = state.daemon.list_networks().await.unwrap_or_default();
    let networks: Vec<_> = all_networks.into_iter()
        .filter(|n| instance.network_ids.contains(&n.id))
        .collect();

    let all_volumes = state.daemon.list_volumes().await.unwrap_or_default();
    let volumes: Vec<_> = all_volumes.into_iter()
        .filter(|v| instance.volume_ids.contains(&v.id))
        .collect();

    let all_snapshots = state.daemon.list_snapshots(instance.vm_id.as_deref()).await.unwrap_or_default();
    let snapshots: Vec<_> = all_snapshots.into_iter()
        .filter(|s| instance.snapshot_ids.contains(&s.id) || instance.vm_id.as_ref().map(|id| &s.vm_id == id).unwrap_or(false))
        .collect();

    let terraform_hcl = generate_appliance_terraform(&instance, template.as_ref(), &state.cfg.daemon_addr);

    // Sign the export bundle
    let key_pair = infrasim_common::crypto::KeyPair::generate();
    let bundle_data = serde_json::json!({
        "version": "1.0",
        "type": "infrasim_appliance_export",
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "appliance": instance,
        "template": template,
        "vm_spec": vm.as_ref().map(|v| serde_json::json!({
            "arch": v.arch,
            "machine": v.machine,
            "cpu_cores": v.cpu_cores,
            "memory_mb": v.memory_mb,
        })),
        "networks": networks,
        "volumes": volumes.iter().map(|v| serde_json::json!({
            "name": v.name,
            "kind": v.kind,
            "format": v.format,
            "size_bytes": v.size_bytes,
            "source": v.source,
            "digest": v.digest,
        })).collect::<Vec<_>>(),
        "snapshots": snapshots.iter().map(|s| serde_json::json!({
            "name": s.name,
            "include_memory": s.include_memory,
            "include_disk": s.include_disk,
            "digest": s.digest,
            "size_bytes": s.size_bytes,
        })).collect::<Vec<_>>(),
        "terraform_hcl": terraform_hcl,
    });

    let bundle_bytes = serde_json::to_vec(&bundle_data).unwrap_or_default();
    let signature = key_pair.sign(&bundle_bytes);

    (StatusCode::OK, Json(serde_json::json!({
        "bundle": bundle_data,
        "signature": hex::encode(&signature),
        "public_key": hex::encode(key_pair.public_key_bytes()),
    }))).into_response()
}

/// Import an appliance from an export bundle.
async fn import_appliance_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<ImportApplianceRequest>,
) -> Response {
    // Validate bundle structure
    let bundle = &req.bundle;
    let bundle_type = bundle.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if bundle_type != "infrasim_appliance_export" {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "invalid bundle type, expected 'infrasim_appliance_export'",
        }))).into_response();
    }

    let original_name = bundle.pointer("/appliance/name")
        .and_then(|v| v.as_str())
        .unwrap_or("imported");
    let template_id = bundle.pointer("/appliance/template_id")
        .and_then(|v| v.as_str())
        .unwrap_or("pi-like-aarch64-desktop");

    let new_name = req.new_name.unwrap_or_else(|| format!("{}-imported", original_name));
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    let instance = ApplianceInstance {
        id: id.clone(),
        name: new_name.clone(),
        template_id: template_id.to_string(),
        created_at: now,
        vm_id: None,
        status: "imported".to_string(),
        network_ids: vec![],
        volume_ids: vec![],
        console_id: None,
        snapshot_ids: vec![],
        updated_at: now,
    };

    let mut appliances = state.appliances.write().await;
    appliances.insert(id.clone(), instance.clone());

    (StatusCode::CREATED, Json(serde_json::json!({
        "appliance": instance,
        "imported_from": original_name,
        "note": "Appliance imported. Use POST /api/appliances/{id}/boot to launch.",
    }))).into_response()
}

/// Archive an appliance (backup to a persistent store).
async fn archive_appliance_handler(
    State(state): State<Arc<WebServerState>>,
    Path(appliance_id): Path<String>,
    Json(req): Json<ArchiveApplianceRequest>,
) -> Response {
    let appliances = state.appliances.read().await;
    let Some(instance) = appliances.get(&appliance_id) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "appliance not found"}))).into_response();
    };

    let templates = builtin_appliance_templates();
    let template = templates.iter().find(|t| t.id == instance.template_id).cloned();

    // Gather all resources for archive
    let vm = if let Some(vm_id) = &instance.vm_id {
        state.daemon.get_vm(vm_id).await.ok()
    } else {
        None
    };

    let all_volumes = state.daemon.list_volumes().await.unwrap_or_default();
    let volumes: Vec<_> = all_volumes.into_iter()
        .filter(|v| instance.volume_ids.contains(&v.id))
        .collect();

    let all_snapshots = state.daemon.list_snapshots(instance.vm_id.as_deref()).await.unwrap_or_default();
    let snapshots: Vec<_> = if req.include_all_snapshots {
        all_snapshots
    } else {
        all_snapshots.into_iter()
            .filter(|s| instance.snapshot_ids.contains(&s.id))
            .collect()
    };

    // Build archive manifest
    let archive_manifest = serde_json::json!({
        "version": "1.0",
        "type": "infrasim_appliance_archive",
        "format": req.format,
        "archived_at": chrono::Utc::now().to_rfc3339(),
        "appliance": instance,
        "template": template,
        "include_memory": req.include_memory,
        "vm": vm,
        "volumes": volumes.iter().map(|v| serde_json::json!({
            "id": v.id,
            "name": v.name,
            "local_path": v.local_path,
            "size_bytes": v.size_bytes,
            "digest": v.digest,
        })).collect::<Vec<_>>(),
        "snapshots": snapshots.iter().map(|s| serde_json::json!({
            "id": s.id,
            "name": s.name,
            "disk_snapshot_path": s.disk_snapshot_path,
            "memory_snapshot_path": if req.include_memory { &s.memory_snapshot_path } else { "" },
            "size_bytes": s.size_bytes,
            "digest": s.digest,
        })).collect::<Vec<_>>(),
    });

    // Sign the archive
    let key_pair = infrasim_common::crypto::KeyPair::generate();
    let manifest_bytes = serde_json::to_vec(&archive_manifest).unwrap_or_default();
    let signature = key_pair.sign(&manifest_bytes);

    // For JSON format, just return the manifest. For tar.gz/zip, we'd need to actually create the archive.
    // MVP: return JSON manifest with file paths that can be used to create the archive externally.
    (StatusCode::OK, Json(serde_json::json!({
        "archive_id": uuid::Uuid::new_v4().to_string(),
        "format": req.format,
        "manifest": archive_manifest,
        "signature": hex::encode(&signature),
        "public_key": hex::encode(key_pair.public_key_bytes()),
        "files_to_archive": volumes.iter().map(|v| &v.local_path).chain(
            snapshots.iter().map(|s| &s.disk_snapshot_path)
        ).filter(|p| !p.is_empty()).collect::<Vec<_>>(),
    }))).into_response()
}

/// Get attestation report for an appliance's VM.
async fn appliance_attestation_handler(
    State(state): State<Arc<WebServerState>>,
    Path(appliance_id): Path<String>,
) -> Response {
    let appliances = state.appliances.read().await;
    let Some(instance) = appliances.get(&appliance_id) else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "appliance not found"}))).into_response();
    };

    let Some(vm_id) = &instance.vm_id else {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "no VM associated with appliance"}))).into_response();
    };

    match state.daemon.get_attestation(vm_id).await {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

/// Generate Terraform HCL for an appliance.
fn generate_appliance_terraform(instance: &ApplianceInstance, template: Option<&ApplianceTemplate>, daemon_addr: &str) -> String {
    let mut hcl = String::new();
    
    let tpl_id = template.map(|t| t.id.as_str()).unwrap_or(&instance.template_id);
    hcl.push_str(&format!(r#"# Terraform for appliance: {} (template: {})
terraform {{
  required_providers {{
    infrasim = {{
      source  = "infrasim/infrasim"
      version = ">= 0.1.0"
    }}
  }}
}}

provider "infrasim" {{
  endpoint = "{}"
}}

"#, instance.name, tpl_id, daemon_addr));

    if let Some(tpl) = template {
        for net in &tpl.networks {
            hcl.push_str(&format!(r#"resource "infrasim_network" "{}-{}" {{
  name         = "{}-{}"
  mode         = "{}"
  cidr         = "{}"
  gateway      = "{}"
  dhcp_enabled = {}
}}

"#,
                instance.name, net.id,
                instance.name, net.id,
                net.mode,
                net.cidr.as_deref().unwrap_or(""),
                net.gateway.as_deref().unwrap_or(""),
                net.dhcp,
            ));
        }

        for vol in &tpl.volumes {
            hcl.push_str(&format!(r#"resource "infrasim_volume" "{}-{}" {{
  name      = "{}-{}"
  size_mb   = {}
  kind      = "{}"
  format    = "qcow2"
}}

"#,
                instance.name, vol.id,
                instance.name, vol.id,
                vol.size_mb,
                vol.kind,
            ));
        }

        hcl.push_str(&format!(r#"resource "infrasim_vm" "{}" {{
  name             = "{}"
  arch             = "{}"
  machine          = "{}"
  cpu_cores        = {}
  memory_mb        = {}
  compatibility_mode = {}

  network_ids = [{}]
  volume_ids  = [{}]
}}

"#,
            instance.name,
            instance.name,
            tpl.arch,
            tpl.machine,
            tpl.cpu_cores,
            tpl.memory_mb,
            tpl.compatibility_mode,
            tpl.networks.iter().map(|n| format!("infrasim_network.{}-{}.id", instance.name, n.id)).collect::<Vec<_>>().join(", "),
            tpl.volumes.iter().map(|v| format!("infrasim_volume.{}-{}.id", instance.name, v.id)).collect::<Vec<_>>().join(", "),
        ));

        hcl.push_str(&format!(r#"resource "infrasim_console" "{}-console" {{
  vm_id      = infrasim_vm.{}.id
  enable_vnc = true
  vnc_port   = 5900
  enable_web = true
  web_port   = 6080
}}
"#, instance.name, instance.name));
    }

    hcl
}

// ============================================================================
// AI / LangChain-style LLM Integration
// ============================================================================

/// LLM backend configuration (from environment or config).
/// Set INFRASIM_LLM_BACKEND to "ollama", "vllm", "openai", or "none".
fn llm_backend() -> LlmBackend {
    match std::env::var("INFRASIM_LLM_BACKEND").as_deref() {
        Ok("ollama") => LlmBackend::Ollama {
            base_url: std::env::var("INFRASIM_OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".to_string()),
            model: std::env::var("INFRASIM_OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".to_string()),
        },
        Ok("vllm") => LlmBackend::VLLM {
            base_url: std::env::var("INFRASIM_VLLM_URL").unwrap_or_else(|_| "http://localhost:8000".to_string()),
            model: std::env::var("INFRASIM_VLLM_MODEL").unwrap_or_else(|_| "default".to_string()),
        },
        Ok("openai") => LlmBackend::OpenAI {
            api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            model: std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
        },
        _ => LlmBackend::RuleBased,
    }
}

#[derive(Debug, Clone)]
enum LlmBackend {
    /// Use Ollama local LLM
    Ollama { base_url: String, model: String },
    /// Use vLLM server
    VLLM { base_url: String, model: String },
    /// Use OpenAI-compatible API
    OpenAI { api_key: String, model: String },
    /// Fall back to rule-based pattern matching
    RuleBased,
}

/// System prompt for infrastructure definition tasks.
const INFRA_SYSTEM_PROMPT: &str = r#"You are an infrastructure definition assistant for InfraSim. 
Given a user prompt, produce a JSON object with the following structure:
{
  "intent": "<action_type>",
  "appliance_template_id": "<template_id or null>",
  "networks": [{"id": "...", "mode": "user|vmnet_bridged", "cidr": "...", "gateway": "...", "dhcp": true}],
  "volumes": [{"id": "...", "size_mb": 1024, "mount_path": "/data", "kind": "disk"}],
  "tools": [{"name": "nginx", "version": "latest", "purpose": "..."}]
}
Available templates: pi-like-aarch64-desktop, keycloak-aarch64
Network modes: user (NAT), vmnet_bridged (bridge to host network)
Only output valid JSON."#;

/// Call an LLM backend (Ollama/vLLM/OpenAI) for infrastructure definition.
async fn call_llm_backend(backend: &LlmBackend, prompt: &str) -> Option<String> {
    let client = reqwest::Client::new();
    match backend {
        LlmBackend::Ollama { base_url, model } => {
            let url = format!("{}/api/generate", base_url);
            let body = serde_json::json!({
                "model": model,
                "prompt": format!("{}\n\nUser: {}", INFRA_SYSTEM_PROMPT, prompt),
                "stream": false,
                "format": "json",
            });
            match client.post(&url).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        return json.get("response").and_then(|v| v.as_str()).map(String::from);
                    }
                }
                Ok(resp) => warn!("Ollama returned status {}", resp.status()),
                Err(e) => warn!("Ollama request failed: {}", e),
            }
            None
        }
        LlmBackend::VLLM { base_url, model } => {
            let url = format!("{}/v1/chat/completions", base_url);
            let body = serde_json::json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": INFRA_SYSTEM_PROMPT},
                    {"role": "user", "content": prompt},
                ],
                "max_tokens": 1024,
            });
            match client.post(&url).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        return json.pointer("/choices/0/message/content")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                }
                Ok(resp) => warn!("vLLM returned status {}", resp.status()),
                Err(e) => warn!("vLLM request failed: {}", e),
            }
            None
        }
        LlmBackend::OpenAI { api_key, model } => {
            if api_key.is_empty() {
                return None;
            }
            let url = "https://api.openai.com/v1/chat/completions";
            let body = serde_json::json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": INFRA_SYSTEM_PROMPT},
                    {"role": "user", "content": prompt},
                ],
                "max_tokens": 1024,
                "response_format": {"type": "json_object"},
            });
            match client.post(url).bearer_auth(api_key).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        return json.pointer("/choices/0/message/content")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                }
                Ok(resp) => warn!("OpenAI returned status {}", resp.status()),
                Err(e) => warn!("OpenAI request failed: {}", e),
            }
            None
        }
        LlmBackend::RuleBased => None,
    }
}

/// Parse LLM JSON response into structured components.
fn parse_llm_response(json_str: &str) -> Option<(String, Option<String>, Vec<NetworkDef>, Vec<VolumeDef>, Vec<ToolDef>)> {
    let v: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let intent = v.get("intent").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    let template_id = v.get("appliance_template_id").and_then(|v| v.as_str()).map(String::from);
    
    let networks: Vec<NetworkDef> = v.get("networks")
        .and_then(|arr| serde_json::from_value(arr.clone()).ok())
        .unwrap_or_default();
    let volumes: Vec<VolumeDef> = v.get("volumes")
        .and_then(|arr| serde_json::from_value(arr.clone()).ok())
        .unwrap_or_default();
    let tools: Vec<ToolDef> = v.get("tools")
        .and_then(|arr| serde_json::from_value(arr.clone()).ok())
        .unwrap_or_default();
    
    Some((intent, template_id, networks, volumes, tools))
}

/// AI / LangChain-style prompt bridge handler.
async fn ai_define_handler(
    State(_state): State<Arc<WebServerState>>,
    Json(req): Json<AiDefineRequest>,
) -> Response {
    let backend = llm_backend();
    let prompt_lower = req.prompt.to_lowercase();
    
    // Try LLM backend first (if configured).
    if !matches!(backend, LlmBackend::RuleBased) {
        if let Some(llm_response) = call_llm_backend(&backend, &req.prompt).await {
            if let Some((intent, template_id, networks, volumes, tools)) = parse_llm_response(&llm_response) {
                let templates = builtin_appliance_templates();
                let appliance_template = template_id
                    .as_ref()
                    .and_then(|tid| templates.iter().find(|t| &t.id == tid))
                    .cloned();
                
                let terraform_hcl = generate_terraform_for_resources(&networks, &volumes, appliance_template.as_ref());
                
                let resp = AiDefineResponse {
                    intent,
                    appliance_template,
                    networks,
                    volumes,
                    tools,
                    terraform_hcl,
                    notes: format!("Generated via LLM backend ({:?}).", backend),
                };
                return (StatusCode::OK, Json(resp)).into_response();
            }
        }
    }

    // Fallback: rule-based pattern matching.
    let mut intent = "unknown".to_string();
    let mut appliance_template: Option<ApplianceTemplate> = None;
    let mut networks: Vec<NetworkDef> = vec![];
    let mut volumes: Vec<VolumeDef> = vec![];
    let mut tools: Vec<ToolDef> = vec![];
    let mut notes = String::new();

    // Keycloak / Identity patterns
    if prompt_lower.contains("keycloak") || prompt_lower.contains("identity") || prompt_lower.contains("sso") || prompt_lower.contains("oauth") || prompt_lower.contains("oidc") {
        intent = "create_keycloak_appliance".to_string();
        let templates = builtin_appliance_templates();
        if let Some(kc) = templates.iter().find(|t| t.id == "keycloak-aarch64") {
            appliance_template = Some(kc.clone());
            networks = kc.networks.clone();
            volumes = kc.volumes.clone();
            tools = kc.tools.clone();
        }
        notes = "Matched Keycloak appliance template from prompt.".to_string();
    }
    // Pi-like desktop patterns
    else if prompt_lower.contains("pi") || prompt_lower.contains("raspberry") || prompt_lower.contains("desktop") || prompt_lower.contains("kali") {
        intent = "create_pi_desktop".to_string();
        let templates = builtin_appliance_templates();
        if let Some(pi) = templates.iter().find(|t| t.id == "pi-like-aarch64-desktop") {
            appliance_template = Some(pi.clone());
            networks = pi.networks.clone();
            volumes = pi.volumes.clone();
            tools = pi.tools.clone();
        }
        notes = "Matched Pi-like desktop template from prompt.".to_string();
    }
    // Web server patterns
    else if prompt_lower.contains("nginx") || prompt_lower.contains("reverse proxy") || prompt_lower.contains("load balancer") {
        intent = "define_nginx_tool".to_string();
        tools.push(ToolDef { name: "nginx".to_string(), version: Some("latest".to_string()), purpose: "Reverse proxy / load balancer".to_string() });
        networks.push(NetworkDef { id: "web".to_string(), mode: "user".to_string(), cidr: Some("10.0.2.0/24".to_string()), gateway: Some("10.0.2.2".to_string()), dhcp: true });
        notes = "Inferred nginx tool + default network from prompt.".to_string();
    }
    else if prompt_lower.contains("apache") || prompt_lower.contains("httpd") || prompt_lower.contains("web server") {
        intent = "define_apache_tool".to_string();
        tools.push(ToolDef { name: "apache2".to_string(), version: Some("latest".to_string()), purpose: "Web server".to_string() });
        networks.push(NetworkDef { id: "web".to_string(), mode: "user".to_string(), cidr: Some("10.0.2.0/24".to_string()), gateway: Some("10.0.2.2".to_string()), dhcp: true });
        notes = "Inferred Apache tool + default network from prompt.".to_string();
    }
    // Database patterns
    else if prompt_lower.contains("postgres") || prompt_lower.contains("postgresql") || prompt_lower.contains("database") {
        intent = "define_postgres".to_string();
        tools.push(ToolDef { name: "postgresql".to_string(), version: Some("16".to_string()), purpose: "Relational database".to_string() });
        volumes.push(VolumeDef { id: "pgdata".to_string(), size_mb: 8192, mount_path: "/var/lib/postgresql/data".to_string(), kind: "disk".to_string() });
        notes = "Inferred PostgreSQL + persistent volume from prompt.".to_string();
    }
    else if prompt_lower.contains("redis") || prompt_lower.contains("cache") {
        intent = "define_redis".to_string();
        tools.push(ToolDef { name: "redis".to_string(), version: Some("7".to_string()), purpose: "In-memory cache / message broker".to_string() });
        notes = "Inferred Redis cache from prompt.".to_string();
    }
    // Storage patterns
    else if prompt_lower.contains("storage") || prompt_lower.contains("volume") || prompt_lower.contains("disk") || prompt_lower.contains("persistent") {
        intent = "define_storage".to_string();
        let size = if prompt_lower.contains("large") || prompt_lower.contains("big") { 16384 } else { 4096 };
        volumes.push(VolumeDef { id: "data".to_string(), size_mb: size, mount_path: "/data".to_string(), kind: "disk".to_string() });
        notes = format!("Inferred {}MB storage volume from prompt.", size);
    }
    // Network patterns
    else if prompt_lower.contains("network") || prompt_lower.contains("bridge") || prompt_lower.contains("nat") || prompt_lower.contains("vlan") {
        intent = "define_network".to_string();
        let mode = if prompt_lower.contains("bridge") { "vmnet_bridged" } else { "user" };
        let cidr = if prompt_lower.contains("192.168") { "192.168.1.0/24" } else { "10.0.2.0/24" };
        networks.push(NetworkDef { id: "net0".to_string(), mode: mode.to_string(), cidr: Some(cidr.to_string()), gateway: Some(cidr.replace(".0/24", ".1")), dhcp: true });
        notes = format!("Inferred {} network ({}) from prompt.", mode, cidr);
    }
    // Forwarder / proxy patterns
    else if prompt_lower.contains("forwarder") || prompt_lower.contains("haproxy") || prompt_lower.contains("envoy") {
        intent = "define_forwarder".to_string();
        let tool_name = if prompt_lower.contains("haproxy") { "haproxy" } else if prompt_lower.contains("envoy") { "envoy" } else { "haproxy" };
        tools.push(ToolDef { name: tool_name.to_string(), version: Some("latest".to_string()), purpose: "TCP/HTTP load balancer / forwarder".to_string() });
        notes = format!("Inferred {} forwarder from prompt.", tool_name);
    }
    // Container runtime patterns
    else if prompt_lower.contains("container") || prompt_lower.contains("docker") || prompt_lower.contains("podman") {
        intent = "define_container_runtime".to_string();
        let runtime = if prompt_lower.contains("podman") { "podman" } else { "docker" };
        tools.push(ToolDef { name: runtime.to_string(), version: Some("latest".to_string()), purpose: "Container runtime".to_string() });
        notes = format!("Inferred {} container runtime from prompt.", runtime);
    }
    else {
        notes = "Could not infer intent from prompt. Try: 'keycloak', 'pi desktop', 'nginx', 'postgres', 'storage', 'network', 'forwarder'.".to_string();
    }

    let terraform_hcl = generate_terraform_for_resources(&networks, &volumes, appliance_template.as_ref());

    let resp = AiDefineResponse {
        intent,
        appliance_template,
        networks,
        volumes,
        tools,
        terraform_hcl,
        notes,
    };

    (StatusCode::OK, Json(resp)).into_response()
}

/// Generate Terraform HCL for given network/volume/appliance resources.
fn generate_terraform_for_resources(
    networks: &[NetworkDef],
    volumes: &[VolumeDef],
    appliance: Option<&ApplianceTemplate>,
) -> String {
    let mut hcl = String::new();

    for net in networks {
        hcl.push_str(&format!(r#"resource "infrasim_network" "{}" {{
  name         = "{}"
  mode         = "{}"
  cidr         = "{}"
  gateway      = "{}"
  dhcp_enabled = {}
}}

"#,
            net.id, net.id, net.mode,
            net.cidr.as_deref().unwrap_or(""),
            net.gateway.as_deref().unwrap_or(""),
            net.dhcp,
        ));
    }

    for vol in volumes {
        hcl.push_str(&format!(r#"resource "infrasim_volume" "{}" {{
  name    = "{}"
  size_mb = {}
  kind    = "{}"
}}

"#,
            vol.id, vol.id, vol.size_mb, vol.kind,
        ));
    }

    if let Some(tpl) = appliance {
        hcl.push_str(&format!(r#"resource "infrasim_vm" "{}" {{
  name       = "{}"
  arch       = "{}"
  machine    = "{}"
  cpu_cores  = {}
  memory_mb  = {}
  image      = "{}"
}}

"#,
            tpl.id, tpl.id, tpl.arch, tpl.machine,
            tpl.cpu_cores, tpl.memory_mb,
            tpl.image.as_deref().unwrap_or(""),
        ));
    }

    hcl
}

async fn list_prompts_handler(
    State(state): State<Arc<WebServerState>>,
    Path(project_id): Path<String>,
) -> Response {
    let projects = state.projects.read().await;
    let Some(project) = projects.get(&project_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "project not found"})),
        )
            .into_response();
    };

    Json(serde_json::json!({"prompts": project.prompts})).into_response()
}

async fn create_prompt_handler(
    State(state): State<Arc<WebServerState>>,
    Path(project_id): Path<String>,
    Json(req): Json<CreatePromptRequest>,
) -> Response {
    if req.title.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "title must not be empty"})),
        )
            .into_response();
    }

    let mut projects = state.projects.write().await;
    let Some(project) = projects.get_mut(&project_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "project not found"})),
        )
            .into_response();
    };

    let prompt = Prompt {
        id: uuid::Uuid::new_v4().to_string(),
        title: req.title,
        body: req.body,
        created_at: chrono::Utc::now().timestamp(),
        llm_provider: req.llm_provider,
    };
    project.prompts.push(prompt.clone());

    (StatusCode::CREATED, Json(prompt)).into_response()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TerraformGenerateRequest {
    project_id: String,
    goal: String,
}

async fn terraform_generate_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<TerraformGenerateRequest>,
) -> Response {
    // MVP: deterministic scaffold; later this will call configured LLMs.
    let projects = state.projects.read().await;
    if !projects.contains_key(&req.project_id) {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "project not found"})),
        )
            .into_response();
    }

    let tf = format!(
        r#"# Generated by InfraSim Web UI

terraform {{
  required_providers {{
    infrasim = {{
      source  = \"registry.terraform.io/infrasim/infrasim\"
      version = \"~> 0.1\"
    }}
  }}
}}

provider \"infrasim\" {{
  daemon_address = \"{}\"
}}

# Goal:
# {}
"#,
        state.cfg.daemon_addr, req.goal
    );

    Json(serde_json::json!({"terraform": tf})).into_response()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TerraformAuditRequest {
    terraform: String,
}

async fn terraform_audit_handler(Json(req): Json<TerraformAuditRequest>) -> impl IntoResponse {
    // MVP static checks: secrets, remote-exec, local-exec, plain HTTP etc.
    let mut findings = Vec::new();
    let src = req.terraform;
    let lowered = src.to_lowercase();

    if lowered.contains("local-exec") {
        findings.push(serde_json::json!({
            "id": "TF-AUDIT-LOCAL-EXEC",
            "severity": "high",
            "message": "Uses local-exec provisioner; prefer immutable images and explicit artifacts.",
        }));
    }
    if lowered.contains("remote-exec") {
        findings.push(serde_json::json!({
            "id": "TF-AUDIT-REMOTE-EXEC",
            "severity": "high",
            "message": "Uses remote-exec provisioner; avoid imperative configuration in Terraform.",
        }));
    }
    if lowered.contains("http://") {
        findings.push(serde_json::json!({
            "id": "TF-AUDIT-PLAINTEXT-HTTP",
            "severity": "medium",
            "message": "Contains plaintext HTTP URL; prefer HTTPS or verified digests for downloads.",
        }));
    }
    if lowered.contains("private_key") || lowered.contains("-----begin") {
        findings.push(serde_json::json!({
            "id": "TF-AUDIT-EMBEDDED-KEY",
            "severity": "critical",
            "message": "Potential embedded private key material. Do not store secrets in Terraform configs.",
        }));
    }

    Json(serde_json::json!({"findings": findings}))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttestProjectRequest {
    project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProvenanceEvidenceRequest {
    /// Optional: bind evidence to an appliance instance.
    appliance_id: Option<String>,
    /// Optional: bind evidence to a project.
    project_id: Option<String>,
    /// Free-form purpose string (e.g. "snapshot", "launch", "baseline").
    purpose: Option<String>,
}

async fn attest_project_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<AttestProjectRequest>,
) -> Response {
    let projects = state.projects.read().await;
    let Some(project) = projects.get(&req.project_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "project not found"})),
        )
            .into_response();
    };

    let key_pair = KeyPair::generate();
    let payload = serde_json::json!({
        "project": project,
        "daemon_addr": state.cfg.daemon_addr,
        "captured_at": chrono::Utc::now().timestamp(),
    });
    let serialized = serde_json::to_vec(&payload).unwrap_or_default();
    let digest = infrasim_common::cas::ContentAddressedStore::hash(&serialized);
    let signature = key_pair.sign(digest.as_bytes());

    (StatusCode::OK, Json(serde_json::json!({
        "digest": format!("sha256:{}", digest),
        "signature": hex::encode(signature),
        "public_key": key_pair.public_key_hex(),
        "note": "MVP attestation for project metadata; wire into daemon attestation for VMs/volumes next.",
    })))
        .into_response()
}

async fn provenance_evidence_handler(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<ProvenanceEvidenceRequest>,
) -> Response {
    if req.appliance_id.is_none() && req.project_id.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "must provide appliance_id or project_id"})),
        )
            .into_response();
    }

    let appliance = if let Some(id) = &req.appliance_id {
        let appliances = state.appliances.read().await;
        match appliances.get(id).cloned() {
            Some(a) => Some(a),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": "appliance not found"})),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    let project = if let Some(id) = &req.project_id {
        let projects = state.projects.read().await;
        match projects.get(id).cloned() {
            Some(p) => Some(p),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": "project not found"})),
                )
                    .into_response();
            }
        }
    } else {
        None
    };

    // Evidence manifest deliberately avoids non-deterministic key ordering differences by using
    // serde_json canonicalization via a consistent struct->Value conversion.
    let manifest = serde_json::json!({
        "schema": "infrasim.web/evidence/v1",
        "captured_at": chrono::Utc::now().timestamp(),
        "daemon": {
            "addr": state.cfg.daemon_addr,
        },
        "purpose": req.purpose.unwrap_or_else(|| "unspecified".to_string()),
        "bindings": {
            "appliance": appliance,
            "project": project,
        },
    });

    let bytes = match serde_json::to_vec(&manifest) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("serialize manifest: {e}")})),
            )
                .into_response();
        }
    };

    let digest_hex = infrasim_common::cas::ContentAddressedStore::hash(&bytes);
    let digest = format!("sha256:{}", digest_hex);

    // For MVP we use an ephemeral signature key. Next step: use daemon signing key / TPM-backed key.
    let key_pair = KeyPair::generate();
    let sig = key_pair.sign(digest.as_bytes());

    (StatusCode::OK, Json(serde_json::json!({
        "digest": digest,
        "signature": hex::encode(sig),
        "public_key": key_pair.public_key_hex(),
        "manifest": manifest,
        "note": "MVP evidence bundle: signs manifest digest. Wire to daemon CAS + attestation provider next.",
    })))
        .into_response()
}

async fn list_vms_handler(
    State(state): State<Arc<WebServerState>>,
) -> impl IntoResponse {
    let targets = state.vnc_targets.read().await;
    let vms: Vec<_> = targets
        .iter()
        .map(|(id, (host, port))| {
            serde_json::json!({
                "id": id,
                "vnc_host": host,
                "vnc_port": port,
                "web_url": format!("/vnc.html?autoconnect=1&path=websockify/{}", id)
            })
        })
        .collect();

    Json(serde_json::json!({ "vms": vms }))
}

#[derive(Deserialize)]
struct VncQuery {
    token: Option<String>,
}

async fn vnc_info_handler(
    State(state): State<Arc<WebServerState>>,
    Path(vm_id): Path<String>,
) -> Response {
    let targets = state.vnc_targets.read().await;
    
    match targets.get(&vm_id) {
        Some((host, port)) => Json(serde_json::json!({
            "vm_id": vm_id,
            "vnc_host": host,
            "vnc_port": port,
            "websocket_path": format!("/websockify/{}", vm_id),
            "web_url": format!("/vnc.html?autoconnect=1&path=websockify/{}", vm_id)
        }))
        .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "VM not found" })),
        )
            .into_response(),
    }
}

async fn websocket_handler(
    State(state): State<Arc<WebServerState>>,
    Path(vm_id): Path<String>,
    Query(query): Query<VncQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    // Validate token if required
    // (MVP: optional token validation)

    let targets = state.vnc_targets.read().await;
    
    match targets.get(&vm_id).cloned() {
        Some((host, port)) => {
            ws.on_upgrade(move |socket| async move {
                if let Err(e) = handle_vnc_websocket(socket, host, port).await {
                    error!("VNC WebSocket error: {}", e);
                }
            })
        }
        None => (
            StatusCode::NOT_FOUND,
            "VM not found",
        )
            .into_response(),
    }
}

async fn handle_vnc_websocket(
    socket: WebSocket,
    vnc_host: String,
    vnc_port: u16,
) -> anyhow::Result<()> {
    debug!("VNC WebSocket connecting to {}:{}", vnc_host, vnc_port);

    let proxy = VncProxy::new(&vnc_host, vnc_port);
    proxy.bridge(socket).await?;

    Ok(())
}

async fn index_handler() -> impl IntoResponse {
    Html(include_str!("../static/index.html"))
}

async fn vnc_html_handler() -> impl IntoResponse {
    Html(VNC_HTML)
}

async fn vnc_lite_handler() -> impl IntoResponse {
    Html(VNC_LITE_HTML)
}

async fn pipeline_analyzer_handler() -> impl IntoResponse {
    Html(include_str!("../static/pipeline-analyzer.html"))
}

async fn static_handler(
    State(state): State<Arc<WebServerState>>,
    Path(path): Path<String>,
) -> Response {
    state.static_files.serve(&path).await
}

async fn ui_index_handler(State(state): State<Arc<WebServerState>>) -> Response {
    ui_serve_path(state, "index.html").await
}

async fn ui_static_handler(
    State(state): State<Arc<WebServerState>>,
    Path(path): Path<String>,
) -> Response {
    let rel = path.trim_start_matches('/');
    let res = ui_serve_path(state.clone(), rel).await;
    if res.status() != StatusCode::NOT_FOUND {
        return res;
    }
    // SPA fallback: unknown routes map to index.html
    ui_serve_path(state, "index.html").await
}

async fn ui_serve_path(state: Arc<WebServerState>, rel: &str) -> Response {
    let Some(dir) = state.ui_static.dir.as_ref() else {
        return (StatusCode::NOT_FOUND, "Console UI not configured").into_response();
    };

    let rel = rel.trim_start_matches('/');
    let requested = dir.join(rel);

    // Prevent path traversal: canonicalize and ensure the requested path stays within dir.
    let Ok(canon_dir) = dir.canonicalize() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Bad UI dir").into_response();
    };
    let Ok(canon_req) = requested.canonicalize() else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };
    if !canon_req.starts_with(&canon_dir) {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }

    match tokio::fs::read(&canon_req).await {
        Ok(bytes) => {
            let mime = if rel.ends_with(".html") {
                "text/html"
            } else if rel.ends_with(".js") {
                "application/javascript"
            } else if rel.ends_with(".css") {
                "text/css"
            } else if rel.ends_with(".svg") {
                "image/svg+xml"
            } else if rel.ends_with(".png") {
                "image/png"
            } else if rel.ends_with(".ico") {
                "image/x-icon"
            } else if rel.ends_with(".woff2") {
                "font/woff2"
            } else {
                "application/octet-stream"
            };
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, mime)],
                bytes,
            )
                .into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

// ============================================================================
// UI Manifest Handler
// ============================================================================

async fn ui_manifest_handler(State(state): State<Arc<WebServerState>>) -> impl IntoResponse {
    // Try to read ui.manifest.json from static directory
    if let Some(ref dir) = state.ui_static.dir {
        let manifest_path = dir.join("ui.manifest.json");
        if let Ok(content) = tokio::fs::read_to_string(&manifest_path).await {
            if let Ok(manifest) = serde_json::from_str::<UiManifest>(&content) {
                return Json(manifest).into_response();
            }
        }
    }
    
    // Return a default/dev manifest if not found
    let dev_manifest = UiManifest {
        schema_version: "1".to_string(),
        ui_version: "0.0.0-dev".to_string(),
        git_commit: "".to_string(),
        git_branch: "".to_string(),
        build_timestamp: chrono::Utc::now().to_rfc3339(),
        total_size_bytes: 0,
        asset_count: 0,
        api_schema_version: "1".to_string(),
        declared_resource_kinds: vec!["appliance".to_string(), "filesystem".to_string()],
        mount_point: "/ui/".to_string(),
        assets: vec![],
    };
    Json(dev_manifest).into_response()
}

// ============================================================================
// Filesystem Resource Handlers
// ============================================================================

async fn list_filesystems_handler(
    State(state): State<Arc<WebServerState>>,
) -> impl IntoResponse {
    let filesystems = state.filesystems.read().await;
    let list: Vec<&Filesystem> = filesystems.values().collect();
    Json(list).into_response()
}

async fn create_filesystem_handler(
    State(state): State<Arc<WebServerState>>,
    Json(mut fs): Json<Filesystem>,
) -> impl IntoResponse {
    // Generate ID if not provided
    if fs.id.is_empty() {
        fs.id = uuid::Uuid::new_v4().to_string();
    }
    fs.created_at = chrono::Utc::now().timestamp();
    fs.updated_at = fs.created_at;
    
    let mut filesystems = state.filesystems.write().await;
    let id = fs.id.clone();
    filesystems.insert(id.clone(), fs.clone());
    
    (StatusCode::CREATED, Json(fs)).into_response()
}

async fn get_filesystem_handler(
    State(state): State<Arc<WebServerState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let filesystems = state.filesystems.read().await;
    match filesystems.get(&id) {
        Some(fs) => Json(fs.clone()).into_response(),
        None => (StatusCode::NOT_FOUND, "Filesystem not found").into_response(),
    }
}

async fn update_filesystem_handler(
    State(state): State<Arc<WebServerState>>,
    Path(id): Path<String>,
    Json(mut fs): Json<Filesystem>,
) -> impl IntoResponse {
    let mut filesystems = state.filesystems.write().await;
    if !filesystems.contains_key(&id) {
        return (StatusCode::NOT_FOUND, "Filesystem not found").into_response();
    }
    
    fs.id = id.clone();
    fs.updated_at = chrono::Utc::now().timestamp();
    filesystems.insert(id, fs.clone());
    
    Json(fs).into_response()
}

async fn delete_filesystem_handler(
    State(state): State<Arc<WebServerState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut filesystems = state.filesystems.write().await;
    match filesystems.remove(&id) {
        Some(_) => StatusCode::NO_CONTENT.into_response(),
        None => (StatusCode::NOT_FOUND, "Filesystem not found").into_response(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FilesystemSnapshotRequest {
    name: String,
    description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FilesystemSnapshot {
    id: String,
    filesystem_id: String,
    name: String,
    description: Option<String>,
    created_at: String,
    size_bytes: u64,
    checksum: Option<String>,
}

async fn create_filesystem_snapshot_handler(
    State(state): State<Arc<WebServerState>>,
    Path(id): Path<String>,
    Json(req): Json<FilesystemSnapshotRequest>,
) -> impl IntoResponse {
    let filesystems = state.filesystems.read().await;
    if !filesystems.contains_key(&id) {
        return (StatusCode::NOT_FOUND, "Filesystem not found").into_response();
    }
    
    // Create snapshot record
    let snapshot = FilesystemSnapshot {
        id: uuid::Uuid::new_v4().to_string(),
        filesystem_id: id.clone(),
        name: req.name,
        description: req.description,
        created_at: chrono::Utc::now().to_rfc3339(),
        size_bytes: 0, // Would be calculated from actual filesystem
        checksum: None, // Would be computed from actual content
    };
    
    (StatusCode::CREATED, Json(snapshot)).into_response()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FilesystemAttachRequest {
    appliance_id: String,
    mount_point: String,
    #[serde(default)]
    read_only: bool,
}

async fn attach_filesystem_handler(
    State(state): State<Arc<WebServerState>>,
    Path(id): Path<String>,
    Json(req): Json<FilesystemAttachRequest>,
) -> impl IntoResponse {
    let mut filesystems = state.filesystems.write().await;
    let fs = match filesystems.get_mut(&id) {
        Some(fs) => fs,
        None => return (StatusCode::NOT_FOUND, "Filesystem not found").into_response(),
    };
    
    // Check if already attached to this appliance
    if fs.attached_to.contains(&req.appliance_id) {
        return (StatusCode::CONFLICT, "Already attached to this appliance").into_response();
    }
    
    fs.attached_to.push(req.appliance_id);
    fs.updated_at = chrono::Utc::now().timestamp();
    
    Json(fs.clone()).into_response()
}

async fn detach_filesystem_handler(
    State(state): State<Arc<WebServerState>>,
    Path((id, appliance_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let mut filesystems = state.filesystems.write().await;
    let fs = match filesystems.get_mut(&id) {
        Some(fs) => fs,
        None => return (StatusCode::NOT_FOUND, "Filesystem not found").into_response(),
    };
    
    fs.attached_to.retain(|a| a != &appliance_id);
    fs.updated_at = chrono::Utc::now().timestamp();
    
    Json(fs.clone()).into_response()
}

// ============================================================================
// Resource Graph Handlers
// ============================================================================

async fn get_resource_graph_handler(
    State(state): State<Arc<WebServerState>>,
) -> impl IntoResponse {
    // Build graph from current state
    let appliances = state.appliances.read().await;
    let filesystems = state.filesystems.read().await;
    
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    
    // Add appliance nodes
    for (id, appliance) in appliances.iter() {
        nodes.push(ResourceNode {
            id: id.clone(),
            node_type: "appliance".to_string(),
            name: appliance.name.clone(),
            data: serde_json::json!({
                "address": format!("infrasim_appliance.{}", appliance.name),
                "status": format!("{:?}", appliance.status).to_lowercase(),
                "template_id": appliance.template_id,
                "vm_id": appliance.vm_id,
            }),
            position: None,
        });
    }
    
    // Add filesystem nodes and edges
    for (id, fs) in filesystems.iter() {
        nodes.push(ResourceNode {
            id: id.clone(),
            node_type: "filesystem".to_string(),
            name: fs.name.clone(),
            data: serde_json::json!({
                "address": format!("infrasim_filesystem.{}", fs.name),
                "fs_type": fs.fs_type,
                "size_bytes": fs.size_bytes,
                "mount_path": fs.mount_path,
                "attached_to": fs.attached_to,
            }),
            position: None,
        });
        
        // Add edges for attachments
        for appliance_id in &fs.attached_to {
            edges.push(ResourceEdge {
                id: format!("{}-{}", id, appliance_id),
                source: id.clone(),
                target: appliance_id.clone(),
                edge_type: "attached_to".to_string(),
                data: serde_json::json!({}),
            });
        }
    }
    
    let graph = ResourceGraph {
        nodes,
        edges,
        version: "1".to_string(),
        computed_at: chrono::Utc::now().timestamp(),
    };
    
    Json(graph).into_response()
}

async fn plan_graph_changes_handler(
    State(_state): State<Arc<WebServerState>>,
    Json(_req): Json<PlanGraphRequest>,
) -> impl IntoResponse {
    // Simulate planning - in production this would validate and compute diffs
    let result = GraphPlanResult {
        adds: vec![],
        updates: vec![],
        deletes: vec![],
        warnings: vec![],
        valid: true,
    };
    
    Json(result).into_response()
}

async fn apply_graph_changes_handler(
    State(_state): State<Arc<WebServerState>>,
    Json(req): Json<ApplyGraphRequest>,
) -> impl IntoResponse {
    // Stub: accept the graph and return the planned result shape for now.
    // A future implementation would compute a plan (or use a plan id) and execute.
    let _ = req;
    let result = GraphPlanResult {
        adds: vec![],
        updates: vec![],
        deletes: vec![],
        warnings: vec!["apply is currently a no-op".to_string()],
        valid: true,
    };
    Json(result).into_response()
}

async fn validate_graph_handler(
    State(_state): State<Arc<WebServerState>>,
    Json(_req): Json<ValidateGraphRequest>,
) -> impl IntoResponse {
    // Stub: mark graphs as valid for now.
    // This is where we'd enforce invariants (e.g. attachment constraints, geobounds).
    (StatusCode::OK, Json(serde_json::json!({"valid": true, "errors": [], "warnings": []})))
        .into_response()
}


async fn not_found_handler() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Not found")
}

// ============================================================================
// Embedded noVNC HTML
// ============================================================================

const VNC_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>InfraSim Console</title>
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }
        html, body {
            height: 100%;
            background: #1a1a2e;
            overflow: hidden;
        }
        #container {
            display: flex;
            flex-direction: column;
            height: 100%;
        }
        #header {
            background: #16213e;
            color: #e94560;
            padding: 10px 20px;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }
        #header h1 {
            font-size: 1.2em;
            font-weight: normal;
        }
        #status {
            color: #4ecca3;
            font-size: 0.9em;
        }
        #vnc-container {
            flex: 1;
            display: flex;
            align-items: center;
            justify-content: center;
            position: relative;
        }
        #vnc-screen {
            max-width: 100%;
            max-height: 100%;
        }
        #connecting {
            color: #fff;
            font-size: 1.5em;
        }
        .controls {
            display: flex;
            gap: 10px;
        }
        .btn {
            background: #e94560;
            color: white;
            border: none;
            padding: 8px 16px;
            border-radius: 4px;
            cursor: pointer;
            font-size: 0.9em;
        }
        .btn:hover {
            background: #ff6b6b;
        }
        .btn.secondary {
            background: #0f3460;
        }
        .btn.secondary:hover {
            background: #16213e;
        }
    </style>
</head>
<body>
    <div id="container">
        <div id="header">
            <h1>🖥️ InfraSim Console</h1>
            <div class="controls">
                <button class="btn secondary" onclick="sendCtrlAltDel()">Ctrl+Alt+Del</button>
                <button class="btn secondary" onclick="toggleFullscreen()">Fullscreen</button>
                <button class="btn" onclick="disconnect()">Disconnect</button>
            </div>
            <div id="status">Connecting...</div>
        </div>
        <div id="vnc-container">
            <div id="connecting">Connecting to VM...</div>
            <canvas id="vnc-screen" style="display: none;"></canvas>
        </div>
    </div>

    <script type="module">
        // Minimal VNC client implementation
        // In production, use noVNC library from: https://github.com/novnc/noVNC

        const params = new URLSearchParams(window.location.search);
        const path = params.get('path') || 'websockify/default';
        const autoconnect = params.get('autoconnect') === '1';

        const statusEl = document.getElementById('status');
        const connectingEl = document.getElementById('connecting');
        const canvasEl = document.getElementById('vnc-screen');
        const ctx = canvasEl.getContext('2d');

        let ws = null;

        function updateStatus(msg, isError = false) {
            statusEl.textContent = msg;
            statusEl.style.color = isError ? '#e94560' : '#4ecca3';
        }

        function connect() {
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${protocol}//${window.location.host}/${path}`;
            
            updateStatus('Connecting...');
            
            ws = new WebSocket(wsUrl);
            ws.binaryType = 'arraybuffer';

            ws.onopen = () => {
                updateStatus('Connected');
                connectingEl.style.display = 'none';
                canvasEl.style.display = 'block';
                
                // Send RFB version
                ws.send(new TextEncoder().encode('RFB 003.008\n'));
            };

            ws.onmessage = (event) => {
                // Handle RFB protocol messages
                handleRfbMessage(event.data);
            };

            ws.onclose = () => {
                updateStatus('Disconnected', true);
                canvasEl.style.display = 'none';
                connectingEl.textContent = 'Disconnected. Click to reconnect.';
                connectingEl.style.display = 'block';
                connectingEl.style.cursor = 'pointer';
                connectingEl.onclick = connect;
            };

            ws.onerror = (err) => {
                console.error('WebSocket error:', err);
                updateStatus('Connection error', true);
            };
        }

        let rfbState = 'version';
        let framebufferWidth = 800;
        let framebufferHeight = 600;

        function handleRfbMessage(data) {
            const bytes = new Uint8Array(data);
            
            switch (rfbState) {
                case 'version':
                    // Server sent version, respond with security type
                    rfbState = 'security';
                    ws.send(new Uint8Array([1])); // No authentication
                    break;
                    
                case 'security':
                    // Security result
                    rfbState = 'init';
                    ws.send(new Uint8Array([1])); // Shared flag
                    break;
                    
                case 'init':
                    // ServerInit message
                    if (bytes.length >= 24) {
                        framebufferWidth = (bytes[0] << 8) | bytes[1];
                        framebufferHeight = (bytes[2] << 8) | bytes[3];
                        
                        canvasEl.width = framebufferWidth;
                        canvasEl.height = framebufferHeight;
                        
                        rfbState = 'normal';
                        updateStatus(`Connected (${framebufferWidth}x${framebufferHeight})`);
                        
                        // Request framebuffer update
                        requestFramebufferUpdate();
                    }
                    break;
                    
                case 'normal':
                    handleFramebufferUpdate(bytes);
                    break;
            }
        }

        function requestFramebufferUpdate() {
            if (ws && ws.readyState === WebSocket.OPEN) {
                const msg = new Uint8Array(10);
                msg[0] = 3; // FramebufferUpdateRequest
                msg[1] = 0; // Incremental
                // x, y, width, height (big-endian)
                msg[2] = 0; msg[3] = 0;
                msg[4] = 0; msg[5] = 0;
                msg[6] = framebufferWidth >> 8; msg[7] = framebufferWidth & 0xff;
                msg[8] = framebufferHeight >> 8; msg[9] = framebufferHeight & 0xff;
                ws.send(msg);
                
                // Request again after a delay
                setTimeout(requestFramebufferUpdate, 50);
            }
        }

        function handleFramebufferUpdate(bytes) {
            // Simplified - just show a placeholder
            // Full implementation would parse RFB encoding
            ctx.fillStyle = '#333';
            ctx.fillRect(0, 0, canvasEl.width, canvasEl.height);
            ctx.fillStyle = '#fff';
            ctx.font = '20px monospace';
            ctx.fillText('VNC Connected - Use noVNC for full support', 20, 30);
        }

        // Keyboard handling
        canvasEl.tabIndex = 0;
        canvasEl.addEventListener('keydown', (e) => {
            if (ws && ws.readyState === WebSocket.OPEN) {
                sendKeyEvent(e.keyCode, true);
                e.preventDefault();
            }
        });
        canvasEl.addEventListener('keyup', (e) => {
            if (ws && ws.readyState === WebSocket.OPEN) {
                sendKeyEvent(e.keyCode, false);
                e.preventDefault();
            }
        });

        function sendKeyEvent(key, down) {
            const msg = new Uint8Array(8);
            msg[0] = 4; // KeyEvent
            msg[1] = down ? 1 : 0;
            // Key code (simplified)
            msg[4] = 0; msg[5] = 0;
            msg[6] = (key >> 8) & 0xff;
            msg[7] = key & 0xff;
            ws.send(msg);
        }

        // Mouse handling
        canvasEl.addEventListener('mousedown', (e) => sendPointerEvent(e, 1));
        canvasEl.addEventListener('mouseup', (e) => sendPointerEvent(e, 0));
        canvasEl.addEventListener('mousemove', (e) => sendPointerEvent(e, -1));

        let lastButtonMask = 0;
        function sendPointerEvent(e, button) {
            if (!ws || ws.readyState !== WebSocket.OPEN) return;
            
            const rect = canvasEl.getBoundingClientRect();
            const x = Math.floor((e.clientX - rect.left) * (canvasEl.width / rect.width));
            const y = Math.floor((e.clientY - rect.top) * (canvasEl.height / rect.height));
            
            if (button >= 0) {
                lastButtonMask = button;
            }
            
            const msg = new Uint8Array(6);
            msg[0] = 5; // PointerEvent
            msg[1] = lastButtonMask;
            msg[2] = (x >> 8) & 0xff; msg[3] = x & 0xff;
            msg[4] = (y >> 8) & 0xff; msg[5] = y & 0xff;
            ws.send(msg);
        }

        // Global functions
        window.sendCtrlAltDel = function() {
            // Send Ctrl+Alt+Del sequence
            sendKeyEvent(0xffe3, true);  // Ctrl
            sendKeyEvent(0xffe9, true);  // Alt
            sendKeyEvent(0xffff, true);  // Delete
            sendKeyEvent(0xffff, false);
            sendKeyEvent(0xffe9, false);
            sendKeyEvent(0xffe3, false);
        };

        window.toggleFullscreen = function() {
            if (!document.fullscreenElement) {
                document.documentElement.requestFullscreen();
            } else {
                document.exitFullscreen();
            }
        };

        window.disconnect = function() {
            if (ws) {
                ws.close();
            }
        };

        // Auto-connect
        if (autoconnect) {
            connect();
        } else {
            connectingEl.textContent = 'Click to connect';
            connectingEl.style.cursor = 'pointer';
            connectingEl.onclick = connect;
        }
    </script>
</body>
</html>
"#;

const VNC_LITE_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <title>InfraSim Console (Lite)</title>
    <style>
        body { margin: 0; background: #000; }
        #screen { width: 100vw; height: 100vh; object-fit: contain; }
    </style>
</head>
<body>
    <canvas id="screen"></canvas>
    <script>
        // Minimal VNC - see vnc.html for full version
        const params = new URLSearchParams(location.search);
        const path = params.get('path') || 'websockify/default';
        const ws = new WebSocket(`ws://${location.host}/${path}`);
        ws.binaryType = 'arraybuffer';
        ws.onopen = () => ws.send(new TextEncoder().encode('RFB 003.008\n'));
    </script>
</body>
</html>
"#;
