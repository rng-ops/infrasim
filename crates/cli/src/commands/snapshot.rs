//! Snapshot Commands

use clap::Subcommand;
use anyhow::Result;
use serde::Serialize;

use crate::client::DaemonClient;
use crate::output::{OutputFormat, TableDisplay, print_item, print_list, print_success};
use crate::generated::{Snapshot, SnapshotSpec};

#[derive(Subcommand)]
pub enum SnapshotCommands {
    /// List all snapshots
    List {
        /// Filter by VM ID
        #[arg(long)]
        vm_id: Option<String>,
    },

    /// Get snapshot details
    Get {
        /// Snapshot ID
        id: String,
    },

    /// Create a new snapshot
    Create {
        /// VM ID to snapshot
        #[arg(short, long)]
        vm_id: String,

        /// Snapshot name
        #[arg(short, long)]
        name: String,

        /// Description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// Delete a snapshot
    Delete {
        /// Snapshot ID
        id: String,
    },

    /// Restore a VM from snapshot
    Restore {
        /// Snapshot ID
        snapshot_id: String,

        /// Target VM ID (optional, defaults to original VM)
        #[arg(long)]
        target_vm: Option<String>,
    },
}

/// Snapshot display wrapper for serialization
#[derive(Serialize)]
pub struct SnapshotDisplay {
    pub id: String,
    pub name: String,
    pub vm_id: String,
    pub size: i64,
    pub created_at: String,
}

impl From<Snapshot> for SnapshotDisplay {
    fn from(snap: Snapshot) -> Self {
        let meta = snap.meta.unwrap_or_default();
        let spec = snap.spec.unwrap_or_default();
        let status = snap.status.unwrap_or_default();
        
        Self {
            id: meta.id,
            name: meta.name,
            vm_id: spec.vm_id,
            size: status.size_bytes,
            created_at: chrono::DateTime::from_timestamp(meta.created_at, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default(),
        }
    }
}

impl TableDisplay for SnapshotDisplay {
    fn headers() -> Vec<&'static str> {
        vec!["ID", "Name", "VM ID", "Size", "Created"]
    }

    fn row(&self) -> Vec<String> {
        let size_str = if self.size > 1024 * 1024 * 1024 {
            format!("{:.1}GB", self.size as f64 / 1024.0 / 1024.0 / 1024.0)
        } else if self.size > 1024 * 1024 {
            format!("{:.1}MB", self.size as f64 / 1024.0 / 1024.0)
        } else {
            format!("{}B", self.size)
        };
        
        vec![
            self.id.clone(),
            self.name.clone(),
            self.vm_id.clone(),
            size_str,
            self.created_at.clone(),
        ]
    }
}

pub async fn execute(cmd: SnapshotCommands, mut client: DaemonClient, format: OutputFormat) -> Result<()> {
    match cmd {
        SnapshotCommands::List { vm_id } => {
            let snapshots = client.list_snapshots(vm_id).await?;
            let displays: Vec<SnapshotDisplay> = snapshots.into_iter().map(SnapshotDisplay::from).collect();
            print_list(&displays, format);
        }

        SnapshotCommands::Get { id } => {
            let snap = client.get_snapshot(&id).await?;
            let display = SnapshotDisplay::from(snap);
            print_item(&display, format);
        }

        SnapshotCommands::Create { vm_id, name, description } => {
            let spec = SnapshotSpec {
                vm_id: vm_id.clone(),
                description: description.unwrap_or_default(),
                include_memory: true,
                include_disk: true,
            };

            let snap = client.create_snapshot(&name, spec).await?;
            let display = SnapshotDisplay::from(snap);
            print_success(&format!("Snapshot '{}' created for VM '{}'", display.name, vm_id));
            print_item(&display, format);
        }

        SnapshotCommands::Delete { id } => {
            client.delete_snapshot(&id).await?;
            print_success(&format!("Snapshot '{}' deleted", id));
        }

        SnapshotCommands::Restore { snapshot_id, target_vm } => {
            let vm = client.restore_snapshot(&snapshot_id, target_vm).await?;
            let meta = vm.meta.unwrap_or_default();
            print_success(&format!("VM '{}' restored from snapshot '{}'", meta.name, snapshot_id));
        }
    }

    Ok(())
}
