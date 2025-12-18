//! VM Commands

use clap::Subcommand;
use anyhow::Result;
use serde::Serialize;

use crate::client::DaemonClient;
use crate::output::{OutputFormat, TableDisplay, print_item, print_list, print_success};
use crate::generated::{Vm, VmSpec, VmState};

#[derive(Subcommand)]
pub enum VmCommands {
    /// List all VMs
    List,

    /// Get VM details
    Get {
        /// VM ID
        id: String,
    },

    /// Create a new VM
    Create {
        /// VM name
        #[arg(short, long)]
        name: String,

        /// Architecture (aarch64)
        #[arg(long, default_value = "aarch64")]
        arch: String,

        /// Machine type (virt, raspi3b)
        #[arg(long, default_value = "virt")]
        machine: String,

        /// Number of CPUs
        #[arg(short, long, default_value = "2")]
        cpus: i32,

        /// Memory in MB
        #[arg(short, long, default_value = "2048")]
        memory: i64,

        /// Boot disk volume ID
        #[arg(short, long)]
        boot_disk: String,

        /// Network IDs to attach
        #[arg(long)]
        network: Vec<String>,

        /// Volume IDs to attach
        #[arg(long)]
        volume: Vec<String>,

        /// QoS profile ID
        #[arg(long)]
        qos_profile: Option<String>,

        /// Enable TPM
        #[arg(long)]
        enable_tpm: bool,

        /// Compatibility mode (slow raspi emulation)
        #[arg(long)]
        compatibility_mode: bool,
    },

    /// Start a VM
    Start {
        /// VM ID
        id: String,
    },

    /// Stop a VM
    Stop {
        /// VM ID
        id: String,

        /// Force stop (SIGKILL)
        #[arg(short, long)]
        force: bool,
    },

    /// Delete a VM
    Delete {
        /// VM ID
        id: String,

        /// Force delete (even if running)
        #[arg(short, long)]
        force: bool,
    },

    /// Restart a VM
    Restart {
        /// VM ID
        id: String,

        /// Force restart
        #[arg(short, long)]
        force: bool,
    },
}

/// VM display wrapper for serialization
#[derive(Serialize)]
pub struct VmDisplay {
    pub id: String,
    pub name: String,
    pub state: String,
    pub cpus: i32,
    pub memory_mb: i64,
    pub arch: String,
    pub machine: String,
}

impl From<Vm> for VmDisplay {
    fn from(vm: Vm) -> Self {
        let meta = vm.meta.unwrap_or_default();
        let spec = vm.spec.unwrap_or_default();
        let status = vm.status.unwrap_or_default();
        
        let state_str = VmState::try_from(status.state)
            .map(|s| format!("{:?}", s))
            .unwrap_or_else(|_| "Unknown".to_string());
        
        Self {
            id: meta.id,
            name: meta.name,
            state: state_str,
            cpus: spec.cpu_cores,
            memory_mb: spec.memory_mb,
            arch: spec.arch,
            machine: spec.machine,
        }
    }
}

impl TableDisplay for VmDisplay {
    fn headers() -> Vec<&'static str> {
        vec!["ID", "Name", "State", "CPUs", "Memory", "Arch", "Machine"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.name.clone(),
            self.state.clone(),
            self.cpus.to_string(),
            format!("{}MB", self.memory_mb),
            self.arch.clone(),
            self.machine.clone(),
        ]
    }
}

pub async fn execute(cmd: VmCommands, mut client: DaemonClient, format: OutputFormat) -> Result<()> {
    match cmd {
        VmCommands::List => {
            let vms = client.list_vms().await?;
            let displays: Vec<VmDisplay> = vms.into_iter().map(VmDisplay::from).collect();
            print_list(&displays, format);
        }

        VmCommands::Get { id } => {
            let vm = client.get_vm(&id).await?;
            let display = VmDisplay::from(vm);
            print_item(&display, format);
        }

        VmCommands::Create {
            name,
            arch,
            machine,
            cpus,
            memory,
            boot_disk,
            network,
            volume,
            qos_profile,
            enable_tpm,
            compatibility_mode,
        } => {
            let spec = VmSpec {
                arch,
                machine,
                cpu_cores: cpus,
                memory_mb: memory,
                volume_ids: volume,
                network_ids: network,
                qos_profile_id: qos_profile.unwrap_or_default(),
                enable_tpm,
                boot_disk_id: boot_disk,
                extra_args: Default::default(),
                compatibility_mode,
            };

            let vm = client.create_vm(&name, spec).await?;
            let display = VmDisplay::from(vm);
            print_success(&format!("VM '{}' created", display.name));
            print_item(&display, format);
        }

        VmCommands::Start { id } => {
            let vm = client.start_vm(&id).await?;
            let display = VmDisplay::from(vm);
            print_success(&format!("VM '{}' started", display.name));
        }

        VmCommands::Stop { id, force } => {
            let vm = client.stop_vm(&id, force).await?;
            let display = VmDisplay::from(vm);
            print_success(&format!("VM '{}' stopped", display.name));
        }

        VmCommands::Delete { id, force } => {
            client.delete_vm(&id, force).await?;
            print_success(&format!("VM '{}' deleted", id));
        }

        VmCommands::Restart { id, force } => {
            client.stop_vm(&id, force).await?;
            let vm = client.start_vm(&id).await?;
            let display = VmDisplay::from(vm);
            print_success(&format!("VM '{}' restarted", display.name));
        }
    }

    Ok(())
}
