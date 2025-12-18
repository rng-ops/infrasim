//! Daemon configuration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Store directory path
    pub store_path: PathBuf,

    /// gRPC listen address
    pub grpc_listen: String,

    /// Web console port
    pub web_port: u16,

    /// QEMU configuration
    pub qemu: QemuConfig,

    /// Network configuration
    pub network: NetworkConfig,

    /// Security configuration
    pub security: SecurityConfig,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            store_path: infrasim_common::default_store_path(),
            grpc_listen: "127.0.0.1:9090".to_string(),
            web_port: 6080,
            qemu: QemuConfig::default(),
            network: NetworkConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

/// QEMU-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QemuConfig {
    /// Path to qemu-system-aarch64 binary
    pub binary_path: Option<String>,

    /// Default accelerator
    pub accelerator: String,

    /// Default machine type
    pub machine_type: String,

    /// Default CPU type
    pub cpu_type: String,

    /// Enable HVF (Hypervisor.framework) on macOS
    pub enable_hvf: bool,

    /// VNC base port
    pub vnc_base_port: u16,

    /// QMP socket directory
    pub qmp_socket_dir: Option<PathBuf>,
}

impl Default for QemuConfig {
    fn default() -> Self {
        Self {
            binary_path: None, // Will auto-detect
            accelerator: "hvf".to_string(),
            machine_type: "virt,highmem=on".to_string(),
            cpu_type: "host".to_string(),
            enable_hvf: true,
            vnc_base_port: 5900,
            qmp_socket_dir: None,
        }
    }
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Default network mode
    pub default_mode: String,

    /// Default CIDR for user-mode networking
    pub default_cidr: String,

    /// Enable vmnet (requires entitlement on macOS)
    pub enable_vmnet: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            default_mode: "user".to_string(),
            default_cidr: "10.42.0.0/24".to_string(),
            enable_vmnet: false,
        }
    }
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Path to signing key
    pub signing_key_path: Option<PathBuf>,

    /// Encrypt memory snapshots at rest
    pub encrypt_snapshots: bool,

    /// Enable attestation
    pub enable_attestation: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            signing_key_path: None,
            encrypt_snapshots: true,
            enable_attestation: true,
        }
    }
}

impl DaemonConfig {
    /// Load configuration from file
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let config: Self = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// Save configuration to file
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get the database path
    pub fn db_path(&self) -> PathBuf {
        self.store_path.join("state.db")
    }

    /// Get the CAS path
    pub fn cas_path(&self) -> PathBuf {
        self.store_path.join("store")
    }

    /// Get the QMP socket directory
    pub fn qmp_socket_dir(&self) -> PathBuf {
        self.qemu.qmp_socket_dir.clone()
            .unwrap_or_else(|| self.store_path.join("sockets"))
    }

    /// Get the signing key path
    pub fn signing_key_path(&self) -> PathBuf {
        self.security.signing_key_path.clone()
            .unwrap_or_else(|| self.store_path.join("signing.key"))
    }
}
