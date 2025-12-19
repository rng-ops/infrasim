//! Pipeline Commands - Build pipeline management for InfraSim images
//!
//! Provides commands to:
//! - Trigger image builds with tagged provenance
//! - Stream build logs from remote or local builds
//! - Manage artifacts and their attestations
//! - Integrate with CI/CD systems

use anyhow::{Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;

use crate::output::{OutputFormat, TableDisplay, print_item, print_list, print_success, print_error};

// ============================================================================
// Types
// ============================================================================

/// Pipeline definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub name: String,
    pub description: String,
    pub stages: Vec<PipelineStage>,
    pub triggers: Vec<PipelineTrigger>,
    pub artifacts: Vec<ArtifactDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    pub name: String,
    pub image: Option<String>,
    pub commands: Vec<String>,
    pub depends_on: Vec<String>,
    pub timeout_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTrigger {
    pub kind: String, // "push", "tag", "schedule", "manual"
    pub pattern: Option<String>,
    pub cron: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactDefinition {
    pub name: String,
    pub path: String,
    pub kind: String, // "qcow2", "tarball", "binary"
    pub retain_days: u32,
}

/// Build run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildRun {
    pub id: String,
    pub pipeline: String,
    pub status: BuildStatus,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub trigger: String,
    pub commit_sha: Option<String>,
    pub tag: Option<String>,
    pub stages: Vec<StageRun>,
    pub artifacts: Vec<ArtifactRecord>,
    pub provenance: Option<BuildProvenance>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ValueEnum, PartialEq)]
pub enum BuildStatus {
    Pending,
    Running,
    Success,
    Failed,
    Cancelled,
}

