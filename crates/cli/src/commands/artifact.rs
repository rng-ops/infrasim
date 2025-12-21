//! Artifact inspection commands

use std::path::PathBuf;

use clap::{Args, Subcommand};
use colored::Colorize;

use crate::client::DaemonClient;
use crate::output::OutputFormat;

#[derive(Subcommand)]
pub enum ArtifactCommands {
    /// Inspect a build artifact bundle
    Inspect(InspectArgs),
}

#[derive(Args)]
pub struct InspectArgs {
    /// Path to the artifact bundle (.zip or .tar.gz)
    #[arg(required = true)]
    pub path: PathBuf,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Run locally without daemon (default for this command)
    #[arg(long, default_value = "true")]
    pub local: bool,
}

pub async fn execute(
    cmd: ArtifactCommands,
    client: Option<DaemonClient>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    match cmd {
        ArtifactCommands::Inspect(args) => inspect(args, client).await,
    }
}

async fn inspect(args: InspectArgs, _client: Option<DaemonClient>) -> anyhow::Result<()> {
    use infrasim_common::artifact::ArtifactInspector;

    if !args.path.exists() {
        eprintln!("{} File not found: {}", "Error:".red().bold(), args.path.display());
        std::process::exit(1);
    }

    let mut inspector = ArtifactInspector::new();
    
    let report = match inspector.inspect(&args.path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{} Failed to inspect artifact: {}", "Error:".red().bold(), e);
            std::process::exit(1);
        }
    };

    if args.json {
        // Output full JSON report
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        // Human-readable summary
        print_summary(&report);
    }

    if !report.passed {
        std::process::exit(1);
    }

    Ok(())
}

