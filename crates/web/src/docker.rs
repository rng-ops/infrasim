//! Docker/Container image management module.
//!
//! Provides APIs for:
//! - Listing local and remote container images
//! - Pulling images from registries
//! - Building custom appliance images with overlays
//! - Converting container images to qcow2 VM images
//! - Defining network interfaces for appliances

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use tokio::process::Command as AsyncCommand;

/// Container runtime detection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerRuntime {
    Docker,
    Podman,
}

impl ContainerRuntime {
    /// Detect available container runtime
    pub fn detect() -> Option<Self> {
        // Check podman first (rootless friendly)
        if Command::new("podman").arg("--version").output().is_ok() {
            return Some(Self::Podman);
        }
        // Then docker
        if Command::new("docker").arg("--version").output().is_ok() {
            return Some(Self::Docker);
        }
        None
    }

    /// Get the CLI command name
    pub fn command(&self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }
}

/// A container image from local or registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerImage {
    pub id: String,
    pub repository: String,
    pub tag: String,
    pub digest: Option<String>,
    pub created: String,
    pub size: String,
    pub size_bytes: i64,
    pub labels: HashMap<String, String>,
    /// Whether this is a local image
    pub local: bool,
    /// Image architecture
    pub arch: Option<String>,
    /// Image OS
    pub os: Option<String>,
}

/// Registry search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySearchResult {
    pub name: String,
    pub description: String,
    pub stars: i64,
    pub official: bool,
    pub automated: bool,
}

/// Network interface definition for an appliance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub nic_type: NetworkInterfaceType,
    /// MAC address (generated if not specified)
    pub mac_address: Option<String>,
    /// IP address (for static config)
    pub ip_address: Option<String>,
    /// Gateway (for static config)
    pub gateway: Option<String>,
    /// VLAN ID (optional)
    pub vlan_id: Option<u16>,
    /// MTU (default 1500)
    pub mtu: u32,
    /// Bridge to connect to (for bridged type)
    pub bridge: Option<String>,
    /// Enable promiscuous mode
    pub promiscuous: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NetworkInterfaceType {
    /// NAT/User networking (SLIRP)
    User,
    /// Bridged to host interface
    Bridge,
    /// macOS vmnet shared
    VmnetShared,
    /// macOS vmnet bridged
    VmnetBridged,
    /// Passthrough to physical device
    Passthrough,
}

impl Default for NetworkInterfaceType {
    fn default() -> Self {
        Self::User
    }
}

impl Default for NetworkInterface {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "eth0".to_string(),
            nic_type: NetworkInterfaceType::User,
            mac_address: None,
            ip_address: None,
            gateway: None,
            vlan_id: None,
            mtu: 1500,
            bridge: None,
            promiscuous: false,
        }
    }
}

/// Overlay definition for building custom images
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageOverlay {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub overlay_type: OverlayType,
    /// For files/directories
    pub source_path: Option<String>,
    /// Destination in image
    pub dest_path: Option<String>,
    /// For shell commands
    pub commands: Vec<String>,
    /// For packages
    pub packages: Vec<String>,
    /// For environment variables
    pub env_vars: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OverlayType {
    /// Copy files/directories into the image
    Files,
    /// Run shell commands
    Shell,
    /// Install packages (apt, apk, yum)
    Packages,
    /// Set environment variables
    Environment,
    /// Cloud-init user-data
    CloudInit,
}

/// Appliance build spec - combines base image, overlays, and network config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplianceBuildSpec {
    pub name: String,
    pub description: Option<String>,
    /// Base container image (e.g., "alpine:3.19")
    pub base_image: String,
    /// Target architecture (aarch64, x86_64)
    pub arch: String,
    /// Memory in MB
    pub memory_mb: i64,
    /// CPU cores
    pub cpu_cores: i32,
    /// Network interfaces
    pub interfaces: Vec<NetworkInterface>,
    /// Image overlays (applied in order)
    pub overlays: Vec<ImageOverlay>,
    /// Output format
    pub output_format: OutputFormat,
    /// Cloud-init user-data (optional)
    pub cloud_init: Option<CloudInitConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// QCOW2 disk image
    Qcow2,
    /// Raw disk image
    Raw,
    /// OCI/Docker container image
    Container,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Qcow2
    }
}

