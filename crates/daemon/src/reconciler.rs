//! Reconciliation loop
//!
//! Continuously monitors and reconciles desired state with actual state.

use crate::qemu::{QemuLauncher, VolumePreparer};
use crate::state::StateManager;
use infrasim_common::types::*;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Reconciler that ensures actual state matches desired state
pub struct Reconciler {
    state: StateManager,
    qemu: QemuLauncher,
    volume_preparer: VolumePreparer,
}

impl Reconciler {
    /// Create a new reconciler
    pub fn new(state: StateManager) -> Self {
        let config = state.config().clone();
        Self {
            qemu: QemuLauncher::new(config.clone()),
            volume_preparer: VolumePreparer::new(config),
            state,
        }
    }

    /// Run the reconciliation loop
    pub async fn run(&self) {
        info!("Reconciler started");

        loop {
            if let Err(e) = self.reconcile_all().await {
                error!("Reconciliation error: {}", e);
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    /// Reconcile all resources
    async fn reconcile_all(&self) -> infrasim_common::Result<()> {
        self.reconcile_volumes().await?;
        self.reconcile_vms().await?;
        self.reconcile_consoles().await?;
        self.cleanup_orphans().await?;
        Ok(())
    }

    /// Reconcile volumes
    async fn reconcile_volumes(&self) -> infrasim_common::Result<()> {
        let volumes = self.state.list_volumes()?;

        for volume in volumes {
            if !volume.status.ready {
                debug!("Preparing volume: {}", volume.meta.name);
                match self.volume_preparer.prepare(&self.state, &volume).await {
                    Ok(_) => {
                        info!("Volume ready: {}", volume.meta.name);
                    }
                    Err(e) => {
                        warn!("Failed to prepare volume {}: {}", volume.meta.name, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Reconcile VMs
    async fn reconcile_vms(&self) -> infrasim_common::Result<()> {
        let vms = self.state.list_vms()?;

        for vm in vms {
            match self.reconcile_vm(&vm).await {
                Ok(_) => {}
                Err(e) => {
                    warn!("Failed to reconcile VM {}: {}", vm.meta.name, e);
                    
                    // Update status with error
                    let status = VmStatus {
                        state: VmState::Error,
                        error_message: Some(e.to_string()),
                        ..vm.status.clone()
                    };
                    let _ = self.state.update_vm_status(&vm.meta.id, status);
                }
            }
        }

        Ok(())
    }

    /// Reconcile a single VM
    async fn reconcile_vm(&self, vm: &Vm) -> infrasim_common::Result<()> {
        let process = self.state.get_vm_process(&vm.meta.id);
        let is_running = process.as_ref().map_or(false, |p| {
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(p.pid as i32), None).is_ok()
        });

        match (&vm.status.state, is_running) {
            // Should be running but isn't
            (VmState::Running, false) => {
                // Check if all volumes are ready
                let volumes_ready = self.check_volumes_ready(vm)?;
                if !volumes_ready {
                    debug!("Waiting for volumes for VM: {}", vm.meta.name);
                    return Ok(());
                }

                // Start the VM
                info!("Starting VM: {}", vm.meta.name);
                self.qemu.start(&self.state, vm).await?;
            }

            // Is running but shouldn't be
            (VmState::Stopped, true) => {
                warn!("VM {} should be stopped but is running", vm.meta.name);
                self.qemu.stop(&self.state, &vm.meta.id, false).await?;
            }

            // Running and should be - update uptime
            (VmState::Running, true) if process.is_some() => {
                let process = process.unwrap();
                let uptime = (chrono::Utc::now().timestamp() - process.started_at) as u64;
                
                let status = VmStatus {
                    state: VmState::Running,
                    qemu_pid: Some(process.pid),
                    qmp_socket: Some(process.qmp_socket.clone()),
                    vnc_display: process.vnc_port.map(|p| format!(":{}", p - 5900)),
                    error_message: None,
                    uptime_seconds: uptime,
                };
                self.state.update_vm_status(&vm.meta.id, status)?;
            }

            // Pending state - try to start if possible
            (VmState::Pending, false) => {
                let volumes_ready = self.check_volumes_ready(vm)?;
                if volumes_ready {
                    info!("Starting pending VM: {}", vm.meta.name);
                    
                    // Mark as running to trigger start
                    let status = VmStatus {
                        state: VmState::Running,
                        ..vm.status.clone()
                    };
                    self.state.update_vm_status(&vm.meta.id, status)?;
                }
            }

            // Other states - no action needed
            _ => {}
        }

        Ok(())
    }

    /// Check if all volumes for a VM are ready
    fn check_volumes_ready(&self, vm: &Vm) -> infrasim_common::Result<bool> {
        // Check boot disk
        if let Some(boot_id) = &vm.spec.boot_disk_id {
            if let Some(vol) = self.state.get_volume(boot_id)? {
                if !vol.status.ready {
                    return Ok(false);
                }
            } else {
                return Ok(false);
            }
        }

        // Check other volumes
        for vol_id in &vm.spec.volume_ids {
            if let Some(vol) = self.state.get_volume(vol_id)? {
                if !vol.status.ready {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Reconcile consoles
    async fn reconcile_consoles(&self) -> infrasim_common::Result<()> {
        // Console status is managed by the web server
        // This is a placeholder for future console-specific reconciliation
        Ok(())
    }

    /// Clean up orphaned processes
    async fn cleanup_orphans(&self) -> infrasim_common::Result<()> {
        let processes = self.state.list_vm_processes();

        for process in processes {
            // Check if VM still exists
            if self.state.get_vm(&process.vm_id)?.is_none() {
                warn!("Cleaning up orphan process for deleted VM: {}", process.vm_id);
                self.qemu.stop(&self.state, &process.vm_id, true).await?;
            }
        }

        Ok(())
    }
}

/// Drift detector for detecting configuration drift
pub struct DriftDetector {
    state: StateManager,
}

impl DriftDetector {
    pub fn new(state: StateManager) -> Self {
        Self { state }
    }

    /// Detect drift for all VMs
    pub async fn detect_all(&self) -> infrasim_common::Result<Vec<DriftReport>> {
        let mut reports = Vec::new();

        for vm in self.state.list_vms()? {
            if let Some(drift) = self.detect_vm_drift(&vm).await? {
                reports.push(drift);
            }
        }

        Ok(reports)
    }

    /// Detect drift for a single VM
    async fn detect_vm_drift(&self, vm: &Vm) -> infrasim_common::Result<Option<DriftReport>> {
        let process = self.state.get_vm_process(&vm.meta.id);

        // Check if process state matches desired state
        let is_running = process.as_ref().map_or(false, |p| {
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(p.pid as i32), None).is_ok()
        });

        let should_be_running = matches!(vm.status.state, VmState::Running);

        if is_running != should_be_running {
            return Ok(Some(DriftReport {
                resource_type: "vm".to_string(),
                resource_id: vm.meta.id.clone(),
                resource_name: vm.meta.name.clone(),
                drift_type: if is_running {
                    DriftType::UnexpectedRunning
                } else {
                    DriftType::UnexpectedStopped
                },
                message: format!(
                    "VM is {} but should be {}",
                    if is_running { "running" } else { "stopped" },
                    if should_be_running { "running" } else { "stopped" }
                ),
            }));
        }

        Ok(None)
    }
}

/// Drift report
#[derive(Debug, Clone)]
pub struct DriftReport {
    pub resource_type: String,
    pub resource_id: String,
    pub resource_name: String,
    pub drift_type: DriftType,
    pub message: String,
}

/// Types of drift
#[derive(Debug, Clone)]
pub enum DriftType {
    UnexpectedRunning,
    UnexpectedStopped,
    ConfigMismatch,
    ResourceMissing,
}
