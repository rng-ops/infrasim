//! gRPC server implementation

use crate::config::DaemonConfig;
use crate::generated::infra_sim_daemon_server::{InfraSimDaemon, InfraSimDaemonServer};
use crate::generated::{
    self, 
    VmState as ProtoVmState,
    NetworkMode as ProtoNetworkMode,
    VolumeKind as ProtoVolumeKind,
    ResourceMeta, Vm, VmSpec, VmStatus, 
    Network, NetworkSpec, NetworkStatus,
    Volume, VolumeSpec, IntegrityConfig,
    Snapshot, SnapshotSpec,
    QoSProfile, QoSProfileSpec,
    CreateVmRequest, CreateVmResponse,
    GetVmRequest, GetVmResponse,
    UpdateVmRequest, UpdateVmResponse,
    DeleteVmRequest, DeleteVmResponse,
    ListVMsRequest, ListVMsResponse,
    StartVmRequest, StartVmResponse,
    StopVmRequest, StopVmResponse,
    CreateNetworkRequest, CreateNetworkResponse,
    GetNetworkRequest, GetNetworkResponse,
    DeleteNetworkRequest, DeleteNetworkResponse,
    ListNetworksRequest, ListNetworksResponse,
    CreateQoSProfileRequest, CreateQoSProfileResponse,
    GetQoSProfileRequest, GetQoSProfileResponse,
    DeleteQoSProfileRequest, DeleteQoSProfileResponse,
    ListQoSProfilesRequest, ListQoSProfilesResponse,
    CreateVolumeRequest, CreateVolumeResponse,
    GetVolumeRequest, GetVolumeResponse,
    DeleteVolumeRequest, DeleteVolumeResponse,
    ListVolumesRequest, ListVolumesResponse,
    CreateConsoleRequest, CreateConsoleResponse,
    GetConsoleRequest, GetConsoleResponse,
    DeleteConsoleRequest, DeleteConsoleResponse,
    CreateSnapshotRequest, CreateSnapshotResponse,
    GetSnapshotRequest, GetSnapshotResponse,
    DeleteSnapshotRequest, DeleteSnapshotResponse,
    ListSnapshotsRequest, ListSnapshotsResponse,
    RestoreSnapshotRequest, RestoreSnapshotResponse,
    CreateBenchmarkRunRequest, CreateBenchmarkRunResponse,
    GetBenchmarkRunRequest, GetBenchmarkRunResponse,
    ListBenchmarkRunsRequest, ListBenchmarkRunsResponse,
    GetAttestationRequest, GetAttestationResponse,
    CreateLoRaDeviceRequest, CreateLoRaDeviceResponse,
    GetLoRaDeviceRequest, GetLoRaDeviceResponse,
    DeleteLoRaDeviceRequest, DeleteLoRaDeviceResponse,
    GetHealthRequest, GetHealthResponse,
    GetDaemonStatusRequest, GetDaemonStatusResponse,
    Console, ConsoleSpec, ConsoleStatus,
    HostProvenance, AttestationReport,
};
use crate::qemu::{QemuLauncher, VolumePreparer};
use crate::state::StateManager;
use infrasim_common::{
    attestation::AttestationProvider,
    types::{self, NetworkMode, VolumeKind},
};
use std::collections::HashMap;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

/// gRPC service implementation
pub struct DaemonService {
    state: StateManager,
    qemu: QemuLauncher,
    volume_preparer: VolumePreparer,
    config: DaemonConfig,
}

impl DaemonService {
    pub fn new(state: StateManager, config: DaemonConfig) -> Self {
        Self {
            qemu: QemuLauncher::new(config.clone()),
            volume_preparer: VolumePreparer::new(config.clone()),
            state,
            config,
        }
    }
}

#[tonic::async_trait]
impl InfraSimDaemon for DaemonService {
    // ========================================================================
    // VM operations
    // ========================================================================

