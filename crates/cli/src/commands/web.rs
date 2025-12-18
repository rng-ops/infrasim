//! CLI commands for the InfraSim Web Server
//!
//! Provides comprehensive control over the web server including:
//! - Development mode with Vite hot reload
//! - Production mode with static assets
//! - Embedded UI bundle support

use clap::{Args, Subcommand};
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, error, info, warn};

#[derive(Subcommand)]
pub enum WebCommands {
    /// Start the web server
    Serve(WebServeArgs),

    /// Build the UI for production
    Build(WebBuildArgs),

    /// Generate UI manifest
    Manifest(WebManifestArgs),
}

#[derive(Args)]
pub struct WebServeArgs {
    /// Web server bind address
    #[arg(long, default_value = "127.0.0.1:8080")]
    pub addr: String,

    /// Daemon gRPC address
    #[arg(long, default_value = "http://127.0.0.1:50051")]
    pub daemon_addr: String,

    /// Enable Vite development server with hot reload
    /// In this mode, Rust serves /api/* and /websockify/*
    /// Vite serves /ui/* with hot module replacement
    #[arg(long)]
    pub ui_dev: bool,

    /// Proxy /api and /websockify to Rust from Vite dev server
    /// Only meaningful with --ui-dev
    #[arg(long)]
    pub ui_dev_proxy: bool,

    /// Vite dev server port (default: 4173)
    #[arg(long, default_value = "4173")]
    pub ui_dev_port: u16,

    /// Path to UI source directory (for --ui-dev mode)
    #[arg(long)]
    pub ui_src_dir: Option<PathBuf>,

    /// Serve built UI from disk (production mode)
    /// Path should contain index.html and assets/
    #[arg(long)]
    pub ui_static_dir: Option<PathBuf>,

    /// Embed UI bundle into Rust binary (production mode)
    /// Requires building with INFRASIM_EMBED_UI=1
    #[arg(long)]
    pub ui_embed: bool,

    /// Authentication mode: token, jwt, dev-random, none
    #[arg(long, default_value = "dev-random")]
    pub auth_mode: String,

    /// Static bearer token (for --auth-mode=token)
    #[arg(long, env = "INFRASIM_WEB_AUTH_TOKEN")]
    pub auth_token: Option<String>,

    /// Enable local admin controls
    #[arg(long)]
    pub control_enabled: bool,

    /// Admin token for control endpoints
    #[arg(long, env = "INFRASIM_WEB_ADMIN_TOKEN")]
    pub admin_token: Option<String>,

    /// Daemon PID file path (for restart/stop controls)
    #[arg(long, env = "INFRASIM_DAEMON_PIDFILE")]
    pub daemon_pidfile: Option<String>,

    /// Log level
    #[arg(long, default_value = "info")]
    pub log_level: String,
}

#[derive(Args)]
pub struct WebBuildArgs {
    /// Path to UI source directory
    #[arg(long, default_value = "ui/apps/console")]
    pub ui_src_dir: PathBuf,

    /// Output directory for built assets
    #[arg(long, default_value = "ui/apps/console/dist")]
    pub output_dir: PathBuf,

    /// Generate ui.manifest.json with checksums
    #[arg(long, default_value = "true")]
    pub manifest: bool,

    /// Production mode (minify, optimize)
    #[arg(long, default_value = "true")]
    pub production: bool,

    /// Base path for assets (must match server mount point)
    #[arg(long, default_value = "/ui/")]
    pub base: String,
}

#[derive(Args)]
pub struct WebManifestArgs {
    /// Path to built UI directory
    #[arg(long, default_value = "ui/apps/console/dist")]
    pub dist_dir: PathBuf,

    /// Output manifest path
    #[arg(long)]
    pub output: Option<PathBuf>,
}

pub async fn execute(cmd: WebCommands) -> anyhow::Result<()> {
    match cmd {
        WebCommands::Serve(args) => execute_serve(args).await,
        WebCommands::Build(args) => execute_build(args).await,
        WebCommands::Manifest(args) => execute_manifest(args).await,
    }
}

