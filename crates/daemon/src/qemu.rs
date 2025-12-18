//! QEMU process management
//!
//! Handles launching and managing QEMU processes.

use crate::config::DaemonConfig;
use crate::state::{StateManager, VmProcess};
use infrasim_common::{
    attestation::is_hvf_available,
    qmp::{wait_for_qmp, QmpClient},
    types::*,
    Error, Result,
};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tokio::fs;
use tracing::{debug, error, info, warn};

/// QEMU launcher for managing VM lifecycles
pub struct QemuLauncher {
    config: DaemonConfig,
}

impl QemuLauncher {
    /// Create a new QEMU launcher
    pub fn new(config: DaemonConfig) -> Self {
        Self { config }
    }

    /// Get the QEMU binary path
    fn qemu_path(&self) -> String {
        self.config
            .qemu
            .binary_path
            .clone()
            .unwrap_or_else(|| "qemu-system-aarch64".to_string())
    }

    /// Build QEMU command line arguments
    pub fn build_args(
        &self,
        vm: &Vm,
        volumes: &[Volume],
        networks: &[Network],
        qmp_socket: &Path,
        vnc_display: u16,
    ) -> Vec<String> {
        let mut args = Vec::new();

        // Machine type
        let machine = if vm.spec.compatibility_mode {
            // Raspberry Pi 3B emulation (slow but compatible)
            warn!("Using compatibility mode (raspi3b) - this is significantly slower");
            "raspi3b".to_string()
        } else {
            // Fast virt machine (default)
            self.config.qemu.machine_type.clone()
        };
        args.extend(["-machine".to_string(), machine]);

        // Accelerator (HVF on macOS)
        if !vm.spec.compatibility_mode && is_hvf_available() && self.config.qemu.enable_hvf {
            args.extend(["-accel".to_string(), "hvf".to_string()]);
        } else if vm.spec.compatibility_mode {
            // TCG for compatibility mode
            args.extend(["-accel".to_string(), "tcg".to_string()]);
        }

        // CPU
        let cpu = if vm.spec.compatibility_mode {
            "cortex-a53".to_string()
        } else {
            self.config.qemu.cpu_type.clone()
        };
        args.extend(["-cpu".to_string(), cpu]);

        // SMP
        args.extend(["-smp".to_string(), vm.spec.cpu_cores.to_string()]);

        // Memory
        args.extend(["-m".to_string(), format!("{}M", vm.spec.memory_mb)]);

        // QMP socket
        args.extend([
            "-qmp".to_string(),
            format!("unix:{},server,nowait", qmp_socket.display()),
        ]);

        // VNC display
        args.extend(["-vnc".to_string(), format!(":{}", vnc_display)]);

        // Headless by default
        args.push("-nographic".to_string());

        // Boot disk
        if let Some(boot_disk_id) = &vm.spec.boot_disk_id {
            if let Some(vol) = volumes.iter().find(|v| v.meta.id == *boot_disk_id) {
                if let Some(path) = &vol.status.local_path {
                    args.extend([
                        "-drive".to_string(),
                        format!(
                            "file={},format={},if=virtio,id=boot",
                            path,
                            vol.spec.format
                        ),
                    ]);
                }
            }
        }

        // Additional volumes
        for (idx, vol) in volumes.iter().enumerate() {
            if Some(&vol.meta.id) == vm.spec.boot_disk_id.as_ref() {
                continue; // Skip boot disk
            }
            if let Some(path) = &vol.status.local_path {
                let read_only = if vol.spec.read_only { ",readonly=on" } else { "" };
                args.extend([
                    "-drive".to_string(),
                    format!(
                        "file={},format={},if=virtio,id=disk{}{}",
                        path,
                        vol.spec.format,
                        idx,
                        read_only
                    ),
                ]);
            }
        }

        // Network interfaces
        for (idx, _net) in networks.iter().enumerate() {
            // User-mode networking (default, works without privileges)
            args.extend([
                "-netdev".to_string(),
                format!("user,id=net{},hostfwd=tcp::222{}-:22", idx, idx),
                "-device".to_string(),
                format!("virtio-net-pci,netdev=net{}", idx),
            ]);
        }

        // Default network if none specified
        if networks.is_empty() {
            args.extend([
                "-netdev".to_string(),
                "user,id=net0,hostfwd=tcp::2222-:22".to_string(),
                "-device".to_string(),
                "virtio-net-pci,netdev=net0".to_string(),
            ]);
        }

        // virtio-rng for entropy
        args.extend(["-device".to_string(), "virtio-rng-pci".to_string()]);

        // TPM (scaffold - requires swtpm)
        if vm.spec.enable_tpm {
            warn!("TPM support requires swtpm - scaffold only");
            // Would add:
            // -chardev socket,id=chrtpm,path=/tmp/swtpm.sock
            // -tpmdev emulator,id=tpm0,chardev=chrtpm
            // -device tpm-tis,tpmdev=tpm0
        }

        // Extra args from spec
        for (key, value) in &vm.spec.extra_args {
            args.push(format!("-{}", key));
            if !value.is_empty() {
                args.push(value.clone());
            }
        }

        args
    }

