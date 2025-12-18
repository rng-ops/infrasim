//! Content-Addressed Store (CAS) implementation
//!
//! Stores artifacts by their SHA-256 digest, providing:
//! - Deduplication
//! - Integrity verification
//! - Atomic writes

use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info, warn};

/// Content-addressed store for artifacts
#[derive(Debug, Clone)]
pub struct ContentAddressedStore {
    root: PathBuf,
}

impl ContentAddressedStore {
    /// Create a new CAS at the given root directory
    pub async fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        
        // Create directory structure
        fs::create_dir_all(root.join("objects")).await?;
        fs::create_dir_all(root.join("runs")).await?;
        fs::create_dir_all(root.join("tmp")).await?;
        
        info!("Initialized CAS at {:?}", root);
        
        Ok(Self { root })
    }

    /// Get the root path of the store
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the objects directory
    pub fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    /// Get the runs directory
    pub fn runs_dir(&self) -> PathBuf {
        self.root.join("runs")
    }

    /// Compute SHA-256 hash of data
    pub fn hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Compute SHA-256 hash of a file
    pub async fn hash_file(path: impl AsRef<Path>) -> Result<String> {
        let mut file = fs::File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer

        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        Ok(hex::encode(hasher.finalize()))
    }

    /// Get the path for an object by its digest
    pub fn object_path(&self, digest: &str) -> PathBuf {
        // Use first 2 chars as subdirectory for sharding
        let (prefix, _) = digest.split_at(2.min(digest.len()));
        self.objects_dir()
            .join("sha256")
            .join(prefix)
            .join(digest)
    }

    /// Check if an object exists
    pub async fn has(&self, digest: &str) -> bool {
        self.object_path(digest).exists()
    }

    /// Store data and return its digest
    pub async fn put(&self, data: &[u8]) -> Result<String> {
        let digest = Self::hash(data);
        
        if self.has(&digest).await {
            debug!("Object {} already exists", digest);
            return Ok(digest);
        }

        let path = self.object_path(&digest);
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write atomically via temp file
        let tmp_path = self.root.join("tmp").join(format!("{}.tmp", digest));
        fs::write(&tmp_path, data).await?;
        fs::rename(&tmp_path, &path).await?;

        debug!("Stored object {} ({} bytes)", digest, data.len());
        Ok(digest)
    }

    /// Store a file and return its digest
    pub async fn put_file(&self, src: impl AsRef<Path>) -> Result<String> {
        let digest = Self::hash_file(&src).await?;
        
        if self.has(&digest).await {
            debug!("Object {} already exists", digest);
            return Ok(digest);
        }

        let path = self.object_path(&digest);
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Copy atomically via temp file
        let tmp_path = self.root.join("tmp").join(format!("{}.tmp", digest));
        fs::copy(&src, &tmp_path).await?;
        fs::rename(&tmp_path, &path).await?;

        let size = fs::metadata(&path).await?.len();
        debug!("Stored object {} ({} bytes)", digest, size);
        Ok(digest)
    }

    /// Get data by digest
    pub async fn get(&self, digest: &str) -> Result<Vec<u8>> {
        let path = self.object_path(digest);
        
        if !path.exists() {
            return Err(Error::NotFound {
                kind: "object".to_string(),
                id: digest.to_string(),
            });
        }

        let data = fs::read(&path).await?;
        
        // Verify integrity
        let actual_digest = Self::hash(&data);
        if actual_digest != digest {
            return Err(Error::IntegrityError(format!(
                "Digest mismatch: expected {}, got {}",
                digest, actual_digest
            )));
        }

        Ok(data)
    }

    /// Get path to object (for memory-mapped access)
    pub async fn get_path(&self, digest: &str) -> Result<PathBuf> {
        let path = self.object_path(digest);
        
        if !path.exists() {
            return Err(Error::NotFound {
                kind: "object".to_string(),
                id: digest.to_string(),
            });
        }

        Ok(path)
    }

    /// Delete an object by digest
    pub async fn delete(&self, digest: &str) -> Result<()> {
        let path = self.object_path(digest);
        
        if path.exists() {
            fs::remove_file(&path).await?;
            debug!("Deleted object {}", digest);
        }

        Ok(())
    }

    /// Create a run directory and return its path
    pub async fn create_run(&self, run_id: &str) -> Result<PathBuf> {
        let run_dir = self.runs_dir().join(run_id);
        fs::create_dir_all(&run_dir).await?;
        debug!("Created run directory: {:?}", run_dir);
        Ok(run_dir)
    }

    /// Store a run artifact
    pub async fn put_run_artifact(
        &self,
        run_id: &str,
        name: &str,
        data: &[u8],
    ) -> Result<String> {
        let run_dir = self.runs_dir().join(run_id);
        fs::create_dir_all(&run_dir).await?;

        let path = run_dir.join(name);
        fs::write(&path, data).await?;

        let digest = Self::hash(data);
        debug!("Stored run artifact {}/{} (digest: {})", run_id, name, digest);
        Ok(digest)
    }

    /// Get a run artifact
    pub async fn get_run_artifact(&self, run_id: &str, name: &str) -> Result<Vec<u8>> {
        let path = self.runs_dir().join(run_id).join(name);
        
        if !path.exists() {
            return Err(Error::NotFound {
                kind: "run artifact".to_string(),
                id: format!("{}/{}", run_id, name),
            });
        }

        Ok(fs::read(&path).await?)
    }

    /// List all runs
    pub async fn list_runs(&self) -> Result<Vec<String>> {
        let runs_dir = self.runs_dir();
        let mut runs = Vec::new();

        if runs_dir.exists() {
            let mut entries = fs::read_dir(&runs_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                if entry.file_type().await?.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        runs.push(name.to_string());
                    }
                }
            }
        }

        Ok(runs)
    }

    /// Store encrypted memory dump
    pub async fn put_memory_dump(
        &self,
        run_id: &str,
        data: &[u8],
        encryption_key: &[u8; 32],
    ) -> Result<String> {
        use sha2::Sha256;
        
        // Simple XOR encryption with key derivation (in production, use ChaCha20-Poly1305)
        let mut encrypted = Vec::with_capacity(data.len());
        let mut key_stream = Sha256::new();
        key_stream.update(encryption_key);
        
        for (i, &byte) in data.iter().enumerate() {
            let key_byte = encryption_key[i % 32];
            encrypted.push(byte ^ key_byte);
        }

        self.put_run_artifact(run_id, "snapshot.mem.enc", &encrypted).await
    }

    /// Garbage collect unreferenced objects
    pub async fn gc(&self, referenced: &[String]) -> Result<GcStats> {
        let mut stats = GcStats::default();
        let objects_dir = self.objects_dir().join("sha256");

        if !objects_dir.exists() {
            return Ok(stats);
        }

        let referenced_set: std::collections::HashSet<_> = referenced.iter().collect();

        // Walk all objects
        for entry in walkdir::WalkDir::new(&objects_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                if let Some(digest) = entry.file_name().to_str() {
                    stats.total_objects += 1;
                    stats.total_bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);

                    if !referenced_set.contains(&digest.to_string()) {
                        if let Err(e) = fs::remove_file(entry.path()).await {
                            warn!("Failed to delete unreferenced object {}: {}", digest, e);
                        } else {
                            stats.deleted_objects += 1;
                            stats.deleted_bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
                        }
                    }
                }
            }
        }

        info!(
            "GC complete: deleted {}/{} objects ({} bytes freed)",
            stats.deleted_objects, stats.total_objects, stats.deleted_bytes
        );

        Ok(stats)
    }
}

