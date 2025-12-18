//! Network Resource handler for Terraform

use anyhow::Result;
use crate::client::DaemonClient;
use crate::state::{
    DynamicValue, get_string_attr, get_int_attr, get_bool_attr,
    make_state, string_value, int_value, bool_value,
};
use crate::generated::infrasim::{NetworkSpec, NetworkMode};
use super::Resource;

pub struct NetworkResource;

#[async_trait::async_trait]
impl Resource for NetworkResource {
    fn type_name() -> &'static str {
        "infrasim_network"
    }

    async fn create(client: &mut DaemonClient, config: &DynamicValue) -> Result<DynamicValue> {
        let name = get_string_attr(config, "name");
        
        let mode = match get_string_attr(config, "mode").as_str() {
            "vmnet_shared" => NetworkMode::VmnetShared as i32,
            "vmnet_bridged" => NetworkMode::VmnetBridged as i32,
            _ => NetworkMode::User as i32,
        };
        
        let spec = NetworkSpec {
            mode,
            cidr: get_string_attr(config, "cidr"),
            gateway: get_string_attr(config, "gateway"),
            dns: get_string_attr(config, "dns"),
            dhcp_enabled: get_bool_attr(config, "dhcp_enabled", true),
            mtu: get_int_attr(config, "mtu", 1500) as i32,
        };

        let network = client.create_network(&name, spec).await?;
        network_to_state(&network)
    }

    async fn read(client: &mut DaemonClient, state: &DynamicValue) -> Result<DynamicValue> {
        let id = get_string_attr(state, "id");
        let network = client.get_network(&id).await?;
        network_to_state(&network)
    }

    async fn update(client: &mut DaemonClient, state: &DynamicValue, _config: &DynamicValue) -> Result<DynamicValue> {
        // Networks are currently immutable - just read the current state
        Self::read(client, state).await
    }

    async fn delete(client: &mut DaemonClient, state: &DynamicValue) -> Result<()> {
        let id = get_string_attr(state, "id");
        client.delete_network(&id).await
    }
}

fn network_to_state(net: &crate::generated::infrasim::Network) -> Result<DynamicValue> {
    let meta = net.meta.clone().unwrap_or_default();
    let spec = net.spec.clone().unwrap_or_default();
    let status = net.status.clone().unwrap_or_default();
    
    let mode_str = NetworkMode::try_from(spec.mode)
        .map(|m| format!("{:?}", m))
        .unwrap_or_else(|_| "Unknown".to_string());
    
    Ok(make_state(vec![
        ("id", string_value(&meta.id)),
        ("name", string_value(&meta.name)),
        ("mode", string_value(&mode_str)),
        ("cidr", string_value(&spec.cidr)),
        ("gateway", string_value(&spec.gateway)),
        ("dns", string_value(&spec.dns)),
        ("dhcp_enabled", bool_value(spec.dhcp_enabled)),
        ("mtu", int_value(spec.mtu as i64)),
        ("active", bool_value(status.active)),
    ]))
}