/// Cloud-init configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInitConfig {
    /// User-data (shell script or cloud-config YAML)
    pub user_data: String,
    /// Meta-data (instance metadata)
    pub meta_data: Option<String>,
    /// Network config (optional, v2 format)
    pub network_config: Option<String>,
}

/// Build result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplianceBuildResult {
    pub id: String,
    pub name: String,
    pub status: BuildStatus,
    pub output_path: Option<String>,
    pub output_size_bytes: i64,
    pub output_digest: Option<String>,
    pub build_log: Vec<String>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BuildStatus {
    Pending,
    Pulling,
    Building,
    Converting,
    Complete,
    Failed,
}

/// Docker/Container manager
pub struct ContainerManager {
    pub runtime: Option<ContainerRuntime>,
}

impl ContainerManager {
    pub fn new() -> Self {
        Self {
            runtime: ContainerRuntime::detect(),
        }
    }

    /// List local images
    pub async fn list_local_images(&self) -> Result<Vec<ContainerImage>, String> {
        let runtime = self.runtime.ok_or("No container runtime available")?;
        let cmd = runtime.command();

        let output = AsyncCommand::new(cmd)
            .args(["images", "--format", "{{json .}}"])
            .output()
            .await
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut images = Vec::new();

        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }
            // Parse JSON line
            if let Ok(img) = serde_json::from_str::<DockerImageJson>(line) {
                images.push(ContainerImage {
                    id: img.ID.unwrap_or_default(),
                    repository: img.Repository.unwrap_or_else(|| "<none>".to_string()),
                    tag: img.Tag.unwrap_or_else(|| "<none>".to_string()),
                    digest: img.Digest,
                    created: img.CreatedSince.unwrap_or_default(),
                    size: img.Size.clone().unwrap_or_default(),
                    size_bytes: parse_size(&img.Size.unwrap_or_default()),
                    labels: HashMap::new(),
                    local: true,
                    arch: None,
                    os: None,
                });
            }
        }

        Ok(images)
    }

    /// Search registry for images
    pub async fn search_registry(&self, query: &str) -> Result<Vec<RegistrySearchResult>, String> {
        let runtime = self.runtime.ok_or("No container runtime available")?;
        let cmd = runtime.command();

        let output = AsyncCommand::new(cmd)
            .args(["search", "--format", "{{json .}}", query])
            .output()
            .await
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();

        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(r) = serde_json::from_str::<DockerSearchJson>(line) {
                results.push(RegistrySearchResult {
                    name: r.Name.unwrap_or_default(),
                    description: r.Description.unwrap_or_default(),
                    stars: r.StarCount.unwrap_or(0),
                    official: r.IsOfficial.unwrap_or(false),
                    automated: r.IsAutomated.unwrap_or(false),
                });
            }
        }

        Ok(results)
    }

    /// Inspect an image
    pub async fn inspect_image(&self, image_ref: &str) -> Result<serde_json::Value, String> {
        let runtime = self.runtime.ok_or("No container runtime available")?;
        let cmd = runtime.command();

        let output = AsyncCommand::new(cmd)
            .args(["inspect", image_ref])
            .output()
            .await
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        serde_json::from_slice(&output.stdout)
            .map_err(|e| e.to_string())
    }

    /// Pull an image
    pub async fn pull_image(&self, image_ref: &str) -> Result<String, String> {
        let runtime = self.runtime.ok_or("No container runtime available")?;
        let cmd = runtime.command();

        let output = AsyncCommand::new(cmd)
            .args(["pull", image_ref])
            .output()
            .await
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Get image layers/history
    pub async fn get_image_history(&self, image_ref: &str) -> Result<Vec<ImageLayer>, String> {
        let runtime = self.runtime.ok_or("No container runtime available")?;
        let cmd = runtime.command();

        let output = AsyncCommand::new(cmd)
            .args(["history", "--format", "{{json .}}", image_ref])
            .output()
            .await
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut layers = Vec::new();

        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(l) = serde_json::from_str::<DockerHistoryJson>(line) {
                layers.push(ImageLayer {
                    id: l.ID.unwrap_or_default(),
                    created_by: l.CreatedBy.unwrap_or_default(),
                    size: l.Size.clone().unwrap_or_default(),
                    size_bytes: parse_size(&l.Size.unwrap_or_default()),
                    created: l.CreatedSince.unwrap_or_default(),
                    comment: l.Comment.unwrap_or_default(),
                });
            }
        }

        Ok(layers)
    }

    /// Generate a random MAC address
    pub fn generate_mac_address() -> String {
        let bytes: [u8; 6] = rand::random();
        // Set locally administered bit, clear multicast bit
        format!(
            "52:54:00:{:02x}:{:02x}:{:02x}",
            bytes[3], bytes[4], bytes[5]
        )
    }

    /// Create a default network interface config
    pub fn default_interfaces() -> Vec<NetworkInterface> {
        vec![NetworkInterface {
            id: uuid::Uuid::new_v4().to_string(),
            name: "eth0".to_string(),
            nic_type: NetworkInterfaceType::User,
            mac_address: Some(Self::generate_mac_address()),
            ip_address: None, // DHCP
            gateway: None,
            vlan_id: None,
            mtu: 1500,
            bridge: None,
            promiscuous: false,
        }]
    }

    /// Generate Terraform HCL for network interfaces
    pub fn interfaces_to_terraform(interfaces: &[NetworkInterface]) -> String {
        let mut hcl = String::new();
        for (i, iface) in interfaces.iter().enumerate() {
            hcl.push_str(&format!(
                r#"
resource "infrasim_network_interface" "{}" {{
  name = "{}"
  type = "{:?}"
  mac_address = {}
  mtu = {}
{}{}}}
"#,
                iface.id.replace('-', "_"),
                iface.name,
                iface.nic_type,
                iface.mac_address.as_ref().map(|m| format!("\"{}\"", m)).unwrap_or("null".to_string()),
                iface.mtu,
                iface.bridge.as_ref().map(|b| format!("  bridge = \"{}\"\n", b)).unwrap_or_default(),
                iface.vlan_id.map(|v| format!("  vlan_id = {}\n", v)).unwrap_or_default(),
            ));
        }
        hcl
    }
}