/// Garbage collection statistics
#[derive(Debug, Default)]
pub struct GcStats {
    pub total_objects: usize,
    pub total_bytes: u64,
    pub deleted_objects: usize,
    pub deleted_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_put_get() {
        let tmp = TempDir::new().unwrap();
        let cas = ContentAddressedStore::new(tmp.path()).await.unwrap();

        let data = b"hello world";
        let digest = cas.put(data).await.unwrap();

        assert!(cas.has(&digest).await);

        let retrieved = cas.get(&digest).await.unwrap();
        assert_eq!(data.as_slice(), retrieved.as_slice());
    }

    #[tokio::test]
    async fn test_deduplication() {
        let tmp = TempDir::new().unwrap();
        let cas = ContentAddressedStore::new(tmp.path()).await.unwrap();

        let data = b"duplicate data";
        let digest1 = cas.put(data).await.unwrap();
        let digest2 = cas.put(data).await.unwrap();

        assert_eq!(digest1, digest2);
    }

    #[tokio::test]
    async fn test_integrity_check() {
        let tmp = TempDir::new().unwrap();
        let cas = ContentAddressedStore::new(tmp.path()).await.unwrap();

        let data = b"test data";
        let digest = cas.put(data).await.unwrap();

        // Corrupt the file
        let path = cas.object_path(&digest);
        fs::write(&path, b"corrupted").await.unwrap();

        // Should fail integrity check
        assert!(cas.get(&digest).await.is_err());
    }
}
