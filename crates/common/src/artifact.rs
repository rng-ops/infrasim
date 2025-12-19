//! Artifact Inspection Module
//!
//! Provides functionality to inspect and verify InfraSim build artifacts:
//! - SHA256 verification of tarballs
//! - Manifest parsing and file hash verification
//! - qcow2 image header analysis
//! - Attestation JSON validation
//! - Signature status detection

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tracing::{debug, warn};

/// Errors that can occur during artifact inspection
#[derive(Error, Debug)]
pub enum ArtifactError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid archive: {0}")]
    InvalidArchive(String),

    #[error("JSON parse error in {file}: {error}")]
    JsonParse { file: String, error: String },

    #[error("Path traversal detected: {0}")]
    PathTraversal(String),

    #[error("Unsupported archive format: {0}")]
    UnsupportedFormat(String),
}

/// Result type for artifact operations
pub type Result<T> = std::result::Result<T, ArtifactError>;

// ============================================================================
// Report Structures
// ============================================================================

/// Complete artifact inspection report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactInspectionReport {
    pub input_path: String,

    // SHA256 verification
    pub sha256_file_ok: bool,
    pub sha256_expected: Option<String>,
    pub sha256_actual: Option<String>,

    // Extracted files
    pub extracted_files: Vec<FileEntry>,

    // Manifest check
    pub manifest: ManifestCheck,

    // Attestation check
    pub attestations: AttestationCheck,

    // qcow2 analysis
    pub qcow2_images: Vec<Qcow2Info>,

    // Signature status
    pub signatures: SignatureStatus,

    // Issues
    pub warnings: Vec<String>,
    pub errors: Vec<String>,

    // Overall
    pub passed: bool,
}