    async fn create_vm(
        &self,
        request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        let req = request.into_inner();
        debug!("CreateVM: {}", req.name);

        let spec = req.spec.ok_or_else(|| Status::invalid_argument("spec required"))?;

        let vm_spec = types::VmSpec {
            arch: spec.arch,
            machine: spec.machine,
            cpu_cores: spec.cpu_cores as u32,
            memory_mb: spec.memory_mb as u64,
            volume_ids: spec.volume_ids,
            network_ids: spec.network_ids,
            qos_profile_id: if spec.qos_profile_id.is_empty() {
                None
            } else {
                Some(spec.qos_profile_id)
            },
            enable_tpm: spec.enable_tpm,
            boot_disk_id: if spec.boot_disk_id.is_empty() {
                None
            } else {
                Some(spec.boot_disk_id)
            },
            extra_args: spec.extra_args,
            compatibility_mode: spec.compatibility_mode,
        };

        let vm = self
            .state
            .create_vm(req.name, vm_spec, req.labels)
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(CreateVmResponse {
            vm: Some(vm_to_proto(&vm)),
        }))
    }

    async fn get_vm(&self, request: Request<GetVmRequest>) -> Result<Response<GetVmResponse>, Status> {
        let req = request.into_inner();

        let vm = self
            .state
            .get_vm(&req.id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("VM not found"))?;

        Ok(Response::new(GetVmResponse {
            vm: Some(vm_to_proto(&vm)),
        }))
    }

    async fn update_vm(
        &self,
        request: Request<UpdateVmRequest>,
    ) -> Result<Response<UpdateVmResponse>, Status> {
        let req = request.into_inner();
        let spec = req.spec.ok_or_else(|| Status::invalid_argument("spec required"))?;

        let vm_spec = types::VmSpec {
            arch: spec.arch,
            machine: spec.machine,
            cpu_cores: spec.cpu_cores as u32,
            memory_mb: spec.memory_mb as u64,
            volume_ids: spec.volume_ids,
            network_ids: spec.network_ids,
            qos_profile_id: if spec.qos_profile_id.is_empty() {
                None
            } else {
                Some(spec.qos_profile_id)
            },
            enable_tpm: spec.enable_tpm,
            boot_disk_id: if spec.boot_disk_id.is_empty() {
                None
            } else {
                Some(spec.boot_disk_id)
            },
            extra_args: spec.extra_args,
            compatibility_mode: spec.compatibility_mode,
        };

        self.state
            .update_vm_spec(&req.id, vm_spec)
            .map_err(|e| Status::from(e))?;

        let vm = self
            .state
            .get_vm(&req.id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("VM not found"))?;

        Ok(Response::new(UpdateVmResponse {
            vm: Some(vm_to_proto(&vm)),
        }))
    }

    async fn delete_vm(
        &self,
        request: Request<DeleteVmRequest>,
    ) -> Result<Response<DeleteVmResponse>, Status> {
        let req = request.into_inner();

        // Stop VM if running
        if req.force {
            let _ = self.qemu.stop(&self.state, &req.id, true).await;
        }

        self.state
            .delete_vm(&req.id)
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(DeleteVmResponse {}))
    }

    async fn list_v_ms(
        &self,
        _request: Request<ListVMsRequest>,
    ) -> Result<Response<ListVMsResponse>, Status> {
        let vms = self.state.list_vms().map_err(|e| Status::from(e))?;

        Ok(Response::new(ListVMsResponse {
            vms: vms.into_iter().map(|vm| vm_to_proto(&vm)).collect(),
        }))
    }

    async fn start_vm(
        &self,
        request: Request<StartVmRequest>,
    ) -> Result<Response<StartVmResponse>, Status> {
        let req = request.into_inner();

        let mut vm = self
            .state
            .get_vm(&req.id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("VM not found"))?;

        // Set desired state to running
        let status = types::VmStatus {
            state: types::VmState::Running,
            ..vm.status.clone()
        };
        self.state
            .update_vm_status(&req.id, status.clone())
            .map_err(|e| Status::from(e))?;

        vm.status = status;

        // Trigger immediate start
        self.qemu
            .start(&self.state, &vm)
            .await
            .map_err(|e| Status::from(e))?;

        // Refresh status
        let vm = self
            .state
            .get_vm(&req.id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("VM not found"))?;

        Ok(Response::new(StartVmResponse {
            vm: Some(vm_to_proto(&vm)),
        }))
    }

    async fn stop_vm(
        &self,
        request: Request<StopVmRequest>,
    ) -> Result<Response<StopVmResponse>, Status> {
        let req = request.into_inner();

        self.qemu
            .stop(&self.state, &req.id, req.force)
            .await
            .map_err(|e| Status::from(e))?;

        let vm = self
            .state
            .get_vm(&req.id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("VM not found"))?;

        Ok(Response::new(StopVmResponse {
            vm: Some(vm_to_proto(&vm)),
        }))
    }

    // ========================================================================
    // Network operations
    // ========================================================================

    async fn create_network(
        &self,
        request: Request<CreateNetworkRequest>,
    ) -> Result<Response<CreateNetworkResponse>, Status> {
        let req = request.into_inner();
        let spec = req.spec.ok_or_else(|| Status::invalid_argument("spec required"))?;

        let net_spec = types::NetworkSpec {
            mode: match ProtoNetworkMode::try_from(spec.mode) {
                Ok(ProtoNetworkMode::User) => NetworkMode::User,
                Ok(ProtoNetworkMode::VmnetShared) => NetworkMode::VmnetShared,
                Ok(ProtoNetworkMode::VmnetBridged) => NetworkMode::VmnetBridged,
                _ => NetworkMode::User,
            },
            cidr: spec.cidr,
            gateway: if spec.gateway.is_empty() {
                None
            } else {
                Some(spec.gateway)
            },
            dns: if spec.dns.is_empty() { None } else { Some(spec.dns) },
            dhcp_enabled: spec.dhcp_enabled,
            mtu: spec.mtu as u32,
        };

        let network = self
            .state
            .create_network(req.name, net_spec, req.labels)
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(CreateNetworkResponse {
            network: Some(network_to_proto(&network)),
        }))
    }

    async fn get_network(
        &self,
        request: Request<GetNetworkRequest>,
    ) -> Result<Response<GetNetworkResponse>, Status> {
        let req = request.into_inner();

        let network = self
            .state
            .get_network(&req.id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("Network not found"))?;

        Ok(Response::new(GetNetworkResponse {
            network: Some(network_to_proto(&network)),
        }))
    }

    async fn delete_network(
        &self,
        request: Request<DeleteNetworkRequest>,
    ) -> Result<Response<DeleteNetworkResponse>, Status> {
        let req = request.into_inner();

        self.state
            .delete_network(&req.id)
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(DeleteNetworkResponse {}))
    }

    async fn list_networks(
        &self,
        _request: Request<ListNetworksRequest>,
    ) -> Result<Response<ListNetworksResponse>, Status> {
        let networks = self.state.list_networks().map_err(|e| Status::from(e))?;

        Ok(Response::new(ListNetworksResponse {
            networks: networks
                .into_iter()
                .map(|n| network_to_proto(&n))
                .collect(),
        }))
    }

    // ========================================================================
    // QoS Profile operations
    // ========================================================================

    async fn create_qo_s_profile(
        &self,
        request: Request<CreateQoSProfileRequest>,
    ) -> Result<Response<CreateQoSProfileResponse>, Status> {
        let req = request.into_inner();
        let spec = req.spec.ok_or_else(|| Status::invalid_argument("spec required"))?;

        let qos_spec = types::QosProfileSpec {
            latency_ms: spec.latency_ms as u32,
            jitter_ms: spec.jitter_ms as u32,
            loss_percent: spec.loss_percent,
            rate_limit_mbps: spec.rate_limit_mbps as u32,
            packet_padding_bytes: spec.packet_padding_bytes as u32,
            burst_shaping: spec.burst_shaping,
            burst_size_kb: spec.burst_size_kb as u32,
        };

        let profile = self
            .state
            .create_qos_profile(req.name, qos_spec, req.labels)
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(CreateQoSProfileResponse {
            profile: Some(qos_profile_to_proto(&profile)),
        }))
    }

    async fn get_qo_s_profile(
        &self,
        request: Request<GetQoSProfileRequest>,
    ) -> Result<Response<GetQoSProfileResponse>, Status> {
        let req = request.into_inner();

        let profile = self
            .state
            .get_qos_profile(&req.id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("QoS profile not found"))?;

        Ok(Response::new(GetQoSProfileResponse {
            profile: Some(qos_profile_to_proto(&profile)),
        }))
    }

    async fn delete_qo_s_profile(
        &self,
        request: Request<DeleteQoSProfileRequest>,
    ) -> Result<Response<DeleteQoSProfileResponse>, Status> {
        let req = request.into_inner();

        self.state
            .delete_qos_profile(&req.id)
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(DeleteQoSProfileResponse {}))
    }

    async fn list_qo_s_profiles(
        &self,
        _request: Request<ListQoSProfilesRequest>,
    ) -> Result<Response<ListQoSProfilesResponse>, Status> {
        let profiles = self.state.list_qos_profiles().map_err(|e| Status::from(e))?;

        Ok(Response::new(ListQoSProfilesResponse {
            profiles: profiles
                .into_iter()
                .map(|p| qos_profile_to_proto(&p))
                .collect(),
        }))
    }

    // ========================================================================
    // Volume operations
    // ========================================================================

    async fn create_volume(
        &self,
        request: Request<CreateVolumeRequest>,
    ) -> Result<Response<CreateVolumeResponse>, Status> {
        let req = request.into_inner();
        let spec = req.spec.ok_or_else(|| Status::invalid_argument("spec required"))?;

        let vol_spec = types::VolumeSpec {
            kind: match ProtoVolumeKind::try_from(spec.kind) {
                Ok(ProtoVolumeKind::Disk) => VolumeKind::Disk,
                Ok(ProtoVolumeKind::Weights) => VolumeKind::Weights,
                _ => VolumeKind::Disk,
            },
            source: spec.source,
            integrity: spec.integrity.map(|i| types::IntegrityConfig {
                scheme: i.scheme,
                public_key: i.public_key,
                signature: i.signature,
                expected_digest: if i.expected_digest.is_empty() {
                    None
                } else {
                    Some(i.expected_digest)
                },
            }).unwrap_or_default(),
            read_only: spec.read_only,
            size_bytes: if spec.size_bytes > 0 {
                Some(spec.size_bytes as u64)
            } else {
                None
            },
            format: if spec.format.is_empty() {
                "qcow2".to_string()
            } else {
                spec.format
            },
            overlay: spec.overlay,
        };

        let volume = self
            .state
            .create_volume(req.name, vol_spec, req.labels)
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(CreateVolumeResponse {
            volume: Some(volume_to_proto(&volume)),
        }))
    }

    async fn get_volume(
        &self,
        request: Request<GetVolumeRequest>,
    ) -> Result<Response<GetVolumeResponse>, Status> {
        let req = request.into_inner();

        let volume = self
            .state
            .get_volume(&req.id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("Volume not found"))?;

        Ok(Response::new(GetVolumeResponse {
            volume: Some(volume_to_proto(&volume)),
        }))
    }

    async fn delete_volume(
        &self,
        request: Request<DeleteVolumeRequest>,
    ) -> Result<Response<DeleteVolumeResponse>, Status> {
        let req = request.into_inner();

        self.state
            .delete_volume(&req.id)
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(DeleteVolumeResponse {}))
    }

    async fn list_volumes(
        &self,
        _request: Request<ListVolumesRequest>,
    ) -> Result<Response<ListVolumesResponse>, Status> {
        let volumes = self.state.list_volumes().map_err(|e| Status::from(e))?;

        Ok(Response::new(ListVolumesResponse {
            volumes: volumes
                .into_iter()
                .map(|v| volume_to_proto(&v))
                .collect(),
        }))
    }

    // ========================================================================
    // Console operations
    // ========================================================================

    async fn create_console(
        &self,
        request: Request<CreateConsoleRequest>,
    ) -> Result<Response<CreateConsoleResponse>, Status> {
        let req = request.into_inner();
        let spec = req.spec.ok_or_else(|| Status::invalid_argument("spec required"))?;

        let console_spec = types::ConsoleSpec {
            vm_id: spec.vm_id,
            enable_vnc: spec.enable_vnc,
            vnc_port: if spec.vnc_port > 0 {
                Some(spec.vnc_port as u16)
            } else {
                None
            },
            enable_web: spec.enable_web,
            web_port: if spec.web_port > 0 {
                Some(spec.web_port as u16)
            } else {
                None
            },
            auth_token: if spec.auth_token.is_empty() {
                None
            } else {
                Some(spec.auth_token)
            },
        };

        let console = self
            .state
            .create_console(req.name, console_spec)
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(CreateConsoleResponse {
            console: Some(console_to_proto(&console)),
        }))
    }

    async fn get_console(
        &self,
        request: Request<GetConsoleRequest>,
    ) -> Result<Response<GetConsoleResponse>, Status> {
        let req = request.into_inner();

        let console = self
            .state
            .get_console(&req.id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("Console not found"))?;

        Ok(Response::new(GetConsoleResponse {
            console: Some(console_to_proto(&console)),
        }))
    }

    async fn delete_console(
        &self,
        request: Request<DeleteConsoleRequest>,
    ) -> Result<Response<DeleteConsoleResponse>, Status> {
        let req = request.into_inner();

        self.state
            .delete_console(&req.id)
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(DeleteConsoleResponse {}))
    }

    // ========================================================================
    // Snapshot operations
    // ========================================================================

    async fn create_snapshot(
        &self,
        request: Request<CreateSnapshotRequest>,
    ) -> Result<Response<CreateSnapshotResponse>, Status> {
        let req = request.into_inner();
        let spec = req.spec.ok_or_else(|| Status::invalid_argument("spec required"))?;

        let snap_spec = types::SnapshotSpec {
            vm_id: spec.vm_id.clone(),
            include_memory: spec.include_memory,
            include_disk: spec.include_disk,
            description: if spec.description.is_empty() {
                None
            } else {
                Some(spec.description)
            },
        };

        let snapshot = self
            .state
            .create_snapshot(req.name.clone(), snap_spec, req.labels)
            .map_err(|e| Status::from(e))?;

        // Actually create the snapshot
        if snapshot.spec.include_memory {
            let run_dir = self.state.cas().create_run(&snapshot.meta.id).await
                .map_err(|e| Status::from(e))?;
            let mem_path = run_dir.join("snapshot.mem");
            
            self.qemu
                .create_memory_snapshot(&self.state, &spec.vm_id, &mem_path)
                .await
                .map_err(|e| Status::from(e))?;

            // Update snapshot status
            let status = types::SnapshotStatus {
                complete: true,
                memory_snapshot_path: Some(mem_path.to_string_lossy().to_string()),
                ..snapshot.status.clone()
            };
            self.state
                .update_snapshot_status(&snapshot.meta.id, status)
                .map_err(|e| Status::from(e))?;
        }

        let snapshot = self
            .state
            .get_snapshot(&snapshot.meta.id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("Snapshot not found"))?;

        Ok(Response::new(CreateSnapshotResponse {
            snapshot: Some(snapshot_to_proto(&snapshot)),
        }))
    }

    async fn get_snapshot(
        &self,
        request: Request<GetSnapshotRequest>,
    ) -> Result<Response<GetSnapshotResponse>, Status> {
        let req = request.into_inner();

        let snapshot = self
            .state
            .get_snapshot(&req.id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("Snapshot not found"))?;

        Ok(Response::new(GetSnapshotResponse {
            snapshot: Some(snapshot_to_proto(&snapshot)),
        }))
    }

    async fn delete_snapshot(
        &self,
        request: Request<DeleteSnapshotRequest>,
    ) -> Result<Response<DeleteSnapshotResponse>, Status> {
        let req = request.into_inner();

        self.state
            .delete_snapshot(&req.id)
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(DeleteSnapshotResponse {}))
    }

    async fn list_snapshots(
        &self,
        request: Request<ListSnapshotsRequest>,
    ) -> Result<Response<ListSnapshotsResponse>, Status> {
        let req = request.into_inner();
        let vm_id = if req.vm_id.is_empty() {
            None
        } else {
            Some(req.vm_id.as_str())
        };

        let snapshots = self
            .state
            .list_snapshots(vm_id)
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(ListSnapshotsResponse {
            snapshots: snapshots
                .into_iter()
                .map(|s| snapshot_to_proto(&s))
                .collect(),
        }))
    }

    async fn restore_snapshot(
        &self,
        request: Request<RestoreSnapshotRequest>,
    ) -> Result<Response<RestoreSnapshotResponse>, Status> {
        let req = request.into_inner();

        let snapshot = self
            .state
            .get_snapshot(&req.snapshot_id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("Snapshot not found"))?;

        // Restore via QMP
        self.qemu
            .restore_internal_snapshot(&self.state, &req.target_vm_id, &snapshot.meta.name)
            .await
            .map_err(|e| Status::from(e))?;

        let vm = self
            .state
            .get_vm(&req.target_vm_id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("VM not found"))?;

        Ok(Response::new(RestoreSnapshotResponse {
            vm: Some(vm_to_proto(&vm)),
        }))
    }

    // ========================================================================
    // Benchmark operations
    // ========================================================================

    async fn create_benchmark_run(
        &self,
        _request: Request<CreateBenchmarkRunRequest>,
    ) -> Result<Response<CreateBenchmarkRunResponse>, Status> {
        Err(Status::unimplemented("Benchmark runs not yet implemented"))
    }

    async fn get_benchmark_run(
        &self,
        _request: Request<GetBenchmarkRunRequest>,
    ) -> Result<Response<GetBenchmarkRunResponse>, Status> {
        Err(Status::unimplemented("Benchmark runs not yet implemented"))
    }

    async fn list_benchmark_runs(
        &self,
        _request: Request<ListBenchmarkRunsRequest>,
    ) -> Result<Response<ListBenchmarkRunsResponse>, Status> {
        Err(Status::unimplemented("Benchmark runs not yet implemented"))
    }

    // ========================================================================
    // Attestation operations
    // ========================================================================

    async fn get_attestation(
        &self,
        request: Request<GetAttestationRequest>,
    ) -> Result<Response<GetAttestationResponse>, Status> {
        let req = request.into_inner();

        let vm = self
            .state
            .get_vm(&req.vm_id)
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found("VM not found"))?;

        let process = self
            .state
            .get_vm_process(&req.vm_id)
            .ok_or_else(|| Status::failed_precondition("VM not running"))?;

        // Collect volumes
        let volumes: Vec<types::Volume> = vm
            .spec
            .volume_ids
            .iter()
            .filter_map(|id| self.state.get_volume(id).ok().flatten())
            .collect();

        // Get QEMU args from the command line (we'd need to store these)
        let qemu_args = vec![format!("qemu-system-aarch64")];

        // Generate attestation
        let provider = AttestationProvider::new((*self.state.key_pair()).clone());
        let report = provider
            .generate_report(&vm, &volumes, &qemu_args)
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(GetAttestationResponse {
            report: Some(attestation_to_proto(&report)),
        }))
    }

    // ========================================================================
    // LoRa operations
    // ========================================================================

    async fn create_lo_ra_device(
        &self,
        _request: Request<CreateLoRaDeviceRequest>,
    ) -> Result<Response<CreateLoRaDeviceResponse>, Status> {
        Err(Status::unimplemented("LoRa devices not yet implemented"))
    }

    async fn get_lo_ra_device(
        &self,
        _request: Request<GetLoRaDeviceRequest>,
    ) -> Result<Response<GetLoRaDeviceResponse>, Status> {
        Err(Status::unimplemented("LoRa devices not yet implemented"))
    }

    async fn delete_lo_ra_device(
        &self,
        _request: Request<DeleteLoRaDeviceRequest>,
    ) -> Result<Response<DeleteLoRaDeviceResponse>, Status> {
        Err(Status::unimplemented("LoRa devices not yet implemented"))
    }

    // ========================================================================
    // Health operations
    // ========================================================================

    async fn get_health(
        &self,
        _request: Request<GetHealthRequest>,
    ) -> Result<Response<GetHealthResponse>, Status> {
        Ok(Response::new(GetHealthResponse {
            healthy: true,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: 0, // TODO: track uptime
        }))
    }

    async fn get_daemon_status(
        &self,
        _request: Request<GetDaemonStatusRequest>,
    ) -> Result<Response<GetDaemonStatusResponse>, Status> {
        let vms = self.state.list_vms().map_err(|e| Status::from(e))?;
        let running = vms.iter().filter(|v| matches!(v.status.state, types::VmState::Running)).count();

        let qemu_available = infrasim_common::attestation::is_qemu_available();
        let qemu_version = if qemu_available {
            std::process::Command::new("qemu-system-aarch64")
                .arg("--version")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .unwrap_or_default()
        } else {
            String::new()
        };

        Ok(Response::new(GetDaemonStatusResponse {
            running_vms: running as i32,
            total_vms: vms.len() as i32,
            memory_used_bytes: 0,
            disk_used_bytes: 0,
            store_path: self.config.store_path.to_string_lossy().to_string(),
            qemu_available,
            qemu_version,
            hvf_available: infrasim_common::attestation::is_hvf_available(),
        }))
    }
}

