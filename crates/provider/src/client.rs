//! Client for communicating with the InfraSim daemon

use tonic::transport::Channel;
use anyhow::Result;

use crate::generated::infrasim::infra_sim_daemon_client::InfraSimDaemonClient;
use crate::generated::infrasim::*;

/// Client wrapper for daemon communication
pub struct DaemonClient {
    client: InfraSimDaemonClient<Channel>,
}

impl DaemonClient {
    /// Connect to the daemon
    pub async fn connect(addr: &str) -> Result<Self> {
        let client = InfraSimDaemonClient::connect(addr.to_string()).await?;
        Ok(Self { client })
    }

    // Network operations

    pub async fn create_network(&mut self, name: &str, spec: NetworkSpec) -> Result<Network> {
        let request = tonic::Request::new(CreateNetworkRequest {
            name: name.to_string(),
            spec: Some(spec),
            labels: Default::default(),
        });
        let response = self.client.create_network(request).await?;
        response.into_inner().network
            .ok_or_else(|| anyhow::anyhow!("No network in response"))
    }

    pub async fn get_network(&mut self, id: &str) -> Result<Network> {
        let request = tonic::Request::new(GetNetworkRequest { id: id.to_string() });
        let response = self.client.get_network(request).await?;
        response.into_inner().network
            .ok_or_else(|| anyhow::anyhow!("Network not found"))
    }

    pub async fn delete_network(&mut self, id: &str) -> Result<()> {
        let request = tonic::Request::new(DeleteNetworkRequest {
            id: id.to_string(),
        });
        self.client.delete_network(request).await?;
        Ok(())
    }

    // VM operations

    pub async fn create_vm(&mut self, name: &str, spec: VmSpec) -> Result<Vm> {
        let request = tonic::Request::new(CreateVmRequest {
            name: name.to_string(),
            spec: Some(spec),
            labels: Default::default(),
        });
        let response = self.client.create_vm(request).await?;
        response.into_inner().vm
            .ok_or_else(|| anyhow::anyhow!("No VM in response"))
    }

    pub async fn get_vm(&mut self, id: &str) -> Result<Vm> {
        let request = tonic::Request::new(GetVmRequest { id: id.to_string() });
        let response = self.client.get_vm(request).await?;
        response.into_inner().vm
            .ok_or_else(|| anyhow::anyhow!("VM not found"))
    }

    pub async fn start_vm(&mut self, id: &str) -> Result<Vm> {
        let request = tonic::Request::new(StartVmRequest { id: id.to_string() });
        let response = self.client.start_vm(request).await?;
        response.into_inner().vm
            .ok_or_else(|| anyhow::anyhow!("No VM in response"))
    }

    pub async fn stop_vm(&mut self, id: &str, force: bool) -> Result<Vm> {
        let request = tonic::Request::new(StopVmRequest {
            id: id.to_string(),
            force,
        });
        let response = self.client.stop_vm(request).await?;
        response.into_inner().vm
            .ok_or_else(|| anyhow::anyhow!("No VM in response"))
    }

    pub async fn delete_vm(&mut self, id: &str) -> Result<()> {
        let request = tonic::Request::new(DeleteVmRequest {
            id: id.to_string(),
            force: true,
        });
        self.client.delete_vm(request).await?;
        Ok(())
    }

    // Volume operations

    pub async fn create_volume(&mut self, name: &str, spec: VolumeSpec) -> Result<Volume> {
        let request = tonic::Request::new(CreateVolumeRequest {
            name: name.to_string(),
            spec: Some(spec),
            labels: Default::default(),
        });
        let response = self.client.create_volume(request).await?;
        response.into_inner().volume
            .ok_or_else(|| anyhow::anyhow!("No volume in response"))
    }

    pub async fn get_volume(&mut self, id: &str) -> Result<Volume> {
        let request = tonic::Request::new(GetVolumeRequest { id: id.to_string() });
        let response = self.client.get_volume(request).await?;
        response.into_inner().volume
            .ok_or_else(|| anyhow::anyhow!("Volume not found"))
    }

    pub async fn delete_volume(&mut self, id: &str) -> Result<()> {
        let request = tonic::Request::new(DeleteVolumeRequest {
            id: id.to_string(),
        });
        self.client.delete_volume(request).await?;
        Ok(())
    }

    // Snapshot operations

    pub async fn create_snapshot(&mut self, name: &str, spec: SnapshotSpec) -> Result<Snapshot> {
        let request = tonic::Request::new(CreateSnapshotRequest {
            name: name.to_string(),
            spec: Some(spec),
            labels: Default::default(),
        });
        let response = self.client.create_snapshot(request).await?;
        response.into_inner().snapshot
            .ok_or_else(|| anyhow::anyhow!("No snapshot in response"))
    }

    pub async fn restore_snapshot(&mut self, snapshot_id: &str, target_vm_id: Option<&str>) -> Result<Vm> {
        let request = tonic::Request::new(RestoreSnapshotRequest { 
            snapshot_id: snapshot_id.to_string(),
            target_vm_id: target_vm_id.unwrap_or_default().to_string(),
        });
        let response = self.client.restore_snapshot(request).await?;
        response.into_inner().vm
            .ok_or_else(|| anyhow::anyhow!("No VM in response"))
    }

    pub async fn delete_snapshot(&mut self, id: &str) -> Result<()> {
        let request = tonic::Request::new(DeleteSnapshotRequest { id: id.to_string() });
        self.client.delete_snapshot(request).await?;
        Ok(())
    }

    // Console operations

    pub async fn get_console(&mut self, id: &str) -> Result<String> {
        let request = tonic::Request::new(GetConsoleRequest { id: id.to_string() });
        let response = self.client.get_console(request).await?;
        Ok(response.into_inner().console
            .and_then(|c| c.status)
            .map(|s| s.web_url)
            .unwrap_or_default())
    }
}
