//! Snapshot Browser Module
//!
//! Provides web endpoints for browsing, managing, and pinning snapshots:
//! - List and filter snapshots with metadata
//! - View provenance information from Git LFS
//! - Memory pinning for fast access
//! - Snapshot comparison and diff
//! - Git LFS integration for large file tracking

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ============================================================================
// Types
// ============================================================================

/// Snapshot with full provenance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotWithProvenance {
    /// Core snapshot info
    pub id: String,
    pub name: String,
    pub vm_id: String,
    pub created_at: u64,
    pub size_bytes: u64,

    /// Disk snapshot path
    pub disk_path: Option<String>,
    /// Memory snapshot path (if include_memory was true)
    pub memory_path: Option<String>,

    /// Provenance information
    pub provenance: SnapshotProvenance,

    /// Memory pinning status
    pub pin_status: PinStatus,

    /// Associated labels/tags
    pub labels: HashMap<String, String>,
}

/// Provenance information for a snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotProvenance {
    /// SHA256 digest of the snapshot content
    pub digest: String,
    /// Git LFS OID (if tracked)
    pub lfs_oid: Option<String>,
    /// Git commit that created/registered this snapshot
    pub git_commit: Option<String>,
    /// Git branch
    pub git_branch: Option<String>,
    /// Signature over the digest
    pub signature: Option<String>,
    /// Public key used for signing
    pub signer_key: Option<String>,
    /// Chain of custody (ordered list of handlers)
    pub custody_chain: Vec<CustodyEntry>,
    /// Build pipeline run ID
    pub build_run_id: Option<String>,
    /// CI workflow reference
    pub ci_workflow: Option<String>,
}

/// Entry in the chain of custody
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustodyEntry {
    pub timestamp: u64,
    pub actor: String,
    pub action: String,
    pub location: Option<String>,
    pub signature: Option<String>,
}

/// Memory pinning status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinStatus {
    /// Whether the snapshot is currently pinned in memory
    pub pinned: bool,
    /// When the pin was created
    pub pinned_at: Option<u64>,
    /// How long to keep pinned (0 = indefinite)
    pub ttl_seconds: u64,
    /// Priority level (higher = less likely to be evicted)
    pub priority: u32,
    /// Actual memory usage of pinned data
    pub pinned_bytes: u64,
}

impl Default for PinStatus {
    fn default() -> Self {
        Self {
            pinned: false,
            pinned_at: None,
            ttl_seconds: 0,
            priority: 0,
            pinned_bytes: 0,
        }
    }
}

/// Snapshot comparison result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDiff {
    pub snapshot_a: String,
    pub snapshot_b: String,
    pub disk_diff: DiskDiff,
    pub memory_diff: Option<MemoryDiff>,
    pub time_diff_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskDiff {
    /// Size difference in bytes
    pub size_diff_bytes: i64,
    /// Number of changed blocks (if qemu-img compare was run)
    pub changed_blocks: Option<u64>,
    /// Percentage of disk that changed
    pub change_percentage: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDiff {
    /// Size difference in bytes
    pub size_diff_bytes: i64,
    /// Estimated page changes
    pub changed_pages: Option<u64>,
}

/// Git LFS file info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfsFileInfo {
    pub path: String,
    pub oid: String,
    pub size: u64,
    pub tracked: bool,
    pub fetched: bool,
}

// ============================================================================
// State
// ============================================================================

/// Snapshot browser state
pub struct SnapshotBrowserState {
    /// Pinned snapshots (id -> pin data)
    pub pinned: RwLock<HashMap<String, PinnedSnapshot>>,
    /// LFS tracking cache
    pub lfs_cache: RwLock<HashMap<String, LfsFileInfo>>,
    /// Maximum pinned memory (bytes)
    pub max_pinned_bytes: u64,
    /// Current pinned bytes
    pub current_pinned_bytes: RwLock<u64>,
    /// Store path for snapshots
    pub store_path: PathBuf,
}

pub struct PinnedSnapshot {
    pub id: String,
    pub data: Option<Vec<u8>>,
    pub size: u64,
    pub pinned_at: u64,
    pub priority: u32,
    pub ttl_seconds: u64,
}

