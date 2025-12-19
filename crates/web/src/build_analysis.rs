//! Build Pipeline Analysis Web Handlers
//!
//! Provides HTTP endpoints for:
//! - Dependency graph visualization
//! - Vendor convergence pattern detection
//! - ICMP timing probes for BGP path inference
//! - Build pipeline static analysis

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use infrasim_common::pipeline::{
    AnalysisReport, DependencyGraph, NetworkFingerprint, PipelineAnalyzer, TimingProbe,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Cached analysis results
pub struct AnalysisCache {
    /// Last full analysis report
    pub last_analysis: RwLock<Option<CachedAnalysis>>,
    /// ICMP probes history
    pub timing_history: RwLock<Vec<NetworkFingerprint>>,
    /// Max timing history entries
    pub max_history: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CachedAnalysis {
    pub report: AnalysisReport,
    pub workspace_path: String,
    pub analyzed_at: u64,
}

impl Default for AnalysisCache {
    fn default() -> Self {
        Self {
            last_analysis: RwLock::new(None),
            timing_history: RwLock::new(Vec::new()),
            max_history: 100,
        }
    }
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AnalyzeWorkspaceRequest {
    /// Path to the workspace to analyze
    pub workspace_path: String,
    /// Whether to run ICMP timing probes
    #[serde(default)]
    pub include_timing: bool,
}

#[derive(Debug, Serialize)]
pub struct AnalyzeWorkspaceResponse {
    pub success: bool,
    pub report: Option<AnalysisReport>,
    pub timing: Option<NetworkFingerprint>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TimingProbeRequest {
    /// Custom hosts to probe (optional, uses defaults if empty)
    #[serde(default)]
    pub custom_hosts: Vec<CustomProbeHost>,
    /// Include default time servers
    #[serde(default = "default_true")]
    pub include_defaults: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct CustomProbeHost {
    pub host: String,
    pub label: String,
    #[serde(default)]
    pub region: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TimingProbeResponse {
    pub fingerprint: NetworkFingerprint,
    pub summary: TimingSummary,
}

#[derive(Debug, Serialize)]
pub struct TimingSummary {
    pub total_probes: usize,
    pub successful_probes: usize,
    pub average_rtt_ms: Option<f64>,
    pub min_rtt_ms: Option<f64>,
    pub max_rtt_ms: Option<f64>,
    pub inferred_region: Option<String>,
    pub anomaly_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct GraphQueryParams {
    /// Filter by node type
    #[serde(default)]
    pub node_type: Option<String>,
    /// Maximum depth from root
    #[serde(default)]
    pub max_depth: Option<usize>,
    /// Include dev dependencies
    #[serde(default = "default_true")]
    pub include_dev: bool,
    /// Include build dependencies
    #[serde(default = "default_true")]
    pub include_build: bool,
}

/// D3.js compatible graph format
#[derive(Debug, Serialize)]
pub struct D3Graph {
    pub nodes: Vec<D3Node>,
    pub links: Vec<D3Link>,
    pub metadata: D3Metadata,
}

#[derive(Debug, Serialize)]
pub struct D3Node {
    pub id: String,
    pub name: String,
    pub group: String,
    pub version: Option<String>,
    pub source_type: String,
    pub radius: f64,
    pub color: String,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct D3Link {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub optional: bool,
    pub strength: f64,
}

#[derive(Debug, Serialize)]
pub struct D3Metadata {
    pub total_nodes: usize,
    pub total_links: usize,
    pub max_depth: usize,
    pub risk_score: f64,
    pub cycle_count: usize,
    pub suspicious_pattern_count: usize,
}

/// Cytoscape.js compatible format
#[derive(Debug, Serialize)]
pub struct CytoscapeGraph {
    pub elements: CytoscapeElements,
    pub layout: CytoscapeLayout,
}

#[derive(Debug, Serialize)]
pub struct CytoscapeElements {
    pub nodes: Vec<CytoscapeNode>,
    pub edges: Vec<CytoscapeEdge>,
}

#[derive(Debug, Serialize)]
pub struct CytoscapeNode {
    pub data: CytoscapeNodeData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}

#[derive(Debug, Serialize)]
pub struct CytoscapeNodeData {
    pub id: String,
    pub label: String,
    pub node_type: String,
    pub version: Option<String>,
    pub source: String,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct CytoscapeEdge {
    pub data: CytoscapeEdgeData,
}

#[derive(Debug, Serialize)]
pub struct CytoscapeEdgeData {
    pub id: String,
    pub source: String,
    pub target: String,
    pub edge_type: String,
}

#[derive(Debug, Serialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Serialize)]
pub struct CytoscapeLayout {
    pub name: String,
    #[serde(flatten)]
    pub options: HashMap<String, serde_json::Value>,
}

// ============================================================================
// Graph Conversion
// ============================================================================

impl From<&AnalysisReport> for D3Graph {
    fn from(report: &AnalysisReport) -> Self {
        let mut nodes = Vec::new();
        let mut links = Vec::new();

        // Convert nodes
        for (id, node) in &report.graph.nodes {
            let (source_type, color) = match &node.source {
                infrasim_common::pipeline::DependencySource::Registry { .. } => {
                    ("registry".to_string(), "#4CAF50".to_string())
                }
                infrasim_common::pipeline::DependencySource::Git { .. } => {
                    ("git".to_string(), "#2196F3".to_string())
                }
                infrasim_common::pipeline::DependencySource::Path { .. } => {
                    ("path".to_string(), "#FF9800".to_string())
                }
                infrasim_common::pipeline::DependencySource::Vendored { .. } => {
                    ("vendored".to_string(), "#9C27B0".to_string())
                }
                infrasim_common::pipeline::DependencySource::Unknown => {
                    ("unknown".to_string(), "#9E9E9E".to_string())
                }
            };

            let is_root = report.graph.root_nodes.contains(id);
            let radius = if is_root { 15.0 } else { 8.0 };

            nodes.push(D3Node {
                id: id.clone(),
                name: node.name.clone(),
                group: source_type.clone(),
                version: node.version.clone(),
                source_type,
                radius,
                color,
                metadata: node.metadata.clone(),
            });
        }

        // Convert edges
        for edge in &report.graph.edges {
            let kind = match edge.kind {
                infrasim_common::pipeline::EdgeKind::Normal => "normal",
                infrasim_common::pipeline::EdgeKind::Dev => "dev",
                infrasim_common::pipeline::EdgeKind::Build => "build",
                infrasim_common::pipeline::EdgeKind::Proc => "proc",
            };

            let strength = match edge.kind {
                infrasim_common::pipeline::EdgeKind::Normal => 1.0,
                infrasim_common::pipeline::EdgeKind::Build => 0.8,
                infrasim_common::pipeline::EdgeKind::Dev => 0.5,
                infrasim_common::pipeline::EdgeKind::Proc => 0.9,
            };

            links.push(D3Link {
                source: edge.from.clone(),
                target: edge.to.clone(),
                kind: kind.to_string(),
                optional: edge.optional,
                strength,
            });
        }

        D3Graph {
            metadata: D3Metadata {
                total_nodes: nodes.len(),
                total_links: links.len(),
                max_depth: report.graph.metadata.max_depth,
                risk_score: report.risk_score,
                cycle_count: report.cycles.len(),
                suspicious_pattern_count: report.suspicious_patterns.len(),
            },
            nodes,
            links,
        }
    }
}

impl From<&AnalysisReport> for CytoscapeGraph {
    fn from(report: &AnalysisReport) -> Self {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        // Convert nodes
        for (id, node) in &report.graph.nodes {
            let source = match &node.source {
                infrasim_common::pipeline::DependencySource::Registry { name, .. } => {
                    name.clone()
                }
                infrasim_common::pipeline::DependencySource::Git { url, .. } => url.clone(),
                infrasim_common::pipeline::DependencySource::Path { path } => path.clone(),
                infrasim_common::pipeline::DependencySource::Vendored { path } => {
                    format!("vendored:{}", path)
                }
                infrasim_common::pipeline::DependencySource::Unknown => "unknown".to_string(),
            };

            let node_type = match &node.source {
                infrasim_common::pipeline::DependencySource::Registry { .. } => "registry",
                infrasim_common::pipeline::DependencySource::Git { .. } => "git",
                infrasim_common::pipeline::DependencySource::Path { .. } => "local",
                infrasim_common::pipeline::DependencySource::Vendored { .. } => "vendored",
                infrasim_common::pipeline::DependencySource::Unknown => "unknown",
            };

            let is_root = report.graph.root_nodes.contains(id);
            let mut extra = HashMap::new();
            extra.insert(
                "isRoot".to_string(),
                serde_json::Value::Bool(is_root),
            );

            nodes.push(CytoscapeNode {
                data: CytoscapeNodeData {
                    id: id.clone(),
                    label: node.name.clone(),
                    node_type: node_type.to_string(),
                    version: node.version.clone(),
                    source,
                    extra,
                },
                position: None,
            });
        }

        // Convert edges
        for (i, edge) in report.graph.edges.iter().enumerate() {
            let edge_type = match edge.kind {
                infrasim_common::pipeline::EdgeKind::Normal => "depends",
                infrasim_common::pipeline::EdgeKind::Dev => "dev-depends",
                infrasim_common::pipeline::EdgeKind::Build => "build-depends",
                infrasim_common::pipeline::EdgeKind::Proc => "proc-depends",
            };

            edges.push(CytoscapeEdge {
                data: CytoscapeEdgeData {
                    id: format!("e{}", i),
                    source: edge.from.clone(),
                    target: edge.to.clone(),
                    edge_type: edge_type.to_string(),
                },
            });
        }

        let mut layout_options = HashMap::new();
        layout_options.insert(
            "spacingFactor".to_string(),
            serde_json::Value::Number(serde_json::Number::from(1)),
        );
        layout_options.insert(
            "nodeDimensionsIncludeLabels".to_string(),
            serde_json::Value::Bool(true),
        );

        CytoscapeGraph {
            elements: CytoscapeElements { nodes, edges },
            layout: CytoscapeLayout {
                name: "dagre".to_string(),
                options: layout_options,
            },
        }
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// Analyze a workspace and return dependency graph with patterns
pub async fn analyze_workspace_handler(
    State(cache): State<Arc<AnalysisCache>>,
    Json(req): Json<AnalyzeWorkspaceRequest>,
) -> impl IntoResponse {
    let path = PathBuf::from(&req.workspace_path);

    if !path.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(AnalyzeWorkspaceResponse {
                success: false,
                report: None,
                timing: None,
                error: Some(format!("Workspace path not found: {}", req.workspace_path)),
            }),
        )
            .into_response();
    }

    // Run analysis in blocking task
    let workspace_path = req.workspace_path.clone();
    let analysis_result = tokio::task::spawn_blocking(move || {
        let mut analyzer = PipelineAnalyzer::new();
        analyzer.analyze_cargo_workspace(&path)
    })
    .await;

    let report = match analysis_result {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AnalyzeWorkspaceResponse {
                    success: false,
                    report: None,
                    timing: None,
                    error: Some(format!("Analysis failed: {}", e)),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AnalyzeWorkspaceResponse {
                    success: false,
                    report: None,
                    timing: None,
                    error: Some(format!("Task failed: {}", e)),
                }),
            )
                .into_response();
        }
    };

    // Optionally run timing probes
    let timing = if req.include_timing {
        let fingerprint = tokio::task::spawn_blocking(NetworkFingerprint::collect)
            .await
            .ok();

        if let Some(ref fp) = fingerprint {
            let mut history = cache.timing_history.write().await;
            history.push(fp.clone());
            if history.len() > cache.max_history {
                history.remove(0);
            }
        }

        fingerprint
    } else {
        None
    };

    // Cache the analysis
    {
        let mut cached = cache.last_analysis.write().await;
        *cached = Some(CachedAnalysis {
            report: report.clone(),
            workspace_path,
            analyzed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        });
    }

    (
        StatusCode::OK,
        Json(AnalyzeWorkspaceResponse {
            success: true,
            report: Some(report),
            timing,
            error: None,
        }),
    )
        .into_response()
}

/// Get the dependency graph in D3.js format
pub async fn get_d3_graph_handler(
    State(cache): State<Arc<AnalysisCache>>,
    Query(params): Query<GraphQueryParams>,
) -> impl IntoResponse {
    let cached = cache.last_analysis.read().await;

    match cached.as_ref() {
        Some(analysis) => {
            let d3_graph = D3Graph::from(&analysis.report);
            (StatusCode::OK, Json(d3_graph)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "No analysis available. Run POST /api/analysis/workspace first."
            })),
        )
            .into_response(),
    }
}

/// Get the dependency graph in Cytoscape.js format
pub async fn get_cytoscape_graph_handler(
    State(cache): State<Arc<AnalysisCache>>,
    Query(params): Query<GraphQueryParams>,
) -> impl IntoResponse {
    let cached = cache.last_analysis.read().await;

    match cached.as_ref() {
        Some(analysis) => {
            let cytoscape_graph = CytoscapeGraph::from(&analysis.report);
            (StatusCode::OK, Json(cytoscape_graph)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "No analysis available. Run POST /api/analysis/workspace first."
            })),
        )
            .into_response(),
    }
}

/// Get detected cycles
pub async fn get_cycles_handler(State(cache): State<Arc<AnalysisCache>>) -> impl IntoResponse {
    let cached = cache.last_analysis.read().await;

    match cached.as_ref() {
        Some(analysis) => {
            (StatusCode::OK, Json(&analysis.report.cycles)).into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "cycles": [] }))).into_response(),
    }
}