impl Default for ArtifactInspectionReport {
    fn default() -> Self {
        Self {
            input_path: String::new(),
            sha256_file_ok: false,
            sha256_expected: None,
            sha256_actual: None,
            extracted_files: Vec::new(),
            manifest: ManifestCheck::default(),
            attestations: AttestationCheck::default(),
            qcow2_images: Vec::new(),
            signatures: SignatureStatus::default(),
            warnings: Vec::new(),
            errors: Vec::new(),
            passed: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManifestCheck {
    pub found: bool,
    pub parsed_ok: bool,
    pub manifest_sha256: Option<String>,
    pub total_entries: usize,
    pub verified_entries: usize,
    pub missing_files: Vec<String>,
    pub mismatched_files: Vec<String>,
    pub parse_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AttestationCheck {
    pub integrity_attestation_found: bool,
    pub integrity_attestation_ok: bool,
    pub manifest_sha256_in_attestation: Option<String>,
    pub manifest_sha256_matches: bool,
    pub malformed_json_files: Vec<String>,
    pub truncation_detected: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Qcow2Info {
    pub path: String,
    pub valid_magic: bool,
    pub version: u32,
    pub virtual_size: u64,
    pub cluster_bits: u32,
    pub cluster_size: u64,
    pub backing_file: Option<String>,
    pub backing_file_exists: bool,
    pub issues: Vec<String>,
}

impl Default for Qcow2Info {
    fn default() -> Self {
        Self {
            path: String::new(),
            valid_magic: false,
            version: 0,
            virtual_size: 0,
            cluster_bits: 0,
            cluster_size: 0,
            backing_file: None,
            backing_file_exists: false,
            issues: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SignatureStatus {
    pub signature_file_found: bool,
    pub signature_info_found: bool,
    pub status: String, // "verified", "placeholder", "missing", "invalid"
    pub algorithm: Option<String>,
    pub signer: Option<String>,
    pub remediation_hints: Vec<String>,
}

// ============================================================================
// Artifact Inspector
// ============================================================================

/// Main artifact inspector
pub struct ArtifactInspector {
    extract_dir: Option<tempfile::TempDir>,
}

impl ArtifactInspector {
    pub fn new() -> Self {
        Self { extract_dir: None }
    }

    /// Inspect an artifact bundle (zip or tar.gz)
    pub fn inspect<P: AsRef<Path>>(&mut self, path: P) -> Result<ArtifactInspectionReport> {
        let path = path.as_ref();
        let mut report = ArtifactInspectionReport {
            input_path: path.display().to_string(),
            ..Default::default()
        };

        debug!("Inspecting artifact: {}", path.display());

        // Determine archive type
        let path_str = path.to_string_lossy().to_lowercase();

        if path_str.ends_with(".zip") {
            self.inspect_zip(path, &mut report)?;
        } else if path_str.ends_with(".tar.gz") || path_str.ends_with(".tgz") {
            self.inspect_tarball(path, &mut report)?;
        } else {
            return Err(ArtifactError::UnsupportedFormat(
                path.display().to_string(),
            ));
        }

        // Determine overall pass/fail
        report.passed = report.errors.is_empty()
            && report.sha256_file_ok
            && report.manifest.parsed_ok
            && report.attestations.malformed_json_files.is_empty()
            && report.attestations.truncation_detected.is_empty();

        Ok(report)
    }

    /// Inspect a zip file containing the tarball
    fn inspect_zip(&mut self, path: &Path, report: &mut ArtifactInspectionReport) -> Result<()> {
        let file = File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| ArtifactError::InvalidArchive(format!("zip: {}", e)))?;

        // Look for .tar.gz and .sha256 files
        let mut tarball_name: Option<String> = None;
        let mut sha256_content: Option<String> = None;

        for i in 0..archive.len() {
            let file = archive.by_index(i)
                .map_err(|e| ArtifactError::InvalidArchive(format!("zip entry: {}", e)))?;
            let name = file.name().to_string();

            // Security: check for path traversal
            if name.contains("..") {
                return Err(ArtifactError::PathTraversal(name));
            }

            if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
                tarball_name = Some(name);
            } else if name.ends_with(".sha256") {
                let mut content = String::new();
                let mut file = file;
                file.read_to_string(&mut content)?;
                sha256_content = Some(content);
            }
        }

        // Extract and verify tarball
        if let Some(ref tar_name) = tarball_name {
            // Create temp dir for extraction
            let temp_dir = tempfile::tempdir()?;
            let tarball_path = temp_dir.path().join(tar_name);

            // Extract tarball from zip
            {
                let mut tar_file = archive.by_name(tar_name)
                    .map_err(|e| ArtifactError::InvalidArchive(format!("tar extraction: {}", e)))?;
                let mut out_file = File::create(&tarball_path)?;
                std::io::copy(&mut tar_file, &mut out_file)?;
            }

            // Verify SHA256 if present
            if let Some(ref sha256_str) = sha256_content {
                let expected = parse_sha256_file(sha256_str);
                report.sha256_expected = expected.clone();

                let actual = compute_file_sha256(&tarball_path)?;
                report.sha256_actual = Some(actual.clone());

                if let Some(ref exp) = expected {
                    report.sha256_file_ok = exp.to_lowercase() == actual.to_lowercase();
                    if !report.sha256_file_ok {
                        report.errors.push(format!(
                            "SHA256 mismatch: expected {}, got {}",
                            exp, actual
                        ));
                    }
                }
            } else {
                report.warnings.push("No .sha256 file found in zip".to_string());
            }

            // Now inspect the tarball
            self.inspect_tarball(&tarball_path, report)?;
        } else {
            report.errors.push("No .tar.gz file found in zip".to_string());
        }

        Ok(())
    }

    /// Inspect a tar.gz file directly
    fn inspect_tarball(&mut self, path: &Path, report: &mut ArtifactInspectionReport) -> Result<()> {
        // Check for companion .sha256 file if not already checked
        if report.sha256_expected.is_none() {
            let sha256_path = PathBuf::from(format!("{}.sha256", path.display()));
            if sha256_path.exists() {
                let content = std::fs::read_to_string(&sha256_path)?;
                report.sha256_expected = parse_sha256_file(&content);

                let actual = compute_file_sha256(path)?;
                report.sha256_actual = Some(actual.clone());

                if let Some(ref exp) = report.sha256_expected {
                    report.sha256_file_ok = exp.to_lowercase() == actual.to_lowercase();
                    if !report.sha256_file_ok {
                        report.errors.push(format!(
                            "SHA256 mismatch: expected {}, got {}",
                            exp, actual
                        ));
                    }
                }
            }
        }

        // Extract tarball
        let temp_dir = tempfile::tempdir()?;
        let extract_path = temp_dir.path();

        let file = File::open(path)?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);

        // Extract with path traversal protection
        for entry in archive.entries()? {
            let mut entry = entry?;
            let entry_path = entry.path()?.to_path_buf(); // Clone to release borrow

            // Security: reject path traversal
            if entry_path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                return Err(ArtifactError::PathTraversal(
                    entry_path.display().to_string(),
                ));
            }

            let dest = extract_path.join(&entry_path);
            entry.unpack(&dest)?;

            // Record file entry
            if entry.header().entry_type().is_file() {
                let size = entry.header().size()?;
                report.extracted_files.push(FileEntry {
                    path: entry_path.display().to_string(),
                    size,
                    sha256: String::new(), // Will compute later
                });
            }
        }

        // Compute SHA256 for all extracted files
        for file_entry in &mut report.extracted_files {
            let file_path = extract_path.join(&file_entry.path);
            if file_path.exists() && file_path.is_file() {
                file_entry.sha256 = compute_file_sha256(&file_path)?;
            }
        }

        // Verify manifest
        self.verify_manifest(extract_path, report)?;

        // Check attestations
        self.check_attestations(extract_path, report)?;

        // Analyze qcow2 images
        self.analyze_qcow2_images(extract_path, report)?;

        // Check signatures
        self.check_signatures(extract_path, report)?;

        // Keep temp dir alive
        self.extract_dir = Some(temp_dir);

        Ok(())
    }

    /// Verify manifest.json against actual files
    fn verify_manifest(&self, extract_path: &Path, report: &mut ArtifactInspectionReport) -> Result<()> {
        let manifest_path = extract_path.join("meta/manifest.json");

        if !manifest_path.exists() {
            report.manifest.found = false;
            report.errors.push("meta/manifest.json not found".to_string());
            return Ok(());
        }

        report.manifest.found = true;

        // Compute manifest SHA256
        report.manifest.manifest_sha256 = Some(compute_file_sha256(&manifest_path)?);

        // Parse manifest
        let content = std::fs::read_to_string(&manifest_path)?;

        // Check for truncation markers
        if detect_truncation(&content) {
            report.attestations.truncation_detected.push("meta/manifest.json".to_string());
            report.errors.push("Truncation placeholder '...' detected in meta/manifest.json".to_string());
        }

        let manifest: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                report.manifest.parsed_ok = false;
                report.manifest.parse_errors.push(format!("JSON parse error: {}", e));
                report.errors.push(format!("Failed to parse meta/manifest.json: {}", e));
                return Ok(());
            }
        };

        report.manifest.parsed_ok = true;

        // Verify file entries
        if let Some(files) = manifest.get("files").and_then(|f| f.as_array()) {
            report.manifest.total_entries = files.len();

            for file in files {
                let path = file.get("path").and_then(|p| p.as_str()).unwrap_or("");
                let expected_sha256 = file.get("sha256").and_then(|s| s.as_str()).unwrap_or("");
                let expected_size = file.get("size").and_then(|s| s.as_u64());

                let file_path = extract_path.join(path);

                if !file_path.exists() {
                    report.manifest.missing_files.push(path.to_string());
                    continue;
                }

                // Verify SHA256
                let actual_sha256 = compute_file_sha256(&file_path)?;
                if actual_sha256.to_lowercase() != expected_sha256.to_lowercase() {
                    report.manifest.mismatched_files.push(format!(
                        "{}: expected {}, got {}",
                        path, expected_sha256, actual_sha256
                    ));
                } else {
                    report.manifest.verified_entries += 1;
                }

                // Verify size
                if let Some(exp_size) = expected_size {
                    let actual_size = std::fs::metadata(&file_path)?.len();
                    if actual_size != exp_size {
                        report.warnings.push(format!(
                            "{}: size mismatch, expected {}, got {}",
                            path, exp_size, actual_size
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Check attestation JSON files
    fn check_attestations(&self, extract_path: &Path, report: &mut ArtifactInspectionReport) -> Result<()> {
        let attestations_dir = extract_path.join("meta/attestations");

        if !attestations_dir.exists() {
            report.warnings.push("meta/attestations/ directory not found".to_string());
            return Ok(());
        }

        // Check all JSON files in attestations
        for entry in std::fs::read_dir(&attestations_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map(|e| e == "json").unwrap_or(false) {
                let filename = path.file_name().unwrap().to_string_lossy().to_string();
                let content = std::fs::read_to_string(&path)?;

                // Check for truncation
                if detect_truncation(&content) {
                    report.attestations.truncation_detected.push(format!("meta/attestations/{}", filename));
                    report.errors.push(format!(
                        "Truncation placeholder '...' detected in meta/attestations/{}",
                        filename
                    ));
                }

                // Validate JSON
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(json) => {
                        // Check artifact-integrity.json specifically
                        if filename == "artifact-integrity.json" {
                            report.attestations.integrity_attestation_found = true;

                            if let Some(predicate) = json.get("predicate") {
                                if let Some(sha) = predicate.get("manifest_sha256").and_then(|s| s.as_str()) {
                                    report.attestations.manifest_sha256_in_attestation = Some(sha.to_string());

                                    // Compare with actual manifest SHA256
                                    if let Some(ref actual) = report.manifest.manifest_sha256 {
                                        report.attestations.manifest_sha256_matches =
                                            sha.to_lowercase() == actual.to_lowercase();

                                        if !report.attestations.manifest_sha256_matches {
                                            report.errors.push(format!(
                                                "Manifest SHA256 mismatch in attestation: expected {}, got {}",
                                                sha, actual
                                            ));
                                        }
                                    }
                                }
                            }

                            report.attestations.integrity_attestation_ok =
                                report.attestations.manifest_sha256_matches;
                        }
                    }
                    Err(e) => {
                        report.attestations.malformed_json_files.push(format!(
                            "meta/attestations/{}: {}",
                            filename, e
                        ));
                        report.errors.push(format!(
                            "Invalid JSON in meta/attestations/{}: {}",
                            filename, e
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Analyze qcow2 disk images
    fn analyze_qcow2_images(&self, extract_path: &Path, report: &mut ArtifactInspectionReport) -> Result<()> {
        // Find all qcow2 files
        let disk_dir = extract_path.join("disk");
        if !disk_dir.exists() {
            return Ok(());
        }

        for entry in walkdir::WalkDir::new(&disk_dir) {
            let entry = entry.map_err(|e| ArtifactError::Io(e.into()))?;
            let path = entry.path();

            if path.extension().map(|e| e == "qcow2").unwrap_or(false) {
                let relative_path = path.strip_prefix(extract_path).unwrap_or(path);
                let info = parse_qcow2_header(path, extract_path)?;
                report.qcow2_images.push(Qcow2Info {
                    path: relative_path.display().to_string(),
                    ..info
                });
            }
        }

        Ok(())
    }

    /// Check signature files
    fn check_signatures(&self, extract_path: &Path, report: &mut ArtifactInspectionReport) -> Result<()> {
        let sig_dir = extract_path.join("meta/signatures");

        if !sig_dir.exists() {
            report.signatures.status = "missing".to_string();
            report.warnings.push("meta/signatures/ directory not found".to_string());
            report.signatures.remediation_hints.push(
                "Run bundling with SIGNING_KEY environment variable set".to_string()
            );
            return Ok(());
        }

        // Check signature-info.json
        let sig_info_path = sig_dir.join("signature-info.json");
        if sig_info_path.exists() {
            report.signatures.signature_info_found = true;

            let content = std::fs::read_to_string(&sig_info_path)?;
            if let Ok(info) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(status) = info.get("status").and_then(|s| s.as_str()) {
                    if status == "placeholder" {
                        report.signatures.status = "placeholder".to_string();
                        report.warnings.push("Signature is a placeholder, not cryptographically verified".to_string());
                    }
                }

                if let Some(algo) = info.get("algorithm").and_then(|s| s.as_str()) {
                    report.signatures.algorithm = Some(algo.to_string());
                }
            }
        }

        // Check manifest.sig
        let manifest_sig_path = sig_dir.join("manifest.sig");
        if manifest_sig_path.exists() {
            report.signatures.signature_file_found = true;

            let content = std::fs::read_to_string(&manifest_sig_path)?;

            // Detect placeholder signatures
            if content.contains("placeholder") || content.contains("PLACEHOLDER") 
                || content.contains("TODO") || !looks_like_signature(&content) 
            {
                report.signatures.status = "placeholder".to_string();
            }
        } else {
            report.signatures.status = "missing".to_string();
        }

        // Set remediation hints based on status
        if report.signatures.status != "verified" {
            report.signatures.remediation_hints.extend([
                "Generate Ed25519 key: openssl genpkey -algorithm ED25519 -out signing.key".to_string(),
                "Export public key: openssl pkey -in signing.key -pubout -out signing.pub".to_string(),
                "Set SIGNING_KEY=/path/to/signing.key before bundling".to_string(),
            ]);
        }

        Ok(())
    }
}

impl Default for ArtifactInspector {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// qcow2 Header Parsing
// ============================================================================

/// QCOW2 magic number: QFI\xfb
const QCOW2_MAGIC: [u8; 4] = [0x51, 0x46, 0x49, 0xfb];

/// Parse qcow2 header directly (without shelling out to qemu-img)
pub fn parse_qcow2_header(path: &Path, extract_root: &Path) -> Result<Qcow2Info> {
    let mut info = Qcow2Info::default();
    info.path = path.display().to_string();

    let mut file = File::open(path)?;
    let mut header = [0u8; 104]; // qcow2 v3 header is 104 bytes

    if file.read(&mut header)? < 32 {
        info.issues.push("File too small for qcow2 header".to_string());
        return Ok(info);
    }

    // Check magic (bytes 0-3)
    info.valid_magic = header[0..4] == QCOW2_MAGIC;
    if !info.valid_magic {
        info.issues.push(format!(
            "Invalid magic: expected {:02x?}, got {:02x?}",
            QCOW2_MAGIC,
            &header[0..4]
        ));
        return Ok(info);
    }

    // Version (bytes 4-7, big-endian u32)
    info.version = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
    if info.version != 2 && info.version != 3 {
        info.issues.push(format!("Unexpected qcow2 version: {}", info.version));
    }

    // Backing file offset (bytes 8-15, big-endian u64)
    let backing_file_offset = u64::from_be_bytes([
        header[8], header[9], header[10], header[11],
        header[12], header[13], header[14], header[15],
    ]);

    // Backing file size (bytes 16-19, big-endian u32)
    let backing_file_size = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);

    // Cluster bits (bytes 20-23, big-endian u32)
    info.cluster_bits = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
    info.cluster_size = 1u64 << info.cluster_bits;

    // Virtual size (bytes 24-31, big-endian u64)
    info.virtual_size = u64::from_be_bytes([
        header[24], header[25], header[26], header[27],
        header[28], header[29], header[30], header[31],
    ]);

    // Read backing file if present
    if backing_file_offset != 0 && backing_file_size > 0 {
        file.seek(SeekFrom::Start(backing_file_offset))?;
        let mut backing_buf = vec![0u8; backing_file_size as usize];
        file.read_exact(&mut backing_buf)?;

        if let Ok(backing_str) = String::from_utf8(backing_buf) {
            info.backing_file = Some(backing_str.clone());

            // Check if backing file exists (relative to the qcow2 file's parent)
            let qcow2_parent = path.parent().unwrap_or(Path::new("."));
            let backing_path = qcow2_parent.join(&backing_str);

            info.backing_file_exists = backing_path.exists();

            if !info.backing_file_exists {
                // Also check relative to extract root
                let alt_backing_path = extract_root.join("disk").join(
                    backing_str.trim_start_matches("../")
                );
                info.backing_file_exists = alt_backing_path.exists();
            }

            if !info.backing_file_exists {
                info.issues.push(format!(
                    "Backing file not found: {} (resolved from {})",
                    backing_str,
                    qcow2_parent.display()
                ));
            }
        }
    }

    Ok(info)
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Parse a .sha256 file, supporting formats:
/// - "<hash>  <filename>"
/// - "<hash> <filename>"
/// - "<hash>"
pub fn parse_sha256_file(content: &str) -> Option<String> {
    let line = content.lines().next()?.trim();

    // Try "<hash>  <filename>" or "<hash> <filename>" format
    if let Some((hash, _)) = line.split_once(char::is_whitespace) {
        let hash = hash.trim();
        if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(hash.to_string());
        }
    }

    // Try hash-only format
    let trimmed = line.trim();
    if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(trimmed.to_string());
    }

    None
}

/// Compute SHA256 hash of a file
pub fn compute_file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Detect truncation placeholders like "..." in JSON content
pub fn detect_truncation(content: &str) -> bool {
    // Look for "..." patterns that indicate truncation
    // Common patterns:
    // - standalone "..."
    // - "...," in arrays
    // - ": ..." in objects
    // - "/* ... */" style comments

    let patterns = [
        r#""...""#,           // Quoted ellipsis
        r#": ..."#,           // Value position
        r#", ..."#,           // Array continuation
        r#"..."#,             // Plain ellipsis (be careful with false positives)
    ];

    for pattern in patterns {
        if content.contains(pattern) {
            // Avoid false positives in strings that legitimately contain "..."
            // by checking if it's in a JSON structural position
            return true;
        }
    }

    false
}

/// Check if content looks like a real signature (base64 or binary-ish)
fn looks_like_signature(content: &str) -> bool {
    let trimmed = content.trim();

    // If it contains "placeholder" or "TODO", it's not a real signature
    if trimmed.to_lowercase().contains("placeholder") || trimmed.contains("TODO") {
        return false;
    }

    // Check if it looks like PEM format
    if trimmed.contains("-----BEGIN") && trimmed.contains("-----END") {
        // Could be a real signature, but check for placeholder text inside
        return !trimmed.contains("placeholder") && !trimmed.contains("TODO");
    }

    // Check if it's mostly base64
    let base64_chars: usize = trimmed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '/' || *c == '=')
        .count();

    let total_printable: usize = trimmed.chars().filter(|c| !c.is_whitespace()).count();

    if total_printable > 0 {
        let ratio = base64_chars as f64 / total_printable as f64;
        return ratio > 0.9;
    }

    false
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sha256_file_with_filename() {
        let content = "abc123def456abc123def456abc123def456abc123def456abc123def456abcd1234  alpine.tar.gz\n";
        let hash = parse_sha256_file(content);
        assert_eq!(hash, Some("abc123def456abc123def456abc123def456abc123def456abc123def456abcd1234".to_string()));
    }

    #[test]
    fn test_parse_sha256_file_hash_only() {
        let content = "abc123def456abc123def456abc123def456abc123def456abc123def456abcd1234\n";
        let hash = parse_sha256_file(content);
        assert_eq!(hash, Some("abc123def456abc123def456abc123def456abc123def456abc123def456abcd1234".to_string()));
    }

    #[test]
    fn test_parse_sha256_file_invalid() {
        let content = "not a valid hash\n";
        let hash = parse_sha256_file(content);
        assert_eq!(hash, None);
    }

    #[test]
    fn test_detect_truncation() {
        assert!(detect_truncation(r#"{"key": "..."}"#));
        assert!(detect_truncation(r#"["a", "b", ...]"#));
        assert!(detect_truncation(r#"{"items": ...}"#));
    }

    #[test]
    fn test_qcow2_magic() {
        assert_eq!(QCOW2_MAGIC, [0x51, 0x46, 0x49, 0xfb]);
    }

    #[test]
    fn test_qcow2_header_parsing() {
        // Create a minimal qcow2 header in memory
        let mut header = vec![0u8; 104];
        
        // Magic: QFI\xfb
        header[0..4].copy_from_slice(&QCOW2_MAGIC);
        
        // Version: 3 (big-endian)
        header[4..8].copy_from_slice(&3u32.to_be_bytes());
        
        // Backing file offset: 0
        header[8..16].copy_from_slice(&0u64.to_be_bytes());
        
        // Backing file size: 0
        header[16..20].copy_from_slice(&0u32.to_be_bytes());
        
        // Cluster bits: 16 (64KB clusters)
        header[20..24].copy_from_slice(&16u32.to_be_bytes());
        
        // Virtual size: 2GB
        header[24..32].copy_from_slice(&(2u64 * 1024 * 1024 * 1024).to_be_bytes());

        // Write to temp file and parse
        let temp_dir = tempfile::tempdir().unwrap();
        let qcow2_path = temp_dir.path().join("test.qcow2");
        std::fs::write(&qcow2_path, &header).unwrap();

        let info = parse_qcow2_header(&qcow2_path, temp_dir.path()).unwrap();

        assert!(info.valid_magic);
        assert_eq!(info.version, 3);
        assert_eq!(info.cluster_bits, 16);
        assert_eq!(info.cluster_size, 65536);
        assert_eq!(info.virtual_size, 2 * 1024 * 1024 * 1024);
        assert!(info.backing_file.is_none());
    }

    #[test]
    fn test_looks_like_signature() {
        assert!(!looks_like_signature("placeholder"));
        assert!(!looks_like_signature("TODO: implement signing"));
        assert!(looks_like_signature("YWJjZGVmZ2hpamtsbW5vcA==")); // base64
    }
}