// ============================================================================
// Proto conversion helpers
// ============================================================================

fn resource_meta_to_proto(meta: &types::ResourceMeta) -> ResourceMeta {
    ResourceMeta {
        id: meta.id.clone(),
        name: meta.name.clone(),
        labels: meta.labels.clone(),
        annotations: meta.annotations.clone(),
        created_at: meta.created_at,
        updated_at: meta.updated_at,
        generation: meta.generation,
    }
}

fn vm_to_proto(vm: &types::Vm) -> Vm {
    Vm {
        meta: Some(resource_meta_to_proto(&vm.meta)),
        spec: Some(VmSpec {
            arch: vm.spec.arch.clone(),
            machine: vm.spec.machine.clone(),
            cpu_cores: vm.spec.cpu_cores as i32,
            memory_mb: vm.spec.memory_mb as i64,
            volume_ids: vm.spec.volume_ids.clone(),
            network_ids: vm.spec.network_ids.clone(),
            qos_profile_id: vm.spec.qos_profile_id.clone().unwrap_or_default(),
            enable_tpm: vm.spec.enable_tpm,
            boot_disk_id: vm.spec.boot_disk_id.clone().unwrap_or_default(),
            extra_args: vm.spec.extra_args.clone(),
            compatibility_mode: vm.spec.compatibility_mode,
        }),
        status: Some(VmStatus {
            state: match vm.status.state {
                types::VmState::Pending => ProtoVmState::Pending as i32,
                types::VmState::Running => ProtoVmState::Running as i32,
                types::VmState::Stopped => ProtoVmState::Stopped as i32,
                types::VmState::Paused => ProtoVmState::Paused as i32,
                types::VmState::Error => ProtoVmState::Error as i32,
            },
            qemu_pid: vm.status.qemu_pid.map(|p| p.to_string()).unwrap_or_default(),
            qmp_socket: vm.status.qmp_socket.clone().unwrap_or_default(),
            vnc_display: vm.status.vnc_display.clone().unwrap_or_default(),
            error_message: vm.status.error_message.clone().unwrap_or_default(),
            uptime_seconds: vm.status.uptime_seconds as i64,
        }),
    }
}