async fn execute_serve(args: WebServeArgs) -> anyhow::Result<()> {
    // Set environment variables for web server configuration
    std::env::set_var("INFRASIM_WEB_ADDR", &args.addr);
    std::env::set_var("INFRASIM_DAEMON_ADDR", &args.daemon_addr);

    if args.control_enabled {
        std::env::set_var("INFRASIM_WEB_CONTROL_ENABLED", "1");
    }
    if let Some(ref token) = args.admin_token {
        std::env::set_var("INFRASIM_WEB_ADMIN_TOKEN", token);
    }
    if let Some(ref pidfile) = args.daemon_pidfile {
        std::env::set_var("INFRASIM_DAEMON_PIDFILE", pidfile);
    }

    // Configure authentication
    match args.auth_mode.as_str() {
        "jwt" => {
            std::env::set_var("INFRASIM_AUTH_MODE", "jwt");
        }
        "token" => {
            if let Some(ref token) = args.auth_token {
                std::env::set_var("INFRASIM_WEB_AUTH_TOKEN", token);
            } else {
                anyhow::bail!("--auth-token is required when using --auth-mode=token");
            }
        }
        "none" => {
            // WebUiAuth::None - not recommended
            warn!("Running with no authentication - not recommended for production");
        }
        _ => {
            // Default: dev-random - generates ephemeral token
        }
    }

    // UI serving mode
    if args.ui_dev {
        info!("Starting in UI development mode (Vite hot reload)");
        return execute_dev_mode(args).await;
    }

    if let Some(ref static_dir) = args.ui_static_dir {
        info!("Serving UI from disk: {:?}", static_dir);
        std::env::set_var("INFRASIM_WEB_STATIC_DIR", static_dir.to_string_lossy().as_ref());
    } else if args.ui_embed {
        info!("Using embedded UI bundle");
        std::env::set_var("INFRASIM_WEB_UI_EMBED", "1");
    } else {
        // Default: try to find static dir relative to binary
        let default_paths = [
            "ui/apps/console/dist",
            "../ui/apps/console/dist",
            "../../ui/apps/console/dist",
        ];
        let mut found = false;
        for path in default_paths {
            let p = PathBuf::from(path);
            if p.join("index.html").exists() {
                info!("Auto-detected UI at: {:?}", p);
                std::env::set_var("INFRASIM_WEB_STATIC_DIR", path);
                found = true;
                break;
            }
        }
        if !found {
            warn!("No UI directory found. Run `infrasim web build` first or use --ui-static-dir");
        }
    }

    // Start the web server
    let addr: std::net::SocketAddr = args.addr.parse()?;
    
    // Import and run the web server
    // Note: This would typically be done via the infrasim-web crate
    info!("Starting InfraSim Web on http://{}", addr);
    info!("Daemon: {}", args.daemon_addr);
    
    // The actual server startup is handled by infrasim-web
    // For now, print configuration and exit
    // In production, this would call infrasim_web::server::serve()
    
    println!("Web server configuration:");
    println!("  Address: {}", args.addr);
    println!("  Daemon: {}", args.daemon_addr);
    println!("  UI Dev Mode: {}", args.ui_dev);
    println!("  UI Static Dir: {:?}", args.ui_static_dir);
    println!("  Auth Mode: {}", args.auth_mode);
    println!();
    println!("To start the server, run:");
    println!("  cargo run -p infrasim-web --bin infrasim-web");
    
    Ok(())
}

