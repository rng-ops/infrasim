//! Volume Resource handler for Terraform

use anyhow::Result;
use crate::client::DaemonClient;
use crate::state::{
    DynamicValue, get_string_attr, get_int_attr, get_bool_attr,
    make_state, string_value, int_value, bool_value,
};
use crate::generated::infrasim::{VolumeSpec, VolumeKind};
use super::Resource;

pub struct VolumeResource;

#[async_trait::async_trait]
impl Resource for VolumeResource {
    fn type_name() -> &'static str {
        "infrasim_volume"
    }

    async fn create(client: &mut DaemonClient, config: &DynamicValue) -> Result<DynamicValue> {
        let name = get_string_attr(config, "name");
        
        let kind = match get_string_attr(config, "kind").as_str() {
            "weights" => VolumeKind::Weights as i32,
            _ => VolumeKind::Disk as i32,
        };
        
        let spec = VolumeSpec {
            kind,
            source: get_string_attr(config, "source"),
            integrity: None,
            read_only: get_bool_attr(config, "read_only", false),
            size_bytes: get_int_attr(config, "size_bytes", 10 * 1024 * 1024 * 1024),
            format: get_string_attr(config, "format"),
            overlay: get_bool_attr(config, "overlay", false),
        };

        let volume = client.create_volume(&name, spec).await?;
        volume_to_state(&volume)
    }

    async fn read(client: &mut DaemonClient, state: &DynamicValue) -> Result<DynamicValue> {
        let id = get_string_attr(state, "id");
        let volume = client.get_volume(&id).await?;
        volume_to_state(&volume)
    }

    async fn update(client: &mut DaemonClient, state: &DynamicValue, _config: &DynamicValue) -> Result<DynamicValue> {
        // Volumes are currently immutable - just read the current state
        Self::read(client, state).await
    }

    async fn delete(client: &mut DaemonClient, state: &DynamicValue) -> Result<()> {
        let id = get_string_attr(state, "id");
        client.delete_volume(&id).await
    }
}

fn volume_to_state(vol: &crate::generated::infrasim::Volume) -> Result<DynamicValue> {
    let meta = vol.meta.clone().unwrap_or_default();
    let spec = vol.spec.clone().unwrap_or_default();
    let status = vol.status.clone().unwrap_or_default();
    
    let kind_str = VolumeKind::try_from(spec.kind)
        .map(|k| format!("{:?}", k))
        .unwrap_or_else(|_| "Unknown".to_string());
    
    Ok(make_state(vec![
        ("id", string_value(&meta.id)),
        ("name", string_value(&meta.name)),
        ("kind", string_value(&kind_str)),
        ("source", string_value(&spec.source)),
        ("format", string_value(&spec.format)),
        ("size_bytes", int_value(spec.size_bytes)),
        ("read_only", bool_value(spec.read_only)),
        ("overlay", bool_value(spec.overlay)),
        ("ready", bool_value(status.ready)),
        ("digest", string_value(&status.digest)),
    ]))
}