    /// Start a VM
    pub async fn start(
        &self,
        state: &StateManager,
        vm: &Vm,
    ) -> Result<VmProcess> {
        info!("Starting VM: {} ({})", vm.meta.name, vm.meta.id);

        // Gather volumes
        let volumes: Vec<Volume> = vm
            .spec
            .volume_ids
            .iter()
            .filter_map(|id| state.get_volume(id).ok().flatten())
            .collect();

        // Add boot disk if not in volume_ids
        if let Some(boot_id) = &vm.spec.boot_disk_id {
            if !vm.spec.volume_ids.contains(boot_id) {
                if let Some(vol) = state.get_volume(boot_id)? {
                    let mut vols = volumes.clone();
                    vols.push(vol);
                }
            }
        }

        // Gather networks
        let networks: Vec<Network> = vm
            .spec
            .network_ids
            .iter()
            .filter_map(|id| state.get_network(id).ok().flatten())
            .collect();

        // Prepare QMP socket path
        let socket_dir = state.config().qmp_socket_dir();
        fs::create_dir_all(&socket_dir).await?;
        let qmp_socket = socket_dir.join(format!("{}.qmp", vm.meta.id));

        // Clean up old socket if exists
        if qmp_socket.exists() {
            fs::remove_file(&qmp_socket).await?;
        }

        // Allocate VNC display (simple increment)
        let vnc_display = self.allocate_vnc_display(state)?;

        // Build command
        let args = self.build_args(vm, &volumes, &networks, &qmp_socket, vnc_display);

        debug!("QEMU command: {} {}", self.qemu_path(), args.join(" "));

        // Spawn QEMU process
        let child = Command::new(self.qemu_path())
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Qemu(format!("Failed to spawn QEMU: {}", e)))?;

        let pid = child.id();
        info!("QEMU started with PID {}", pid);

        // Wait for QMP socket
        let qmp = wait_for_qmp(&qmp_socket, 30).await?;
        
        // Query version to confirm it's working
        let version = qmp.query_version().await?;
        info!("Connected to QEMU {}", version);

        let process = VmProcess {
            vm_id: vm.meta.id.clone(),
            pid,
            qmp_socket: qmp_socket.to_string_lossy().to_string(),
            vnc_port: Some(self.config.qemu.vnc_base_port + vnc_display),
            started_at: chrono::Utc::now().timestamp(),
        };

        // Update VM status
        let status = VmStatus {
            state: VmState::Running,
            qemu_pid: Some(pid),
            qmp_socket: Some(process.qmp_socket.clone()),
            vnc_display: Some(format!(":{}", vnc_display)),
            error_message: None,
            uptime_seconds: 0,
        };
        state.update_vm_status(&vm.meta.id, status)?;
        state.register_vm_process(process.clone());

