//! Snapshot Resource Implementation

use anyhow::Result;
use crate::client::DaemonClient;
use crate::state::{
    DynamicValue, get_string_attr, get_bool_attr,
    make_state, string_value, int_value, bool_value,
};
use crate::generated::infrasim::SnapshotSpec;
use super::Resource;

pub struct SnapshotResource;

#[async_trait::async_trait]
impl Resource for SnapshotResource {
    fn type_name() -> &'static str {
        "infrasim_snapshot"
    }

    async fn create(client: &mut DaemonClient, config: &DynamicValue) -> Result<DynamicValue> {
        let name = get_string_attr(config, "name");
        let vm_id = get_string_attr(config, "vm_id");
        let include_memory = get_bool_attr(config, "include_memory", false);
        let include_disk = get_bool_attr(config, "include_disk", true);
        let description = get_string_attr(config, "description");

        let spec = SnapshotSpec {
            vm_id: vm_id.clone(),
            include_memory,
            include_disk,
            description,
        };

        let snapshot = client.create_snapshot(&name, spec).await?;
        let meta = snapshot.meta.unwrap_or_default();
        let snap_spec = snapshot.spec.unwrap_or_default();
        let status = snapshot.status.unwrap_or_default();

        Ok(make_state(vec![
            ("id", string_value(&meta.id)),
            ("name", string_value(&meta.name)),
            ("vm_id", string_value(&snap_spec.vm_id)),
            ("include_memory", bool_value(snap_spec.include_memory)),
            ("include_disk", bool_value(snap_spec.include_disk)),
            ("description", string_value(&snap_spec.description)),
            ("size_bytes", int_value(status.size_bytes)),
            ("complete", bool_value(status.complete)),
        ]))
    }

    async fn read(_client: &mut DaemonClient, state: &DynamicValue) -> Result<DynamicValue> {
        // Snapshots don't have a direct read API, so we return the current state
        // In a full implementation, we would have a GetSnapshot RPC
        Ok(state.clone())
    }

    async fn update(_client: &mut DaemonClient, state: &DynamicValue, _config: &DynamicValue) -> Result<DynamicValue> {
        // Snapshots are immutable
        Ok(state.clone())
    }

    async fn delete(client: &mut DaemonClient, state: &DynamicValue) -> Result<()> {
        let id = get_string_attr(state, "id");
        client.delete_snapshot(&id).await
    }
}
