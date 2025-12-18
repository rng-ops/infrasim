//! Identity provisioning service
//!
//! Handles async provisioning of:
//! - Subdomain DNS records
//! - Matrix account creation
//! - Storage bucket setup
//!
//! Uses a provider interface for future extensibility.

use crate::meshnet::db::{MeshnetDb, MeshnetIdentity, ProvisioningState};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Overall provisioning status for an identity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisioningStatus {
    pub subdomain: ProvisioningState,
    pub matrix: ProvisioningState,
    pub storage: ProvisioningState,
    pub all_active: bool,
    pub has_error: bool,
    pub last_error: Option<String>,
}

impl From<&MeshnetIdentity> for ProvisioningStatus {
    fn from(identity: &MeshnetIdentity) -> Self {
        let all_active = identity.status_subdomain == ProvisioningState::Active
            && identity.status_matrix == ProvisioningState::Active
            && identity.status_storage == ProvisioningState::Active;
        let has_error = identity.status_subdomain == ProvisioningState::Error
            || identity.status_matrix == ProvisioningState::Error
            || identity.status_storage == ProvisioningState::Error;
        Self {
            subdomain: identity.status_subdomain,
            matrix: identity.status_matrix,
            storage: identity.status_storage,
            all_active,
            has_error,
            last_error: identity.last_error.clone(),
        }
    }
}

// ============================================================================
// Provider interfaces
// ============================================================================

/// Subdomain DNS provider interface
#[async_trait]
pub trait SubdomainProvider: Send + Sync {
    async fn create_subdomain(&self, handle: &str, target: &str) -> Result<(), String>;
    async fn delete_subdomain(&self, handle: &str) -> Result<(), String>;
    async fn check_subdomain(&self, handle: &str) -> Result<bool, String>;
}

/// Matrix homeserver provider interface
#[async_trait]
pub trait MatrixProvider: Send + Sync {
    async fn create_user(&self, handle: &str) -> Result<String, String>; // Returns matrix_id
    async fn delete_user(&self, matrix_id: &str) -> Result<(), String>;
    async fn check_user(&self, matrix_id: &str) -> Result<bool, String>;
}

/// Storage provider interface
#[async_trait]
pub trait StorageProvider: Send + Sync {
    async fn create_bucket(&self, handle: &str) -> Result<String, String>; // Returns bucket path
    async fn delete_bucket(&self, handle: &str) -> Result<(), String>;
    async fn check_bucket(&self, handle: &str) -> Result<bool, String>;
}

// ============================================================================
// Stub implementations (MVP)
// ============================================================================

/// Stub subdomain provider that simulates provisioning
pub struct StubSubdomainProvider {
    pub base_domain: String,
    pub delay_ms: u64,
}

impl Default for StubSubdomainProvider {
    fn default() -> Self {
        Self {
            base_domain: std::env::var("BASE_DOMAIN")
                .unwrap_or_else(|_| "mesh.local".to_string()),
            delay_ms: 500,
        }
    }
}

#[async_trait]
impl SubdomainProvider for StubSubdomainProvider {
    async fn create_subdomain(&self, handle: &str, _target: &str) -> Result<(), String> {
        debug!("Stub: Creating subdomain {}.{}", handle, self.base_domain);
        tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
        info!("Stub: Subdomain {}.{} created", handle, self.base_domain);
        Ok(())
    }

    async fn delete_subdomain(&self, handle: &str) -> Result<(), String> {
        debug!("Stub: Deleting subdomain {}.{}", handle, self.base_domain);
        Ok(())
    }

    async fn check_subdomain(&self, handle: &str) -> Result<bool, String> {
        debug!("Stub: Checking subdomain {}.{}", handle, self.base_domain);
        Ok(true) // Always active in stub
    }
}

/// Stub Matrix provider
pub struct StubMatrixProvider {
    pub matrix_domain: String,
    pub delay_ms: u64,
}

impl Default for StubMatrixProvider {
    fn default() -> Self {
        Self {
            matrix_domain: std::env::var("MATRIX_DOMAIN")
                .unwrap_or_else(|_| "matrix.mesh.local".to_string()),
            delay_ms: 500,
        }
    }
}