/// Get vendor convergence patterns
pub async fn get_vendor_convergence_handler(
    State(cache): State<Arc<AnalysisCache>>,
) -> impl IntoResponse {
    let cached = cache.last_analysis.read().await;

    match cached.as_ref() {
        Some(analysis) => {
            (StatusCode::OK, Json(&analysis.report.vendor_convergence)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "vendor_convergence": [] })),
        )
            .into_response(),
    }
}

/// Get suspicious patterns
pub async fn get_suspicious_patterns_handler(
    State(cache): State<Arc<AnalysisCache>>,
) -> impl IntoResponse {
    let cached = cache.last_analysis.read().await;

    match cached.as_ref() {
        Some(analysis) => {
            (StatusCode::OK, Json(&analysis.report.suspicious_patterns)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "suspicious_patterns": [] })),
        )
            .into_response(),
    }
}

/// Run ICMP timing probes
pub async fn run_timing_probes_handler(
    State(cache): State<Arc<AnalysisCache>>,
    Json(req): Json<TimingProbeRequest>,
) -> impl IntoResponse {
    let fingerprint = tokio::task::spawn_blocking(move || {
        if req.include_defaults {
            NetworkFingerprint::collect()
        } else {
            // Custom probes only - would need to extend NetworkFingerprint
            // For now, just collect defaults
            NetworkFingerprint::collect()
        }
    })
    .await;

    match fingerprint {
        Ok(fp) => {
            // Calculate summary
            let successful: Vec<&TimingProbe> =
                fp.probes.iter().filter(|p| p.success).collect();
            let rtts: Vec<f64> = successful.iter().filter_map(|p| p.rtt_ms).collect();

            let summary = TimingSummary {
                total_probes: fp.probes.len(),
                successful_probes: successful.len(),
                average_rtt_ms: if rtts.is_empty() {
                    None
                } else {
                    Some(rtts.iter().sum::<f64>() / rtts.len() as f64)
                },
                min_rtt_ms: rtts.iter().cloned().reduce(f64::min),
                max_rtt_ms: rtts.iter().cloned().reduce(f64::max),
                inferred_region: fp.inferred_region.clone(),
                anomaly_count: fp.routing_anomalies.len(),
            };

            // Store in history
            {
                let mut history = cache.timing_history.write().await;
                history.push(fp.clone());
                if history.len() > cache.max_history {
                    history.remove(0);
                }
            }

            (
                StatusCode::OK,
                Json(TimingProbeResponse {
                    fingerprint: fp,
                    summary,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Timing probes failed: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Get timing probe history
pub async fn get_timing_history_handler(
    State(cache): State<Arc<AnalysisCache>>,
    Query(params): Query<HistoryQueryParams>,
) -> impl IntoResponse {
    let history = cache.timing_history.read().await;
    let limit = params.limit.unwrap_or(10).min(100);
    let offset = params.offset.unwrap_or(0);

    let total = history.len();
    let items: Vec<_> = history
        .iter()
        .rev()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "total": total,
            "offset": offset,
            "limit": limit,
            "items": items
        })),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct HistoryQueryParams {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Get analysis summary
pub async fn get_analysis_summary_handler(
    State(cache): State<Arc<AnalysisCache>>,
) -> impl IntoResponse {
    let cached = cache.last_analysis.read().await;

    match cached.as_ref() {
        Some(analysis) => {
            let report = &analysis.report;
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "workspace_path": analysis.workspace_path,
                    "analyzed_at": analysis.analyzed_at,
                    "total_nodes": report.graph.metadata.total_nodes,
                    "total_edges": report.graph.metadata.total_edges,
                    "max_depth": report.graph.metadata.max_depth,
                    "risk_score": report.risk_score,
                    "cycle_count": report.cycles.len(),
                    "vendor_convergence_count": report.vendor_convergence.len(),
                    "suspicious_pattern_count": report.suspicious_patterns.len(),
                    "warnings": report.warnings,
                    "recommendations": report.recommendations
                })),
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "No analysis available"
            })),
        )
            .into_response(),
    }
}

