//! VM Resource handler for Terraform

use anyhow::Result;
use crate::client::DaemonClient;
use crate::state::{
    DynamicValue, get_string_attr, get_int_attr, get_bool_attr,
    make_state, string_value, int_value, bool_value,
};
use crate::generated::infrasim::{VmSpec, VmState};
use super::Resource;

pub struct VmResource;

#[async_trait::async_trait]
impl Resource for VmResource {
    fn type_name() -> &'static str {
        "infrasim_vm"
    }

    async fn create(client: &mut DaemonClient, config: &DynamicValue) -> Result<DynamicValue> {
        let name = get_string_attr(config, "name");
        
        let spec = VmSpec {
            arch: get_string_attr(config, "arch"),
            machine: get_string_attr(config, "machine"),
            cpu_cores: get_int_attr(config, "cpu_cores", 2) as i32,
            memory_mb: get_int_attr(config, "memory_mb", 2048),
            volume_ids: vec![],
            network_ids: vec![],
            qos_profile_id: get_string_attr(config, "qos_profile_id"),
            enable_tpm: get_bool_attr(config, "enable_tpm", false),
            boot_disk_id: get_string_attr(config, "boot_disk_id"),
            extra_args: Default::default(),
            compatibility_mode: false,
        };

        let vm = client.create_vm(&name, spec).await?;
        vm_to_state(&vm)
    }

    async fn read(client: &mut DaemonClient, state: &DynamicValue) -> Result<DynamicValue> {
        let id = get_string_attr(state, "id");
        let vm = client.get_vm(&id).await?;
        vm_to_state(&vm)
    }

    async fn update(client: &mut DaemonClient, state: &DynamicValue, _config: &DynamicValue) -> Result<DynamicValue> {
        // VMs are currently immutable - just read the current state
        Self::read(client, state).await
    }

    async fn delete(client: &mut DaemonClient, state: &DynamicValue) -> Result<()> {
        let id = get_string_attr(state, "id");
        client.delete_vm(&id).await
    }
}

fn vm_to_state(vm: &crate::generated::infrasim::Vm) -> Result<DynamicValue> {
    let meta = vm.meta.clone().unwrap_or_default();
    let spec = vm.spec.clone().unwrap_or_default();
    let status = vm.status.clone().unwrap_or_default();
    
    let state_str = VmState::try_from(status.state)
        .map(|s| format!("{:?}", s))
        .unwrap_or_else(|_| "Unknown".to_string());
    
    Ok(make_state(vec![
        ("id", string_value(&meta.id)),
        ("name", string_value(&meta.name)),
        ("arch", string_value(&spec.arch)),
        ("machine", string_value(&spec.machine)),
        ("cpu_cores", int_value(spec.cpu_cores as i64)),
        ("memory_mb", int_value(spec.memory_mb as i64)),
        ("boot_disk_id", string_value(&spec.boot_disk_id)),
        ("state", string_value(&state_str)),
        ("enable_tpm", bool_value(spec.enable_tpm)),
    ]))
}