fn network_to_proto(net: &types::Network) -> Network {
    Network {
        meta: Some(resource_meta_to_proto(&net.meta)),
        spec: Some(NetworkSpec {
            mode: match net.spec.mode {
                NetworkMode::User => ProtoNetworkMode::User as i32,
                NetworkMode::VmnetShared => ProtoNetworkMode::VmnetShared as i32,
                NetworkMode::VmnetBridged => ProtoNetworkMode::VmnetBridged as i32,
            },
            cidr: net.spec.cidr.clone(),
            gateway: net.spec.gateway.clone().unwrap_or_default(),
            dns: net.spec.dns.clone().unwrap_or_default(),
            dhcp_enabled: net.spec.dhcp_enabled,
            mtu: net.spec.mtu as i32,
        }),
        status: Some(NetworkStatus {
            active: net.status.active,
            bridge_interface: net.status.bridge_interface.clone().unwrap_or_default(),
            connected_vms: net.status.connected_vms as i32,
        }),
    }
}

fn qos_profile_to_proto(profile: &types::QosProfile) -> QoSProfile {
    QoSProfile {
        meta: Some(resource_meta_to_proto(&profile.meta)),
        spec: Some(QoSProfileSpec {
            latency_ms: profile.spec.latency_ms as i32,
            jitter_ms: profile.spec.jitter_ms as i32,
            loss_percent: profile.spec.loss_percent,
            rate_limit_mbps: profile.spec.rate_limit_mbps as i32,
            packet_padding_bytes: profile.spec.packet_padding_bytes as i32,
            burst_shaping: profile.spec.burst_shaping,
            burst_size_kb: profile.spec.burst_size_kb as i32,
        }),
    }
}

