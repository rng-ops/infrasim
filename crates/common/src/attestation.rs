//! Attestation module for InfraSim
//!
//! Provides host provenance collection and attestation report generation.

use crate::{
    crypto::{KeyPair, Signer},
    types::{AttestationReport, HostProvenance, Vm, Volume},
    Result,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::process::Command;
use tracing::{debug, warn};
use uuid::Uuid;

/// Attestation provider
pub struct AttestationProvider {
    key_pair: KeyPair,
}

impl AttestationProvider {
    /// Create a new attestation provider
    pub fn new(key_pair: KeyPair) -> Self {
        Self { key_pair }
    }

    /// Generate attestation report for a VM
    pub fn generate_report(
        &self,
        vm: &Vm,
        volumes: &[Volume],
        qemu_args: &[String],
    ) -> Result<AttestationReport> {
        let provenance = self.collect_host_provenance(vm, volumes, qemu_args)?;
        let digest = self.compute_provenance_digest(&provenance)?;
        let signature = self.key_pair.sign(digest.as_bytes());

        let report = AttestationReport {
            id: Uuid::new_v4().to_string(),
            vm_id: vm.meta.id.clone(),
            host_provenance: provenance,
            digest,
            signature,
            created_at: chrono::Utc::now().timestamp(),
            attestation_type: "host_provenance".to_string(),
        };

        debug!("Generated attestation report: {}", report.id);
        Ok(report)
    }

    /// Collect host provenance data
    fn collect_host_provenance(
        &self,
        vm: &Vm,
        volumes: &[Volume],
        qemu_args: &[String],
    ) -> Result<HostProvenance> {
        let qemu_version = get_qemu_version().unwrap_or_else(|_| "unknown".to_string());
        let macos_version = get_macos_version().unwrap_or_else(|_| "unknown".to_string());
        let cpu_model = get_cpu_model().unwrap_or_else(|_| "unknown".to_string());
        let hostname = get_hostname().unwrap_or_else(|_| "unknown".to_string());
        let hvf_enabled = is_hvf_available();

        // Collect volume hashes
        let mut volume_hashes = HashMap::new();
        for vol in volumes {
            if let Some(digest) = &vol.status.digest {
                volume_hashes.insert(vol.meta.id.clone(), digest.clone());
            }
        }

        // Get base image hash (from boot disk)
        let base_image_hash = volumes
            .iter()
            .find(|v| Some(&v.meta.id) == vm.spec.boot_disk_id.as_ref())
            .and_then(|v| v.status.digest.clone())
            .unwrap_or_else(|| "unknown".to_string());

        Ok(HostProvenance {
            qemu_version,
            qemu_args: qemu_args.to_vec(),
            base_image_hash,
            volume_hashes,
            macos_version,
            cpu_model,
            hvf_enabled,
            hostname,
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    /// Compute digest of provenance data
    fn compute_provenance_digest(&self, provenance: &HostProvenance) -> Result<String> {
        let serialized = serde_json::to_vec(provenance)?;
        let mut hasher = Sha256::new();
        hasher.update(&serialized);
        Ok(hex::encode(hasher.finalize()))
    }

    /// Verify an attestation report
    pub fn verify_report(&self, report: &AttestationReport) -> Result<bool> {
        use crate::crypto::Verifier;

        // Recompute digest
        let computed_digest = self.compute_provenance_digest(&report.host_provenance)?;
        if computed_digest != report.digest {
            return Ok(false);
        }

        // Verify signature
        self.key_pair
            .verify(report.digest.as_bytes(), &report.signature)?;

        Ok(true)
    }
}

/// Get QEMU version
fn get_qemu_version() -> Result<String> {
    let output = Command::new("qemu-system-aarch64")
        .arg("--version")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Parse: "QEMU emulator version 8.2.0"
    if let Some(line) = stdout.lines().next() {
        if let Some(version) = line.strip_prefix("QEMU emulator version ") {
            return Ok(version.split_whitespace().next().unwrap_or("unknown").to_string());
        }
    }

    Ok("unknown".to_string())
}

/// Get macOS version
fn get_macos_version() -> Result<String> {
    let output = Command::new("sw_vers")
        .arg("-productVersion")
        .output()?;

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(version)
}

/// Get CPU model
fn get_cpu_model() -> Result<String> {
    let output = Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()?;

    let model = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if model.is_empty() {
        // Fallback for Apple Silicon
        let output = Command::new("sysctl")
            .args(["-n", "hw.model"])
            .output()?;
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
    Ok(model)
}

/// Get hostname
fn get_hostname() -> Result<String> {
    let output = Command::new("hostname").output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Check if HVF (Hypervisor.framework) is available
pub fn is_hvf_available() -> bool {
    // Check if we're on Apple Silicon or Intel Mac with HVF
    let output = Command::new("sysctl")
        .args(["-n", "kern.hv_support"])
        .output();

    match output {
        Ok(o) => {
            let val = String::from_utf8_lossy(&o.stdout).trim().to_string();
            val == "1"
        }
        Err(_) => false,
    }
}

/// Check if QEMU is available
pub fn is_qemu_available() -> bool {
    Command::new("which")
        .arg("qemu-system-aarch64")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get QEMU path
pub fn get_qemu_path() -> Option<String> {
    let output = Command::new("which")
        .arg("qemu-system-aarch64")
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// vTPM attestation (scaffold for future implementation)
pub mod vtpm {
    use super::*;

    /// vTPM configuration
    #[derive(Debug, Clone)]
    pub struct VtpmConfig {
        pub socket_path: String,
        pub version: String,
    }

    /// vTPM attestation provider (scaffold)
    pub struct VtpmAttestationProvider {
        config: VtpmConfig,
    }

    impl VtpmAttestationProvider {
        /// Create new vTPM provider
        pub fn new(config: VtpmConfig) -> Self {
            warn!("vTPM attestation is a scaffold - not fully implemented");
            Self { config }
        }

        /// Check if vTPM is available
        pub fn is_available(&self) -> bool {
            // Would check for swtpm availability
            false
        }

        /// Get attestation quote (scaffold)
        pub async fn get_quote(&self, _nonce: &[u8]) -> Result<Vec<u8>> {
            // Would implement TPM2_Quote
            Err(crate::Error::AttestationError(
                "vTPM not implemented".to_string(),
            ))
        }
    }

    /// Document Linux host requirements for vTPM
    pub fn linux_vtpm_requirements() -> &'static str {
        r#"
vTPM Support Requirements (Linux Host):

1. Install swtpm:
   - Ubuntu/Debian: sudo apt install swtpm swtpm-tools
   - Fedora: sudo dnf install swtpm swtpm-tools
   - macOS: brew install swtpm (limited support)

2. Configure QEMU with TPM:
   -chardev socket,id=chrtpm,path=/tmp/swtpm.sock
   -tpmdev emulator,id=tpm0,chardev=chrtpm
   -device tpm-tis,tpmdev=tpm0

3. Guest kernel requirements:
   - CONFIG_TCG_TIS=y or CONFIG_TCG_TIS=m
   - CONFIG_TCG_TPM=y

4. Guest userspace:
   - tpm2-tools package
   - tpm2-tss library

5. IMA (Integrity Measurement Architecture):
   - CONFIG_IMA=y
   - Boot with ima_policy=tcb

Note: Full vTPM support requires Linux host. macOS support is limited.
"#
    }
}

/// SEV-SNP attestation (scaffold for future AMD support)
pub mod sev_snp {
    /// Document SEV-SNP requirements
    pub fn requirements() -> &'static str {
        r#"
AMD SEV-SNP Requirements:

1. Hardware:
   - AMD EPYC 7003 series (Milan) or newer
   - SEV-SNP enabled in BIOS

2. Host kernel:
   - Linux 5.19+ with SEV-SNP patches
   - CONFIG_AMD_MEM_ENCRYPT=y
   - CONFIG_KVM_AMD_SEV=y

3. QEMU:
   - QEMU 7.0+ with SEV-SNP support
   - -object sev-snp-guest,id=sev0

4. Guest:
   - Linux kernel with SEV-SNP support
   - Attestation via /dev/sev-guest

Note: Not applicable to Apple Silicon (macOS). 
This is a placeholder for future cloud deployment scenarios.
"#
    }
}

/// Intel TDX attestation (scaffold for future Intel support)
pub mod tdx {
    /// Document TDX requirements
    pub fn requirements() -> &'static str {
        r#"
Intel TDX Requirements:

1. Hardware:
   - 4th Gen Intel Xeon Scalable (Sapphire Rapids) or newer
   - TDX enabled in BIOS

2. Host:
   - Linux kernel with TDX support
   - TDX module loaded

3. QEMU:
   - QEMU with TDX support
   - -machine q35,kernel-irqchip=split,confidential-guest-support=tdx

4. Guest:
   - TDX-aware kernel
   - TDCALL for attestation

Note: Not applicable to Apple Silicon (macOS).
This is a placeholder for future cloud deployment scenarios.
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ResourceMeta, VmSpec, VmStatus, VolumeSpec, VolumeStatus};

    #[test]
    fn test_hvf_check() {
        // This will depend on the host system
        let _available = is_hvf_available();
    }

    #[test]
    fn test_attestation_generation() {
        let key_pair = KeyPair::generate();
        let provider = AttestationProvider::new(key_pair);

        let vm = Vm {
            meta: ResourceMeta::new("test-vm".to_string()),
            spec: VmSpec::default(),
            status: VmStatus::default(),
        };

        let volume = Volume {
            meta: ResourceMeta::new("test-vol".to_string()),
            spec: VolumeSpec::default(),
            status: VolumeStatus {
                ready: true,
                digest: Some("abc123".to_string()),
                ..Default::default()
            },
        };

        let report = provider
            .generate_report(&vm, &[volume], &["qemu-system-aarch64".to_string()])
            .unwrap();

        assert!(!report.id.is_empty());
        assert!(!report.digest.is_empty());
        assert!(!report.signature.is_empty());
    }

    #[test]
    fn test_attestation_verification() {
        let key_pair = KeyPair::generate();
        let provider = AttestationProvider::new(key_pair);

        let vm = Vm {
            meta: ResourceMeta::new("test-vm".to_string()),
            spec: VmSpec::default(),
            status: VmStatus::default(),
        };

        let report = provider.generate_report(&vm, &[], &[]).unwrap();
        assert!(provider.verify_report(&report).unwrap());
    }
}