impl Default for SnapshotBrowserState {
    fn default() -> Self {
        Self {
            pinned: RwLock::new(HashMap::new()),
            lfs_cache: RwLock::new(HashMap::new()),
            max_pinned_bytes: 4 * 1024 * 1024 * 1024, // 4GB default
            current_pinned_bytes: RwLock::new(0),
            store_path: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(".infrasim/snapshots"),
        }
    }
}

mod dirs {
    pub fn home_dir() -> Option<std::path::PathBuf> {
        std::env::var_os("HOME").map(std::path::PathBuf::from)
    }
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListSnapshotsQuery {
    /// Filter by VM ID
    #[serde(default)]
    pub vm_id: Option<String>,
    /// Filter by label key=value
    #[serde(default)]
    pub label: Option<String>,
    /// Only show pinned snapshots
    #[serde(default)]
    pub pinned_only: bool,
    /// Include provenance details
    #[serde(default = "default_true")]
    pub include_provenance: bool,
    /// Limit results
    #[serde(default)]
    pub limit: Option<usize>,
    /// Offset for pagination
    #[serde(default)]
    pub offset: Option<usize>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct ListSnapshotsResponse {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub snapshots: Vec<SnapshotWithProvenance>,
}

#[derive(Debug, Deserialize)]
pub struct PinSnapshotRequest {
    /// TTL in seconds (0 = indefinite)
    #[serde(default)]
    pub ttl_seconds: u64,
    /// Priority level
    #[serde(default = "default_priority")]
    pub priority: u32,
    /// Whether to preload data into memory
    #[serde(default)]
    pub preload: bool,
}

fn default_priority() -> u32 {
    50
}

#[derive(Debug, Serialize)]
pub struct PinSnapshotResponse {
    pub success: bool,
    pub snapshot_id: String,
    pub pin_status: PinStatus,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CompareSnapshotsRequest {
    pub snapshot_a: String,
    pub snapshot_b: String,
    #[serde(default)]
    pub include_memory: bool,
}

#[derive(Debug, Serialize)]
pub struct CompareSnapshotsResponse {
    pub success: bool,
    pub diff: Option<SnapshotDiff>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LfsTrackRequest {
    pub snapshot_id: String,
    #[serde(default)]
    pub push: bool,
}

#[derive(Debug, Serialize)]
pub struct LfsTrackResponse {
    pub success: bool,
    pub lfs_info: Option<LfsFileInfo>,
    pub error: Option<String>,
}

// ============================================================================
// Git LFS Integration
// ============================================================================

/// Get Git LFS status for a file
fn get_lfs_status(path: &PathBuf) -> Option<LfsFileInfo> {
    let output = Command::new("git")
        .args(["lfs", "ls-files", "--long", path.to_str()?])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // Format: "oid - path" or "oid * path" (* = fetched)
        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        if parts.len() >= 3 {
            let oid = parts[0].to_string();
            let fetched = parts[1] == "*";
            let file_path = parts[2].to_string();

            let size = std::fs::metadata(path).ok().map(|m| m.len()).unwrap_or(0);

            return Some(LfsFileInfo {
                path: file_path,
                oid,
                size,
                tracked: true,
                fetched,
            });
        }
    }

    None
}

/// Track a file with Git LFS
fn lfs_track_file(path: &PathBuf) -> Result<String, String> {
    // Add to LFS tracking
    let output = Command::new("git")
        .args(["lfs", "track", path.to_str().ok_or("Invalid path")?])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    // Get the OID
    let output = Command::new("git")
        .args(["lfs", "ls-files", "--long", path.to_str().unwrap()])
        .output()
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let oid = stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .map(|s| s.to_string())
        .unwrap_or_default();

    Ok(oid)
}

/// Get Git commit info
fn get_git_info() -> (Option<String>, Option<String>) {
    let commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    (commit, branch)
}

// ============================================================================
// Provenance Helpers
// ============================================================================

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn compute_file_digest(path: &PathBuf) -> Option<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer).ok()?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Some(hex::encode(hasher.finalize()))
}

// ============================================================================
// Handlers
// ============================================================================

/// List snapshots with provenance
pub async fn list_snapshots_handler(
    State(state): State<Arc<SnapshotBrowserState>>,
    Query(params): Query<ListSnapshotsQuery>,
) -> impl IntoResponse {
    // In production, this would query the daemon
    // For now, scan the store path
    let mut snapshots = Vec::new();

    if state.store_path.exists() {
        if let Ok(entries) = std::fs::read_dir(&state.store_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "qcow2").unwrap_or(false) {
                    let id = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();

                    let metadata = std::fs::metadata(&path).ok();
                    let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                    let created = metadata
                        .and_then(|m| m.created().ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);

                    // Check pin status
                    let pinned = state.pinned.read().await;
                    let pin_status = pinned
                        .get(&id)
                        .map(|p| PinStatus {
                            pinned: true,
                            pinned_at: Some(p.pinned_at),
                            ttl_seconds: p.ttl_seconds,
                            priority: p.priority,
                            pinned_bytes: p.size,
                        })
                        .unwrap_or_default();

                    // Build provenance
                    let digest = if params.include_provenance {
                        compute_file_digest(&path)
                    } else {
                        None
                    };

                    let lfs_info = if params.include_provenance {
                        get_lfs_status(&path)
                    } else {
                        None
                    };

                    let (git_commit, git_branch) = if params.include_provenance {
                        get_git_info()
                    } else {
                        (None, None)
                    };

                    let provenance = SnapshotProvenance {
                        digest: digest.unwrap_or_default(),
                        lfs_oid: lfs_info.as_ref().map(|l| l.oid.clone()),
                        git_commit,
                        git_branch,
                        signature: None,
                        signer_key: None,
                        custody_chain: Vec::new(),
                        build_run_id: None,
                        ci_workflow: None,
                    };

                    let snapshot = SnapshotWithProvenance {
                        id: id.clone(),
                        name: id,
                        vm_id: String::new(),
                        created_at: created,
                        size_bytes: size,
                        disk_path: Some(path.to_string_lossy().to_string()),
                        memory_path: None,
                        provenance,
                        pin_status,
                        labels: HashMap::new(),
                    };

                    // Apply filters
                    if params.pinned_only && !snapshot.pin_status.pinned {
                        continue;
                    }

                    if let Some(ref vm_filter) = params.vm_id {
                        if &snapshot.vm_id != vm_filter {
                            continue;
                        }
                    }

                    snapshots.push(snapshot);
                }
            }
        }
    }

    // Sort by creation time (newest first)
    snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let total = snapshots.len();
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(50).min(200);

    let paged: Vec<_> = snapshots.into_iter().skip(offset).take(limit).collect();

    (
        StatusCode::OK,
        Json(ListSnapshotsResponse {
            total,
            offset,
            limit,
            snapshots: paged,
        }),
    )
        .into_response()
}

