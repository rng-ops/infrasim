//! Build Pipeline Analysis Module
//!
//! Provides static analysis capabilities for build pipelines:
//! - Dependency graph construction and cycle detection
//! - Vendor convergence pattern detection
//! - Confounding pattern identification
//! - Optional network timing probes (feature-gated, opt-in)
//!
//! # Feature Flags
//!
//! - `network-context`: Enables optional network timing collection (OFF by default).
//!   When enabled, timing probes are still opt-in and require explicit runtime enablement
//!   via `NetworkTimingConfig`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "network-context")]
use std::net::ToSocketAddrs;
#[cfg(feature = "network-context")]
use std::time::Instant;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, warn};

/// Errors in pipeline analysis
#[derive(Error, Debug)]
pub enum AnalysisError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Cycle detected: {0}")]
    CycleDetected(String),

    #[error("Network error: {0}")]
    Network(String),
}

pub type Result<T> = std::result::Result<T, AnalysisError>;

// ============================================================================
// Dependency Graph
// ============================================================================

/// A node in the dependency graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyNode {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub source: DependencySource,
    pub checksum: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// Source of a dependency
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DependencySource {
    /// crates.io or similar registry
    Registry { name: String, url: String },
    /// Git repository
    Git { url: String, rev: Option<String>, branch: Option<String> },
    /// Local path
    Path { path: String },
    /// Vendored in-tree
    Vendored { path: String },
    /// Unknown/external
    Unknown,
}

/// An edge in the dependency graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    pub optional: bool,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EdgeKind {
    Normal,
    Dev,
    Build,
    Proc,
}

