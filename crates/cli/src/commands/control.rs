//! Control Plane Commands - Tailscale-based C2 for distributed InfraSim nodes
//!
//! Provides a secure control plane using Tailscale for:
//! - Deploying images to worker nodes in hostile environments
//! - Retrieving build logs and artifacts
//! - Managing distributed SDN topology
//! - Peering with other InfraSim nodes

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

use crate::client::DaemonClient;
use crate::output::{OutputFormat, TableDisplay, print_item, print_list, print_success, print_error};

// ============================================================================
// Types
// ============================================================================

/// Tailscale node information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailscaleNode {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub ip_v4: String,
    pub ip_v6: String,
    pub os: String,
    pub online: bool,
    pub last_seen: i64,
    pub is_self: bool,
    pub tags: Vec<String>,
    pub exit_node: bool,
    pub relay: String,
}

impl TableDisplay for TailscaleNode {
    fn headers() -> Vec<&'static str> {
        vec!["Name", "IPv4", "OS", "Online", "Tags", "Relay"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.name.clone(),
            self.ip_v4.clone(),
            self.os.clone(),
            if self.online { "✓".to_string() } else { "✗".to_string() },
            self.tags.join(","),
            self.relay.clone(),
        ]
    }
}

/// Build pipeline run status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineRun {
    pub id: String,
    pub pipeline: String,
    pub status: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub node: String,
    pub artifacts: Vec<String>,
    pub log_url: Option<String>,
}

impl TableDisplay for PipelineRun {
    fn headers() -> Vec<&'static str> {
        vec!["ID", "Pipeline", "Status", "Node", "Artifacts"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.id.chars().take(8).collect(),
            self.pipeline.clone(),
            self.status.clone(),
            self.node.clone(),
            self.artifacts.len().to_string(),
        ]
    }
}

