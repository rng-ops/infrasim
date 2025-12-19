//! InfraSim CLI - Main Entry Point
//!
//! Provides a comprehensive command-line interface for managing
//! InfraSim virtual machines, networks, volumes, and more.

use clap::{Parser, Subcommand};

mod commands;
mod client;
mod output;

mod generated {
    include!("generated/infrasim.v1.rs");
}

use commands::{vm, network, volume, console, snapshot, benchmark, attestation, web, git};

/// InfraSim CLI - Terraform-Compatible QEMU Platform
#[derive(Parser)]
#[command(name = "infrasim")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Daemon address
    #[arg(long, default_value = "http://127.0.0.1:50051", global = true)]
    daemon_addr: String,

    /// Output format
    #[arg(long, default_value = "table", global = true)]
    format: output::OutputFormat,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage virtual machines
    #[command(subcommand)]
    Vm(vm::VmCommands),

    /// Manage networks
    #[command(subcommand)]
    Network(network::NetworkCommands),

    /// Manage volumes
    #[command(subcommand)]
    Volume(volume::VolumeCommands),

    /// Access VM console
    Console(console::ConsoleArgs),

    /// Manage snapshots
    #[command(subcommand)]
    Snapshot(snapshot::SnapshotCommands),

    /// Run benchmarks
    Benchmark(benchmark::BenchmarkArgs),

    /// Attestation and provenance
    #[command(subcommand)]
    Attestation(attestation::AttestationCommands),

    /// Web server and UI management
    #[command(subcommand)]
    Web(web::WebCommands),

    /// Git utilities for branch management
    #[command(subcommand)]
    Git(git::GitCommands),

    /// Check daemon status
    Status,

    /// Show version information
    Version,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .with_target(false)
        .init();

    // Create client
    let client = client::DaemonClient::new(&cli.daemon_addr).await;

    match cli.command {
        Commands::Vm(cmd) => vm::execute(cmd, client?, cli.format).await?,
        Commands::Network(cmd) => network::execute(cmd, client?, cli.format).await?,
        Commands::Volume(cmd) => volume::execute(cmd, client?, cli.format).await?,
        Commands::Console(args) => console::execute(args, client?).await?,
        Commands::Snapshot(cmd) => snapshot::execute(cmd, client?, cli.format).await?,
        Commands::Benchmark(args) => benchmark::execute(args, client?, cli.format).await?,
        Commands::Attestation(cmd) => attestation::execute(cmd, client?, cli.format).await?,
        Commands::Web(cmd) => web::execute(cmd).await?,
        Commands::Git(cmd) => git::execute(cmd, cli.format).await?,
        Commands::Status => {
            match client {
                Ok(mut c) => {
                    let healthy = c.health_check().await;
                    if healthy {
                        println!("✅ Daemon is running at {}", cli.daemon_addr);
                    } else {
                        println!("❌ Daemon is not responding at {}", cli.daemon_addr);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    println!("❌ Cannot connect to daemon: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Version => {
            println!("InfraSim CLI v{}", env!("CARGO_PKG_VERSION"));
            println!("Terraform-Compatible QEMU Platform for macOS");
            println!();
            println!("Build info:");
            println!("  Target: aarch64-apple-darwin");
            println!("  QEMU: aarch64 with HVF acceleration");
            println!("  Guest: Raspberry Pi style ARM64");
        }
    }

    Ok(())
}