        Ok(process)
    }

    /// Stop a VM
    pub async fn stop(&self, state: &StateManager, vm_id: &str, force: bool) -> Result<()> {
        info!("Stopping VM: {}", vm_id);

        if let Some(process) = state.get_vm_process(vm_id) {
            // Try graceful shutdown via QMP
            if !force {
                let qmp = QmpClient::new(&process.qmp_socket);
                if qmp.connect().await.is_ok() {
                    if let Err(e) = qmp.system_powerdown().await {
                        warn!("Graceful shutdown failed: {}", e);
                    } else {
                        // Wait for graceful shutdown
                        for _ in 0..30 {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            if !self.is_process_running(process.pid) {
                                break;
                            }
                        }
                    }
                }
            }

            // Force kill if still running
            if self.is_process_running(process.pid) {
                info!("Force killing QEMU process {}", process.pid);
                let _ = kill(Pid::from_raw(process.pid as i32), Signal::SIGKILL);
            }

            // Clean up
            state.remove_vm_process(vm_id);

            // Clean up QMP socket
            let socket_path = PathBuf::from(&process.qmp_socket);
            if socket_path.exists() {
                let _ = fs::remove_file(&socket_path).await;
            }
        }

        // Update status
        let status = VmStatus {
            state: VmState::Stopped,
            qemu_pid: None,
            qmp_socket: None,
            vnc_display: None,
            error_message: None,
            uptime_seconds: 0,
        };
        state.update_vm_status(vm_id, status)?;

        Ok(())
    }

    /// Check if a process is running
    fn is_process_running(&self, pid: u32) -> bool {
        kill(Pid::from_raw(pid as i32), None).is_ok()
    }

    /// Allocate a VNC display number
    fn allocate_vnc_display(&self, state: &StateManager) -> Result<u16> {
        let used: std::collections::HashSet<u16> = state
            .list_vm_processes()
            .iter()
            .filter_map(|p| p.vnc_port.map(|port| port - self.config.qemu.vnc_base_port))
            .collect();

        for display in 0..100 {
            if !used.contains(&display) {
                return Ok(display);
            }
        }

        Err(Error::Qemu("No available VNC displays".to_string()))
    }

    /// Create memory snapshot
    pub async fn create_memory_snapshot(
        &self,
        state: &StateManager,
        vm_id: &str,
        snapshot_path: &Path,
    ) -> Result<()> {
        let process = state
            .get_vm_process(vm_id)
            .ok_or_else(|| Error::Qemu("VM not running".to_string()))?;

        let qmp = QmpClient::new(&process.qmp_socket);
        qmp.connect().await?;

        // Pause VM
        qmp.stop().await?;

        // Dump memory
        qmp.dump_guest_memory(snapshot_path.to_string_lossy().as_ref(), true)
            .await?;

        // Resume VM
        qmp.cont().await?;

        info!("Memory snapshot saved to {:?}", snapshot_path);
        Ok(())
    }

    /// Create internal snapshot
    pub async fn create_internal_snapshot(
        &self,
        state: &StateManager,
        vm_id: &str,
        name: &str,
    ) -> Result<()> {
        let process = state
            .get_vm_process(vm_id)
            .ok_or_else(|| Error::Qemu("VM not running".to_string()))?;

        let qmp = QmpClient::new(&process.qmp_socket);
        qmp.connect().await?;

        qmp.savevm(name).await?;

        info!("Internal snapshot '{}' created", name);
        Ok(())
    }

    /// Restore internal snapshot
    pub async fn restore_internal_snapshot(
        &self,
        state: &StateManager,
        vm_id: &str,
        name: &str,
    ) -> Result<()> {
        let process = state
            .get_vm_process(vm_id)
            .ok_or_else(|| Error::Qemu("VM not running".to_string()))?;

        let qmp = QmpClient::new(&process.qmp_socket);
        qmp.connect().await?;

        qmp.loadvm(name).await?;

        info!("Restored snapshot '{}'", name);
        Ok(())
    }

    /// Get VM status via QMP
    pub async fn query_status(&self, state: &StateManager, vm_id: &str) -> Result<VmState> {
        let process = state
            .get_vm_process(vm_id)
            .ok_or_else(|| Error::Qemu("VM not running".to_string()))?;

        let qmp = QmpClient::new(&process.qmp_socket);
        qmp.connect().await?;

        let status = qmp.query_status().await?;

        Ok(if status.running {
            VmState::Running
        } else {
            VmState::Paused
        })
    }

    /// Get VNC info
    pub async fn get_vnc_info(
        &self,
        state: &StateManager,
        vm_id: &str,
    ) -> Result<(String, u16)> {
        let process = state
            .get_vm_process(vm_id)
            .ok_or_else(|| Error::Qemu("VM not running".to_string()))?;

        let qmp = QmpClient::new(&process.qmp_socket);
        qmp.connect().await?;

        let vnc = qmp.query_vnc().await?;

        let host = vnc.host.clone().unwrap_or_else(|| "127.0.0.1".to_string());
        let port = vnc.port().unwrap_or(5900);

        Ok((host, port))
    }
}