/// Get a single snapshot with full provenance
pub async fn get_snapshot_handler(
    State(state): State<Arc<SnapshotBrowserState>>,
    Path(snapshot_id): Path<String>,
) -> impl IntoResponse {
    let path = state.store_path.join(format!("{}.qcow2", snapshot_id));

    if !path.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Snapshot not found" })),
        )
            .into_response();
    }

    let metadata = std::fs::metadata(&path).ok();
    let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
    let created = metadata
        .and_then(|m| m.created().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let pinned = state.pinned.read().await;
    let pin_status = pinned
        .get(&snapshot_id)
        .map(|p| PinStatus {
            pinned: true,
            pinned_at: Some(p.pinned_at),
            ttl_seconds: p.ttl_seconds,
            priority: p.priority,
            pinned_bytes: p.size,
        })
        .unwrap_or_default();

    let digest = compute_file_digest(&path);
    let lfs_info = get_lfs_status(&path);
    let (git_commit, git_branch) = get_git_info();

    let provenance = SnapshotProvenance {
        digest: digest.unwrap_or_default(),
        lfs_oid: lfs_info.as_ref().map(|l| l.oid.clone()),
        git_commit,
        git_branch,
        signature: None,
        signer_key: None,
        custody_chain: Vec::new(),
        build_run_id: std::env::var("GITHUB_RUN_ID").ok(),
        ci_workflow: std::env::var("GITHUB_WORKFLOW").ok(),
    };

    let snapshot = SnapshotWithProvenance {
        id: snapshot_id.clone(),
        name: snapshot_id,
        vm_id: String::new(),
        created_at: created,
        size_bytes: size,
        disk_path: Some(path.to_string_lossy().to_string()),
        memory_path: None,
        provenance,
        pin_status,
        labels: HashMap::new(),
    };

    (StatusCode::OK, Json(snapshot)).into_response()
}