#[async_trait]
impl MatrixProvider for StubMatrixProvider {
    async fn create_user(&self, handle: &str) -> Result<String, String> {
        debug!("Stub: Creating Matrix user @{}:{}", handle, self.matrix_domain);
        tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
        let matrix_id = format!("@{}:{}", handle, self.matrix_domain);
        info!("Stub: Matrix user {} created", matrix_id);
        Ok(matrix_id)
    }

    async fn delete_user(&self, matrix_id: &str) -> Result<(), String> {
        debug!("Stub: Deleting Matrix user {}", matrix_id);
        Ok(())
    }

    async fn check_user(&self, matrix_id: &str) -> Result<bool, String> {
        debug!("Stub: Checking Matrix user {}", matrix_id);
        Ok(true)
    }
}

/// Stub storage provider
pub struct StubStorageProvider {
    pub delay_ms: u64,
}

impl Default for StubStorageProvider {
    fn default() -> Self {
        Self { delay_ms: 500 }
    }
}

#[async_trait]
impl StorageProvider for StubStorageProvider {
    async fn create_bucket(&self, handle: &str) -> Result<String, String> {
        debug!("Stub: Creating storage bucket for {}", handle);
        tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
        let path = format!("/storage/{}", handle);
        info!("Stub: Storage bucket {} created", path);
        Ok(path)
    }

    async fn delete_bucket(&self, handle: &str) -> Result<(), String> {
        debug!("Stub: Deleting storage bucket for {}", handle);
        Ok(())
    }

    async fn check_bucket(&self, handle: &str) -> Result<bool, String> {
        debug!("Stub: Checking storage bucket for {}", handle);
        Ok(true)
    }
}

// ============================================================================
// Identity service
// ============================================================================

/// Identity provisioning service
pub struct IdentityService {
    db: MeshnetDb,
    subdomain_provider: Arc<dyn SubdomainProvider>,
    matrix_provider: Arc<dyn MatrixProvider>,
    storage_provider: Arc<dyn StorageProvider>,
    base_domain: String,
    matrix_domain: String,
    /// Active provisioning jobs (identity_id -> handle)
    active_jobs: RwLock<std::collections::HashMap<Uuid, tokio::task::JoinHandle<()>>>,
}