/// Complete dependency graph with adjacency lists for O(1) lookups
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DependencyGraph {
    pub nodes: HashMap<String, DependencyNode>,
    pub edges: Vec<DependencyEdge>,
    pub root_nodes: Vec<String>,
    pub metadata: GraphMetadata,
    /// Adjacency list: node_id -> list of outgoing edge targets
    #[serde(skip)]
    adjacency_out: HashMap<String, Vec<String>>,
    /// Reverse adjacency: node_id -> list of incoming edge sources
    #[serde(skip)]
    adjacency_in: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphMetadata {
    pub analyzed_at: u64,
    pub source_path: String,
    pub git_commit: Option<String>,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub max_depth: usize,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, node: DependencyNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    /// Add an edge to the graph, updating adjacency lists
    pub fn add_edge(&mut self, edge: DependencyEdge) {
        // Update adjacency lists for O(1) lookups
        self.adjacency_out
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
        self.adjacency_in
            .entry(edge.to.clone())
            .or_default()
            .push(edge.from.clone());
        self.edges.push(edge);
    }

    /// Rebuild adjacency lists from edges (call after deserialization)
    pub fn rebuild_adjacency(&mut self) {
        self.adjacency_out.clear();
        self.adjacency_in.clear();
        for edge in &self.edges {
            self.adjacency_out
                .entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
            self.adjacency_in
                .entry(edge.to.clone())
                .or_default()
                .push(edge.from.clone());
        }
    }

    /// Get all dependencies of a node (O(1) lookup via adjacency list)
    pub fn dependencies(&self, node_id: &str) -> Vec<&DependencyNode> {
        self.adjacency_out
            .get(node_id)
            .map(|targets| {
                targets
                    .iter()
                    .filter_map(|t| self.nodes.get(t))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all dependents of a node (reverse dependencies, O(1) lookup)
    pub fn dependents(&self, node_id: &str) -> Vec<&DependencyNode> {
        self.adjacency_in
            .get(node_id)
            .map(|sources| {
                sources
                    .iter()
                    .filter_map(|s| self.nodes.get(s))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get outgoing neighbor IDs efficiently
    pub fn outgoing_neighbors(&self, node_id: &str) -> &[String] {
        self.adjacency_out
            .get(node_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get incoming neighbor IDs efficiently  
    pub fn incoming_neighbors(&self, node_id: &str) -> &[String] {
        self.adjacency_in
            .get(node_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Compute graph statistics
    pub fn compute_stats(&mut self) {
        self.metadata.total_nodes = self.nodes.len();
        self.metadata.total_edges = self.edges.len();
        self.metadata.analyzed_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Compute max depth via BFS from roots
        let mut max_depth = 0;
        for root in &self.root_nodes {
            if let Some(depth) = self.compute_depth(root) {
                max_depth = max_depth.max(depth);
            }
        }
        self.metadata.max_depth = max_depth;
    }

    fn compute_depth(&self, start: &str) -> Option<usize> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back((start.to_string(), 0usize));

        let mut max_depth = 0;

        while let Some((node, depth)) = queue.pop_front() {
            if !visited.insert(node.clone()) {
                continue;
            }
            max_depth = max_depth.max(depth);

            // Use adjacency list for O(1) neighbor lookup
            for neighbor in self.outgoing_neighbors(&node) {
                queue.push_back((neighbor.clone(), depth + 1));
            }
        }

        Some(max_depth)
    }
}

// ============================================================================
// Analysis Results
// ============================================================================

/// Results of static analysis on a dependency graph
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalysisReport {
    pub graph: DependencyGraph,
    pub cycles: Vec<CycleInfo>,
    pub vendor_convergence: Vec<VendorConvergence>,
    pub suspicious_patterns: Vec<SuspiciousPattern>,
    pub timing_probes: Vec<TimingProbe>,
    pub risk_score: f64,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
}

/// Detected dependency cycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleInfo {
    pub nodes: Vec<String>,
    pub kind: CycleKind,
    pub severity: Severity,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CycleKind {
    Direct,          // A -> B -> A
    Transitive,      // A -> B -> C -> A
    BuildTime,       // build-dependencies create cycle
    Feature,         // Feature flags create cycle
}

/// Vendor convergence pattern (multiple paths to same vendor)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorConvergence {
    pub vendor: String,
    pub convergence_point: String,
    pub paths: Vec<Vec<String>>,
    pub severity: Severity,
    pub description: String,
}

/// Suspicious patterns that may indicate intentional obfuscation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuspiciousPattern {
    pub pattern_type: PatternType,
    pub nodes_involved: Vec<String>,
    pub severity: Severity,
    pub description: String,
    pub evidence: Vec<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PatternType {
    /// Multiple unrelated packages from same obscure source
    VendorConcentration,
    /// Unnecessary transitive dependencies
    DependencyInflation,
    /// Diamond dependencies with version conflicts
    DiamondConflict,
    /// Typosquatting-like names
    NameConfusion,
    /// Abandoned maintainer with recent transfer
    MaintainerAnomaly,
    /// Circular feature dependencies
    FeatureLoop,
    /// Proc-macro with unusual capabilities
    ProcMacroSuspicious,
    /// Build script with network access
    BuildScriptNetwork,
    /// Pinned to unusual commit
    UnusualPin,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Ord, PartialOrd, Eq)]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

// ============================================================================
// Network Timing Probes (Feature-gated, Opt-in)
// ============================================================================

/// Configuration for optional network timing collection.
/// 
/// Network timing is:
/// - Feature-gated: requires `network-context` Cargo feature
/// - Opt-in: requires explicit runtime enablement via this config
/// - Privacy-respecting: no hardcoded servers, user provides targets
/// - Coarse-grained: collects only aggregated RTT statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkTimingConfig {
    /// Whether network timing is enabled (default: false)
    pub enabled: bool,
    /// Custom probe targets (host, label pairs)
    /// If empty and enabled, no probes are performed
    pub probe_targets: Vec<ProbeTarget>,
    /// Maximum number of probes to perform
    pub max_probes: usize,
    /// Timeout per probe in milliseconds
    pub timeout_ms: u64,
}

/// A custom probe target
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeTarget {
    pub host: String,
    pub label: String,
}

impl NetworkTimingConfig {
    /// Create a disabled configuration (default)
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            probe_targets: Vec::new(),
            max_probes: 10,
            timeout_ms: 2000,
        }
    }

    /// Create an enabled configuration with custom targets
    pub fn with_targets(targets: Vec<ProbeTarget>) -> Self {
        Self {
            enabled: true,
            probe_targets: targets,
            max_probes: 10,
            timeout_ms: 2000,
        }
    }
}

/// Result of a timing probe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingProbe {
    pub target: String,
    pub target_ip: Option<String>,
    pub label: String,
    pub timestamp: u64,
    pub rtt_ms: Option<f64>,
    pub ttl: Option<u8>,
    pub success: bool,
    pub error: Option<String>,
}

/// Aggregated timing statistics (coarse-grained, privacy-respecting)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AggregatedTimingStats {
    pub total_probes: usize,
    pub successful_probes: usize,
    pub min_rtt_ms: Option<f64>,
    pub max_rtt_ms: Option<f64>,
    pub avg_rtt_ms: Option<f64>,
    pub collection_timestamp: u64,
}

/// Collected timing data (no geographic inference)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkFingerprint {
    pub probes: Vec<TimingProbe>,
    pub collection_start: u64,
    pub collection_end: u64,
    pub aggregated_stats: AggregatedTimingStats,
}

#[cfg(feature = "network-context")]
impl NetworkFingerprint {
    /// Collect network timing probes using provided configuration.
    /// Returns None if timing is disabled or no targets configured.
    pub fn collect(config: &NetworkTimingConfig) -> Option<Self> {
        if !config.enabled || config.probe_targets.is_empty() {
            return None;
        }

        let mut fingerprint = NetworkFingerprint {
            collection_start: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            ..Default::default()
        };

        let targets_to_probe = config
            .probe_targets
            .iter()
            .take(config.max_probes);

        for target in targets_to_probe {
            let probe = ping_host(&target.host, &target.label, config.timeout_ms);
            fingerprint.probes.push(probe);
        }

        fingerprint.collection_end = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Compute coarse aggregated stats only
        fingerprint.compute_aggregated_stats();

        Some(fingerprint)
    }

    fn compute_aggregated_stats(&mut self) {
        let successful_rtts: Vec<f64> = self
            .probes
            .iter()
            .filter(|p| p.success)
            .filter_map(|p| p.rtt_ms)
            .collect();

        self.aggregated_stats = AggregatedTimingStats {
            total_probes: self.probes.len(),
            successful_probes: successful_rtts.len(),
            min_rtt_ms: successful_rtts.iter().cloned().reduce(f64::min),
            max_rtt_ms: successful_rtts.iter().cloned().reduce(f64::max),
            avg_rtt_ms: if successful_rtts.is_empty() {
                None
            } else {
                Some(successful_rtts.iter().sum::<f64>() / successful_rtts.len() as f64)
            },
            collection_timestamp: self.collection_start,
        };
    }
}

#[cfg(not(feature = "network-context"))]
impl NetworkFingerprint {
    /// Network timing is disabled when feature is not enabled
    pub fn collect(_config: &NetworkTimingConfig) -> Option<Self> {
        None
    }
}

/// Ping a host and record timing
#[cfg(feature = "network-context")]
fn ping_host(host: &str, label: &str, timeout_ms: u64) -> TimingProbe {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Resolve hostname
    let target_ip = format!("{}:80", host)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.next())
        .map(|addr| addr.ip().to_string());

    let timeout_secs = (timeout_ms / 1000).max(1);

    // Execute ping (macOS/Linux compatible)
    let output = Command::new("ping")
        .args(["-c", "1", "-W", &timeout_secs.to_string(), host])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);

            // Parse RTT from ping output
            let rtt_ms = parse_ping_rtt(&stdout);
            let ttl = parse_ping_ttl(&stdout);

            TimingProbe {
                target: host.to_string(),
                target_ip,
                label: label.to_string(),
                timestamp,
                rtt_ms,
                ttl,
                success: out.status.success(),
                error: if out.status.success() {
                    None
                } else {
                    Some("Ping failed".to_string())
                },
            }
        }
        Err(e) => TimingProbe {
            target: host.to_string(),
            target_ip,
            label: label.to_string(),
            timestamp,
            rtt_ms: None,
            ttl: None,
            success: false,
            error: Some(e.to_string()),
        },
    }
}