/// Pin a snapshot in memory
pub async fn pin_snapshot_handler(
    State(state): State<Arc<SnapshotBrowserState>>,
    Path(snapshot_id): Path<String>,
    Json(req): Json<PinSnapshotRequest>,
) -> impl IntoResponse {
    let path = state.store_path.join(format!("{}.qcow2", snapshot_id));

    if !path.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(PinSnapshotResponse {
                success: false,
                snapshot_id,
                pin_status: PinStatus::default(),
                error: Some("Snapshot not found".to_string()),
            }),
        )
            .into_response();
    }

    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    // Check memory limits
    {
        let current = *state.current_pinned_bytes.read().await;
        if current + size > state.max_pinned_bytes {
            return (
                StatusCode::INSUFFICIENT_STORAGE,
                Json(PinSnapshotResponse {
                    success: false,
                    snapshot_id,
                    pin_status: PinStatus::default(),
                    error: Some(format!(
                        "Insufficient memory: need {} bytes, {} available",
                        size,
                        state.max_pinned_bytes - current
                    )),
                }),
            )
                .into_response();
        }
    }

    // Preload data if requested
    let data = if req.preload {
        tokio::fs::read(&path).await.ok()
    } else {
        None
    };

    let pinned_at = now_epoch();
    let pinned_bytes = data.as_ref().map(|d| d.len() as u64).unwrap_or(0);

    let pin = PinnedSnapshot {
        id: snapshot_id.clone(),
        data,
        size,
        pinned_at,
        priority: req.priority,
        ttl_seconds: req.ttl_seconds,
    };

    // Update state
    {
        let mut pinned = state.pinned.write().await;
        pinned.insert(snapshot_id.clone(), pin);
    }
    {
        let mut current = state.current_pinned_bytes.write().await;
        *current += pinned_bytes;
    }

    let pin_status = PinStatus {
        pinned: true,
        pinned_at: Some(pinned_at),
        ttl_seconds: req.ttl_seconds,
        priority: req.priority,
        pinned_bytes,
    };

    (
        StatusCode::OK,
        Json(PinSnapshotResponse {
            success: true,
            snapshot_id,
            pin_status,
            error: None,
        }),
    )
        .into_response()
}

/// Unpin a snapshot
pub async fn unpin_snapshot_handler(
    State(state): State<Arc<SnapshotBrowserState>>,
    Path(snapshot_id): Path<String>,
) -> impl IntoResponse {
    let mut pinned = state.pinned.write().await;

    if let Some(pin) = pinned.remove(&snapshot_id) {
        let freed = pin.data.as_ref().map(|d| d.len() as u64).unwrap_or(0);
        {
            let mut current = state.current_pinned_bytes.write().await;
            *current = current.saturating_sub(freed);
        }

        (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "snapshot_id": snapshot_id,
                "freed_bytes": freed
            })),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "success": false,
                "error": "Snapshot was not pinned"
            })),
        )
            .into_response()
    }
}

