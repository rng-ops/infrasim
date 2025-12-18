//! Network Commands

use clap::Subcommand;
use anyhow::Result;
use serde::Serialize;

use crate::client::DaemonClient;
use crate::output::{OutputFormat, TableDisplay, print_item, print_list, print_success};
use crate::generated::{Network, NetworkSpec, NetworkMode};

#[derive(Subcommand)]
pub enum NetworkCommands {
    /// List all networks
    List,

    /// Get network details
    Get {
        /// Network ID
        id: String,
    },

    /// Create a new network
    Create {
        /// Network name
        #[arg(short, long)]
        name: String,

        /// Network mode (user, vmnet-shared, vmnet-bridged)
        #[arg(short, long, default_value = "user")]
        mode: String,

        /// CIDR notation (e.g., 192.168.1.0/24)
        #[arg(long, default_value = "192.168.100.0/24")]
        cidr: String,

        /// Gateway IP
        #[arg(long)]
        gateway: Option<String>,

        /// DNS server
        #[arg(long)]
        dns: Option<String>,

        /// Enable DHCP
        #[arg(long, default_value = "true")]
        dhcp: bool,

        /// MTU size
        #[arg(long, default_value = "1500")]
        mtu: i32,
    },

    /// Delete a network
    Delete {
        /// Network ID
        id: String,
    },
}

/// Network display wrapper for serialization
#[derive(Serialize)]
pub struct NetworkDisplay {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub cidr: String,
    pub gateway: String,
    pub active: bool,
}

impl From<Network> for NetworkDisplay {
    fn from(net: Network) -> Self {
        let meta = net.meta.unwrap_or_default();
        let spec = net.spec.unwrap_or_default();
        let status = net.status.unwrap_or_default();
        
        let mode_str = NetworkMode::try_from(spec.mode)
            .map(|m| format!("{:?}", m))
            .unwrap_or_else(|_| "Unknown".to_string());
        
        Self {
            id: meta.id,
            name: meta.name,
            mode: mode_str,
            cidr: spec.cidr,
            gateway: spec.gateway,
            active: status.active,
        }
    }
}

impl TableDisplay for NetworkDisplay {
    fn headers() -> Vec<&'static str> {
        vec!["ID", "Name", "Mode", "CIDR", "Gateway", "Active"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.id.clone(),
            self.name.clone(),
            self.mode.clone(),
            self.cidr.clone(),
            self.gateway.clone(),
            self.active.to_string(),
        ]
    }
}

pub async fn execute(cmd: NetworkCommands, mut client: DaemonClient, format: OutputFormat) -> Result<()> {
    match cmd {
        NetworkCommands::List => {
            let networks = client.list_networks().await?;
            let displays: Vec<NetworkDisplay> = networks.into_iter().map(NetworkDisplay::from).collect();
            print_list(&displays, format);
        }

        NetworkCommands::Get { id } => {
            let net = client.get_network(&id).await?;
            let display = NetworkDisplay::from(net);
            print_item(&display, format);
        }

        NetworkCommands::Create {
            name,
            mode,
            cidr,
            gateway,
            dns,
            dhcp,
            mtu,
        } => {
            let mode_enum = match mode.to_lowercase().as_str() {
                "user" => NetworkMode::User,
                "vmnet-shared" | "vmnet_shared" => NetworkMode::VmnetShared,
                "vmnet-bridged" | "vmnet_bridged" => NetworkMode::VmnetBridged,
                _ => NetworkMode::User,
            };

            let spec = NetworkSpec {
                mode: mode_enum as i32,
                cidr,
                gateway: gateway.unwrap_or_default(),
                dns: dns.unwrap_or_default(),
                dhcp_enabled: dhcp,
                mtu,
            };

            let net = client.create_network(&name, spec).await?;
            let display = NetworkDisplay::from(net);
            print_success(&format!("Network '{}' created", display.name));
            print_item(&display, format);
        }

        NetworkCommands::Delete { id } => {
            client.delete_network(&id).await?;
            print_success(&format!("Network '{}' deleted", id));
        }
    }

    Ok(())
}