#[cfg(feature = "network-context")]
fn parse_ping_rtt(output: &str) -> Option<f64> {
    // macOS: "round-trip min/avg/max/stddev = 1.234/2.345/3.456/0.123 ms"
    // Linux: "rtt min/avg/max/mdev = 1.234/2.345/3.456/0.123 ms"
    for line in output.lines() {
        if line.contains("round-trip") || line.contains("rtt ") {
            if let Some(stats) = line.split('=').nth(1) {
                let parts: Vec<&str> = stats.trim().split('/').collect();
                if parts.len() >= 2 {
                    // Return average RTT
                    return parts[1].trim().parse().ok();
                }
            }
        }
        // Also try "time=X.XX ms" pattern
        if line.contains("time=") {
            if let Some(time_part) = line.split("time=").nth(1) {
                if let Some(ms_str) = time_part.split_whitespace().next() {
                    return ms_str.parse().ok();
                }
            }
        }
    }
    None
}

#[cfg(feature = "network-context")]
fn parse_ping_ttl(output: &str) -> Option<u8> {
    for line in output.lines() {
        if line.contains("ttl=") || line.contains("TTL=") {
            let lower = line.to_lowercase();
            if let Some(ttl_part) = lower.split("ttl=").nth(1) {
                if let Some(ttl_str) = ttl_part.split_whitespace().next() {
                    return ttl_str.parse().ok();
                }
            }
        }
    }
    None
}

