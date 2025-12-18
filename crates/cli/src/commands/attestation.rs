//! Attestation Commands

use clap::Subcommand;
use anyhow::Result;
use serde::Serialize;

use crate::client::DaemonClient;
use crate::output::{OutputFormat, TableDisplay, print_item, print_success};
use crate::generated::AttestationReport;

#[derive(Subcommand)]
pub enum AttestationCommands {
    /// Get attestation report for a VM
    Get {
        /// VM ID
        vm_id: String,
    },

    /// Verify attestation report
    Verify {
        /// VM ID
        vm_id: String,

        /// Expected digest (optional)
        #[arg(long)]
        expected_digest: Option<String>,
    },
}

/// Attestation report display wrapper for serialization
#[derive(Serialize)]
pub struct AttestationDisplay {
    pub id: String,
    pub vm_id: String,
    pub attestation_type: String,
    pub digest: String,
    pub signature: String,
    pub created_at: String,
}

impl From<AttestationReport> for AttestationDisplay {
    fn from(report: AttestationReport) -> Self {
        Self {
            id: report.id,
            vm_id: report.vm_id,
            attestation_type: report.attestation_type,
            digest: report.digest.clone(),
            signature: hex::encode(&report.signature),
            created_at: chrono::DateTime::from_timestamp(report.created_at, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default(),
        }
    }
}

impl TableDisplay for AttestationDisplay {
    fn headers() -> Vec<&'static str> {
        vec!["ID", "VM ID", "Type", "Digest", "Created"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.id.chars().take(8).collect::<String>(),
            self.vm_id.chars().take(8).collect::<String>(),
            self.attestation_type.clone(),
            self.digest.chars().take(16).collect::<String>(),
            self.created_at.clone(),
        ]
    }
}

pub async fn execute(cmd: AttestationCommands, mut client: DaemonClient, format: OutputFormat) -> Result<()> {
    match cmd {
        AttestationCommands::Get { vm_id } => {
            let report = client.get_attestation(&vm_id).await?;
            let display = AttestationDisplay::from(report);
            print_item(&display, format);
        }

        AttestationCommands::Verify { vm_id, expected_digest } => {
            let report = client.get_attestation(&vm_id).await?;
            
            let verified = if let Some(ref expected) = expected_digest {
                report.digest == *expected
            } else {
                true // No expected digest, just show the report
            };

            if verified {
                print_success(&format!("✓ Attestation report for VM '{}'", vm_id));
                println!("  Digest: {}", report.digest);
                println!("  Type: {}", report.attestation_type);
            } else {
                println!("✗ Attestation verification failed for VM '{}'", vm_id);
                println!("  Reason: Digest mismatch");
                println!("  Expected: {}", expected_digest.unwrap_or_default());
                println!("  Actual: {}", report.digest);
            }
        }
    }

    Ok(())
}
