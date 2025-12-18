//! InfraSim Daemon
//!
//! The main daemon that orchestrates QEMU VMs and reconciles state.

use clap::Parser;
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod config;
mod grpc;
mod qemu;
mod reconciler;
mod state;

pub mod generated {
    #![allow(clippy::all)]
    include!("generated/infrasim.v1.rs");
}

use config::DaemonConfig;

#[derive(Parser)]
#[command(name = "infrasimd")]
#[command(about = "InfraSim daemon - Terraform-compatible QEMU orchestration")]
#[command(version)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, default_value = "~/.infrasim/config.toml")]
    config: PathBuf,

    /// Store directory
    #[arg(short, long)]
    store: Option<PathBuf>,

    /// gRPC listen address
    #[arg(short, long, default_value = "127.0.0.1:9090")]
    listen: String,

    /// Web console port
    #[arg(short, long, default_value = "6080")]
    web_port: u16,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Run in foreground
    #[arg(short, long)]
    foreground: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.debug {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();

    info!("InfraSim daemon v{}", env!("CARGO_PKG_VERSION"));

    // Load or create configuration
    let store_path = cli.store.unwrap_or_else(infrasim_common::default_store_path);
    let config = DaemonConfig {
        store_path: store_path.clone(),
        grpc_listen: cli.listen.clone(),
        web_port: cli.web_port,
        ..Default::default()
    };

    // Ensure store directory exists
    tokio::fs::create_dir_all(&store_path).await?;

    // Initialize state manager
    let state = state::StateManager::new(&config).await?;

    // Start reconciler
    let reconciler = reconciler::Reconciler::new(state.clone());
    let reconciler_handle = tokio::spawn(async move {
        reconciler.run().await
    });

    // Start gRPC server
    let grpc_handle = tokio::spawn(grpc::serve(config.clone(), state.clone()));

    info!("Daemon started on {}", config.grpc_listen);
    info!("Web console available at http://127.0.0.1:{}", config.web_port);

    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        result = grpc_handle => {
            if let Err(e) = result {
                tracing::error!("gRPC server error: {}", e);
            }
        }
        result = reconciler_handle => {
            if let Err(e) = result {
                tracing::error!("Reconciler error: {}", e);
            }
        }
    }

    info!("Daemon shutdown complete");
    Ok(())
}