// ============================================================================
// Pipeline Analyzer
// ============================================================================

/// Analyzer for build pipeline dependencies
pub struct PipelineAnalyzer {
    graph: DependencyGraph,
}

impl PipelineAnalyzer {
    pub fn new() -> Self {
        Self {
            graph: DependencyGraph::new(),
        }
    }

    /// Analyze a Cargo workspace
    pub fn analyze_cargo_workspace(&mut self, path: &Path) -> Result<AnalysisReport> {
        info!("Analyzing Cargo workspace: {}", path.display());

        // Run cargo metadata
        let output = Command::new("cargo")
            .args(["metadata", "--format-version", "1", "--all-features"])
            .current_dir(path)
            .output()?;

        if !output.status.success() {
            return Err(AnalysisError::Parse(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| AnalysisError::Parse(e.to_string()))?;

        self.parse_cargo_metadata(&metadata)?;
        self.graph.compute_stats();

        // Run analysis
        let mut report = AnalysisReport {
            graph: self.graph.clone(),
            ..Default::default()
        };

        self.detect_cycles(&mut report);
        self.detect_vendor_convergence(&mut report);
        self.detect_suspicious_patterns(&mut report);
        self.calculate_risk_score(&mut report);

        Ok(report)
    }

    fn parse_cargo_metadata(&mut self, metadata: &serde_json::Value) -> Result<()> {
        let packages = metadata["packages"]
            .as_array()
            .ok_or_else(|| AnalysisError::Parse("No packages in metadata".to_string()))?;

        // Build a map of package IDs to their proc-macro status (from targets)
        let mut proc_macro_packages: HashSet<String> = HashSet::new();

        // Build nodes
        for pkg in packages {
            let id = pkg["id"].as_str().unwrap_or("").to_string();
            let name = pkg["name"].as_str().unwrap_or("").to_string();
            let version = pkg["version"].as_str().map(|s| s.to_string());

            // Check if this package is a proc-macro by examining its targets
            let is_proc_macro = pkg["targets"]
                .as_array()
                .map(|targets| {
                    targets.iter().any(|target| {
                        target["kind"]
                            .as_array()
                            .map(|kinds| {
                                kinds.iter().any(|k| k.as_str() == Some("proc-macro"))
                            })
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);

            if is_proc_macro {
                proc_macro_packages.insert(id.clone());
            }

            let source = if let Some(source_str) = pkg["source"].as_str() {
                if source_str.starts_with("registry+") {
                    DependencySource::Registry {
                        name: "crates.io".to_string(),
                        url: source_str.to_string(),
                    }
                } else if source_str.starts_with("git+") {
                    DependencySource::Git {
                        url: source_str.to_string(),
                        rev: None,
                        branch: None,
                    }
                } else {
                    DependencySource::Unknown
                }
            } else {
                // Local path
                if let Some(manifest) = pkg["manifest_path"].as_str() {
                    DependencySource::Path {
                        path: manifest.to_string(),
                    }
                } else {
                    DependencySource::Unknown
                }
            };

            let mut node_metadata = HashMap::new();
            if is_proc_macro {
                node_metadata.insert("proc_macro".to_string(), "true".to_string());
            }

            let node = DependencyNode {
                id: id.clone(),
                name,
                version,
                source,
                checksum: None,
                metadata: node_metadata,
            };

            self.graph.add_node(node);
        }

        // Build edges from resolve
        if let Some(resolve) = metadata["resolve"].as_object() {
            if let Some(nodes) = resolve["nodes"].as_array() {
                for node in nodes {
                    let from = node["id"].as_str().unwrap_or("").to_string();

                    if let Some(deps) = node["deps"].as_array() {
                        for dep in deps {
                            let to = dep["pkg"].as_str().unwrap_or("").to_string();

                            // Determine edge kind from dep_kinds, with proc-macro detection
                            let is_target_proc_macro = proc_macro_packages.contains(&to);

                            let kinds: Vec<EdgeKind> = dep["dep_kinds"]
                                .as_array()
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|k| {
                                            k["kind"].as_str().map(|s| match s {
                                                "dev" => EdgeKind::Dev,
                                                "build" => EdgeKind::Build,
                                                _ if is_target_proc_macro => EdgeKind::Proc,
                                                _ => EdgeKind::Normal,
                                            })
                                        })
                                        .collect()
                                })
                                .unwrap_or_else(|| {
                                    if is_target_proc_macro {
                                        vec![EdgeKind::Proc]
                                    } else {
                                        vec![EdgeKind::Normal]
                                    }
                                });

                            for kind in kinds {
                                self.graph.add_edge(DependencyEdge {
                                    from: from.clone(),
                                    to: to.clone(),
                                    kind,
                                    optional: false,
                                    features: vec![],
                                });
                            }
                        }
                    }
                }
            }

            // Set root nodes
            if let Some(root) = resolve["root"].as_str() {
                self.graph.root_nodes.push(root.to_string());
            }
        }

        Ok(())
    }

    fn detect_cycles(&self, report: &mut AnalysisReport) {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();
        // Track seen cycles to avoid duplicates (normalized to start from min element)
        let mut seen_cycles: HashSet<Vec<String>> = HashSet::new();

        for node_id in self.graph.nodes.keys() {
            if !visited.contains(node_id) {
                self.dfs_cycle(
                    node_id,
                    &mut visited,
                    &mut rec_stack,
                    &mut path,
                    &mut seen_cycles,
                    report,
                );
            }
        }
    }

    fn dfs_cycle(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        seen_cycles: &mut HashSet<Vec<String>>,
        report: &mut AnalysisReport,
    ) {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());

        // Use adjacency list for O(1) neighbor lookup
        for neighbor in self.graph.outgoing_neighbors(node) {
            if !visited.contains(neighbor) {
                self.dfs_cycle(neighbor, visited, rec_stack, path, seen_cycles, report);
            } else if rec_stack.contains(neighbor) {
                // Found cycle - extract and normalize it
                let cycle_start = path.iter().position(|n| n == neighbor).unwrap_or(0);
                let cycle_nodes: Vec<String> = path[cycle_start..].to_vec();

                // Normalize cycle: rotate to start from lexicographically smallest element
                let normalized =    normalize_cycle(&cycle_nodes);

                // Only add if we haven't seen this cycle before
                if seen_cycles.insert(normalized) {
                    let cycle = CycleInfo {
                        nodes: cycle_nodes.clone(),
                        kind: if cycle_nodes.len() == 2 {
                            CycleKind::Direct
                        } else {
                            CycleKind::Transitive
                        },
                        severity: if cycle_nodes.len() <= 3 {
                            Severity::High
                        } else {
                            Severity::Medium
                        },
                        description: format!(
                            "Cycle detected: {} → {}",
                            cycle_nodes.join(" → "),
                            neighbor
                        ),
                    };

                    report.cycles.push(cycle);
                }
            }
        }

        path.pop();
        rec_stack.remove(node);
    }

    fn detect_vendor_convergence(&self, report: &mut AnalysisReport) {
        // Group packages by apparent vendor/maintainer
        let mut vendor_packages: HashMap<String, Vec<String>> = HashMap::new();

        for (id, node) in &self.graph.nodes {
            let vendor = self.infer_vendor(node);
            vendor_packages.entry(vendor).or_default().push(id.clone());
        }

        // Look for vendors with many packages that have multiple dependency paths
        for (vendor, packages) in &vendor_packages {
            if packages.len() >= 3 {
                // Check if multiple root paths lead to this vendor's packages
                let mut paths_to_vendor: Vec<Vec<String>> = Vec::new();

                for pkg_id in packages {
                    for root in &self.graph.root_nodes {
                        if let Some(path) = self.find_path(root, pkg_id) {
                            paths_to_vendor.push(path);
                        }
                    }
                }

                if paths_to_vendor.len() >= 3 {
                    let convergence = VendorConvergence {
                        vendor: vendor.clone(),
                        convergence_point: packages.first().cloned().unwrap_or_default(),
                        paths: paths_to_vendor.clone(),
                        severity: if paths_to_vendor.len() >= 5 {
                            Severity::Medium
                        } else {
                            Severity::Low
                        },
                        description: format!(
                            "Vendor '{}' has {} packages reached via {} different paths",
                            vendor,
                            packages.len(),
                            paths_to_vendor.len()
                        ),
                    };
                    report.vendor_convergence.push(convergence);
                }
            }
        }
    }

    fn infer_vendor(&self, node: &DependencyNode) -> String {
        // Try to infer vendor from package name patterns
        let name = &node.name;

        // Common prefixes
        let prefixes = [
            ("tokio-", "tokio"),
            ("serde_", "serde"),
            ("serde-", "serde"),
            ("tracing-", "tracing"),
            ("tower-", "tower"),
            ("hyper-", "hyper"),
            ("http-", "hyperium"),
            ("tonic-", "tonic"),
            ("prost-", "prost"),
        ];

        for (prefix, vendor) in prefixes {
            if name.starts_with(prefix) {
                return vendor.to_string();
            }
        }

        // Check source URL
        match &node.source {
            DependencySource::Git { url, .. } => {
                if let Some(org) = extract_github_org(url) {
                    return org;
                }
            }
            _ => {}
        }

        // Default to package name
        name.clone()
    }

    fn find_path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(vec![from.to_string()]);

        while let Some(path) = queue.pop_front() {
            let current = path.last()?;

            if current == to {
                return Some(path);
            }

            if visited.insert(current.clone()) {
                // Use adjacency list for O(1) neighbor lookup
                for neighbor in self.graph.outgoing_neighbors(current) {
                    if !visited.contains(neighbor) {
                        let mut new_path = path.clone();
                        new_path.push(neighbor.clone());
                        queue.push_back(new_path);
                    }
                }
            }
        }

        None
    }

