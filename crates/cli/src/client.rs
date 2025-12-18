//! Daemon gRPC Client

use tonic::transport::Channel;
use anyhow::Result;

use crate::generated::infra_sim_daemon_client::InfraSimDaemonClient;
use crate::generated::*;

/// Client for communicating with the InfraSim daemon
pub struct DaemonClient {
    client: InfraSimDaemonClient<Channel>,
}

impl DaemonClient {
    /// Create a new daemon client
    pub async fn new(addr: &str) -> Result<Self> {
        let client = InfraSimDaemonClient::connect(addr.to_string()).await?;
        Ok(Self { client })
    }

    /// Check if the daemon is healthy
    pub async fn health_check(&mut self) -> bool {
        let request = tonic::Request::new(GetHealthRequest {});
        self.client.get_health(request).await.is_ok()
    }

    // VM operations

    /// Create a new VM
    pub async fn create_vm(&mut self, name: &str, spec: VmSpec) -> Result<Vm> {
        let request = tonic::Request::new(CreateVmRequest {
            name: name.to_string(),
            spec: Some(spec),
            labels: Default::default(),
        });
        let response = self.client.create_vm(request).await?;
        response.into_inner().vm.ok_or_else(|| anyhow::anyhow!("No VM in response"))
    }

    /// Get a VM by ID
    pub async fn get_vm(&mut self, id: &str) -> Result<Vm> {
        let request = tonic::Request::new(GetVmRequest { id: id.to_string() });
        let response = self.client.get_vm(request).await?;
        response.into_inner().vm.ok_or_else(|| anyhow::anyhow!("VM not found"))
    }

    /// List all VMs
    pub async fn list_vms(&mut self) -> Result<Vec<Vm>> {
        let request = tonic::Request::new(ListVMsRequest {
            label_selector: Default::default(),
        });
        let response = self.client.list_v_ms(request).await?;
        Ok(response.into_inner().vms)
    }

    /// Start a VM
    pub async fn start_vm(&mut self, id: &str) -> Result<Vm> {
        let request = tonic::Request::new(StartVmRequest { id: id.to_string() });
        let response = self.client.start_vm(request).await?;
        response.into_inner().vm.ok_or_else(|| anyhow::anyhow!("No VM in response"))
    }

    /// Stop a VM
    pub async fn stop_vm(&mut self, id: &str, force: bool) -> Result<Vm> {
        let request = tonic::Request::new(StopVmRequest {
            id: id.to_string(),
            force,
        });
        let response = self.client.stop_vm(request).await?;
        response.into_inner().vm.ok_or_else(|| anyhow::anyhow!("No VM in response"))
    }

    /// Delete a VM
    pub async fn delete_vm(&mut self, id: &str, force: bool) -> Result<()> {
        let request = tonic::Request::new(DeleteVmRequest {
            id: id.to_string(),
            force,
        });
        self.client.delete_vm(request).await?;
        Ok(())
    }

    // Network operations

    /// Create a network
    pub async fn create_network(&mut self, name: &str, spec: NetworkSpec) -> Result<Network> {
        let request = tonic::Request::new(CreateNetworkRequest {
            name: name.to_string(),
            spec: Some(spec),
            labels: Default::default(),
        });
        let response = self.client.create_network(request).await?;
        response.into_inner().network.ok_or_else(|| anyhow::anyhow!("No network in response"))
    }

    /// Get a network by ID
    pub async fn get_network(&mut self, id: &str) -> Result<Network> {
        let request = tonic::Request::new(GetNetworkRequest { id: id.to_string() });
        let response = self.client.get_network(request).await?;
        response.into_inner().network.ok_or_else(|| anyhow::anyhow!("Network not found"))
    }

    /// List all networks
    pub async fn list_networks(&mut self) -> Result<Vec<Network>> {
        let request = tonic::Request::new(ListNetworksRequest {
            label_selector: Default::default(),
        });
        let response = self.client.list_networks(request).await?;
        Ok(response.into_inner().networks)
    }

    /// Delete a network
    pub async fn delete_network(&mut self, id: &str) -> Result<()> {
        let request = tonic::Request::new(DeleteNetworkRequest { id: id.to_string() });
        self.client.delete_network(request).await?;
        Ok(())
    }

    // Volume operations

    /// Create a volume
    pub async fn create_volume(&mut self, name: &str, spec: VolumeSpec) -> Result<Volume> {
        let request = tonic::Request::new(CreateVolumeRequest {
            name: name.to_string(),
            spec: Some(spec),
            labels: Default::default(),
        });
        let response = self.client.create_volume(request).await?;
        response.into_inner().volume.ok_or_else(|| anyhow::anyhow!("No volume in response"))
    }

    /// Get a volume by ID
    pub async fn get_volume(&mut self, id: &str) -> Result<Volume> {
        let request = tonic::Request::new(GetVolumeRequest { id: id.to_string() });
        let response = self.client.get_volume(request).await?;
        response.into_inner().volume.ok_or_else(|| anyhow::anyhow!("Volume not found"))
    }

