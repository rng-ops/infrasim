//! Volume Commands

use clap::Subcommand;
use anyhow::Result;
use serde::Serialize;

use crate::client::DaemonClient;
use crate::output::{OutputFormat, TableDisplay, print_item, print_list, print_success};
use crate::generated::{Volume, VolumeSpec, VolumeKind, IntegrityConfig};

#[derive(Subcommand)]
pub enum VolumeCommands {
    /// List all volumes
    List,

    /// Get volume details
    Get {
        /// Volume ID
        id: String,
    },

    /// Create a new volume
    Create {
        /// Volume name
        #[arg(short, long)]
        name: String,

        /// Volume kind (disk, weights)
        #[arg(short, long, default_value = "disk")]
        kind: String,

        /// Source (OCI reference, file path, URL)
        #[arg(short, long)]
        source: String,

        /// Format (qcow2, raw)
        #[arg(long, default_value = "qcow2")]
        format: String,

        /// Size in bytes (for new volumes)
        #[arg(long)]
        size: Option<i64>,

        /// Read-only volume
        #[arg(long)]
        read_only: bool,

        /// Create copy-on-write overlay
        #[arg(long)]
        overlay: bool,
    },

    /// Delete a volume
    Delete {
        /// Volume ID
        id: String,
    },

    /// Pull a volume from OCI registry
    Pull {
        /// OCI reference (e.g., ghcr.io/infrasim/kali-xfce:latest)
        reference: String,

        /// Volume name
        #[arg(short, long)]
        name: Option<String>,
    },
}

/// Volume display wrapper for serialization
#[derive(Serialize)]
pub struct VolumeDisplay {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub source: String,
    pub size: i64,
    pub ready: bool,
    pub digest: String,
}

impl From<Volume> for VolumeDisplay {
    fn from(vol: Volume) -> Self {
        let meta = vol.meta.unwrap_or_default();
        let spec = vol.spec.unwrap_or_default();
        let status = vol.status.unwrap_or_default();
        
        let kind_str = VolumeKind::try_from(spec.kind)
            .map(|k| format!("{:?}", k))
            .unwrap_or_else(|_| "Unknown".to_string());
        
        Self {
            id: meta.id,
            name: meta.name,
            kind: kind_str,
            source: spec.source,
            size: spec.size_bytes,
            ready: status.ready,
            digest: status.digest,
        }
    }
}

impl TableDisplay for VolumeDisplay {
    fn headers() -> Vec<&'static str> {
        vec!["ID", "Name", "Kind", "Source", "Size", "Ready", "Digest"]
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
            self.kind.clone(),
            self.source.chars().take(30).collect::<String>(),
            size_str,
            self.ready.to_string(),
            self.digest.chars().take(12).collect::<String>(),
        ]
    }
}

pub async fn execute(cmd: VolumeCommands, mut client: DaemonClient, format: OutputFormat) -> Result<()> {
    match cmd {
        VolumeCommands::List => {
            let volumes = client.list_volumes().await?;
            let displays: Vec<VolumeDisplay> = volumes.into_iter().map(VolumeDisplay::from).collect();
            print_list(&displays, format);
        }

        VolumeCommands::Get { id } => {
            let vol = client.get_volume(&id).await?;
            let display = VolumeDisplay::from(vol);
            print_item(&display, format);
        }

        VolumeCommands::Create {
            name,
            kind,
            source,
            format: vol_format,
            size,
            read_only,
            overlay,
        } => {
            let kind_enum = match kind.to_lowercase().as_str() {
                "disk" => VolumeKind::Disk,
                "weights" => VolumeKind::Weights,
                _ => VolumeKind::Disk,
            };

            let spec = VolumeSpec {
                kind: kind_enum as i32,
                source,
                integrity: Some(IntegrityConfig::default()),
                read_only,
                size_bytes: size.unwrap_or(0),
                format: vol_format,
                overlay,
            };

            let vol = client.create_volume(&name, spec).await?;
            let display = VolumeDisplay::from(vol);
            print_success(&format!("Volume '{}' created", display.name));
            print_item(&display, format);
        }

        VolumeCommands::Delete { id } => {
            client.delete_volume(&id).await?;
            print_success(&format!("Volume '{}' deleted", id));
        }

        VolumeCommands::Pull { reference, name } => {
            let vol_name = name.unwrap_or_else(|| {
                reference.split('/').last()
                    .and_then(|s| s.split(':').next())
                    .unwrap_or("volume")
                    .to_string()
            });

            let spec = VolumeSpec {
                kind: VolumeKind::Disk as i32,
                source: reference.clone(),
                integrity: Some(IntegrityConfig::default()),
                read_only: false,
                size_bytes: 0,
                format: "qcow2".to_string(),
                overlay: false,
            };

            let vol = client.create_volume(&vol_name, spec).await?;
            let display = VolumeDisplay::from(vol);
            print_success(&format!("Volume '{}' pulled from {}", display.name, reference));
            print_item(&display, format);
        }
    }

    Ok(())
}