fn volume_to_proto(vol: &types::Volume) -> Volume {
    Volume {
        meta: Some(resource_meta_to_proto(&vol.meta)),
        spec: Some(VolumeSpec {
            kind: match vol.spec.kind {
                VolumeKind::Disk => ProtoVolumeKind::Disk as i32,
                VolumeKind::Weights => ProtoVolumeKind::Weights as i32,
            },
            source: vol.spec.source.clone(),
            integrity: Some(IntegrityConfig {
                scheme: vol.spec.integrity.scheme.clone(),
                public_key: vol.spec.integrity.public_key.clone(),
                signature: vol.spec.integrity.signature.clone(),
                expected_digest: vol.spec.integrity.expected_digest.clone().unwrap_or_default(),
            }),
            read_only: vol.spec.read_only,
            size_bytes: vol.spec.size_bytes.unwrap_or(0) as i64,
            format: vol.spec.format.clone(),
            overlay: vol.spec.overlay,
        }),
        status: Some(crate::generated::VolumeStatus {
            ready: vol.status.ready,
            local_path: vol.status.local_path.clone().unwrap_or_default(),
            digest: vol.status.digest.clone().unwrap_or_default(),
            actual_size: vol.status.actual_size as i64,
            verified: vol.status.verified,
        }),
    }
}