    fn detect_suspicious_patterns(&self, report: &mut AnalysisReport) {
        // Check for name confusion (typosquatting)
        self.detect_name_confusion(report);

        // Check for unusual pins
        self.detect_unusual_pins(report);

        // Check for proc-macro suspicions
        self.detect_proc_macro_suspicions(report);
    }

    fn detect_name_confusion(&self, report: &mut AnalysisReport) {
        let names: Vec<&str> = self.graph.nodes.values().map(|n| n.name.as_str()).collect();

        // Check for similar names
        for i in 0..names.len() {
            for j in (i + 1)..names.len() {
                let distance = levenshtein_distance(names[i], names[j]);
                if distance == 1 && names[i].len() >= 4 {
                    report.suspicious_patterns.push(SuspiciousPattern {
                        pattern_type: PatternType::NameConfusion,
                        nodes_involved: vec![names[i].to_string(), names[j].to_string()],
                        severity: Severity::Medium,
                        description: format!(
                            "Similar package names detected: '{}' and '{}' (edit distance: {})",
                            names[i], names[j], distance
                        ),
                        evidence: vec![format!(
                            "Names differ by only {} character(s)",
                            distance
                        )],
                        confidence: 0.7,
                    });
                }
            }
        }
    }

    fn detect_unusual_pins(&self, report: &mut AnalysisReport) {
        for node in self.graph.nodes.values() {
            if let DependencySource::Git { url, rev, .. } = &node.source {
                if let Some(rev) = rev {
                    // Check if pinned to a short hash (unusual)
                    if rev.len() < 10 && !rev.starts_with('v') {
                        report.suspicious_patterns.push(SuspiciousPattern {
                            pattern_type: PatternType::UnusualPin,
                            nodes_involved: vec![node.id.clone()],
                            severity: Severity::Low,
                            description: format!(
                                "Package '{}' pinned to short git ref: {}",
                                node.name, rev
                            ),
                            evidence: vec![url.clone()],
                            confidence: 0.5,
                        });
                    }
                }
            }
        }
    }