    /// List all volumes
    pub async fn list_volumes(&mut self) -> Result<Vec<Volume>> {
        let request = tonic::Request::new(ListVolumesRequest {
            label_selector: Default::default(),
            kind_filter: 0, // VolumeKind::Unspecified = all
        });
        let response = self.client.list_volumes(request).await?;
        Ok(response.into_inner().volumes)
    }

    /// Delete a volume
    pub async fn delete_volume(&mut self, id: &str) -> Result<()> {
        let request = tonic::Request::new(DeleteVolumeRequest { id: id.to_string() });
        self.client.delete_volume(request).await?;
        Ok(())
    }

    // Snapshot operations

    /// Create a snapshot
    pub async fn create_snapshot(&mut self, name: &str, spec: SnapshotSpec) -> Result<Snapshot> {
        let request = tonic::Request::new(CreateSnapshotRequest {
            name: name.to_string(),
            spec: Some(spec),
            labels: Default::default(),
        });
        let response = self.client.create_snapshot(request).await?;
        response.into_inner().snapshot.ok_or_else(|| anyhow::anyhow!("No snapshot in response"))
    }

    /// Get a snapshot by ID
    pub async fn get_snapshot(&mut self, id: &str) -> Result<Snapshot> {
        let request = tonic::Request::new(GetSnapshotRequest { id: id.to_string() });
        let response = self.client.get_snapshot(request).await?;
        response.into_inner().snapshot.ok_or_else(|| anyhow::anyhow!("Snapshot not found"))
    }

    /// List snapshots
    pub async fn list_snapshots(&mut self, vm_id: Option<String>) -> Result<Vec<Snapshot>> {
        let request = tonic::Request::new(ListSnapshotsRequest {
            vm_id: vm_id.unwrap_or_default(),
            label_selector: Default::default(),
        });
        let response = self.client.list_snapshots(request).await?;
        Ok(response.into_inner().snapshots)
    }

    /// Restore a snapshot
    pub async fn restore_snapshot(&mut self, id: &str, target_vm: Option<String>) -> Result<Vm> {
        let request = tonic::Request::new(RestoreSnapshotRequest {
            snapshot_id: id.to_string(),
            target_vm_id: target_vm.unwrap_or_default(),
        });
        let response = self.client.restore_snapshot(request).await?;
        response.into_inner().vm.ok_or_else(|| anyhow::anyhow!("No VM in response"))
    }

    /// Delete a snapshot
    pub async fn delete_snapshot(&mut self, id: &str) -> Result<()> {
        let request = tonic::Request::new(DeleteSnapshotRequest { id: id.to_string() });
        self.client.delete_snapshot(request).await?;
        Ok(())
    }

    // Console operations

    /// Get console URL
    pub async fn get_console(&mut self, id: &str) -> Result<String> {
        let request = tonic::Request::new(GetConsoleRequest { id: id.to_string() });
        let response = self.client.get_console(request).await?;
        Ok(response.into_inner().console
            .and_then(|c| c.status)
            .map(|s| s.web_url)
            .unwrap_or_default())
    }

    // Benchmark operations

    /// Run a benchmark
    pub async fn run_benchmark(&mut self, vm_id: &str, tests: Vec<String>) -> Result<BenchmarkRun> {
        let request = tonic::Request::new(CreateBenchmarkRunRequest {
            name: format!("benchmark-{}", chrono::Utc::now().timestamp()),
            spec: Some(BenchmarkSpec {
                vm_id: vm_id.to_string(),
                suite_name: "default".to_string(),
                test_names: tests,
                timeout_seconds: 300,
                parameters: Default::default(),
            }),
            labels: Default::default(),
        });
        let response = self.client.create_benchmark_run(request).await?;
        response.into_inner().run.ok_or_else(|| anyhow::anyhow!("No benchmark run in response"))
    }

    /// Get benchmark run
    pub async fn get_benchmark_run(&mut self, id: &str) -> Result<BenchmarkRun> {
        let request = tonic::Request::new(GetBenchmarkRunRequest { id: id.to_string() });
        let response = self.client.get_benchmark_run(request).await?;
        response.into_inner().run.ok_or_else(|| anyhow::anyhow!("Benchmark run not found"))
    }

    /// List benchmark runs
    pub async fn list_benchmark_runs(&mut self, vm_id: Option<String>) -> Result<Vec<BenchmarkRun>> {
        let request = tonic::Request::new(ListBenchmarkRunsRequest {
            vm_id: vm_id.unwrap_or_default(),
            label_selector: Default::default(),
        });
        let response = self.client.list_benchmark_runs(request).await?;
        Ok(response.into_inner().runs)
    }

    // Attestation operations

    /// Get attestation report
    pub async fn get_attestation(&mut self, vm_id: &str) -> Result<AttestationReport> {
        let request = tonic::Request::new(GetAttestationRequest { vm_id: vm_id.to_string() });
        let response = self.client.get_attestation(request).await?;
        response.into_inner().report.ok_or_else(|| anyhow::anyhow!("No report in response"))
    }
}
