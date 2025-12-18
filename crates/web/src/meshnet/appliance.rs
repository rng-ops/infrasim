//! Appliance service and archive generation
//!
//! Generates downloadable archives containing:
//! - disk.qcow2 (placeholder)
//! - mesh/*.conf (WireGuard configs)
//! - terraform/* (rendered tf/json)
//! - signatures/manifest.json + manifest.sig

use crate::meshnet::db::{MeshnetDb, MeshnetAppliance, ApplianceStatus, MeshPeerRecord};
use crate::meshnet::mesh::{MeshProvider, WireGuardProvider};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Appliance service
pub struct ApplianceService {
    db: MeshnetDb,
    mesh_provider: Arc<WireGuardProvider>,
    data_dir: PathBuf,
    /// Active build jobs
    active_jobs: RwLock<std::collections::HashMap<Uuid, tokio::task::JoinHandle<()>>>,
}

impl ApplianceService {
    pub fn new(db: MeshnetDb, mesh_provider: Arc<WireGuardProvider>) -> Self {
        let data_dir = PathBuf::from(
            std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string())
        );
        
        Self {
            db,
            mesh_provider,
            data_dir,
            active_jobs: RwLock::new(std::collections::HashMap::new()),
        }
    }
    
    /// Create a new appliance
    pub async fn create_appliance(&self, user_id: Uuid, name: &str) -> Result<MeshnetAppliance, String> {
        let appliance = self.db.create_appliance(user_id, name)?;
        
        // Start build job
        self.start_build(appliance.id, user_id).await?;
        
        Ok(appliance)
    }
    
    /// Start building an appliance archive
    pub async fn start_build(&self, appliance_id: Uuid, user_id: Uuid) -> Result<(), String> {
        // Check if job already running
        {
            let jobs = self.active_jobs.read().await;
            if let Some(handle) = jobs.get(&appliance_id) {
                if !handle.is_finished() {
                    return Ok(());
                }
            }
        }
        
        let db = self.db.clone();
        let mesh_provider = self.mesh_provider.clone();
        let data_dir = self.data_dir.clone();
        
        info!("Starting build job for appliance {}", appliance_id);
        
        // Update status to building
        db.update_appliance_status(appliance_id, ApplianceStatus::Building, None, None, None, None)?;
        
        let job = tokio::spawn(async move {
            match build_appliance_archive(&db, &mesh_provider, &data_dir, appliance_id, user_id).await {
                Ok(paths) => {
                    let _ = db.update_appliance_status(
                        appliance_id,
                        ApplianceStatus::Ready,
                        paths.qcow_path.as_deref(),
                        paths.archive_path.as_deref(),
                        paths.terraform_path.as_deref(),
                        None,
                    );
                    info!("Appliance {} build complete", appliance_id);
                }
                Err(e) => {
                    error!("Appliance {} build failed: {}", appliance_id, e);
                    let _ = db.update_appliance_status(
                        appliance_id,
                        ApplianceStatus::Error,
                        None,
                        None,
                        None,
                        Some(&e),
                    );
                }
            }
        });
        
        {
            let mut jobs = self.active_jobs.write().await;
            jobs.insert(appliance_id, job);
        }
        
        Ok(())
    }
    
    /// List appliances for a user
    pub fn list_appliances(&self, user_id: Uuid) -> Result<Vec<MeshnetAppliance>, String> {
        self.db.get_appliances(user_id)
    }
    
    /// Get an appliance
    pub fn get_appliance(&self, id: Uuid) -> Result<Option<MeshnetAppliance>, String> {
        self.db.get_appliance(id)
    }
    
    /// Delete an appliance
    pub async fn delete_appliance(&self, id: Uuid) -> Result<(), String> {
        // Get appliance to find files to delete
        if let Some(appliance) = self.db.get_appliance(id)? {
            // Delete archive file if exists
            if let Some(path) = &appliance.archive_path {
                let _ = tokio::fs::remove_file(path).await;
            }
            // Delete terraform file if exists
            if let Some(path) = &appliance.terraform_path {
                let _ = tokio::fs::remove_file(path).await;
            }
            // Delete qcow file if exists
            if let Some(path) = &appliance.qcow_path {
                let _ = tokio::fs::remove_file(path).await;
            }
        }
        
        self.db.delete_appliance(id)
    }
    
    /// Trigger a rebuild
    pub async fn redeploy(&self, id: Uuid) -> Result<(), String> {
        let appliance = self.db.get_appliance(id)?
            .ok_or_else(|| "Appliance not found".to_string())?;
        
        // Reset status and rebuild
        self.db.update_appliance_status(id, ApplianceStatus::Pending, None, None, None, None)?;
        self.start_build(id, appliance.user_id).await
    }
    
    /// Get archive path for download
    pub fn get_archive_path(&self, id: Uuid) -> Result<Option<PathBuf>, String> {
        let appliance = self.db.get_appliance(id)?;
        Ok(appliance.and_then(|a| a.archive_path.map(PathBuf::from)))
    }
    
    /// Get terraform content
    pub fn get_terraform(&self, id: Uuid) -> Result<Option<String>, String> {
        let appliance = self.db.get_appliance(id)?;
        if let Some(path) = appliance.and_then(|a| a.terraform_path) {
            match std::fs::read_to_string(&path) {
                Ok(content) => Ok(Some(content)),
                Err(e) => Err(format!("Failed to read terraform file: {}", e)),
            }
        } else {
            Ok(None)
        }
    }
}