async fn execute_dev_mode(args: WebServeArgs) -> anyhow::Result<()> {
    let ui_src_dir = args.ui_src_dir.unwrap_or_else(|| PathBuf::from("ui/apps/console"));
    
    if !ui_src_dir.exists() {
        anyhow::bail!("UI source directory not found: {:?}", ui_src_dir);
    }

    info!("Starting Vite dev server in {:?}", ui_src_dir);
    info!("Vite will serve /ui/* with hot reload on port {}", args.ui_dev_port);
    info!("Rust will serve /api/*, /websockify/*");

    // In dev mode, we need to:
    // 1. Start Vite dev server (spawned process)
    // 2. Configure Vite to proxy /api and /websockify to Rust
    // 3. Start Rust server without serving /ui/*

    let vite_config = format!(
        r#"
Development Mode Configuration:
================================
Vite Dev Server:
  - Port: {}
  - Proxies /api → http://{}
  - Proxies /websockify → http://{}
  - Hot reload enabled for React components

Rust Server:
  - Address: {}
  - Serves: /api/*, /websockify/*, /admin/*
  - Does NOT serve /ui/* (handled by Vite)

To start development:
  Terminal 1: cd {} && pnpm dev
  Terminal 2: cargo run -p infrasim-web --bin infrasim-web

Open: http://localhost:{}/ui/
"#,
        args.ui_dev_port,
        args.addr,
        args.addr,
        args.addr,
        ui_src_dir.display(),
        args.ui_dev_port,
    );

    println!("{}", vite_config);

    // Optionally spawn Vite automatically
    if args.ui_dev_proxy {
        info!("Spawning Vite dev server...");
        
        let mut vite_cmd = Command::new("pnpm");
        vite_cmd
            .current_dir(&ui_src_dir)
            .arg("dev")
            .arg("--port")
            .arg(args.ui_dev_port.to_string());

        match vite_cmd.spawn() {
            Ok(child) => {
                info!("Vite dev server started (PID: {:?})", child.id());
            }
            Err(e) => {
                warn!("Failed to spawn Vite: {}. Start manually with: cd {} && pnpm dev", e, ui_src_dir.display());
            }
        }
    }

    Ok(())
}

async fn execute_build(args: WebBuildArgs) -> anyhow::Result<()> {
    info!("Building UI for production...");
    info!("Source: {:?}", args.ui_src_dir);
    info!("Output: {:?}", args.output_dir);

    if !args.ui_src_dir.exists() {
        anyhow::bail!("UI source directory not found: {:?}", args.ui_src_dir);
    }

    // Run pnpm install if node_modules doesn't exist
    let node_modules = args.ui_src_dir.join("node_modules");
    if !node_modules.exists() {
        info!("Installing dependencies...");
        let status = Command::new("pnpm")
            .current_dir(&args.ui_src_dir)
            .arg("install")
            .status()?;
        
        if !status.success() {
            anyhow::bail!("pnpm install failed");
        }
    }

    // Run Vite build
    info!("Running Vite build...");
    let mut build_cmd = Command::new("pnpm");
    build_cmd
        .current_dir(&args.ui_src_dir)
        .arg("build");

    if args.production {
        build_cmd.env("NODE_ENV", "production");
    }

    let status = build_cmd.status()?;
    if !status.success() {
        anyhow::bail!("Vite build failed");
    }

    info!("Build complete!");

    // Generate manifest
    if args.manifest {
        let manifest_args = WebManifestArgs {
            dist_dir: args.output_dir.clone(),
            output: Some(args.output_dir.join("ui.manifest.json")),
        };
        execute_manifest(manifest_args).await?;
    }

    Ok(())
}

async fn execute_manifest(args: WebManifestArgs) -> anyhow::Result<()> {
    use sha2::{Sha256, Digest};
    use std::io::Read;

    info!("Generating UI manifest from {:?}", args.dist_dir);

    if !args.dist_dir.exists() {
        anyhow::bail!("Distribution directory not found: {:?}", args.dist_dir);
    }

    let mut assets: Vec<serde_json::Value> = Vec::new();
    let mut total_size: u64 = 0;

    // Walk the dist directory
    fn walk_dir(dir: &PathBuf, base: &PathBuf, assets: &mut Vec<serde_json::Value>, total_size: &mut u64) -> anyhow::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                walk_dir(&path, base, assets, total_size)?;
            } else {
                let rel_path = path.strip_prefix(base)?;
                let mut file = std::fs::File::open(&path)?;
                let mut contents = Vec::new();
                file.read_to_end(&mut contents)?;
                
                let mut hasher = Sha256::new();
                hasher.update(&contents);
                let hash = format!("{:x}", hasher.finalize());
                
                let size = contents.len() as u64;
                *total_size += size;

                assets.push(serde_json::json!({
                    "path": rel_path.to_string_lossy(),
                    "size": size,
                    "sha256": hash,
                }));
            }
        }
        Ok(())
    }

    walk_dir(&args.dist_dir, &args.dist_dir, &mut assets, &mut total_size)?;

    // Get git info
    let git_commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let git_branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Read package.json for version
    let pkg_json_path = args.dist_dir.parent()
        .map(|p| p.join("package.json"))
        .unwrap_or_else(|| PathBuf::from("package.json"));
    
    let ui_version = std::fs::read_to_string(&pkg_json_path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("version").and_then(|v| v.as_str()).map(String::from))
        .unwrap_or_else(|| "0.0.0".to_string());

    let manifest = serde_json::json!({
        "schema_version": "1.0",
        "ui_version": ui_version,
        "git_commit": git_commit,
        "git_branch": git_branch,
        "build_timestamp": chrono::Utc::now().to_rfc3339(),
        "build_host": hostname::get().ok().and_then(|h| h.into_string().ok()).unwrap_or_else(|| "unknown".to_string()),
        "total_size_bytes": total_size,
        "asset_count": assets.len(),
        "api_schema_version": "v1",
        "declared_resource_kinds": [
            "appliance",
            "vm",
            "network",
            "volume",
            "filesystem",
            "snapshot",
            "attestation"
        ],
        "mount_point": "/ui/",
        "assets": assets,
    });

    let output_path = args.output.unwrap_or_else(|| args.dist_dir.join("ui.manifest.json"));
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(&output_path, &manifest_json)?;

    info!("Manifest written to {:?}", output_path);
    info!("  Version: {}", ui_version);
    info!("  Commit: {}", git_commit);
    info!("  Assets: {}", assets.len());
    info!("  Total size: {} bytes", total_size);

    Ok(())
}