/// Volume preparer - handles volume setup
pub struct VolumePreparer {
    config: DaemonConfig,
}

impl VolumePreparer {
    pub fn new(config: DaemonConfig) -> Self {
        Self { config }
    }

    /// Prepare a volume for use
    pub async fn prepare(&self, state: &StateManager, volume: &Volume) -> Result<PathBuf> {
        let vol_dir = self.config.store_path.join("volumes").join(&volume.meta.id);
        fs::create_dir_all(&vol_dir).await?;

        let local_path = if volume.spec.source.starts_with("oci://") {
            // OCI registry pull (stub)
            self.pull_oci(&volume.spec.source, &vol_dir).await?
        } else if volume.spec.source.starts_with("http://") || volume.spec.source.starts_with("https://") {
            // HTTP download
            self.download_http(&volume.spec.source, &vol_dir).await?
        } else {
            // Local file
            let src = PathBuf::from(&volume.spec.source);
            if !src.exists() {
                return Err(Error::VolumeError(format!(
                    "Source file not found: {}",
                    volume.spec.source
                )));
            }

            // If overlay requested, create qcow2 backing
            if volume.spec.overlay {
                self.create_overlay(&src, &vol_dir).await?
            } else {
                // Just use source directly or copy
                src
            }
        };

        // Verify integrity if configured
        if !volume.spec.integrity.scheme.is_empty() {
            self.verify_integrity(&local_path, &volume.spec.integrity).await?;
        }

        // Compute digest
        let digest = infrasim_common::ContentAddressedStore::hash_file(&local_path).await?;

        // Update status
        let status = VolumeStatus {
            ready: true,
            local_path: Some(local_path.to_string_lossy().to_string()),
            digest: Some(digest),
            actual_size: fs::metadata(&local_path).await?.len(),
            verified: !volume.spec.integrity.scheme.is_empty(),
        };
        state.update_volume_status(&volume.meta.id, status)?;

        Ok(local_path)
    }

    /// Pull from OCI registry (stub)
    async fn pull_oci(&self, _reference: &str, _dest: &Path) -> Result<PathBuf> {
        Err(Error::VolumeError(
            "OCI registry pull not implemented".to_string(),
        ))
    }

    /// Download from HTTP
    async fn download_http(&self, _url: &str, _dest: &Path) -> Result<PathBuf> {
        Err(Error::VolumeError(
            "HTTP download not implemented - use local files".to_string(),
        ))
    }

    /// Create qcow2 overlay
    async fn create_overlay(&self, backing: &Path, dest_dir: &Path) -> Result<PathBuf> {
        let overlay_path = dest_dir.join("overlay.qcow2");

        let output = Command::new("qemu-img")
            .args([
                "create",
                "-f",
                "qcow2",
                "-b",
                backing.to_string_lossy().as_ref(),
                "-F",
                "qcow2",
                overlay_path.to_string_lossy().as_ref(),
            ])
            .output()
            .map_err(|e| Error::VolumeError(format!("qemu-img failed: {}", e)))?;

        if !output.status.success() {
            return Err(Error::VolumeError(format!(
                "qemu-img failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(overlay_path)
    }

    /// Verify volume integrity
    async fn verify_integrity(&self, path: &Path, config: &IntegrityConfig) -> Result<()> {
        match config.scheme.as_str() {
            "sha256" => {
                let actual = infrasim_common::ContentAddressedStore::hash_file(path).await?;
                if let Some(expected) = &config.expected_digest {
                    if actual != *expected {
                        return Err(Error::IntegrityError(format!(
                            "Digest mismatch: expected {}, got {}",
                            expected, actual
                        )));
                    }
                }
            }
            "signed_manifest" => {
                use infrasim_common::crypto::{verifying_key_from_bytes, Verifier};

                if config.public_key.is_empty() {
                    return Err(Error::IntegrityError("Missing public key".to_string()));
                }

                let verifying_key = verifying_key_from_bytes(&config.public_key)?;
                let actual = infrasim_common::ContentAddressedStore::hash_file(path).await?;

                verifying_key.verify(actual.as_bytes(), &config.signature)?;
            }
            "" => {
                // No verification
            }
            other => {
                return Err(Error::IntegrityError(format!(
                    "Unknown integrity scheme: {}",
                    other
                )));
            }
        }

        Ok(())
    }
}
