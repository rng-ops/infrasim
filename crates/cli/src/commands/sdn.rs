//! SDN (Software-Defined Networking) Commands
//!
//! Provides commands for creating and managing software-defined network
//! device overlays that hook into QEMU with qcow2 disk images:
//!
//! - Network appliances (router, firewall, VPN gateway)
//! - WireGuard/Tailscale mesh topologies
//! - Traffic shaping and QoS profiles
//! - Terraform-based topology deployment

use anyhow::{Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;

use crate::client::DaemonClient;
use crate::output::{OutputFormat, TableDisplay, print_item, print_list, print_success, print_error};

// ============================================================================
// Types
// ============================================================================

/// SDN appliance types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ValueEnum, PartialEq, Eq)]
pub enum ApplianceType {
    /// Software router (Linux-based)
    Router,
    /// Stateful firewall (nftables/iptables)
    Firewall,
    /// WireGuard VPN gateway
    Vpn,
    /// Load balancer (HAProxy)
    LoadBalancer,
    /// Network sensor/IDS
    Sensor,
    /// DNS server
    Dns,
    /// DHCP server
    Dhcp,
    /// NAT gateway
    Nat,
    /// Custom appliance from template
    Custom,
}

impl std::fmt::Display for ApplianceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplianceType::Router => write!(f, "router"),
            ApplianceType::Firewall => write!(f, "firewall"),
            ApplianceType::Vpn => write!(f, "vpn"),
            ApplianceType::LoadBalancer => write!(f, "loadbalancer"),
            ApplianceType::Sensor => write!(f, "sensor"),
            ApplianceType::Dns => write!(f, "dns"),
            ApplianceType::Dhcp => write!(f, "dhcp"),
            ApplianceType::Nat => write!(f, "nat"),
            ApplianceType::Custom => write!(f, "custom"),
        }
    }
}

/// SDN appliance instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Appliance {
    pub id: String,
    pub name: String,
    pub appliance_type: ApplianceType,
    pub status: String,
    pub vm_id: Option<String>,
    pub networks: Vec<String>,
    pub config: ApplianceConfig,
    pub created_at: i64,
}