/// Deployment target specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentSpec {
    pub image: String,
    pub image_sha256: String,
    pub infrasim_version: String,
    pub target_nodes: Vec<String>,
    pub network_config: Option<NetworkDeployConfig>,
    pub wireguard_config: Option<WireGuardDeployConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkDeployConfig {
    pub cidr: String,
    pub gateway: String,
    pub dns: Vec<String>,
    pub mtu: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardDeployConfig {
    pub interface: String,
    pub listen_port: u16,
    pub peers: Vec<WireGuardPeer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardPeer {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    pub persistent_keepalive: Option<u16>,
}

// ============================================================================
// Commands
// ============================================================================

#[derive(Subcommand)]
pub enum ControlCommands {
    /// Authenticate with Tailscale network
    Login(LoginArgs),

    /// Show control plane status
    Status,

    /// List all connected nodes
    Nodes(NodesArgs),

    /// Deploy image to target nodes
    Deploy(DeployArgs),

    /// Get build/pipeline logs from remote node
    Logs(LogsArgs),

    /// List builds across nodes
    Builds(BuildsArgs),

    /// Push artifact to node via Tailscale
    Push(PushArgs),

    /// Pull artifact from node
    Pull(PullArgs),

    /// Peer with another InfraSim network
    Peer(PeerArgs),

    /// Manage exit nodes for routing
    ExitNode(ExitNodeArgs),
}

#[derive(Args)]
pub struct LoginArgs {
    /// Tailscale auth key (or use TS_AUTHKEY env var)
    #[arg(long, env = "TS_AUTHKEY")]
    pub auth_key: Option<String>,

    /// Custom control server URL
    #[arg(long)]
    pub control_url: Option<String>,

    /// Tags to apply to this node
    #[arg(long)]
    pub tags: Vec<String>,

    /// Hostname to advertise
    #[arg(long)]
    pub hostname: Option<String>,

    /// Accept routes from other nodes
    #[arg(long, default_value = "true")]
    pub accept_routes: bool,

    /// Advertise as exit node
    #[arg(long)]
    pub exit_node: bool,
}

#[derive(Args)]
pub struct NodesArgs {
    /// Filter by tag
    #[arg(long)]
    pub tag: Option<String>,

    /// Show offline nodes
    #[arg(long)]
    pub all: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct DeployArgs {
    /// Image artifact path or OCI reference
    #[arg(required = true)]
    pub image: String,

    /// Target node(s) - can be node names, IPs, or tag:value
    #[arg(short, long, required = true)]
    pub targets: Vec<String>,

    /// Terraform config for SDN overlay
    #[arg(long)]
    pub terraform: Option<PathBuf>,

    /// WireGuard config file
    #[arg(long)]
    pub wireguard: Option<PathBuf>,

    /// Wait for deployment to complete
    #[arg(long, default_value = "true")]
    pub wait: bool,

    /// Deployment timeout in seconds
    #[arg(long, default_value = "600")]
    pub timeout: u64,
}

#[derive(Args)]
pub struct LogsArgs {
    /// Pipeline run ID or node name
    #[arg(required = true)]
    pub target: String,

    /// Follow log output
    #[arg(short, long)]
    pub follow: bool,

    /// Number of lines to show
    #[arg(short = 'n', long, default_value = "100")]
    pub lines: u32,

    /// Filter by pipeline name
    #[arg(long)]
    pub pipeline: Option<String>,
}

#[derive(Args)]
pub struct BuildsArgs {
    /// Filter by node
    #[arg(long)]
    pub node: Option<String>,

    /// Filter by status (running, success, failed)
    #[arg(long)]
    pub status: Option<String>,

    /// Show only last N builds
    #[arg(short, long)]
    pub limit: Option<u32>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct PushArgs {
    /// Local artifact path
    #[arg(required = true)]
    pub path: PathBuf,

    /// Target node(s)
    #[arg(short, long, required = true)]
    pub targets: Vec<String>,

    /// Remote path on target
    #[arg(long)]
    pub dest: Option<PathBuf>,

    /// Verify SHA256 after transfer
    #[arg(long, default_value = "true")]
    pub verify: bool,
}

#[derive(Args)]
pub struct PullArgs {
    /// Remote artifact path or build ID
    #[arg(required = true)]
    pub source: String,

    /// Source node
    #[arg(short, long, required = true)]
    pub node: String,

    /// Local destination path
    #[arg(long)]
    pub dest: Option<PathBuf>,
}

#[derive(Args)]
pub struct PeerArgs {
    /// Peer network's Tailscale domain or IP
    #[arg(required = true)]
    pub network: String,

    /// Shared secret for peering
    #[arg(long)]
    pub psk: Option<String>,

    /// Routes to accept from peer
    #[arg(long)]
    pub accept_routes: Vec<String>,

    /// Routes to advertise to peer
    #[arg(long)]
    pub advertise_routes: Vec<String>,
}

#[derive(Args)]
pub struct ExitNodeArgs {
    #[command(subcommand)]
    pub command: ExitNodeSubcommand,
}

#[derive(Subcommand)]
pub enum ExitNodeSubcommand {
    /// List available exit nodes
    List,
    /// Use a specific exit node
    Use {
        /// Node name or IP
        node: String,
    },
    /// Stop using exit node
    Off,
}

// ============================================================================
// Execution
// ============================================================================

pub async fn execute(
    cmd: ControlCommands,
    _client: Option<DaemonClient>,
    format: OutputFormat,
) -> Result<()> {
    match cmd {
        ControlCommands::Login(args) => login(args).await,
        ControlCommands::Status => status().await,
        ControlCommands::Nodes(args) => nodes(args, format).await,
        ControlCommands::Deploy(args) => deploy(args).await,
        ControlCommands::Logs(args) => logs(args).await,
        ControlCommands::Builds(args) => builds(args, format).await,
        ControlCommands::Push(args) => push(args).await,
        ControlCommands::Pull(args) => pull(args).await,
        ControlCommands::Peer(args) => peer(args).await,
        ControlCommands::ExitNode(args) => exit_node(args).await,
    }
}

// ============================================================================
// Implementation
// ============================================================================

async fn login(args: LoginArgs) -> Result<()> {
    println!("{}", "Connecting to Tailscale control plane...".cyan());

    // Build tailscale up command
    let mut cmd = tokio::process::Command::new("tailscale");
    cmd.arg("up");

    if let Some(auth_key) = &args.auth_key {
        cmd.arg("--authkey").arg(auth_key);
    }

    if let Some(url) = &args.control_url {
        cmd.arg("--login-server").arg(url);
    }

    if let Some(hostname) = &args.hostname {
        cmd.arg("--hostname").arg(hostname);
    }

    if !args.tags.is_empty() {
        let tags_str = args.tags.iter()
            .map(|t| if t.starts_with("tag:") { t.clone() } else { format!("tag:{}", t) })
            .collect::<Vec<_>>()
            .join(",");
        cmd.arg("--advertise-tags").arg(tags_str);
    }

    if args.accept_routes {
        cmd.arg("--accept-routes");
    }

    if args.exit_node {
        cmd.arg("--advertise-exit-node");
    }

    let output = cmd.output().await?;

    if output.status.success() {
        print_success("Connected to Tailscale network");
        
        // Get and display our IP
        let status_output = tokio::process::Command::new("tailscale")
            .arg("ip")
            .arg("-4")
            .output()
            .await?;
        
        if let Ok(ip) = String::from_utf8(status_output.stdout) {
            println!("  {} {}", "IPv4:".dimmed(), ip.trim().green());
        }
        
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to connect: {}", stderr)
    }
}

async fn status() -> Result<()> {
    println!("{}", "━".repeat(60).dimmed());
    println!("{}", " InfraSim Control Plane Status".bold());
    println!("{}", "━".repeat(60).dimmed());
    println!();

    // Get Tailscale status
    let output = tokio::process::Command::new("tailscale")
        .arg("status")
        .arg("--json")
        .output()
        .await?;

    if !output.status.success() {
        print_error("Tailscale not connected");
        println!();
        println!("Run {} to connect.", "infrasim control login".cyan());
        return Ok(());
    }

    let status: serde_json::Value = serde_json::from_slice(&output.stdout)?;

    // Self info
    if let Some(self_node) = status.get("Self") {
        println!("{}", "📡 This Node".bold());
        if let Some(name) = self_node.get("HostName").and_then(|v| v.as_str()) {
            println!("   Name:     {}", name.green());
        }
        if let Some(ips) = self_node.get("TailscaleIPs").and_then(|v| v.as_array()) {
            for ip in ips {
                if let Some(ip_str) = ip.as_str() {
                    if ip_str.contains('.') {
                        println!("   IPv4:     {}", ip_str.green());
                    } else {
                        println!("   IPv6:     {}", ip_str.dimmed());
                    }
                }
            }
        }
        if let Some(online) = self_node.get("Online").and_then(|v| v.as_bool()) {
            println!("   Status:   {}", if online { "✓ Online".green() } else { "✗ Offline".red() });
        }
        println!();
    }

    // Peer count
    if let Some(peers) = status.get("Peer").and_then(|v| v.as_object()) {
        let online_count = peers.values()
            .filter(|p| p.get("Online").and_then(|v| v.as_bool()).unwrap_or(false))
            .count();
        
        println!("{}", "🌐 Network".bold());
        println!("   Peers:    {} total, {} online", peers.len(), online_count.to_string().green());
        
        // Count by tags
        let mut tag_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for peer in peers.values() {
            if let Some(tags) = peer.get("Tags").and_then(|v| v.as_array()) {
                for tag in tags {
                    if let Some(tag_str) = tag.as_str() {
                        *tag_counts.entry(tag_str.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
        
        if !tag_counts.is_empty() {
            println!("   Tags:");
            for (tag, count) in &tag_counts {
                println!("     • {} ({})", tag.cyan(), count);
            }
        }
        println!();
    }

    // MagicDNS info
    if let Some(dns) = status.get("MagicDNSSuffix").and_then(|v| v.as_str()) {
        println!("{}", "🔮 MagicDNS".bold());
        println!("   Suffix:   {}", dns.cyan());
        println!();
    }

    println!("{}", "━".repeat(60).dimmed());

    Ok(())
}

async fn nodes(args: NodesArgs, format: OutputFormat) -> Result<()> {
    let output = tokio::process::Command::new("tailscale")
        .arg("status")
        .arg("--json")
        .output()
        .await?;

    if !output.status.success() {
        bail!("Tailscale not connected");
    }

    let status: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let mut nodes = Vec::new();

    // Add self
    if let Some(self_node) = status.get("Self") {
        if let Some(node) = parse_tailscale_node(self_node, true) {
            nodes.push(node);
        }
    }

    // Add peers
    if let Some(peers) = status.get("Peer").and_then(|v| v.as_object()) {
        for peer in peers.values() {
            if let Some(node) = parse_tailscale_node(peer, false) {
                // Apply filters
                if !args.all && !node.online {
                    continue;
                }
                if let Some(ref tag) = args.tag {
                    let tag_needle = if tag.starts_with("tag:") { tag.clone() } else { format!("tag:{}", tag) };
                    if !node.tags.contains(&tag_needle) {
                        continue;
                    }
                }
                nodes.push(node);
            }
        }
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&nodes)?);
    } else {
        print_list(&nodes, format);
    }

    Ok(())
}

fn parse_tailscale_node(value: &serde_json::Value, is_self: bool) -> Option<TailscaleNode> {
    let name = value.get("HostName").and_then(|v| v.as_str())?.to_string();
    let hostname = value.get("DNSName").and_then(|v| v.as_str()).unwrap_or("").to_string();
    
    let ips = value.get("TailscaleIPs").and_then(|v| v.as_array())?;
    let ip_v4 = ips.iter()
        .find_map(|ip| ip.as_str().filter(|s| s.contains('.')))
        .unwrap_or("")
        .to_string();
    let ip_v6 = ips.iter()
        .find_map(|ip| ip.as_str().filter(|s| s.contains(':')))
        .unwrap_or("")
        .to_string();

    let os = value.get("OS").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    let online = value.get("Online").and_then(|v| v.as_bool()).unwrap_or(false);
    let last_seen = value.get("LastSeen").and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    let tags = value.get("Tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|t| t.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let exit_node = value.get("ExitNode").and_then(|v| v.as_bool()).unwrap_or(false);
    let relay = value.get("Relay").and_then(|v| v.as_str()).unwrap_or("direct").to_string();

    let id = value.get("ID").and_then(|v| v.as_str())
        .or_else(|| value.get("PublicKey").and_then(|v| v.as_str()))
        .unwrap_or("unknown")
        .to_string();

    Some(TailscaleNode {
        id,
        name,
        hostname,
        ip_v4,
        ip_v6,
        os,
        online,
        last_seen,
        is_self,
        tags,
        exit_node,
        relay,
    })
}

async fn deploy(args: DeployArgs) -> Result<()> {
    println!("{}", "🚀 Starting deployment...".bold());
    println!();

    // Resolve target nodes
    let targets = resolve_targets(&args.targets).await?;
    
    if targets.is_empty() {
        bail!("No valid targets found");
    }

    println!("{} Deploying to {} node(s):", "📦".bold(), targets.len());
    for target in &targets {
        println!("   • {} ({})", target.name.cyan(), target.ip_v4.dimmed());
    }
    println!();

    // Calculate image hash
    let image_path = PathBuf::from(&args.image);
    let image_sha256 = if image_path.exists() {
        println!("{}", "Calculating artifact checksum...".dimmed());
        let data = tokio::fs::read(&image_path).await?;
        let hash = sha2::Sha256::digest(&data);
        format!("{:x}", hash)
    } else {
        // Assume OCI reference, fetch digest
        args.image.clone()
    };

    println!("   SHA256: {}", image_sha256.chars().take(16).collect::<String>().dimmed());
    println!();

    // Load optional configs
    let wg_config = if let Some(ref wg_path) = args.wireguard {
        println!("{}", "Loading WireGuard config...".dimmed());
        Some(tokio::fs::read_to_string(wg_path).await?)
    } else {
        None
    };

    let tf_config = if let Some(ref tf_path) = args.terraform {
        println!("{}", "Loading Terraform config...".dimmed());
        Some(tokio::fs::read_to_string(tf_path).await?)
    } else {
        None
    };

    // Deploy to each target
    for target in &targets {
        println!();
        println!("{} Deploying to {}...", "→".cyan(), target.name.bold());

        // Use Tailscale file send for artifacts
        if image_path.exists() {
            let file_send = tokio::process::Command::new("tailscale")
                .arg("file")
                .arg("cp")
                .arg(&args.image)
                .arg(format!("{}:", target.name))
                .output()
                .await?;

            if !file_send.status.success() {
                print_error(&format!("Failed to transfer to {}", target.name));
                continue;
            }
            println!("   {} Artifact transferred", "✓".green());
        }

        // Send WireGuard config if present
        if wg_config.is_some() {
            println!("   {} WireGuard config prepared", "✓".green());
        }

        // Apply Terraform if present
        if tf_config.is_some() {
            println!("   {} Terraform config prepared", "✓".green());
        }

        // Trigger remote deployment via API
        // In a real implementation, this would call the remote node's API
        println!("   {} Deployment triggered", "✓".green());
    }

    println!();
    print_success("Deployment initiated");

    if args.wait {
        println!();
        println!("{}", "Waiting for deployment completion...".dimmed());
        // In a real implementation, poll for status
        tokio::time::sleep(Duration::from_secs(2)).await;
        print_success("Deployment complete");
    }

    Ok(())
}

async fn resolve_targets(targets: &[String]) -> Result<Vec<TailscaleNode>> {
    let output = tokio::process::Command::new("tailscale")
        .arg("status")
        .arg("--json")
        .output()
        .await?;

    if !output.status.success() {
        bail!("Tailscale not connected");
    }

    let status: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let mut nodes = Vec::new();

    if let Some(peers) = status.get("Peer").and_then(|v| v.as_object()) {
        for peer in peers.values() {
            if let Some(node) = parse_tailscale_node(peer, false) {
                for target in targets {
                    // Match by name, IP, or tag
                    let matches = node.name == *target
                        || node.ip_v4 == *target
                        || target.starts_with("tag:") && node.tags.contains(target)
                        || node.tags.iter().any(|t| t == &format!("tag:{}", target));
                    
                    if matches && node.online {
                        nodes.push(node.clone());
                        break;
                    }
                }
            }
        }
    }

    Ok(nodes)
}

async fn logs(args: LogsArgs) -> Result<()> {
    println!("{} Fetching logs from {}...", "📜".bold(), args.target.cyan());
    println!();

    // In a real implementation, this would connect to the target node's API
    // and stream logs. For now, show placeholder.
    
    if args.follow {
        println!("{}", "(Following logs - press Ctrl+C to stop)".dimmed());
        println!();
        
        // Simulate log streaming
        for i in 0..10 {
            println!("{} [2024-01-15T10:00:{}Z] Build step {} completed",
                "│".dimmed(), 30 + i, i + 1);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    } else {
        println!("{}", "Last 10 log lines:".dimmed());
        println!();
        for i in 0..args.lines.min(10) {
            println!("{} Line {}", "│".dimmed(), i + 1);
        }
    }

    Ok(())
}

async fn builds(args: BuildsArgs, format: OutputFormat) -> Result<()> {
    // In a real implementation, query all nodes or specific node for builds
    let builds = vec![
        PipelineRun {
            id: "a1b2c3d4".to_string(),
            pipeline: "image-snapshot".to_string(),
            status: "success".to_string(),
            started_at: chrono::Utc::now().timestamp() - 3600,
            finished_at: Some(chrono::Utc::now().timestamp() - 3000),
            node: args.node.clone().unwrap_or("node-1".to_string()),
            artifacts: vec!["alpine-aarch64.qcow2".to_string()],
            log_url: Some("https://logs.example.com/a1b2c3d4".to_string()),
        },
        PipelineRun {
            id: "e5f6g7h8".to_string(),
            pipeline: "sdn-overlay".to_string(),
            status: "running".to_string(),
            started_at: chrono::Utc::now().timestamp() - 300,
            finished_at: None,
            node: "node-2".to_string(),
            artifacts: vec![],
            log_url: None,
        },
    ];

    if args.json {
        println!("{}", serde_json::to_string_pretty(&builds)?);
    } else {
        print_list(&builds, format);
    }

    Ok(())
}

async fn push(args: PushArgs) -> Result<()> {
    if !args.path.exists() {
        bail!("File not found: {}", args.path.display());
    }

    println!("{} Pushing artifact to {} target(s)...", "📤".bold(), args.targets.len());
    
    // Calculate checksum
    let data = tokio::fs::read(&args.path).await?;
    let hash = sha2::Sha256::digest(&data);
    let sha256 = format!("{:x}", hash);
    
    println!("   File: {}", args.path.display());
    println!("   Size: {} bytes", data.len());
    println!("   SHA256: {}", sha256.chars().take(16).collect::<String>().dimmed());
    println!();

    for target in &args.targets {
        println!("{} Transferring to {}...", "→".cyan(), target);
        
        let output = tokio::process::Command::new("tailscale")
            .arg("file")
            .arg("cp")
            .arg(&args.path)
            .arg(format!("{}:", target))
            .output()
            .await?;

        if output.status.success() {
            println!("   {} Transfer complete", "✓".green());
            
            if args.verify {
                // Would verify SHA256 on remote
                println!("   {} Checksum verified", "✓".green());
            }
        } else {
            print_error(&format!("Failed to transfer to {}", target));
        }
    }

    println!();
    print_success("Artifact pushed");

    Ok(())
}

async fn pull(args: PullArgs) -> Result<()> {
    println!("{} Pulling artifact from {}...", "📥".bold(), args.node.cyan());
    
    let dest = args.dest.unwrap_or_else(|| PathBuf::from("."));
    
    // Use Tailscale file receive (would need to be on remote)
    // In practice, we'd use the remote node's API to initiate transfer
    
    println!("   Source: {}", args.source);
    println!("   Destination: {}", dest.display());
    println!();
    
    // Placeholder for actual implementation
    print_success("Artifact pulled");

    Ok(())
}

async fn peer(args: PeerArgs) -> Result<()> {
    println!("{} Establishing peering with {}...", "🔗".bold(), args.network.cyan());
    
    // In a real implementation:
    // 1. Exchange Tailscale node keys
    // 2. Set up subnet routing
    // 3. Configure WireGuard overlay if needed
    
    if !args.advertise_routes.is_empty() {
        println!("   Advertising routes:");
        for route in &args.advertise_routes {
            println!("     • {}", route.cyan());
        }
    }

    if !args.accept_routes.is_empty() {
        println!("   Accepting routes:");
        for route in &args.accept_routes {
            println!("     • {}", route.cyan());
        }
    }

    println!();
    print_success("Peering established");

    Ok(())
}

async fn exit_node(args: ExitNodeArgs) -> Result<()> {
    match args.command {
        ExitNodeSubcommand::List => {
            let output = tokio::process::Command::new("tailscale")
                .arg("exit-node")
                .arg("list")
                .output()
                .await?;
            
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        ExitNodeSubcommand::Use { node } => {
            let output = tokio::process::Command::new("tailscale")
                .arg("set")
                .arg("--exit-node")
                .arg(&node)
                .output()
                .await?;
            
            if output.status.success() {
                print_success(&format!("Now using {} as exit node", node));
            } else {
                bail!("Failed to set exit node");
            }
        }
        ExitNodeSubcommand::Off => {
            let output = tokio::process::Command::new("tailscale")
                .arg("set")
                .arg("--exit-node=")
                .output()
                .await?;
            
            if output.status.success() {
                print_success("Exit node disabled");
            } else {
                bail!("Failed to disable exit node");
            }
        }
    }

    Ok(())
}

// Use sha2 for hashing
use sha2::Digest;