    fn detect_proc_macro_suspicions(&self, report: &mut AnalysisReport) {
        // Proc-macros with unusual dependencies
        for edge in &self.graph.edges {
            if edge.kind == EdgeKind::Proc {
                // Check if proc-macro depends on networking crates
                if let Some(dep_node) = self.graph.nodes.get(&edge.to) {
                    let suspicious_deps = ["reqwest", "hyper", "tokio", "async-std"];
                    if suspicious_deps.contains(&dep_node.name.as_str()) {
                        report.suspicious_patterns.push(SuspiciousPattern {
                            pattern_type: PatternType::ProcMacroSuspicious,
                            nodes_involved: vec![edge.from.clone(), edge.to.clone()],
                            severity: Severity::High,
                            description: format!(
                                "Proc-macro has unusual runtime dependency: {}",
                                dep_node.name
                            ),
                            evidence: vec![format!(
                                "{} -> {} (proc-macro with network capability)",
                                edge.from, edge.to
                            )],
                            confidence: 0.8,
                        });
                    }
                }
            }
        }
    }

    fn calculate_risk_score(&self, report: &mut AnalysisReport) {
        let mut score = 0.0;

        // Cycles
        for cycle in &report.cycles {
            score += match cycle.severity {
                Severity::Critical => 30.0,
                Severity::High => 20.0,
                Severity::Medium => 10.0,
                Severity::Low => 5.0,
                Severity::Info => 1.0,
            };
        }

        // Vendor convergence
        for conv in &report.vendor_convergence {
            score += match conv.severity {
                Severity::Critical => 20.0,
                Severity::High => 15.0,
                Severity::Medium => 8.0,
                Severity::Low => 3.0,
                Severity::Info => 1.0,
            };
        }

        // Suspicious patterns
        for pattern in &report.suspicious_patterns {
            score += pattern.confidence
                * match pattern.severity {
                    Severity::Critical => 25.0,
                    Severity::High => 18.0,
                    Severity::Medium => 10.0,
                    Severity::Low => 4.0,
                    Severity::Info => 1.0,
                };
        }

        // Normalize to 0-100
        report.risk_score = (score / 100.0 * 100.0).min(100.0);

        // Generate recommendations
        if !report.cycles.is_empty() {
            report.recommendations.push(
                "Review and break dependency cycles to reduce build complexity".to_string(),
            );
        }
        if !report.vendor_convergence.is_empty() {
            report.recommendations.push(
                "Audit vendor-concentrated dependencies for supply chain risk".to_string(),
            );
        }
        if report.risk_score > 50.0 {
            report.recommendations.push(
                "Consider using cargo-vet or cargo-crev for dependency auditing".to_string(),
            );
        }
    }
}