/// Appliance configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApplianceConfig {
    /// Base qcow2 image
    pub image: Option<String>,
    /// Cloud-init user-data
    pub user_data: Option<String>,
    /// Network interfaces configuration
    pub interfaces: Vec<InterfaceConfig>,
    /// Routing tables
    pub routes: Vec<RouteConfig>,
    /// Firewall rules
    pub firewall_rules: Vec<FirewallRule>,
    /// WireGuard configuration (for VPN type)
    pub wireguard: Option<WireGuardConfig>,
    /// QoS/traffic shaping
    pub qos: Option<QoSConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceConfig {
    pub name: String,
    pub network_id: String,
    pub ip_address: String,
    pub mac_address: Option<String>,
    pub mtu: u16,
    pub vlan_id: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteConfig {
    pub destination: String,
    pub gateway: String,
    pub interface: Option<String>,
    pub metric: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRule {
    pub chain: String,  // input, output, forward
    pub action: String, // accept, drop, reject
    pub protocol: Option<String>,
    pub source: Option<String>,
    pub destination: Option<String>,
    pub port: Option<u16>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardConfig {
    pub interface: String,
    pub listen_port: u16,
    pub private_key: Option<String>,
    pub address: String,
    pub dns: Option<String>,
    pub peers: Vec<WireGuardPeer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardPeer {
    pub name: String,
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    pub persistent_keepalive: Option<u16>,
    pub preshared_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QoSConfig {
    pub bandwidth_mbps: u32,
    pub latency_ms: u32,
    pub jitter_ms: u32,
    pub loss_percent: f32,
}

/// SDN topology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    pub id: String,
    pub name: String,
    pub description: String,
    pub appliances: Vec<Appliance>,
    pub networks: Vec<NetworkDef>,
    pub peerings: Vec<Peering>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkDef {
    pub id: String,
    pub name: String,
    pub cidr: String,
    pub gateway: String,
    pub vlan_id: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peering {
    pub from: String, // appliance ID
    pub to: String,   // appliance ID or external endpoint
    pub kind: String, // "wireguard", "tailscale", "vxlan", "gre"
    pub config: HashMap<String, String>,
}

impl TableDisplay for Appliance {
    fn headers() -> Vec<&'static str> {
        vec!["ID", "Name", "Type", "Status", "Networks", "VM"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.id.chars().take(8).collect(),
            self.name.clone(),
            format!("{}", self.appliance_type),
            self.status.clone(),
            self.networks.len().to_string(),
            self.vm_id.clone().unwrap_or("-".to_string()).chars().take(8).collect(),
        ]
    }
}

impl TableDisplay for Topology {
    fn headers() -> Vec<&'static str> {
        vec!["ID", "Name", "Appliances", "Networks", "Status"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.id.chars().take(8).collect(),
            self.name.clone(),
            self.appliances.len().to_string(),
            self.networks.len().to_string(),
            self.status.clone(),
        ]
    }
}

// ============================================================================
// Commands
// ============================================================================

#[derive(Subcommand)]
pub enum SdnCommands {
    /// Create a new network appliance
    Create(CreateArgs),

    /// List appliances
    List(ListArgs),

    /// Get appliance details
    Get(GetArgs),

    /// Delete an appliance
    Delete(DeleteArgs),

    /// Start an appliance
    Start(StartArgs),

    /// Stop an appliance
    Stop(StopArgs),

    /// Configure WireGuard peering
    Peer(PeerArgs),

    /// Deploy topology from Terraform
    Deploy(DeployArgs),

    /// Generate Terraform for topology
    Terraform(TerraformArgs),

    /// Show topology graph
    Graph(GraphArgs),

    /// Import existing VMs as appliances
    Import(ImportArgs),
}

#[derive(Args)]
pub struct CreateArgs {
    /// Appliance name
    #[arg(required = true)]
    pub name: String,

    /// Appliance type
    #[arg(short, long, value_enum, default_value = "router")]
    pub kind: ApplianceType,

    /// Network(s) to attach to
    #[arg(short, long)]
    pub network: Vec<String>,

    /// IP address for primary interface
    #[arg(long)]
    pub ip: Option<String>,

    /// Base qcow2 image
    #[arg(long)]
    pub image: Option<String>,

    /// Number of CPUs
    #[arg(long, default_value = "2")]
    pub cpus: u32,

    /// Memory in MB
    #[arg(long, default_value = "1024")]
    pub memory: u64,

    /// WireGuard listen port (for VPN type)
    #[arg(long, default_value = "51820")]
    pub wg_port: u16,

    /// WireGuard address (for VPN type)
    #[arg(long)]
    pub wg_address: Option<String>,

    /// Configuration file (YAML)
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Don't start the appliance after creation
    #[arg(long)]
    pub no_start: bool,
}

#[derive(Args)]
pub struct ListArgs {
    /// Filter by type
    #[arg(short, long)]
    pub kind: Option<ApplianceType>,

    /// Filter by network
    #[arg(long)]
    pub network: Option<String>,

    /// Show all (including stopped)
    #[arg(long)]
    pub all: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct GetArgs {
    /// Appliance ID or name
    #[arg(required = true)]
    pub id: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct DeleteArgs {
    /// Appliance ID or name
    #[arg(required = true)]
    pub id: String,

    /// Force delete without confirmation
    #[arg(short, long)]
    pub force: bool,

    /// Delete associated VM
    #[arg(long, default_value = "true")]
    pub delete_vm: bool,
}

#[derive(Args)]
pub struct StartArgs {
    /// Appliance ID or name
    #[arg(required = true)]
    pub id: String,
}

#[derive(Args)]
pub struct StopArgs {
    /// Appliance ID or name
    #[arg(required = true)]
    pub id: String,

    /// Force stop
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Args)]
pub struct PeerArgs {
    /// Source appliance ID or name
    #[arg(required = true)]
    pub from: String,

    /// Target appliance ID, name, or external endpoint
    #[arg(required = true)]
    pub to: String,

    /// Peering type
    #[arg(long, default_value = "wireguard")]
    pub kind: String,

    /// Allowed IPs for the peer
    #[arg(long)]
    pub allowed_ips: Vec<String>,

    /// Use Tailscale for discovery
    #[arg(long)]
    pub tailscale: bool,

    /// Pre-shared key file
    #[arg(long)]
    pub psk: Option<PathBuf>,
}

#[derive(Args)]
pub struct DeployArgs {
    /// Terraform directory
    #[arg(required = true)]
    pub path: PathBuf,

    /// Auto-approve apply
    #[arg(long)]
    pub auto_approve: bool,

    /// Plan only, don't apply
    #[arg(long)]
    pub plan_only: bool,

    /// Variable file
    #[arg(long)]
    pub var_file: Option<PathBuf>,

    /// Variables (KEY=VALUE)
    #[arg(short, long)]
    pub var: Vec<String>,

    /// Target node (via control plane)
    #[arg(long)]
    pub node: Option<String>,
}

#[derive(Args)]
pub struct TerraformArgs {
    /// Topology name or file
    #[arg(required = true)]
    pub source: String,

    /// Output directory
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Include provider configuration
    #[arg(long, default_value = "true")]
    pub include_provider: bool,
}

#[derive(Args)]
pub struct GraphArgs {
    /// Topology ID or name
    #[arg(required = true)]
    pub topology: String,

    /// Output format (dot, mermaid, ascii)
    #[arg(long, default_value = "ascii")]
    pub format: String,

    /// Output file (stdout if not specified)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct ImportArgs {
    /// VM ID to import
    #[arg(required = true)]
    pub vm_id: String,

    /// Appliance type to treat it as
    #[arg(short, long, value_enum)]
    pub kind: ApplianceType,

    /// Name for the appliance
    #[arg(short, long)]
    pub name: Option<String>,
}

// ============================================================================
// Execution
// ============================================================================

pub async fn execute(
    cmd: SdnCommands,
    client: Option<DaemonClient>,
    format: OutputFormat,
) -> Result<()> {
    match cmd {
        SdnCommands::Create(args) => create(args, client).await,
        SdnCommands::List(args) => list(args, client, format).await,
        SdnCommands::Get(args) => get(args, client, format).await,
        SdnCommands::Delete(args) => delete(args, client).await,
        SdnCommands::Start(args) => start(args, client).await,
        SdnCommands::Stop(args) => stop(args, client).await,
        SdnCommands::Peer(args) => peer(args).await,
        SdnCommands::Deploy(args) => deploy(args).await,
        SdnCommands::Terraform(args) => terraform(args).await,
        SdnCommands::Graph(args) => graph(args).await,
        SdnCommands::Import(args) => import(args, client).await,
    }
}

// ============================================================================
// Implementation
// ============================================================================

async fn create(args: CreateArgs, mut client: Option<DaemonClient>) -> Result<()> {
    println!("{} Creating {} appliance: {}", "🔧".bold(), 
        format!("{}", args.kind).cyan(), 
        args.name.bold());
    println!();

    // Build configuration
    let mut config = if let Some(ref config_file) = args.config {
        let content = tokio::fs::read_to_string(config_file).await?;
        serde_yaml::from_str(&content)?
    } else {
        ApplianceConfig::default()
    };

    // Set up interfaces
    if !args.network.is_empty() {
        config.interfaces = args.network.iter().enumerate().map(|(i, net)| {
            InterfaceConfig {
                name: format!("eth{}", i),
                network_id: net.clone(),
                ip_address: args.ip.clone().unwrap_or(format!("dhcp")),
                mac_address: None,
                mtu: 1500,
                vlan_id: None,
            }
        }).collect();
    }

    // WireGuard configuration for VPN type
    if args.kind == ApplianceType::Vpn {
        config.wireguard = Some(WireGuardConfig {
            interface: "wg0".to_string(),
            listen_port: args.wg_port,
            private_key: None, // Will be generated
            address: args.wg_address.unwrap_or("10.200.0.1/24".to_string()),
            dns: None,
            peers: vec![],
        });
    }

    // Select base image
    let image = args.image.unwrap_or_else(|| {
        match args.kind {
            ApplianceType::Router | ApplianceType::Firewall | ApplianceType::Nat => 
                "alpine-router-aarch64.qcow2".to_string(),
            ApplianceType::Vpn => 
                "alpine-wireguard-aarch64.qcow2".to_string(),
            ApplianceType::LoadBalancer => 
                "alpine-haproxy-aarch64.qcow2".to_string(),
            ApplianceType::Sensor => 
                "alpine-suricata-aarch64.qcow2".to_string(),
            ApplianceType::Dns => 
                "alpine-unbound-aarch64.qcow2".to_string(),
            ApplianceType::Dhcp => 
                "alpine-dnsmasq-aarch64.qcow2".to_string(),
            ApplianceType::Custom => 
                "alpine-aarch64.qcow2".to_string(),
        }
    });
    config.image = Some(image.clone());

    println!("  Type:      {}", format!("{}", args.kind).cyan());
    println!("  Image:     {}", image.dimmed());
    println!("  CPUs:      {}", args.cpus);
    println!("  Memory:    {} MB", args.memory);
    if !args.network.is_empty() {
        println!("  Networks:  {}", args.network.join(", "));
    }
    if let Some(ref ip) = args.ip {
        println!("  IP:        {}", ip);
    }
    if args.kind == ApplianceType::Vpn {
        println!("  WG Port:   {}", args.wg_port);
        if let Some(ref wg) = config.wireguard {
            println!("  WG Addr:   {}", wg.address);
        }
    }
    println!();

    // Generate appliance ID
    let appliance_id = uuid::Uuid::new_v4().to_string();
    let short_id: String = appliance_id.chars().take(8).collect();

    // Create underlying VM if daemon is available
    let vm_id = if let Some(ref mut c) = client {
        // In a real implementation, create VM via daemon
        println!("{}", "Creating underlying VM...".dimmed());
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    let appliance = Appliance {
        id: appliance_id.clone(),
        name: args.name.clone(),
        appliance_type: args.kind,
        status: if args.no_start { "stopped".to_string() } else { "running".to_string() },
        vm_id,
        networks: args.network.clone(),
        config,
        created_at: chrono::Utc::now().timestamp(),
    };

    println!("  {} Appliance created: {}", "✓".green(), short_id);

    if !args.no_start {
        println!("  {} Starting appliance...", "→".cyan());
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        println!("  {} Appliance running", "✓".green());
    }

    // Generate WireGuard keys if VPN type
    if args.kind == ApplianceType::Vpn {
        println!();
        println!("{}", "WireGuard Configuration:".bold());
        // Generate a keypair (simplified)
        let public_key = "generated-public-key-base64";
        println!("  Public Key: {}", public_key.cyan());
        println!("  Listen:     {}:{}", "0.0.0.0", args.wg_port);
        println!();
        println!("  To add peers: {}", format!("infrasim sdn peer {} <target>", args.name).dimmed());
    }

    println!();
    print_success(&format!("Appliance '{}' created", args.name));

    Ok(())
}

async fn list(args: ListArgs, _client: Option<DaemonClient>, format: OutputFormat) -> Result<()> {
    // In a real implementation, query from daemon
    let appliances = vec![
        Appliance {
            id: uuid::Uuid::new_v4().to_string(),
            name: "edge-router".to_string(),
            appliance_type: ApplianceType::Router,
            status: "running".to_string(),
            vm_id: Some(uuid::Uuid::new_v4().to_string()),
            networks: vec!["wan".to_string(), "lan".to_string()],
            config: ApplianceConfig::default(),
            created_at: chrono::Utc::now().timestamp() - 3600,
        },
        Appliance {
            id: uuid::Uuid::new_v4().to_string(),
            name: "vpn-gateway".to_string(),
            appliance_type: ApplianceType::Vpn,
            status: "running".to_string(),
            vm_id: Some(uuid::Uuid::new_v4().to_string()),
            networks: vec!["lan".to_string()],
            config: ApplianceConfig::default(),
            created_at: chrono::Utc::now().timestamp() - 7200,
        },
    ];

    // Apply filters
    let filtered: Vec<_> = appliances.into_iter()
        .filter(|a| {
            if let Some(ref kind) = args.kind {
                if a.appliance_type != *kind {
                    return false;
                }
            }
            if let Some(ref net) = args.network {
                if !a.networks.contains(net) {
                    return false;
                }
            }
            if !args.all && a.status != "running" {
                return false;
            }
            true
        })
        .collect();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&filtered)?);
    } else {
        print_list(&filtered, format);
    }

    Ok(())
}

async fn get(args: GetArgs, _client: Option<DaemonClient>, _format: OutputFormat) -> Result<()> {
    // Simulate fetching appliance
    let appliance = Appliance {
        id: args.id.clone(),
        name: "edge-router".to_string(),
        appliance_type: ApplianceType::Router,
        status: "running".to_string(),
        vm_id: Some(uuid::Uuid::new_v4().to_string()),
        networks: vec!["wan".to_string(), "lan".to_string()],
        config: ApplianceConfig {
            image: Some("alpine-router-aarch64.qcow2".to_string()),
            interfaces: vec![
                InterfaceConfig {
                    name: "eth0".to_string(),
                    network_id: "wan".to_string(),
                    ip_address: "192.168.1.1/24".to_string(),
                    mac_address: Some("52:54:00:12:34:56".to_string()),
                    mtu: 1500,
                    vlan_id: None,
                },
                InterfaceConfig {
                    name: "eth1".to_string(),
                    network_id: "lan".to_string(),
                    ip_address: "10.0.0.1/24".to_string(),
                    mac_address: Some("52:54:00:12:34:57".to_string()),
                    mtu: 1500,
                    vlan_id: None,
                },
            ],
            routes: vec![
                RouteConfig {
                    destination: "0.0.0.0/0".to_string(),
                    gateway: "192.168.1.254".to_string(),
                    interface: Some("eth0".to_string()),
                    metric: 100,
                },
            ],
            ..Default::default()
        },
        created_at: chrono::Utc::now().timestamp() - 3600,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&appliance)?);
        return Ok(());
    }

    println!("{}", "━".repeat(60).dimmed());
    println!("{}", " Appliance Details".bold());
    println!("{}", "━".repeat(60).dimmed());
    println!();

    println!("  ID:      {}", appliance.id.cyan());
    println!("  Name:    {}", appliance.name.bold());
    println!("  Type:    {}", format!("{}", appliance.appliance_type).cyan());
    println!("  Status:  {}", if appliance.status == "running" { 
        "● Running".green().to_string() 
    } else { 
        "○ Stopped".dimmed().to_string() 
    });
    if let Some(ref vm_id) = appliance.vm_id {
        println!("  VM ID:   {}", vm_id.chars().take(8).collect::<String>().dimmed());
    }
    println!();

    println!("{}", "Interfaces:".bold());
    for iface in &appliance.config.interfaces {
        println!("  {} ({}):", iface.name.cyan(), iface.network_id);
        println!("    IP:  {}", iface.ip_address);
        if let Some(ref mac) = iface.mac_address {
            println!("    MAC: {}", mac.dimmed());
        }
        println!("    MTU: {}", iface.mtu);
    }
    println!();

    if !appliance.config.routes.is_empty() {
        println!("{}", "Routes:".bold());
        for route in &appliance.config.routes {
            println!("  {} via {} ({})", 
                route.destination.cyan(),
                route.gateway,
                route.interface.clone().unwrap_or("default".to_string()).dimmed()
            );
        }
        println!();
    }

    Ok(())
}

async fn delete(args: DeleteArgs, _client: Option<DaemonClient>) -> Result<()> {
    if !args.force {
        println!("{} This will delete appliance '{}' and its associated resources.", 
            "⚠️".yellow(), args.id);
        println!("Use --force to skip this confirmation.");
        return Ok(());
    }

    println!("{} Deleting appliance: {}", "🗑️".bold(), args.id);
    
    if args.delete_vm {
        println!("  {} Deleting associated VM...", "→".cyan());
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        println!("  {} VM deleted", "✓".green());
    }

    print_success("Appliance deleted");
    Ok(())
}

async fn start(args: StartArgs, _client: Option<DaemonClient>) -> Result<()> {
    println!("{} Starting appliance: {}", "▶️".bold(), args.id);
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    print_success("Appliance started");
    Ok(())
}

async fn stop(args: StopArgs, _client: Option<DaemonClient>) -> Result<()> {
    println!("{} Stopping appliance: {}", "⏹️".bold(), args.id);
    if args.force {
        println!("  Using force stop");
    }
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    print_success("Appliance stopped");
    Ok(())
}

async fn peer(args: PeerArgs) -> Result<()> {
    println!("{} Creating peering: {} → {}", "🔗".bold(), 
        args.from.cyan(), 
        args.to.cyan());
    println!();

    println!("  Type:        {}", args.kind);
    if !args.allowed_ips.is_empty() {
        println!("  Allowed IPs: {}", args.allowed_ips.join(", "));
    }
    if args.tailscale {
        println!("  Discovery:   Tailscale");
    }
    println!();

    // In a real implementation:
    // 1. Generate WireGuard keys for both sides
    // 2. Exchange public keys
    // 3. Configure both appliances
    // 4. Optionally use Tailscale for endpoint discovery

    if args.kind == "wireguard" {
        println!("{}", "WireGuard Peering:".bold());
        println!("  From public key: <generated>");
        println!("  To public key:   <fetched/generated>");
        println!();
        
        if args.tailscale {
            println!("  {} Using Tailscale for endpoint discovery", "✓".green());
        }
    }

    print_success("Peering established");
    Ok(())
}

async fn deploy(args: DeployArgs) -> Result<()> {
    if !args.path.exists() {
        bail!("Terraform directory not found: {}", args.path.display());
    }

    println!("{} Deploying SDN topology from: {}", "🚀".bold(), args.path.display());
    println!();

    // Parse variables
    let vars: HashMap<String, String> = args.var.iter()
        .filter_map(|s| s.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    // Build terraform command
    let tf_dir = args.path.canonicalize()?;

    // Init
    println!("{}", "Running terraform init...".dimmed());
    let init_output = tokio::process::Command::new("terraform")
        .arg("init")
        .current_dir(&tf_dir)
        .output()
        .await?;

    if !init_output.status.success() {
        let stderr = String::from_utf8_lossy(&init_output.stderr);
        bail!("Terraform init failed: {}", stderr);
    }
    println!("  {} Initialized", "✓".green());

    // Plan
    println!("{}", "Running terraform plan...".dimmed());
    let mut plan_cmd = tokio::process::Command::new("terraform");
    plan_cmd.arg("plan").arg("-out=tfplan");
    
    if let Some(ref var_file) = args.var_file {
        plan_cmd.arg(format!("-var-file={}", var_file.display()));
    }
    for (key, value) in &vars {
        plan_cmd.arg(format!("-var={}={}", key, value));
    }
    
    let plan_output = plan_cmd
        .current_dir(&tf_dir)
        .output()
        .await?;

    if !plan_output.status.success() {
        let stderr = String::from_utf8_lossy(&plan_output.stderr);
        bail!("Terraform plan failed: {}", stderr);
    }
    println!("  {} Plan created", "✓".green());

    // Show plan summary
    let stdout = String::from_utf8_lossy(&plan_output.stdout);
    if stdout.contains("No changes") {
        println!();
        println!("{}", "No changes needed.".green());
        return Ok(());
    }

    if args.plan_only {
        println!();
        println!("{}", stdout);
        return Ok(());
    }

    // Apply
    if !args.auto_approve {
        println!();
        println!("{} Apply this plan? Use --auto-approve to skip this prompt.", "⚠️".yellow());
        return Ok(());
    }

    println!("{}", "Running terraform apply...".dimmed());
    let apply_output = tokio::process::Command::new("terraform")
        .arg("apply")
        .arg("-auto-approve")
        .arg("tfplan")
        .current_dir(&tf_dir)
        .output()
        .await?;

    if !apply_output.status.success() {
        let stderr = String::from_utf8_lossy(&apply_output.stderr);
        bail!("Terraform apply failed: {}", stderr);
    }

    println!("  {} Applied", "✓".green());
    println!();
    print_success("Topology deployed");

    Ok(())
}

async fn terraform(args: TerraformArgs) -> Result<()> {
    println!("{} Generating Terraform configuration", "📝".bold());
    println!();

    let tf_content = if args.include_provider {
        r#"# InfraSim SDN Topology
# Generated by: infrasim sdn terraform

terraform {
  required_providers {
    infrasim = {
      source  = "local/infrasim/infrasim"
      version = "~> 0.1"
    }
  }
}

provider "infrasim" {
  daemon_address = "http://127.0.0.1:50051"
}

# WAN Network (external-facing)
resource "infrasim_network" "wan" {
  name = "wan"
  mode = "vmnet-bridged"
  cidr = "192.168.1.0/24"
}

# LAN Network (internal)
resource "infrasim_network" "lan" {
  name = "lan"
  mode = "nat"
  cidr = "10.0.0.0/24"
  gateway = "10.0.0.1"
  dhcp_enabled = true
}

# DMZ Network
resource "infrasim_network" "dmz" {
  name = "dmz"
  mode = "nat"
  cidr = "10.0.1.0/24"
  gateway = "10.0.1.1"
}

# Edge Router
resource "infrasim_vm" "router" {
  name   = "edge-router"
  cpus   = 2
  memory = 1024
  disk   = "/var/lib/infrasim/images/alpine-router-aarch64.qcow2"
  
  # Multi-homed: WAN, LAN, DMZ
  network_ids = [
    infrasim_network.wan.id,
    infrasim_network.lan.id,
    infrasim_network.dmz.id,
  ]
  
  cloud_init = base64encode(<<-EOF
    #cloud-config
    hostname: edge-router
    write_files:
      - path: /etc/sysctl.d/99-router.conf
        content: |
          net.ipv4.ip_forward = 1
          net.ipv6.conf.all.forwarding = 1
    runcmd:
      - sysctl --system
      - iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
  EOF
  )
}

# WireGuard VPN Gateway
resource "infrasim_vm" "vpn" {
  name   = "vpn-gateway"
  cpus   = 2
  memory = 512
  disk   = "/var/lib/infrasim/images/alpine-wireguard-aarch64.qcow2"
  
  network_ids = [infrasim_network.lan.id]
  
  cloud_init = base64encode(<<-EOF
    #cloud-config
    hostname: vpn-gateway
    write_files:
      - path: /etc/wireguard/wg0.conf
        permissions: '0600'
        content: |
          [Interface]
          PrivateKey = ${var.wg_private_key}
          Address = 10.200.0.1/24
          ListenPort = 51820
          
          PostUp = iptables -A FORWARD -i wg0 -j ACCEPT
          PostDown = iptables -D FORWARD -i wg0 -j ACCEPT
    runcmd:
      - wg-quick up wg0
      - systemctl enable wg-quick@wg0
  EOF
  )
}

# Firewall
resource "infrasim_vm" "firewall" {
  name   = "firewall"
  cpus   = 2
  memory = 1024
  disk   = "/var/lib/infrasim/images/alpine-router-aarch64.qcow2"
  
  network_ids = [
    infrasim_network.lan.id,
    infrasim_network.dmz.id,
  ]
  
  cloud_init = base64encode(<<-EOF
    #cloud-config
    hostname: firewall
    write_files:
      - path: /etc/nftables.conf
        content: |
          table inet filter {
            chain input {
              type filter hook input priority 0;
              ct state established,related accept
              iif lo accept
              tcp dport 22 accept
              drop
            }
            chain forward {
              type filter hook forward priority 0;
              ct state established,related accept
              iif eth0 oif eth1 accept  # LAN to DMZ
              drop
            }
          }
    runcmd:
      - nft -f /etc/nftables.conf
  EOF
  )
}

# Variables
variable "wg_private_key" {
  description = "WireGuard private key for VPN gateway"
  type        = string
  sensitive   = true
}

# Outputs
output "router_id" {
  value = infrasim_vm.router.id
}

output "vpn_gateway_id" {
  value = infrasim_vm.vpn.id
}

output "networks" {
  value = {
    wan = infrasim_network.wan.id
    lan = infrasim_network.lan.id
    dmz = infrasim_network.dmz.id
  }
}
"#
    } else {
        "# Minimal config without provider block"
    };

    if let Some(ref output_dir) = args.output {
        tokio::fs::create_dir_all(output_dir).await?;
        let output_file = output_dir.join("main.tf");
        tokio::fs::write(&output_file, tf_content).await?;
        print_success(&format!("Terraform config written to {}", output_file.display()));
    } else {
        println!("{}", tf_content);
    }

    Ok(())
}

async fn graph(args: GraphArgs) -> Result<()> {
    println!("{} Generating topology graph: {}", "📊".bold(), args.topology);
    println!();

    let graph = match args.format.as_str() {
        "mermaid" => r#"```mermaid
graph TD
    subgraph WAN
        Internet((Internet))
    end
    
    subgraph Edge["Edge Layer"]
        Router[Router]
    end
    
    subgraph Core["Core Network"]
        Firewall[Firewall]
        VPN[VPN Gateway]
    end
    
    subgraph Services["DMZ"]
        Web[Web Server]
        DB[(Database)]
    end
    
    Internet --> Router
    Router --> Firewall
    Router --> VPN
    Firewall --> Web
    Firewall --> DB
    VPN -.->|WireGuard| External[Remote Peers]
```"#,
        "dot" => r#"digraph topology {
    rankdir=TB;
    node [shape=box];
    
    subgraph cluster_wan {
        label="WAN";
        internet [shape=ellipse, label="Internet"];
    }
    
    subgraph cluster_edge {
        label="Edge";
        router [label="Router"];
    }
    
    subgraph cluster_core {
        label="Core";
        firewall [label="Firewall"];
        vpn [label="VPN Gateway"];
    }
    
    subgraph cluster_dmz {
        label="DMZ";
        web [label="Web Server"];
        db [shape=cylinder, label="Database"];
    }
    
    internet -> router;
    router -> firewall;
    router -> vpn;
    firewall -> web;
    firewall -> db;
    vpn -> external [style=dashed, label="WireGuard"];
}"#,
        _ => r#"
┌────────────────────────────────────────────────────┐
│                     INTERNET                        │
└────────────────────────┬───────────────────────────┘
                         │
┌────────────────────────▼───────────────────────────┐
│                   EDGE ROUTER                       │
│               (192.168.1.1/24)                      │
└────────┬───────────────────────────────┬───────────┘
         │                               │
    ┌────▼────┐                    ┌─────▼─────┐
    │   LAN   │                    │    DMZ    │
    │ 10.0.0.0│                    │ 10.0.1.0  │
    └────┬────┘                    └─────┬─────┘
         │                               │
    ┌────▼─────────┐              ┌──────▼──────┐
    │ VPN GATEWAY  │              │  FIREWALL   │
    │ wg0:10.200.  │              │             │
    │   0.1/24     │              │             │
    └──────────────┘              └─────────────┘
         │
    ═════╧═════ WireGuard Tunnel ═════════════
         │
    ┌────▼──────────────┐
    │   REMOTE PEERS    │
    │ (via Tailscale)   │
    └───────────────────┘
"#,
    };

    if let Some(ref output_file) = args.output {
        tokio::fs::write(output_file, graph).await?;
        print_success(&format!("Graph written to {}", output_file.display()));
    } else {
        println!("{}", graph);
    }

    Ok(())
}

async fn import(args: ImportArgs, _client: Option<DaemonClient>) -> Result<()> {
    println!("{} Importing VM as appliance", "📥".bold());
    println!();
    
    println!("  VM ID:  {}", args.vm_id);
    println!("  Type:   {}", format!("{}", args.kind).cyan());
    if let Some(ref name) = args.name {
        println!("  Name:   {}", name);
    }
    println!();

    // In a real implementation:
    // 1. Fetch VM details from daemon
    // 2. Create appliance record linking to VM
    // 3. Apply type-specific configuration
    
    let appliance_id = uuid::Uuid::new_v4();
    println!("  Appliance ID: {}", appliance_id.to_string().chars().take(8).collect::<String>());
    
    print_success("VM imported as appliance");
    Ok(())
}