fn console_to_proto(console: &types::Console) -> Console {
    Console {
        meta: Some(resource_meta_to_proto(&console.meta)),
        spec: Some(ConsoleSpec {
            vm_id: console.spec.vm_id.clone(),
            enable_vnc: console.spec.enable_vnc,
            vnc_port: console.spec.vnc_port.unwrap_or(0) as i32,
            enable_web: console.spec.enable_web,
            web_port: console.spec.web_port.unwrap_or(0) as i32,
            auth_token: console.spec.auth_token.clone().unwrap_or_default(),
        }),
        status: Some(ConsoleStatus {
            active: console.status.active,
            vnc_host: console.status.vnc_host.clone().unwrap_or_default(),
            vnc_port: console.status.vnc_port.unwrap_or(0) as i32,
            web_url: console.status.web_url.clone().unwrap_or_default(),
            connected_clients: console.status.connected_clients as i32,
        }),
    }
}

fn snapshot_to_proto(snap: &types::Snapshot) -> Snapshot {
    Snapshot {
        meta: Some(resource_meta_to_proto(&snap.meta)),
        spec: Some(SnapshotSpec {
            vm_id: snap.spec.vm_id.clone(),
            include_memory: snap.spec.include_memory,
            include_disk: snap.spec.include_disk,
            description: snap.spec.description.clone().unwrap_or_default(),
        }),
        status: Some(crate::generated::SnapshotStatus {
            complete: snap.status.complete,
            disk_snapshot_path: snap.status.disk_snapshot_path.clone().unwrap_or_default(),
            memory_snapshot_path: snap.status.memory_snapshot_path.clone().unwrap_or_default(),
            digest: snap.status.digest.clone().unwrap_or_default(),
            size_bytes: snap.status.size_bytes as i64,
            encrypted: snap.status.encrypted,
        }),
    }
}