impl Default for PipelineAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Normalize a cycle by rotating it to start from the lexicographically smallest element.
/// This ensures that A -> B -> C -> A and B -> C -> A -> B are detected as the same cycle.
fn normalize_cycle(cycle: &[String]) -> Vec<String> {
    if cycle.is_empty() {
        return Vec::new();
    }

    // Find the index of the lexicographically smallest element
    let min_idx = cycle
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.cmp(b))
        .map(|(i, _)| i)
        .unwrap_or(0);

    // Rotate the cycle to start from the smallest element
    let mut normalized = Vec::with_capacity(cycle.len());
    normalized.extend_from_slice(&cycle[min_idx..]);
    normalized.extend_from_slice(&cycle[..min_idx]);

    normalized
}

fn extract_github_org(url: &str) -> Option<String> {
    // Extract org from URLs like "https://github.com/org/repo" or "git+https://..."
    let url = url.trim_start_matches("git+");
    if url.contains("github.com") {
        let parts: Vec<&str> = url.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            if *part == "github.com" && i + 1 < parts.len() {
                return Some(parts[i + 1].to_string());
            }
        }
    }
    None
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[a_len][b_len]
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
        assert_eq!(levenshtein_distance("abc", "abd"), 1);
        assert_eq!(levenshtein_distance("serde", "serda"), 1);
        assert_eq!(levenshtein_distance("tokio", "tokyo"), 1);
    }

    #[test]
    fn test_extract_github_org() {
        assert_eq!(
            extract_github_org("https://github.com/tokio-rs/tokio"),
            Some("tokio-rs".to_string())
        );
        assert_eq!(
            extract_github_org("git+https://github.com/serde-rs/serde"),
            Some("serde-rs".to_string())
        );
    }

    #[test]
    fn test_normalize_cycle() {
        // Cycle A -> B -> C should normalize to A -> B -> C (A is smallest)
        let cycle = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(normalize_cycle(&cycle), vec!["a", "b", "c"]);

        // Cycle C -> A -> B should normalize to A -> B -> C
        let cycle2 = vec!["c".to_string(), "a".to_string(), "b".to_string()];
        assert_eq!(normalize_cycle(&cycle2), vec!["a", "b", "c"]);

        // Cycle B -> C -> A should normalize to A -> B -> C
        let cycle3 = vec!["b".to_string(), "c".to_string(), "a".to_string()];
        assert_eq!(normalize_cycle(&cycle3), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_adjacency_lists() {
        let mut graph = DependencyGraph::new();

        graph.add_node(DependencyNode {
            id: "a".to_string(),
            name: "a".to_string(),
            version: None,
            source: DependencySource::Unknown,
            checksum: None,
            metadata: HashMap::new(),
        });
        graph.add_node(DependencyNode {
            id: "b".to_string(),
            name: "b".to_string(),
            version: None,
            source: DependencySource::Unknown,
            checksum: None,
            metadata: HashMap::new(),
        });
        graph.add_node(DependencyNode {
            id: "c".to_string(),
            name: "c".to_string(),
            version: None,
            source: DependencySource::Unknown,
            checksum: None,
            metadata: HashMap::new(),
        });

        graph.add_edge(DependencyEdge {
            from: "a".to_string(),
            to: "b".to_string(),
            kind: EdgeKind::Normal,
            optional: false,
            features: vec![],
        });
        graph.add_edge(DependencyEdge {
            from: "a".to_string(),
            to: "c".to_string(),
            kind: EdgeKind::Normal,
            optional: false,
            features: vec![],
        });

        // Test outgoing neighbors
        let neighbors = graph.outgoing_neighbors("a");
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&"b".to_string()));
        assert!(neighbors.contains(&"c".to_string()));

        // Test incoming neighbors
        let incoming = graph.incoming_neighbors("b");
        assert_eq!(incoming.len(), 1);
        assert!(incoming.contains(&"a".to_string()));

        // Test dependencies method
        let deps = graph.dependencies("a");
        assert_eq!(deps.len(), 2);

        // Test dependents method
        let dependents = graph.dependents("b");
        assert_eq!(dependents.len(), 1);
        assert_eq!(dependents[0].id, "a");
    }

    #[test]
    fn test_network_timing_config() {
        // Disabled by default
        let config = NetworkTimingConfig::disabled();
        assert!(!config.enabled);
        assert!(config.probe_targets.is_empty());

        // With custom targets
        let config = NetworkTimingConfig::with_targets(vec![
            ProbeTarget {
                host: "example.com".to_string(),
                label: "Example".to_string(),
            },
        ]);
        assert!(config.enabled);
        assert_eq!(config.probe_targets.len(), 1);
    }

    #[test]
    fn test_cycle_detection() {
        let mut graph = DependencyGraph::new();

        graph.add_node(DependencyNode {
            id: "a".to_string(),
            name: "a".to_string(),
            version: None,
            source: DependencySource::Unknown,
            checksum: None,
            metadata: HashMap::new(),
        });
        graph.add_node(DependencyNode {
            id: "b".to_string(),
            name: "b".to_string(),
            version: None,
            source: DependencySource::Unknown,
            checksum: None,
            metadata: HashMap::new(),
        });

        graph.add_edge(DependencyEdge {
            from: "a".to_string(),
            to: "b".to_string(),
            kind: EdgeKind::Normal,
            optional: false,
            features: vec![],
        });
        graph.add_edge(DependencyEdge {
            from: "b".to_string(),
            to: "a".to_string(),
            kind: EdgeKind::Normal,
            optional: false,
            features: vec![],
        });

        graph.root_nodes.push("a".to_string());

        let mut analyzer = PipelineAnalyzer { graph };
        let mut report = AnalysisReport::default();
        analyzer.detect_cycles(&mut report);

        assert!(!report.cycles.is_empty());
    }
}