impl IdentityService {
    pub fn new(db: MeshnetDb) -> Self {
        let base_domain = std::env::var("BASE_DOMAIN")
            .unwrap_or_else(|_| "mesh.local".to_string());
        let matrix_domain = std::env::var("MATRIX_DOMAIN")
            .unwrap_or_else(|_| "matrix.mesh.local".to_string());

        Self {
            db,
            subdomain_provider: Arc::new(StubSubdomainProvider::default()),
            matrix_provider: Arc::new(StubMatrixProvider::default()),
            storage_provider: Arc::new(StubStorageProvider::default()),
            base_domain,
            matrix_domain,
            active_jobs: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Create a new identity for a user
    pub async fn create_identity(&self, user_id: Uuid, handle: &str) -> Result<MeshnetIdentity, String> {
        // Validate handle first
        let handle = crate::meshnet::handle::validate_handle(handle)
            .map_err(|e| e.to_string())?;
        
        // Check if handle is already taken
        if let Some(_existing) = self.db.get_identity_by_handle(&handle)? {
            return Err(format!("Handle '{}' is already taken", handle));
        }
        
        // Create the identity record
        let identity = self.db.create_identity(
            user_id,
            &handle,
            &self.base_domain,
            &self.matrix_domain,
        )?;
        
        Ok(identity)
    }

    /// Get identity for a user
    pub fn get_identity(&self, user_id: Uuid) -> Result<Option<MeshnetIdentity>, String> {
        self.db.get_identity_by_user(user_id)
    }

    /// Get provisioning status for an identity
    pub fn get_status(&self, user_id: Uuid) -> Result<Option<ProvisioningStatus>, String> {
        match self.db.get_identity_by_user(user_id)? {
            Some(identity) => Ok(Some(ProvisioningStatus::from(&identity))),
            None => Ok(None),
        }
    }

    /// Start provisioning job for an identity (idempotent)
    pub async fn start_provisioning(&self, user_id: Uuid) -> Result<(), String> {
        let identity = self.db.get_identity_by_user(user_id)?
            .ok_or_else(|| "No identity found for user".to_string())?;
        
        // Check if already fully provisioned
        if identity.status_subdomain == ProvisioningState::Active
            && identity.status_matrix == ProvisioningState::Active
            && identity.status_storage == ProvisioningState::Active
        {
            debug!("Identity {} already fully provisioned", identity.handle);
            return Ok(());
        }
        
        // Check if job already running
        {
            let jobs = self.active_jobs.read().await;
            if jobs.contains_key(&identity.id) {
                debug!("Provisioning job already running for {}", identity.handle);
                return Ok(());
            }
        }
        
        // Clone what we need for the async task
        let db = self.db.clone();
        let identity_id = identity.id;
        let handle = identity.handle.clone();
        let subdomain_provider = self.subdomain_provider.clone();
        let matrix_provider = self.matrix_provider.clone();
        let storage_provider = self.storage_provider.clone();
        
        info!("Starting provisioning job for {}", handle);
        
        let job = tokio::spawn(async move {
            // Provision subdomain
            if identity.status_subdomain != ProvisioningState::Active {
                match subdomain_provider.create_subdomain(&handle, "").await {
                    Ok(_) => {
                        let _ = db.update_identity_status(identity_id, Some(ProvisioningState::Active), None, None, None);
                    }
                    Err(e) => {
                        error!("Failed to provision subdomain for {}: {}", handle, e);
                        let _ = db.update_identity_status(identity_id, Some(ProvisioningState::Error), None, None, Some(&e));
                        return;
                    }
                }
            }
            
            // Provision Matrix account
            if identity.status_matrix != ProvisioningState::Active {
                match matrix_provider.create_user(&handle).await {
                    Ok(_) => {
                        let _ = db.update_identity_status(identity_id, None, Some(ProvisioningState::Active), None, None);
                    }
                    Err(e) => {
                        error!("Failed to provision Matrix for {}: {}", handle, e);
                        let _ = db.update_identity_status(identity_id, None, Some(ProvisioningState::Error), None, Some(&e));
                        return;
                    }
                }
            }
            
            // Provision storage
            if identity.status_storage != ProvisioningState::Active {
                match storage_provider.create_bucket(&handle).await {
                    Ok(_) => {
                        let _ = db.update_identity_status(identity_id, None, None, Some(ProvisioningState::Active), None);
                    }
                    Err(e) => {
                        error!("Failed to provision storage for {}: {}", handle, e);
                        let _ = db.update_identity_status(identity_id, None, None, Some(ProvisioningState::Error), Some(&e));
                        return;
                    }
                }
            }
            
            info!("Provisioning complete for {}", handle);
        });
        
        // Track the job
        {
            let mut jobs = self.active_jobs.write().await;
            jobs.insert(identity_id, job);
        }
        
        Ok(())
    }

    /// Check if provisioning is currently running for an identity
    pub async fn is_provisioning(&self, identity_id: Uuid) -> bool {
        let jobs = self.active_jobs.read().await;
        if let Some(handle) = jobs.get(&identity_id) {
            !handle.is_finished()
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use infrasim_common::Database;
    use crate::meshnet::db::MeshnetDb;

    fn test_service() -> IdentityService {
        let db = Database::open_memory().unwrap();
        let mdb = MeshnetDb::new(db);
        mdb.init_schema().unwrap();
        IdentityService::new(mdb)
    }

    #[tokio::test]
    async fn test_create_identity() {
        let svc = test_service();
        let user = svc.db.create_user(Some("test")).unwrap();
        
        let identity = svc.create_identity(user.id, "alice").await.unwrap();
        assert_eq!(identity.handle, "alice");
        assert_eq!(identity.status_subdomain, ProvisioningState::Pending);
    }

    #[tokio::test]
    async fn test_duplicate_handle() {
        let svc = test_service();
        let user1 = svc.db.create_user(Some("user1")).unwrap();
        let user2 = svc.db.create_user(Some("user2")).unwrap();
        
        svc.create_identity(user1.id, "alice").await.unwrap();
        let result = svc.create_identity(user2.id, "alice").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already taken"));
    }

    #[tokio::test]
    async fn test_invalid_handle() {
        let svc = test_service();
        let user = svc.db.create_user(Some("test")).unwrap();
        
        let result = svc.create_identity(user.id, "admin").await;
        assert!(result.is_err()); // Reserved
        
        let result = svc.create_identity(user.id, "ab").await;
        assert!(result.is_err()); // Too short
    }
}