// ============================================================================
// Route Builder
// ============================================================================

use axum::{routing::get, routing::post, Router};

/// Build the analysis routes
pub fn analysis_routes(cache: Arc<AnalysisCache>) -> Router {
    Router::new()
        .route("/workspace", post(analyze_workspace_handler))
        .route("/summary", get(get_analysis_summary_handler))
        .route("/graph/d3", get(get_d3_graph_handler))
        .route("/graph/cytoscape", get(get_cytoscape_graph_handler))
        .route("/cycles", get(get_cycles_handler))
        .route("/vendor-convergence", get(get_vendor_convergence_handler))
        .route("/suspicious-patterns", get(get_suspicious_patterns_handler))
        .route("/timing", post(run_timing_probes_handler))
        .route("/timing/history", get(get_timing_history_handler))
        .with_state(cache)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_d3_graph_conversion() {
        let mut report = AnalysisReport::default();
        report.graph.nodes.insert(
            "test-pkg".to_string(),
            infrasim_common::pipeline::DependencyNode {
                id: "test-pkg".to_string(),
                name: "test".to_string(),
                version: Some("1.0.0".to_string()),
                source: infrasim_common::pipeline::DependencySource::Registry {
                    name: "crates.io".to_string(),
                    url: "https://crates.io".to_string(),
                },
                checksum: None,
                metadata: HashMap::new(),
            },
        );
        report.graph.root_nodes.push("test-pkg".to_string());

        let d3 = D3Graph::from(&report);
        assert_eq!(d3.nodes.len(), 1);
        assert_eq!(d3.nodes[0].name, "test");
    }
}