fn attestation_to_proto(report: &types::AttestationReport) -> AttestationReport {
    AttestationReport {
        id: report.id.clone(),
        vm_id: report.vm_id.clone(),
        host_provenance: Some(HostProvenance {
            qemu_version: report.host_provenance.qemu_version.clone(),
            qemu_args: report.host_provenance.qemu_args.clone(),
            base_image_hash: report.host_provenance.base_image_hash.clone(),
            volume_hashes: report.host_provenance.volume_hashes.clone(),
            macos_version: report.host_provenance.macos_version.clone(),
            cpu_model: report.host_provenance.cpu_model.clone(),
            hvf_enabled: report.host_provenance.hvf_enabled,
            hostname: report.host_provenance.hostname.clone(),
            timestamp: report.host_provenance.timestamp,
        }),
        digest: report.digest.clone(),
        signature: report.signature.clone(),
        created_at: report.created_at,
        attestation_type: report.attestation_type.clone(),
    }
}

// ============================================================================
// Server startup
// ============================================================================

pub async fn serve(config: DaemonConfig, state: StateManager) -> anyhow::Result<()> {
    let addr = config.grpc_listen.parse()?;
    let service = DaemonService::new(state, config);

    info!("gRPC server listening on {}", addr);

    tonic::transport::Server::builder()
        .add_service(InfraSimDaemonServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