/// Build output paths
struct BuildPaths {
    qcow_path: Option<String>,
    archive_path: Option<String>,
    terraform_path: Option<String>,
}

/// Build the appliance archive
async fn build_appliance_archive(
    db: &MeshnetDb,
    mesh_provider: &WireGuardProvider,
    data_dir: &Path,
    appliance_id: Uuid,
    user_id: Uuid,
) -> Result<BuildPaths, String> {
    use sha2::{Sha256, Digest};
    use std::io::Write;
    
    // Get identity
    let identity = db.get_identity_by_user(user_id)?
        .ok_or_else(|| "User has no identity".to_string())?;
    
    // Get appliance
    let appliance = db.get_appliance(appliance_id)?
        .ok_or_else(|| "Appliance not found".to_string())?;
    
    // Create directory structure
    let appliance_dir = data_dir
        .join("users")
        .join(user_id.to_string())
        .join("appliances")
        .join(appliance_id.to_string());
    
    tokio::fs::create_dir_all(&appliance_dir).await
        .map_err(|e| format!("Failed to create directory: {}", e))?;
    
    let mesh_dir = appliance_dir.join("mesh");
    let terraform_dir = appliance_dir.join("terraform");
    let signatures_dir = appliance_dir.join("signatures");
    
    tokio::fs::create_dir_all(&mesh_dir).await.ok();
    tokio::fs::create_dir_all(&terraform_dir).await.ok();
    tokio::fs::create_dir_all(&signatures_dir).await.ok();
    
    // Get peers and generate configs
    let peers = db.get_mesh_peers(user_id)?;
    let mut manifest_entries = Vec::new();
    
    for peer in &peers {
        if peer.revoked_at.is_some() {
            continue;
        }
        
        match mesh_provider.render_client_config(peer, &identity) {
            Ok(config) => {
                let filename = format!("{}-{}.conf", identity.handle, peer.name);
                let path = mesh_dir.join(&filename);
                tokio::fs::write(&path, &config).await
                    .map_err(|e| format!("Failed to write config: {}", e))?;
                
                // Add to manifest
                let mut hasher = Sha256::new();
                hasher.update(config.as_bytes());
                let hash = hex::encode(hasher.finalize());
                manifest_entries.push(ManifestEntry {
                    path: format!("mesh/{}", filename),
                    sha256: hash,
                    size: config.len() as u64,
                });
            }
            Err(e) => {
                warn!("Failed to render config for peer {}: {}", peer.name, e);
            }
        }
    }
    
    // Generate placeholder qcow2 (just a small file for MVP)
    let qcow_path = appliance_dir.join("disk.qcow2");
    let qcow_content = b"QCOW2 PLACEHOLDER - Replace with actual disk image\n";
    tokio::fs::write(&qcow_path, qcow_content).await
        .map_err(|e| format!("Failed to write qcow2: {}", e))?;
    
    let mut hasher = Sha256::new();
    hasher.update(qcow_content);
    manifest_entries.push(ManifestEntry {
        path: "disk.qcow2".to_string(),
        sha256: hex::encode(hasher.finalize()),
        size: qcow_content.len() as u64,
    });
    
    // Generate Terraform
    let terraform_content = generate_terraform(&identity, &appliance, &peers);
    let terraform_path = terraform_dir.join("main.tf.json");
    tokio::fs::write(&terraform_path, &terraform_content).await
        .map_err(|e| format!("Failed to write terraform: {}", e))?;
    
    let mut hasher = Sha256::new();
    hasher.update(terraform_content.as_bytes());
    manifest_entries.push(ManifestEntry {
        path: "terraform/main.tf.json".to_string(),
        sha256: hex::encode(hasher.finalize()),
        size: terraform_content.len() as u64,
    });
    
    // Generate README
    let readme_content = generate_readme(&identity, &appliance);
    let readme_path = appliance_dir.join("README.md");
    tokio::fs::write(&readme_path, &readme_content).await
        .map_err(|e| format!("Failed to write readme: {}", e))?;
    
    let mut hasher = Sha256::new();
    hasher.update(readme_content.as_bytes());
    manifest_entries.push(ManifestEntry {
        path: "README.md".to_string(),
        sha256: hex::encode(hasher.finalize()),
        size: readme_content.len() as u64,
    });
    
    // Generate manifest
    let manifest = Manifest {
        version: "1.0".to_string(),
        appliance_id: appliance_id.to_string(),
        appliance_name: appliance.name.clone(),
        identity_handle: identity.handle.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        files: manifest_entries,
    };
    
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Failed to serialize manifest: {}", e))?;
    
    let manifest_path = signatures_dir.join("manifest.json");
    tokio::fs::write(&manifest_path, &manifest_json).await
        .map_err(|e| format!("Failed to write manifest: {}", e))?;
    
    // Generate stub signature (in real impl, use ed25519)
    let signature = generate_stub_signature(&manifest_json);
    let sig_path = signatures_dir.join("manifest.sig");
    tokio::fs::write(&sig_path, &signature).await
        .map_err(|e| format!("Failed to write signature: {}", e))?;
    
    // Create archive
    let archive_path = appliance_dir.join(format!("{}.tar.gz", appliance.name));
    create_tar_gz(&appliance_dir, &archive_path).await?;
    
    Ok(BuildPaths {
        qcow_path: Some(qcow_path.to_string_lossy().to_string()),
        archive_path: Some(archive_path.to_string_lossy().to_string()),
        terraform_path: Some(terraform_path.to_string_lossy().to_string()),
    })
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    version: String,
    appliance_id: String,
    appliance_name: String,
    identity_handle: String,
    created_at: String,
    files: Vec<ManifestEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ManifestEntry {
    path: String,
    sha256: String,
    size: u64,
}

fn generate_terraform(
    identity: &crate::meshnet::db::MeshnetIdentity,
    appliance: &MeshnetAppliance,
    peers: &[MeshPeerRecord],
) -> String {
    // Generate Terraform JSON format
    let tf = serde_json::json!({
        "terraform": {
            "required_providers": {
                "infrasim": {
                    "source": "infrasim/infrasim",
                    "version": ">= 0.1.0"
                }
            }
        },
        "variable": {
            "identity_handle": {
                "type": "string",
                "default": identity.handle
            },
            "appliance_name": {
                "type": "string",
                "default": appliance.name
            }
        },
        "resource": {
            "infrasim_appliance": {
                "main": {
                    "name": format!("${{{}}}", "var.appliance_name"),
                    "identity_handle": format!("${{{}}}", "var.identity_handle"),
                    "disk_image": "./disk.qcow2",
                    "memory_mb": 2048,
                    "cpu_cores": 2,
                    "mesh_enabled": true,
                    "mesh_peers": peers.iter()
                        .filter(|p| p.revoked_at.is_none())
                        .map(|p| serde_json::json!({
                            "name": p.name,
                            "public_key": p.public_key,
                            "address": p.address
                        }))
                        .collect::<Vec<_>>()
                }
            },
            "infrasim_mesh_sidecar": {
                "wireguard": {
                    "appliance_id": "${infrasim_appliance.main.id}",
                    "provider_type": "wireguard",
                    "config_path": "./mesh/"
                }
            },
            "infrasim_dns_forwarder": {
                "local": {
                    "appliance_id": "${infrasim_appliance.main.id}",
                    "upstream_servers": ["8.8.8.8", "8.8.4.4"],
                    "enabled": true
                }
            }
        },
        "output": {
            "appliance_id": {
                "value": "${infrasim_appliance.main.id}"
            },
            "mesh_status": {
                "value": "${infrasim_mesh_sidecar.wireguard.status}"
            },
            "fqdn": {
                "value": identity.fqdn
            }
        }
    });
    
    serde_json::to_string_pretty(&tf).unwrap_or_default()
}

fn generate_readme(
    identity: &crate::meshnet::db::MeshnetIdentity,
    appliance: &MeshnetAppliance,
) -> String {
    let handle = &identity.handle;
    format!(
r#"# {name} Appliance

**Identity:** {handle} ({fqdn})
**Version:** {version}

## Quick Start

### 1. Import the disk image

```bash
# Using QEMU
qemu-img info disk.qcow2
qemu-system-aarch64 \
    -machine virt \
    -cpu host \
    -m 2048 \
    -drive file=disk.qcow2,format=qcow2,if=virtio
```

### 2. Configure WireGuard

Copy the appropriate config from `mesh/` to your WireGuard client:

```bash
# Linux
sudo cp mesh/{handle}-*.conf /etc/wireguard/
sudo wg-quick up {handle}-laptop

# macOS (using WireGuard app)
# Import mesh/{handle}-*.conf via the app
```

### 3. Apply Terraform (optional)

```bash
cd terraform
terraform init
terraform plan
terraform apply
```

## Files

- `disk.qcow2` - QEMU disk image
- `mesh/*.conf` - WireGuard client configurations
- `terraform/main.tf.json` - Infrastructure as code
- `signatures/manifest.json` - File manifest with checksums
- `signatures/manifest.sig` - Manifest signature

## Support

For issues, visit: https://{fqdn}

---
Generated by Meshnet Console
"#,
        name = appliance.name,
        handle = handle,
        fqdn = identity.fqdn,
        version = appliance.version,
    )
}

fn generate_stub_signature(content: &str) -> String {
    use sha2::{Sha256, Digest};
    
    // Stub: just hash the content. Real impl would use ed25519.
    let mut hasher = Sha256::new();
    hasher.update(b"MESHNET-SIG-V1:");
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    
    format!("MESHNET-SIG-V1:{}", hex::encode(hash))
}

async fn create_tar_gz(source_dir: &Path, archive_path: &Path) -> Result<(), String> {
    use std::process::Command;
    
    // Use system tar for simplicity (cross-platform would need different approach)
    let output = Command::new("tar")
        .current_dir(source_dir)
        .args([
            "-czf",
            archive_path.to_str().unwrap_or("archive.tar.gz"),
            "disk.qcow2",
            "mesh",
            "terraform",
            "signatures",
            "README.md",
        ])
        .output()
        .map_err(|e| format!("Failed to run tar: {}", e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tar failed: {}", stderr));
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub_signature() {
        let sig = generate_stub_signature("test content");
        assert!(sig.starts_with("MESHNET-SIG-V1:"));
        assert_eq!(sig.len(), 16 + 64); // prefix + hex sha256
    }
}
