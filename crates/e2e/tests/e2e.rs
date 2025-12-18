//! E2E test harness entry point
//!
//! This file is the test binary that runs E2E tests from YAML specs.
//! Run with: cargo test --package infrasim-e2e --test e2e

use std::path::PathBuf;
use clap::Parser;
use tracing_subscriber::{fmt, EnvFilter};

use infrasim_e2e::{TestRunner, E2eResult};
use infrasim_e2e::runner::RunnerConfig;
use infrasim_e2e::server::ServerConfig;
use infrasim_e2e::playwright::PlaywrightConfig;
use infrasim_e2e::visual::VisualConfig;

#[derive(Parser, Debug)]
#[command(name = "infrasim-e2e")]
#[command(about = "E2E test runner for InfraSim")]
struct Args {
    /// Path to test specs directory
    #[arg(short, long, default_value = "tests/e2e/specs")]
    specs: PathBuf,

    /// Run only tests matching this tag
    #[arg(short, long)]
    tag: Option<String>,

    /// Run only a specific test by name
    #[arg(short, long)]
    name: Option<String>,

    /// Update visual baselines instead of comparing
    #[arg(long)]
    update_baselines: bool,

    /// Path to web server binary
    #[arg(long, default_value = "target/debug/infrasim-web")]
    server_binary: PathBuf,

    /// Path to static files directory
    #[arg(long, default_value = "ui/apps/console/dist")]
    static_dir: PathBuf,

    /// Port to run server on (0 = auto)
    #[arg(long, default_value = "0")]
    port: u16,

    /// Daemon address
    #[arg(long, default_value = "http://127.0.0.1:9090")]
    daemon_addr: String,

    /// Browser to use (chromium, firefox, webkit)
    #[arg(long, default_value = "chromium")]
    browser: String,

    /// Run in headless mode
    #[arg(long, default_value = "true")]
    headless: bool,

    /// Viewport width
    #[arg(long, default_value = "1280")]
    viewport_width: u32,

    /// Viewport height
    #[arg(long, default_value = "720")]
    viewport_height: u32,

    /// Visual diff threshold (percentage)
    #[arg(long, default_value = "0.5")]
    visual_threshold: f64,

    /// Output directory for results
    #[arg(short, long, default_value = "test-results")]
    output: PathBuf,
}

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    let args = Args::parse();

    // Run async main
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let result = rt.block_on(async_main(args));

    match result {
        Ok(success) => {
            if success {
                std::process::exit(0);
            } else {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(2);
        }
    }
}

async fn async_main(args: Args) -> E2eResult<bool> {
    let browser = match args.browser.as_str() {
        "firefox" => infrasim_e2e::playwright::Browser::Firefox,
        "webkit" => infrasim_e2e::playwright::Browser::Webkit,
        _ => infrasim_e2e::playwright::Browser::Chromium,
    };

    let config = RunnerConfig {
        server: ServerConfig {
            binary_path: args.server_binary,
            static_dir: args.static_dir,
            daemon_addr: args.daemon_addr,
            port: if args.port == 0 { None } else { Some(args.port) },
            ..Default::default()
        },
        playwright: PlaywrightConfig {
            viewport_width: args.viewport_width,
            viewport_height: args.viewport_height,
            browser,
            headless: args.headless,
            ..Default::default()
        },
        visual: VisualConfig {
            threshold: args.visual_threshold,
            auto_update: args.update_baselines,
            ..Default::default()
        },
        specs_dir: args.specs,
        output_dir: args.output,
    };

    let mut runner = TestRunner::with_config(config);

    // Start server
    runner.start_server().await?;

    // Run tests
    let results = if let Some(name) = args.name {
        let result = runner.run_test(&name).await?;
        infrasim_e2e::runner::TestSuiteResult {
            total: 1,
            passed: if result.success { 1 } else { 0 },
            failed: if result.success { 0 } else { 1 },
            skipped: 0,
            duration_ms: result.duration_ms,
            results: vec![result],
        }
    } else if let Some(tag) = args.tag {
        runner.run_tagged(&tag).await?
    } else {
        runner.run_all().await?
    };

    // Update baselines if requested
    if args.update_baselines {
        runner.update_baselines()?;
    }

    // Write results
    runner.write_results(&results)?;

    Ok(results.failed == 0)
}

// Integration test that can be run with cargo test
#[cfg(test)]
mod tests {
    use super::*;
    use infrasim_e2e::spec::TestSpec;

    #[test]
    fn test_parse_sample_spec() {
        let yaml = r#"
name: sample-test
description: A sample test
steps:
  - action: navigate
    url: /login
  - action: wait
    selector: '[data-testid="login-page"]'
  - action: screenshot
    name: login-page
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        assert_eq!(spec.name, "sample-test");
        assert_eq!(spec.steps.len(), 3);
    }
}