/// Compare two snapshots
pub async fn compare_snapshots_handler(
    State(state): State<Arc<SnapshotBrowserState>>,
    Json(req): Json<CompareSnapshotsRequest>,
) -> impl IntoResponse {
    let path_a = state.store_path.join(format!("{}.qcow2", req.snapshot_a));
    let path_b = state.store_path.join(format!("{}.qcow2", req.snapshot_b));

    if !path_a.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(CompareSnapshotsResponse {
                success: false,
                diff: None,
                error: Some(format!("Snapshot A not found: {}", req.snapshot_a)),
            }),
        )
            .into_response();
    }

    if !path_b.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(CompareSnapshotsResponse {
                success: false,
                diff: None,
                error: Some(format!("Snapshot B not found: {}", req.snapshot_b)),
            }),
        )
            .into_response();
    }

    let size_a = std::fs::metadata(&path_a).map(|m| m.len()).unwrap_or(0);
    let size_b = std::fs::metadata(&path_b).map(|m| m.len()).unwrap_or(0);

    let time_a = std::fs::metadata(&path_a)
        .ok()
        .and_then(|m| m.created().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let time_b = std::fs::metadata(&path_b)
        .ok()
        .and_then(|m| m.created().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Try to use qemu-img compare for detailed diff
    let (changed_blocks, change_percentage) = tokio::task::spawn_blocking(move || {
        let output = Command::new("qemu-img")
            .args(["compare", "-f", "qcow2", "-F", "qcow2"])
            .arg(&path_a)
            .arg(&path_b)
            .output();

        match output {
            Ok(o) if o.status.success() => {
                // Images are identical
                (Some(0u64), Some(0.0f64))
            }
            Ok(o) => {
                // Parse the output for differences
                let stderr = String::from_utf8_lossy(&o.stderr);
                // qemu-img compare outputs differences to stderr
                // Format varies, so we estimate
                let lines = stderr.lines().count();
                (Some(lines as u64), None)
            }
            Err(_) => (None, None),
        }
    })
    .await
    .unwrap_or((None, None));

    let diff = SnapshotDiff {
        snapshot_a: req.snapshot_a.clone(),
        snapshot_b: req.snapshot_b.clone(),
        disk_diff: DiskDiff {
            size_diff_bytes: size_b as i64 - size_a as i64,
            changed_blocks,
            change_percentage,
        },
        memory_diff: None, // Would need memory snapshot paths
        time_diff_seconds: time_b - time_a,
    };

    (
        StatusCode::OK,
        Json(CompareSnapshotsResponse {
            success: true,
            diff: Some(diff),
            error: None,
        }),
    )
        .into_response()
}

/// Track a snapshot with Git LFS
pub async fn lfs_track_handler(
    State(state): State<Arc<SnapshotBrowserState>>,
    Json(req): Json<LfsTrackRequest>,
) -> impl IntoResponse {
    let path = state.store_path.join(format!("{}.qcow2", req.snapshot_id));

    if !path.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(LfsTrackResponse {
                success: false,
                lfs_info: None,
                error: Some("Snapshot not found".to_string()),
            }),
        )
            .into_response();
    }

    // Run LFS tracking in blocking task
    let path_clone = path.clone();
    let track_result = tokio::task::spawn_blocking(move || lfs_track_file(&path_clone)).await;

    match track_result {
        Ok(Ok(oid)) => {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

            let lfs_info = LfsFileInfo {
                path: path.to_string_lossy().to_string(),
                oid,
                size,
                tracked: true,
                fetched: true,
            };

            // Cache the LFS info
            {
                let mut cache = state.lfs_cache.write().await;
                cache.insert(req.snapshot_id.clone(), lfs_info.clone());
            }

            (
                StatusCode::OK,
                Json(LfsTrackResponse {
                    success: true,
                    lfs_info: Some(lfs_info),
                    error: None,
                }),
            )
                .into_response()
        }
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(LfsTrackResponse {
                success: false,
                lfs_info: None,
                error: Some(e),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(LfsTrackResponse {
                success: false,
                lfs_info: None,
                error: Some(e.to_string()),
            }),
        )
            .into_response(),
    }
}

/// Get pinning statistics
pub async fn get_pin_stats_handler(
    State(state): State<Arc<SnapshotBrowserState>>,
) -> impl IntoResponse {
    let pinned = state.pinned.read().await;
    let current = *state.current_pinned_bytes.read().await;

    let total_pinned = pinned.len();
    let preloaded_count = pinned.values().filter(|p| p.data.is_some()).count();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "total_pinned": total_pinned,
            "preloaded_count": preloaded_count,
            "current_pinned_bytes": current,
            "max_pinned_bytes": state.max_pinned_bytes,
            "available_bytes": state.max_pinned_bytes - current,
            "utilization_percent": (current as f64 / state.max_pinned_bytes as f64) * 100.0
        })),
    )
        .into_response()
}

// ============================================================================
// Routes
// ============================================================================

/// Build the snapshot browser routes
pub fn snapshot_browser_routes(state: Arc<SnapshotBrowserState>) -> Router {
    Router::new()
        .route("/", get(list_snapshots_handler))
        .route("/:snapshot_id", get(get_snapshot_handler))
        .route("/:snapshot_id/pin", post(pin_snapshot_handler))
        .route("/:snapshot_id/unpin", post(unpin_snapshot_handler))
        .route("/compare", post(compare_snapshots_handler))
        .route("/lfs/track", post(lfs_track_handler))
        .route("/stats/pins", get(get_pin_stats_handler))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_pin_status() {
        let status = PinStatus::default();
        assert!(!status.pinned);
        assert_eq!(status.priority, 0);
    }

    #[test]
    fn test_now_epoch() {
        let now = now_epoch();
        assert!(now > 1700000000); // After 2023
    }
}