impl Default for ContainerManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Image layer info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageLayer {
    pub id: String,
    pub created_by: String,
    pub size: String,
    pub size_bytes: i64,
    pub created: String,
    pub comment: String,
}

// Internal JSON parsing structs (Docker/Podman output)
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct DockerImageJson {
    ID: Option<String>,
    Repository: Option<String>,
    Tag: Option<String>,
    Digest: Option<String>,
    CreatedSince: Option<String>,
    Size: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct DockerSearchJson {
    Name: Option<String>,
    Description: Option<String>,
    StarCount: Option<i64>,
    IsOfficial: Option<bool>,
    IsAutomated: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct DockerHistoryJson {
    ID: Option<String>,
    CreatedBy: Option<String>,
    Size: Option<String>,
    CreatedSince: Option<String>,
    Comment: Option<String>,
}

/// Parse human-readable size to bytes
fn parse_size(s: &str) -> i64 {
    let s = s.trim().to_uppercase();
    let re = regex_lite::Regex::new(r"([0-9.]+)\s*([KMGT]?B?)").ok();
    if let Some(re) = re {
        if let Some(caps) = re.captures(&s) {
            let num: f64 = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0.0);
            let unit = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let multiplier: f64 = match unit {
                "KB" | "K" => 1024.0,
                "MB" | "M" => 1024.0 * 1024.0,
                "GB" | "G" => 1024.0 * 1024.0 * 1024.0,
                "TB" | "T" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
                _ => 1.0,
            };
            return (num * multiplier) as i64;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("1.5GB"), 1610612736);
        assert_eq!(parse_size("100MB"), 104857600);
        assert_eq!(parse_size("512KB"), 524288);
        assert_eq!(parse_size("1024"), 1024); // no unit = bytes
    }

    #[test]
    fn test_generate_mac() {
        let mac = ContainerManager::generate_mac_address();
        assert!(mac.starts_with("52:54:00:"));
        assert_eq!(mac.len(), 17);
    }
}