fn print_summary(report: &infrasim_common::artifact::ArtifactInspectionReport) {
    println!();
    println!("{}", "━".repeat(60).dimmed());
    println!("{}", " Artifact Inspection Report".bold());
    println!("{}", "━".repeat(60).dimmed());
    println!();

    // Input file
    println!("{}  {}", "📦 Input:".bold(), report.input_path);
    println!();

    // SHA256 verification
    println!("{}", "🔐 SHA256 Verification".bold());
    if let Some(ref expected) = report.sha256_expected {
        println!("   Expected: {}", expected.dimmed());
    }
    if let Some(ref actual) = report.sha256_actual {
        println!("   Actual:   {}", actual.dimmed());
    }
    if report.sha256_file_ok {
        println!("   Status:   {}", "✅ MATCH".green());
    } else if report.sha256_expected.is_some() {
        println!("   Status:   {}", "❌ MISMATCH".red());
    } else {
        println!("   Status:   {}", "⚠️  No .sha256 file".yellow());
    }
    println!();

    // Extracted files
    println!("{}", "📂 Extracted Files".bold());
    println!("   Total: {} files", report.extracted_files.len());
    
    // Show qcow2 files specifically
    let qcow2_files: Vec<_> = report.extracted_files.iter()
        .filter(|f| f.path.ends_with(".qcow2"))
        .collect();
    if !qcow2_files.is_empty() {
        println!("   Disk images:");
        for f in qcow2_files {
            println!("     • {} ({} bytes)", f.path.cyan(), f.size);
        }
    }
    println!();

    // Manifest verification
    println!("{}", "📋 Manifest Check".bold());
    if report.manifest.found {
        println!("   Found:    {}", "✅ meta/manifest.json".green());
        if report.manifest.parsed_ok {
            println!("   Parsed:   {}", "✅ Valid JSON".green());
            println!("   Entries:  {}/{} verified", 
                report.manifest.verified_entries.to_string().green(),
                report.manifest.total_entries
            );
        } else {
            println!("   Parsed:   {}", "❌ Invalid JSON".red());
            for err in &report.manifest.parse_errors {
                println!("             {}", err.red());
            }
        }
        if !report.manifest.missing_files.is_empty() {
            println!("   Missing:");
            for f in &report.manifest.missing_files {
                println!("     • {}", f.red());
            }
        }
        if !report.manifest.mismatched_files.is_empty() {
            println!("   Mismatched:");
            for f in &report.manifest.mismatched_files {
                println!("     • {}", f.yellow());
            }
        }
    } else {
        println!("   {}", "❌ meta/manifest.json not found".red());
    }
    println!();

    // Attestations
    println!("{}", "🔏 Attestations".bold());
    if report.attestations.integrity_attestation_found {
        println!("   Integrity: {}", "✅ Found".green());
        if report.attestations.manifest_sha256_matches {
            println!("   Manifest SHA256: {}", "✅ Matches".green());
        } else {
            println!("   Manifest SHA256: {}", "❌ Mismatch".red());
        }
    } else {
        println!("   Integrity: {}", "⚠️  Not found".yellow());
    }

    if !report.attestations.truncation_detected.is_empty() {
        println!("   {} Truncation detected in:", "❌".red());
        for f in &report.attestations.truncation_detected {
            println!("     • {} (contains '...')", f.red());
        }
    }

    if !report.attestations.malformed_json_files.is_empty() {
        println!("   {} Malformed JSON:", "❌".red());
        for f in &report.attestations.malformed_json_files {
            println!("     • {}", f.red());
        }
    }
    println!();

    // qcow2 Analysis
    if !report.qcow2_images.is_empty() {
        println!("{}", "💾 qcow2 Images".bold());
        for img in &report.qcow2_images {
            println!("   {}", img.path.cyan());
            if img.valid_magic {
                println!("     Magic:   {} (QFI\\xfb)", "✅".green());
            } else {
                println!("     Magic:   {}", "❌ Invalid".red());
            }
            println!("     Version: {}", if img.version == 3 { 
                format!("v{} ✅", img.version).green().to_string() 
            } else { 
                format!("v{} ⚠️", img.version).yellow().to_string() 
            });
            println!("     Size:    {} bytes ({:.1} GB virtual)", 
                img.virtual_size, 
                img.virtual_size as f64 / (1024.0 * 1024.0 * 1024.0)
            );
            println!("     Cluster: {} bytes ({} bits)", img.cluster_size, img.cluster_bits);
            
            if let Some(ref backing) = img.backing_file {
                println!("     Backing: {}", backing.dimmed());
                if img.backing_file_exists {
                    println!("              {}", "✅ Exists".green());
                } else {
                    println!("              {}", "❌ Not found".red());
                }
            }

            if !img.issues.is_empty() {
                for issue in &img.issues {
                    println!("     ⚠️  {}", issue.yellow());
                }
            }
        }
        println!();
    }

    // Signatures
    println!("{}", "✍️  Signatures".bold());
    match report.signatures.status.as_str() {
        "verified" => {
            println!("   Status: {}", "✅ VERIFIED".green().bold());
            if let Some(ref algo) = report.signatures.algorithm {
                println!("   Algorithm: {}", algo);
            }
        }
        "placeholder" => {
            println!("   Status: {}", "⚠️  PLACEHOLDER (not verified)".yellow().bold());
            println!("   {}", "Signature exists but is not cryptographically verified".dimmed());
        }
        "missing" => {
            println!("   Status: {}", "❌ MISSING".red().bold());
        }
        _ => {
            println!("   Status: {}", report.signatures.status.yellow());
        }
    }

    if !report.signatures.remediation_hints.is_empty() {
        println!("   {}", "To enable signing:".dimmed());
        for hint in &report.signatures.remediation_hints {
            println!("     • {}", hint.dimmed());
        }
    }
    println!();

    // Warnings
    if !report.warnings.is_empty() {
        println!("{}", "⚠️  Warnings".yellow().bold());
        for w in &report.warnings {
            println!("   • {}", w.yellow());
        }
        println!();
    }

    // Errors
    if !report.errors.is_empty() {
        println!("{}", "❌ Errors".red().bold());
        for e in &report.errors {
            println!("   • {}", e.red());
        }
        println!();
    }

    // Overall result
    println!("{}", "━".repeat(60).dimmed());
    if report.passed {
        println!("{}", " ✅ PASSED - Artifact inspection successful".green().bold());
    } else {
        println!("{}", " ❌ FAILED - Issues detected".red().bold());
    }
    println!("{}", "━".repeat(60).dimmed());
    println!();
}