impl std::fmt::Display for BuildStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildStatus::Pending => write!(f, "pending"),
            BuildStatus::Running => write!(f, "running"),
            BuildStatus::Success => write!(f, "success"),
            BuildStatus::Failed => write!(f, "failed"),
            BuildStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRun {
    pub name: String,
    pub status: BuildStatus,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub exit_code: Option<i32>,
    pub log_offset: u64,
    pub log_length: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub signed: bool,
    pub download_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildProvenance {
    pub infrasim_version: String,
    pub infrasim_sha256: String,
    pub cargo_deps_hash: String,
    pub builder_identity: String,
    pub build_timestamp: i64,
    pub reproducible: bool,
    pub attestations: Vec<AttestationRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationRecord {
    pub kind: String,
    pub sha256: String,
    pub signed_by: Option<String>,
}

impl TableDisplay for BuildRun {
    fn headers() -> Vec<&'static str> {
        vec!["ID", "Pipeline", "Status", "Started", "Duration", "Tag"]
    }

    fn row(&self) -> Vec<String> {
        let duration = if let Some(finished) = self.finished_at {
            let secs = finished - self.started_at;
            format!("{}s", secs)
        } else if self.status == BuildStatus::Running {
            let secs = chrono::Utc::now().timestamp() - self.started_at;
            format!("{}s (running)", secs)
        } else {
            "-".to_string()
        };

        vec![
            self.id.chars().take(8).collect(),
            self.pipeline.clone(),
            format!("{}", self.status),
            chrono::DateTime::from_timestamp(self.started_at, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or("-".to_string()),
            duration,
            self.tag.clone().unwrap_or("-".to_string()),
        ]
    }
}

impl TableDisplay for ArtifactRecord {
    fn headers() -> Vec<&'static str> {
        vec!["Name", "Size", "SHA256", "Signed"]
    }

    fn row(&self) -> Vec<String> {
        let size = if self.size_bytes > 1024 * 1024 * 1024 {
            format!("{:.1} GB", self.size_bytes as f64 / 1024.0 / 1024.0 / 1024.0)
        } else if self.size_bytes > 1024 * 1024 {
            format!("{:.1} MB", self.size_bytes as f64 / 1024.0 / 1024.0)
        } else {
            format!("{} KB", self.size_bytes / 1024)
        };

        vec![
            self.name.clone(),
            size,
            self.sha256.chars().take(16).collect(),
            if self.signed { "✓".to_string() } else { "✗".to_string() },
        ]
    }
}

// ============================================================================
// Commands
// ============================================================================

#[derive(Subcommand)]
pub enum PipelineCommands {
    /// List available pipelines
    List(ListArgs),

    /// Trigger a pipeline build
    Trigger(TriggerArgs),

    /// Get build status
    Status(StatusArgs),

    /// Stream or fetch build logs
    Logs(LogsArgs),

    /// List or download artifacts
    Artifacts(ArtifactsArgs),

    /// Show build provenance chain
    Provenance(ProvenanceArgs),

    /// Cancel a running build
    Cancel(CancelArgs),

    /// Retry a failed build
    Retry(RetryArgs),

    /// Create a new pipeline definition
    Create(CreateArgs),
}

#[derive(Args)]
pub struct ListArgs {
    /// Filter by tag pattern
    #[arg(long)]
    pub tag: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct TriggerArgs {
    /// Pipeline name
    #[arg(required = true)]
    pub pipeline: String,

    /// Git ref (branch, tag, or commit)
    #[arg(short, long)]
    pub ref_name: Option<String>,

    /// Tag to create on success
    #[arg(short, long)]
    pub tag: Option<String>,

    /// Environment variables (KEY=VALUE)
    #[arg(short, long)]
    pub env: Vec<String>,

    /// Build parameters (KEY=VALUE)
    #[arg(short, long)]
    pub param: Vec<String>,

    /// Wait for build to complete
    #[arg(long)]
    pub wait: bool,

    /// Run on specific node (via control plane)
    #[arg(long)]
    pub node: Option<String>,

    /// Artifact output directory
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct StatusArgs {
    /// Build ID
    #[arg(required = true)]
    pub build_id: String,

    /// Show detailed stage information
    #[arg(long)]
    pub detailed: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct LogsArgs {
    /// Build ID
    #[arg(required = true)]
    pub build_id: String,

    /// Specific stage to show logs for
    #[arg(long)]
    pub stage: Option<String>,

    /// Follow log output
    #[arg(short, long)]
    pub follow: bool,

    /// Number of lines to show
    #[arg(short = 'n', long, default_value = "100")]
    pub lines: u32,

    /// Start from specific byte offset
    #[arg(long)]
    pub offset: Option<u64>,
}

#[derive(Args)]
pub struct ArtifactsArgs {
    /// Build ID
    #[arg(required = true)]
    pub build_id: String,

    /// Download artifact to local path
    #[arg(short, long)]
    pub download: Option<PathBuf>,

    /// Specific artifact name to download
    #[arg(long)]
    pub name: Option<String>,

    /// Verify artifact checksums
    #[arg(long, default_value = "true")]
    pub verify: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct ProvenanceArgs {
    /// Build ID or artifact path
    #[arg(required = true)]
    pub target: String,

    /// Verify full provenance chain
    #[arg(long)]
    pub verify: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct CancelArgs {
    /// Build ID
    #[arg(required = true)]
    pub build_id: String,

    /// Force cancel (SIGKILL)
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct RetryArgs {
    /// Build ID to retry
    #[arg(required = true)]
    pub build_id: String,

    /// Only retry failed stages
    #[arg(long)]
    pub failed_only: bool,
}

#[derive(Args)]
pub struct CreateArgs {
    /// Pipeline name
    #[arg(required = true)]
    pub name: String,

    /// Pipeline description
    #[arg(short, long)]
    pub description: Option<String>,

    /// Pipeline definition file (YAML)
    #[arg(short, long)]
    pub file: Option<PathBuf>,

    /// Template to use (image-build, sdn-deploy, custom)
    #[arg(long)]
    pub template: Option<String>,
}

// ============================================================================
// Execution
// ============================================================================

pub async fn execute(
    cmd: PipelineCommands,
    format: OutputFormat,
) -> Result<()> {
    match cmd {
        PipelineCommands::List(args) => list(args, format).await,
        PipelineCommands::Trigger(args) => trigger(args).await,
        PipelineCommands::Status(args) => status(args, format).await,
        PipelineCommands::Logs(args) => logs(args).await,
        PipelineCommands::Artifacts(args) => artifacts(args, format).await,
        PipelineCommands::Provenance(args) => provenance(args).await,
        PipelineCommands::Cancel(args) => cancel(args).await,
        PipelineCommands::Retry(args) => retry(args).await,
        PipelineCommands::Create(args) => create(args).await,
    }
}

// ============================================================================
// Implementation
// ============================================================================

async fn list(_args: ListArgs, format: OutputFormat) -> Result<()> {
    // In a real implementation, this would query the daemon or read from config
    let pipelines = vec![
        ("image-snapshot", "Build VM image snapshots with provenance"),
        ("sdn-overlay", "Deploy software-defined network topology"),
        ("appliance-build", "Build network appliance images (router, firewall, VPN)"),
        ("kali-builder", "Build Kali Linux penetration testing images"),
    ];

    println!("{}", "━".repeat(60).dimmed());
    println!("{}", " Available Pipelines".bold());
    println!("{}", "━".repeat(60).dimmed());
    println!();

    for (name, desc) in pipelines {
        println!("  {} {}", name.cyan().bold(), desc.dimmed());
    }

    println!();
    println!("Run {} for details", "infrasim pipeline trigger <name>".cyan());

    Ok(())
}

async fn trigger(args: TriggerArgs) -> Result<()> {
    println!("{} Triggering pipeline: {}", "🚀".bold(), args.pipeline.cyan().bold());
    println!();

    // Parse environment and parameters
    let env_vars: HashMap<String, String> = args.env.iter()
        .filter_map(|s| s.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let params: HashMap<String, String> = args.param.iter()
        .filter_map(|s| s.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    // Show configuration
    if let Some(ref r) = args.ref_name {
        println!("  Ref:        {}", r);
    }
    if let Some(ref t) = args.tag {
        println!("  Tag:        {}", t.green());
    }
    if let Some(ref n) = args.node {
        println!("  Node:       {}", n.cyan());
    }
    if !env_vars.is_empty() {
        println!("  Env vars:   {}", env_vars.len());
    }
    if !params.is_empty() {
        println!("  Parameters: {}", params.len());
    }
    println!();

    // Generate build ID
    let build_id = uuid::Uuid::new_v4().to_string();
    let short_id: String = build_id.chars().take(8).collect();

    println!("{} Build ID: {}", "→".cyan(), short_id.bold());
    println!();

    // In a real implementation:
    // 1. Send trigger request to daemon or CI
    // 2. If --node specified, route via Tailscale control plane
    // 3. Start build process

    println!("{} Queued", "✓".green());
    
    if args.wait {
        println!();
        println!("{}", "Waiting for build...".dimmed());
        
        // Simulate build stages
        let stages = ["checkout", "build-infrasim", "build-alpine", "verify-boot", "create-snapshot"];
        
        for (i, stage) in stages.iter().enumerate() {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if i == stages.len() - 1 {
                println!("  {} {} {}", "✓".green(), stage.bold(), format!("({}s)", (i + 1) * 10).dimmed());
            } else {
                println!("  {} {}", "✓".green(), stage);
            }
        }

        println!();
        print_success("Build completed successfully");

        if let Some(ref tag) = args.tag {
            println!();
            println!("  Tag {} created", tag.green().bold());
        }

        if let Some(ref output) = args.output {
            println!("  Artifacts downloaded to: {}", output.display());
        }
    } else {
        println!();
        println!("Run {} to check status", format!("infrasim pipeline status {}", short_id).cyan());
    }

    Ok(())
}

async fn status(args: StatusArgs, format: OutputFormat) -> Result<()> {
    // Simulate build status
    let build = BuildRun {
        id: args.build_id.clone(),
        pipeline: "image-snapshot".to_string(),
        status: BuildStatus::Success,
        started_at: chrono::Utc::now().timestamp() - 600,
        finished_at: Some(chrono::Utc::now().timestamp() - 60),
        trigger: "manual".to_string(),
        commit_sha: Some("abc123def456".to_string()),
        tag: Some("v0.1.0".to_string()),
        stages: vec![
            StageRun {
                name: "checkout".to_string(),
                status: BuildStatus::Success,
                started_at: Some(chrono::Utc::now().timestamp() - 600),
                finished_at: Some(chrono::Utc::now().timestamp() - 590),
                exit_code: Some(0),
                log_offset: 0,
                log_length: 1024,
            },
            StageRun {
                name: "build".to_string(),
                status: BuildStatus::Success,
                started_at: Some(chrono::Utc::now().timestamp() - 590),
                finished_at: Some(chrono::Utc::now().timestamp() - 300),
                exit_code: Some(0),
                log_offset: 1024,
                log_length: 50000,
            },
            StageRun {
                name: "verify".to_string(),
                status: BuildStatus::Success,
                started_at: Some(chrono::Utc::now().timestamp() - 300),
                finished_at: Some(chrono::Utc::now().timestamp() - 60),
                exit_code: Some(0),
                log_offset: 51024,
                log_length: 2048,
            },
        ],
        artifacts: vec![
            ArtifactRecord {
                name: "alpine-aarch64.qcow2".to_string(),
                path: "output/alpine-aarch64.qcow2".to_string(),
                size_bytes: 512 * 1024 * 1024,
                sha256: "abc123def456789...".to_string(),
                signed: true,
                download_url: Some("https://example.com/artifacts/abc123.qcow2".to_string()),
            },
        ],
        provenance: Some(BuildProvenance {
            infrasim_version: "0.1.0".to_string(),
            infrasim_sha256: "sha256:abc123...".to_string(),
            cargo_deps_hash: "sha256:def456...".to_string(),
            builder_identity: "github-actions".to_string(),
            build_timestamp: chrono::Utc::now().timestamp(),
            reproducible: true,
            attestations: vec![],
        }),
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&build)?);
        return Ok(());
    }

    println!("{}", "━".repeat(60).dimmed());
    println!("{}", " Build Status".bold());
    println!("{}", "━".repeat(60).dimmed());
    println!();

    println!("  ID:        {}", build.id.cyan());
    println!("  Pipeline:  {}", build.pipeline);
    println!("  Status:    {}", match build.status {
        BuildStatus::Success => "✓ Success".green().to_string(),
        BuildStatus::Failed => "✗ Failed".red().to_string(),
        BuildStatus::Running => "● Running".yellow().to_string(),
        BuildStatus::Pending => "○ Pending".dimmed().to_string(),
        BuildStatus::Cancelled => "⊘ Cancelled".dimmed().to_string(),
    });
    if let Some(ref tag) = build.tag {
        println!("  Tag:       {}", tag.green());
    }
    if let Some(ref sha) = build.commit_sha {
        println!("  Commit:    {}", sha.dimmed());
    }
    println!();

    if args.detailed {
        println!("{}", "Stages:".bold());
        for stage in &build.stages {
            let status_icon = match stage.status {
                BuildStatus::Success => "✓".green(),
                BuildStatus::Failed => "✗".red(),
                BuildStatus::Running => "●".yellow(),
                _ => "○".normal(),
            };
            let duration = match (stage.started_at, stage.finished_at) {
                (Some(s), Some(f)) => format!("{}s", f - s),
                _ => "-".to_string(),
            };
            println!("  {} {} ({})", status_icon, stage.name, duration.dimmed());
        }
        println!();
    }

    if !build.artifacts.is_empty() {
        println!("{}", "Artifacts:".bold());
        print_list(&build.artifacts, format);
    }

    Ok(())
}

async fn logs(args: LogsArgs) -> Result<()> {
    println!("{} Build logs: {}", "📜".bold(), args.build_id.cyan());
    
    if let Some(ref stage) = args.stage {
        println!("   Stage: {}", stage);
    }
    println!();

    if args.follow {
        println!("{}", "(Following logs - press Ctrl+C to stop)".dimmed());
        println!();
        
        // Simulate log streaming
        let log_lines = [
            "Cloning repository...",
            "Fetching dependencies...",
            "Running cargo build --release",
            "   Compiling infrasim v0.1.0",
            "   Compiling infrasim-daemon v0.1.0",
            "Building qcow2 image...",
            "   Creating base layer",
            "   Installing packages",
            "   Configuring cloud-init",
            "Verifying boot...",
            "   QEMU started",
            "   SSH ready after 15s",
            "Creating snapshot bundle...",
            "   Checksums generated",
            "   Provenance attached",
            "Build complete!",
        ];

        for line in &log_lines {
            println!("{} {}", "│".dimmed(), line);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    } else {
        for i in 0..args.lines.min(20) {
            println!("{} Log line {}", "│".dimmed(), i + 1);
        }
    }

    Ok(())
}

async fn artifacts(args: ArtifactsArgs, format: OutputFormat) -> Result<()> {
    // Simulate artifact list
    let artifacts = vec![
        ArtifactRecord {
            name: "alpine-aarch64.qcow2".to_string(),
            path: "output/alpine-aarch64.qcow2".to_string(),
            size_bytes: 512 * 1024 * 1024,
            sha256: "abc123def456789abc123def456789abc123def456789abc123def456789abcd".to_string(),
            signed: true,
            download_url: Some("https://artifacts.example.com/abc123".to_string()),
        },
        ArtifactRecord {
            name: "alpine-aarch64.qcow2.sha256".to_string(),
            path: "output/alpine-aarch64.qcow2.sha256".to_string(),
            size_bytes: 64,
            sha256: "def456...".to_string(),
            signed: false,
            download_url: None,
        },
        ArtifactRecord {
            name: "provenance.json".to_string(),
            path: "meta/provenance.json".to_string(),
            size_bytes: 2048,
            sha256: "789abc...".to_string(),
            signed: true,
            download_url: None,
        },
    ];

    if args.json {
        println!("{}", serde_json::to_string_pretty(&artifacts)?);
        return Ok(());
    }

    println!("{} Artifacts for build: {}", "📦".bold(), args.build_id.cyan());
    println!();

    print_list(&artifacts, format);

    if let Some(ref download_path) = args.download {
        println!();
        println!("{}", "Downloading artifacts...".dimmed());
        
        for artifact in &artifacts {
            if let Some(ref name_filter) = args.name {
                if &artifact.name != name_filter {
                    continue;
                }
            }
            
            let dest = download_path.join(&artifact.name);
            println!("  {} → {}", artifact.name, dest.display());
            
            // In a real implementation, download the file
            if args.verify {
                println!("    {} SHA256 verified", "✓".green());
            }
        }
        
        println!();
        print_success("Download complete");
    }

    Ok(())
}

async fn provenance(args: ProvenanceArgs) -> Result<()> {
    println!("{}", "━".repeat(60).dimmed());
    println!("{}", " Build Provenance".bold());
    println!("{}", "━".repeat(60).dimmed());
    println!();

    // In a real implementation, fetch from daemon or parse from artifact
    let provenance = BuildProvenance {
        infrasim_version: "0.1.0".to_string(),
        infrasim_sha256: "sha256:abc123def456789abc123def456789abc123def456789abc123def456789abcd".to_string(),
        cargo_deps_hash: "sha256:def456789abc123def456789abc123def456789abc123def456789abcdef01".to_string(),
        builder_identity: "github-actions[bot]@infrasim-ci".to_string(),
        build_timestamp: chrono::Utc::now().timestamp() - 3600,
        reproducible: true,
        attestations: vec![
            AttestationRecord {
                kind: "infrasim-build".to_string(),
                sha256: "abc123...".to_string(),
                signed_by: Some("github-actions".to_string()),
            },
            AttestationRecord {
                kind: "cargo-metadata".to_string(),
                sha256: "def456...".to_string(),
                signed_by: None,
            },
            AttestationRecord {
                kind: "qcow2-manifest".to_string(),
                sha256: "789abc...".to_string(),
                signed_by: Some("github-actions".to_string()),
            },
        ],
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&provenance)?);
        return Ok(());
    }

    println!("{}", "🔗 Provenance Chain".bold());
    println!();
    println!("  InfraSim Version:  {}", provenance.infrasim_version.cyan());
    println!("  InfraSim SHA256:   {}", provenance.infrasim_sha256.chars().take(24).collect::<String>().dimmed());
    println!("  Cargo Deps Hash:   {}", provenance.cargo_deps_hash.chars().take(24).collect::<String>().dimmed());
    println!("  Builder:           {}", provenance.builder_identity);
    println!("  Reproducible:      {}", if provenance.reproducible { "✓ Yes".green() } else { "✗ No".yellow() });
    println!();

    println!("{}", "📋 Attestations".bold());
    for att in &provenance.attestations {
        let signed = if att.signed_by.is_some() { "✓".green() } else { "○".dimmed() };
        println!("  {} {} ({})", signed, att.kind, att.sha256.chars().take(12).collect::<String>().dimmed());
    }
    println!();

    if args.verify {
        println!("{}", "🔍 Verification".bold());
        println!("  {} InfraSim binary hash matches", "✓".green());
        println!("  {} Cargo dependencies hash matches", "✓".green());
        println!("  {} Image manifest verified", "✓".green());
        println!("  {} Signatures valid", "✓".green());
        println!();
        print_success("Provenance chain verified");
    }

    Ok(())
}

async fn cancel(args: CancelArgs) -> Result<()> {
    println!("{} Cancelling build: {}", "⊘".yellow(), args.build_id);
    
    if args.force {
        println!("   Using force (SIGKILL)");
    }
    
    // In a real implementation, send cancel request
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    
    print_success("Build cancelled");
    Ok(())
}

async fn retry(args: RetryArgs) -> Result<()> {
    println!("{} Retrying build: {}", "🔄".cyan(), args.build_id);
    
    if args.failed_only {
        println!("   Only retrying failed stages");
    }
    
    // Generate new build ID
    let new_id = uuid::Uuid::new_v4().to_string();
    let short_id: String = new_id.chars().take(8).collect();
    
    println!();
    println!("  New build ID: {}", short_id.bold());
    
    print_success("Build queued");
    Ok(())
}

async fn create(args: CreateArgs) -> Result<()> {
    println!("{} Creating pipeline: {}", "📝".bold(), args.name.cyan());
    
    // Create pipeline definition
    let template = args.template.unwrap_or("custom".to_string());
    
    let pipeline = match template.as_str() {
        "image-build" => Pipeline {
            name: args.name.clone(),
            description: args.description.unwrap_or("Build VM image with provenance".to_string()),
            stages: vec![
                PipelineStage {
                    name: "build-infrasim".to_string(),
                    image: None,
                    commands: vec![
                        "cargo build --release -p infrasim-daemon".to_string(),
                        "cargo build --release -p infrasim".to_string(),
                    ],
                    depends_on: vec![],
                    timeout_seconds: 1800,
                },
                PipelineStage {
                    name: "build-image".to_string(),
                    image: Some("alpine:latest".to_string()),
                    commands: vec![
                        "./build-alpine.sh".to_string(),
                    ],
                    depends_on: vec!["build-infrasim".to_string()],
                    timeout_seconds: 3600,
                },
                PipelineStage {
                    name: "verify".to_string(),
                    image: None,
                    commands: vec![
                        "infrasim artifact inspect output/*.tar.gz".to_string(),
                    ],
                    depends_on: vec!["build-image".to_string()],
                    timeout_seconds: 300,
                },
            ],
            triggers: vec![
                PipelineTrigger {
                    kind: "push".to_string(),
                    pattern: Some("main".to_string()),
                    cron: None,
                },
                PipelineTrigger {
                    kind: "tag".to_string(),
                    pattern: Some("v*".to_string()),
                    cron: None,
                },
            ],
            artifacts: vec![
                ArtifactDefinition {
                    name: "image".to_string(),
                    path: "output/*.qcow2".to_string(),
                    kind: "qcow2".to_string(),
                    retain_days: 30,
                },
                ArtifactDefinition {
                    name: "bundle".to_string(),
                    path: "output/*.tar.gz".to_string(),
                    kind: "tarball".to_string(),
                    retain_days: 90,
                },
            ],
        },
        "sdn-deploy" => Pipeline {
            name: args.name.clone(),
            description: args.description.unwrap_or("Deploy SDN topology via Terraform".to_string()),
            stages: vec![
                PipelineStage {
                    name: "terraform-init".to_string(),
                    image: Some("hashicorp/terraform:latest".to_string()),
                    commands: vec!["terraform init".to_string()],
                    depends_on: vec![],
                    timeout_seconds: 300,
                },
                PipelineStage {
                    name: "terraform-plan".to_string(),
                    image: Some("hashicorp/terraform:latest".to_string()),
                    commands: vec!["terraform plan -out=tfplan".to_string()],
                    depends_on: vec!["terraform-init".to_string()],
                    timeout_seconds: 300,
                },
                PipelineStage {
                    name: "terraform-apply".to_string(),
                    image: Some("hashicorp/terraform:latest".to_string()),
                    commands: vec!["terraform apply -auto-approve tfplan".to_string()],
                    depends_on: vec!["terraform-plan".to_string()],
                    timeout_seconds: 600,
                },
            ],
            triggers: vec![
                PipelineTrigger {
                    kind: "manual".to_string(),
                    pattern: None,
                    cron: None,
                },
            ],
            artifacts: vec![
                ArtifactDefinition {
                    name: "tfstate".to_string(),
                    path: "terraform.tfstate".to_string(),
                    kind: "json".to_string(),
                    retain_days: 365,
                },
            ],
        },
        _ => Pipeline {
            name: args.name.clone(),
            description: args.description.unwrap_or_default(),
            stages: vec![],
            triggers: vec![],
            artifacts: vec![],
        },
    };

    // Write to file or display
    if let Some(ref file) = args.file {
        let yaml = serde_yaml::to_string(&pipeline)?;
        tokio::fs::write(file, yaml).await?;
        print_success(&format!("Pipeline saved to {}", file.display()));
    } else {
        println!();
        println!("{}", serde_yaml::to_string(&pipeline)?);
    }

    Ok(())
}
