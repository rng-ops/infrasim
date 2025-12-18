//! State management for the daemon

use crate::config::DaemonConfig;
use infrasim_common::{
    cas::ContentAddressedStore,
    crypto::KeyPair,
    db::{Database, ResourceRow},
    types::*,
    Error, Result,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// State manager for all daemon resources
#[derive(Clone)]
pub struct StateManager {
    config: DaemonConfig,
    db: Database,
    cas: Arc<ContentAddressedStore>,
    key_pair: Arc<KeyPair>,
    /// Runtime state for running VMs (not persisted)
    vm_processes: Arc<RwLock<HashMap<String, VmProcess>>>,
}

/// Runtime state for a VM process
#[derive(Debug, Clone)]
pub struct VmProcess {
    pub vm_id: String,
    pub pid: u32,
    pub qmp_socket: String,
    pub vnc_port: Option<u16>,
    pub started_at: i64,
}

impl StateManager {
    /// Create a new state manager
    pub async fn new(config: &DaemonConfig) -> Result<Self> {
        // Initialize database
        let db = Database::open(config.db_path())?;

        // Initialize CAS
        let cas = ContentAddressedStore::new(config.cas_path()).await?;

        // Load or generate signing key
        let key_pair = if config.signing_key_path().exists() {
            KeyPair::load(config.signing_key_path()).await?
        } else {
            let kp = KeyPair::generate();
            kp.save(config.signing_key_path()).await?;
            info!("Generated new signing key");
            kp
        };

        info!("Signing key public: {}", key_pair.public_key_hex());

        Ok(Self {
            config: config.clone(),
            db,
            cas: Arc::new(cas),
            key_pair: Arc::new(key_pair),
            vm_processes: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get configuration
    pub fn config(&self) -> &DaemonConfig {
        &self.config
    }

    /// Get database
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Get CAS
    pub fn cas(&self) -> &ContentAddressedStore {
        &self.cas
    }

    /// Get key pair
    pub fn key_pair(&self) -> &KeyPair {
        &self.key_pair
    }

    // ========================================================================
    // VM operations
    // ========================================================================

    /// Create a new VM
    pub fn create_vm(&self, name: String, spec: VmSpec, labels: HashMap<String, String>) -> Result<Vm> {
        // Check if name is already taken
        if self.db.name_exists("vms", &name)? {
            return Err(Error::AlreadyExists {
                kind: "vm".to_string(),
                id: name,
            });
        }

        let meta = ResourceMeta::new(name).with_labels(labels);
        let status = VmStatus::default();

        self.db.insert("vms", &meta.id, &meta.name, &spec, &status, &meta.labels)?;

        debug!("Created VM: {} ({})", meta.name, meta.id);

        Ok(Vm { meta, spec, status })
    }

    /// Get a VM by ID
    pub fn get_vm(&self, id: &str) -> Result<Option<Vm>> {
        let row: Option<ResourceRow<VmSpec, VmStatus>> = self.db.get("vms", id)?;
        Ok(row.map(|r| Vm {
            meta: ResourceMeta {
                id: r.id,
                name: r.name,
                labels: r.labels,
                annotations: r.annotations,
                created_at: r.created_at,
                updated_at: r.updated_at,
                generation: r.generation,
            },
            spec: r.spec,
            status: r.status,
        }))
    }

    /// Get a VM by name
    pub fn get_vm_by_name(&self, name: &str) -> Result<Option<Vm>> {
        let row: Option<ResourceRow<VmSpec, VmStatus>> = self.db.get_by_name("vms", name)?;
        Ok(row.map(|r| Vm {
            meta: ResourceMeta {
                id: r.id,
                name: r.name,
                labels: r.labels,
                annotations: r.annotations,
                created_at: r.created_at,
                updated_at: r.updated_at,
                generation: r.generation,
            },
            spec: r.spec,
            status: r.status,
        }))
    }

    /// List all VMs
    pub fn list_vms(&self) -> Result<Vec<Vm>> {
        let rows: Vec<ResourceRow<VmSpec, VmStatus>> = self.db.list("vms")?;
        Ok(rows
            .into_iter()
            .map(|r| Vm {
                meta: ResourceMeta {
                    id: r.id,
                    name: r.name,
                    labels: r.labels,
                    annotations: r.annotations,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                    generation: r.generation,
                },
                spec: r.spec,
                status: r.status,
            })
            .collect())
    }

    /// Update VM spec
    pub fn update_vm_spec(&self, id: &str, spec: VmSpec) -> Result<()> {
        self.db.update("vms", id, Some(&spec), None::<&VmStatus>)
    }

    /// Update VM status
    pub fn update_vm_status(&self, id: &str, status: VmStatus) -> Result<()> {
        self.db.update("vms", id, None::<&VmSpec>, Some(&status))
    }

    /// Delete a VM
    pub fn delete_vm(&self, id: &str) -> Result<bool> {
        // Remove from runtime state
        self.vm_processes.write().remove(id);
        self.db.delete("vms", id)
    }

    /// Register a running VM process
    pub fn register_vm_process(&self, process: VmProcess) {
        self.vm_processes.write().insert(process.vm_id.clone(), process);
    }

    /// Get VM process
    pub fn get_vm_process(&self, vm_id: &str) -> Option<VmProcess> {
        self.vm_processes.read().get(vm_id).cloned()
    }

    /// Remove VM process
    pub fn remove_vm_process(&self, vm_id: &str) -> Option<VmProcess> {
        self.vm_processes.write().remove(vm_id)
    }

    /// List all running VM processes
    pub fn list_vm_processes(&self) -> Vec<VmProcess> {
        self.vm_processes.read().values().cloned().collect()
    }

    // ========================================================================
    // Network operations
    // ========================================================================

    /// Create a new network
    pub fn create_network(&self, name: String, spec: NetworkSpec, labels: HashMap<String, String>) -> Result<Network> {
        if self.db.name_exists("networks", &name)? {
            return Err(Error::AlreadyExists {
                kind: "network".to_string(),
                id: name,
            });
        }

        let meta = ResourceMeta::new(name).with_labels(labels);
        let status = NetworkStatus::default();

        self.db.insert("networks", &meta.id, &meta.name, &spec, &status, &meta.labels)?;

        Ok(Network { meta, spec, status })
    }

    /// Get a network by ID
    pub fn get_network(&self, id: &str) -> Result<Option<Network>> {
        let row: Option<ResourceRow<NetworkSpec, NetworkStatus>> = self.db.get("networks", id)?;
        Ok(row.map(|r| Network {
            meta: ResourceMeta {
                id: r.id,
                name: r.name,
                labels: r.labels,
                annotations: r.annotations,
                created_at: r.created_at,
                updated_at: r.updated_at,
                generation: r.generation,
            },
            spec: r.spec,
            status: r.status,
        }))
    }

    /// List all networks
    pub fn list_networks(&self) -> Result<Vec<Network>> {
        let rows: Vec<ResourceRow<NetworkSpec, NetworkStatus>> = self.db.list("networks")?;
        Ok(rows
            .into_iter()
            .map(|r| Network {
                meta: ResourceMeta {
                    id: r.id,
                    name: r.name,
                    labels: r.labels,
                    annotations: r.annotations,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                    generation: r.generation,
                },
                spec: r.spec,
                status: r.status,
            })
            .collect())
    }

    /// Delete a network
    pub fn delete_network(&self, id: &str) -> Result<bool> {
        self.db.delete("networks", id)
    }

    // ========================================================================
    // Volume operations
    // ========================================================================

    /// Create a new volume
    pub fn create_volume(&self, name: String, spec: VolumeSpec, labels: HashMap<String, String>) -> Result<Volume> {
        if self.db.name_exists("volumes", &name)? {
            return Err(Error::AlreadyExists {
                kind: "volume".to_string(),
                id: name,
            });
        }

        let meta = ResourceMeta::new(name).with_labels(labels);
        let status = VolumeStatus::default();

        self.db.insert("volumes", &meta.id, &meta.name, &spec, &status, &meta.labels)?;

        Ok(Volume { meta, spec, status })
    }

    /// Get a volume by ID
    pub fn get_volume(&self, id: &str) -> Result<Option<Volume>> {
        let row: Option<ResourceRow<VolumeSpec, VolumeStatus>> = self.db.get("volumes", id)?;
        Ok(row.map(|r| Volume {
            meta: ResourceMeta {
                id: r.id,
                name: r.name,
                labels: r.labels,
                annotations: r.annotations,
                created_at: r.created_at,
                updated_at: r.updated_at,
                generation: r.generation,
            },
            spec: r.spec,
            status: r.status,
        }))
    }

    /// List all volumes
    pub fn list_volumes(&self) -> Result<Vec<Volume>> {
        let rows: Vec<ResourceRow<VolumeSpec, VolumeStatus>> = self.db.list("volumes")?;
        Ok(rows
            .into_iter()
            .map(|r| Volume {
                meta: ResourceMeta {
                    id: r.id,
                    name: r.name,
                    labels: r.labels,
                    annotations: r.annotations,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                    generation: r.generation,
                },
                spec: r.spec,
                status: r.status,
            })
            .collect())
    }

    /// Update volume status
    pub fn update_volume_status(&self, id: &str, status: VolumeStatus) -> Result<()> {
        self.db.update("volumes", id, None::<&VolumeSpec>, Some(&status))
    }

    /// Delete a volume
    pub fn delete_volume(&self, id: &str) -> Result<bool> {
        self.db.delete("volumes", id)
    }

    // ========================================================================
    // QoS Profile operations
    // ========================================================================

    /// Create a new QoS profile
    pub fn create_qos_profile(&self, name: String, spec: QosProfileSpec, labels: HashMap<String, String>) -> Result<QosProfile> {
        if self.db.name_exists("qos_profiles", &name)? {
            return Err(Error::AlreadyExists {
                kind: "qos_profile".to_string(),
                id: name,
            });
        }

        let meta = ResourceMeta::new(name).with_labels(labels);
        // QoS profiles don't have status, use empty object
        let empty_status = serde_json::json!({});

        self.db.insert("qos_profiles", &meta.id, &meta.name, &spec, &empty_status, &meta.labels)?;

        Ok(QosProfile { meta, spec })
    }

    /// Get a QoS profile by ID
    pub fn get_qos_profile(&self, id: &str) -> Result<Option<QosProfile>> {
        let row: Option<ResourceRow<QosProfileSpec, serde_json::Value>> = self.db.get("qos_profiles", id)?;
        Ok(row.map(|r| QosProfile {
            meta: ResourceMeta {
                id: r.id,
                name: r.name,
                labels: r.labels,
                annotations: r.annotations,
                created_at: r.created_at,
                updated_at: r.updated_at,
                generation: r.generation,
            },
            spec: r.spec,
        }))
    }

    /// List all QoS profiles
    pub fn list_qos_profiles(&self) -> Result<Vec<QosProfile>> {
        let rows: Vec<ResourceRow<QosProfileSpec, serde_json::Value>> = self.db.list("qos_profiles")?;
        Ok(rows
            .into_iter()
            .map(|r| QosProfile {
                meta: ResourceMeta {
                    id: r.id,
                    name: r.name,
                    labels: r.labels,
                    annotations: r.annotations,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                    generation: r.generation,
                },
                spec: r.spec,
            })
            .collect())
    }

    /// Delete a QoS profile
    pub fn delete_qos_profile(&self, id: &str) -> Result<bool> {
        self.db.delete("qos_profiles", id)
    }

    // ========================================================================
    // Snapshot operations
    // ========================================================================

    /// Create a new snapshot
    pub fn create_snapshot(&self, name: String, spec: SnapshotSpec, labels: HashMap<String, String>) -> Result<Snapshot> {
        if self.db.name_exists("snapshots", &name)? {
            return Err(Error::AlreadyExists {
                kind: "snapshot".to_string(),
                id: name,
            });
        }

        let meta = ResourceMeta::new(name).with_labels(labels);
        let status = SnapshotStatus::default();

        self.db.insert("snapshots", &meta.id, &meta.name, &spec, &status, &meta.labels)?;

        Ok(Snapshot { meta, spec, status })
    }

    /// Get a snapshot by ID
    pub fn get_snapshot(&self, id: &str) -> Result<Option<Snapshot>> {
        let row: Option<ResourceRow<SnapshotSpec, SnapshotStatus>> = self.db.get("snapshots", id)?;
        Ok(row.map(|r| Snapshot {
            meta: ResourceMeta {
                id: r.id,
                name: r.name,
                labels: r.labels,
                annotations: r.annotations,
                created_at: r.created_at,
                updated_at: r.updated_at,
                generation: r.generation,
            },
            spec: r.spec,
            status: r.status,
        }))
    }

    /// List snapshots for a VM
    pub fn list_snapshots(&self, vm_id: Option<&str>) -> Result<Vec<Snapshot>> {
        let rows: Vec<ResourceRow<SnapshotSpec, SnapshotStatus>> = self.db.list("snapshots")?;
        Ok(rows
            .into_iter()
            .filter(|r| vm_id.map_or(true, |id| r.spec.vm_id == id))
            .map(|r| Snapshot {
                meta: ResourceMeta {
                    id: r.id,
                    name: r.name,
                    labels: r.labels,
                    annotations: r.annotations,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                    generation: r.generation,
                },
                spec: r.spec,
                status: r.status,
            })
            .collect())
    }

    /// Update snapshot status
    pub fn update_snapshot_status(&self, id: &str, status: SnapshotStatus) -> Result<()> {
        self.db.update("snapshots", id, None::<&SnapshotSpec>, Some(&status))
    }

    /// Delete a snapshot
    pub fn delete_snapshot(&self, id: &str) -> Result<bool> {
        self.db.delete("snapshots", id)
    }

    // ========================================================================
    // Console operations
    // ========================================================================

    /// Create a new console
    pub fn create_console(&self, name: String, spec: ConsoleSpec) -> Result<Console> {
        if self.db.name_exists("consoles", &name)? {
            return Err(Error::AlreadyExists {
                kind: "console".to_string(),
                id: name,
            });
        }

        let meta = ResourceMeta::new(name);
        let status = ConsoleStatus::default();

        self.db.insert("consoles", &meta.id, &meta.name, &spec, &status, &meta.labels)?;

        Ok(Console { meta, spec, status })
    }

    /// Get a console by ID
    pub fn get_console(&self, id: &str) -> Result<Option<Console>> {
        let row: Option<ResourceRow<ConsoleSpec, ConsoleStatus>> = self.db.get("consoles", id)?;
        Ok(row.map(|r| Console {
            meta: ResourceMeta {
                id: r.id,
                name: r.name,
                labels: r.labels,
                annotations: r.annotations,
                created_at: r.created_at,
                updated_at: r.updated_at,
                generation: r.generation,
            },
            spec: r.spec,
            status: r.status,
        }))
    }

    /// Update console status
    pub fn update_console_status(&self, id: &str, status: ConsoleStatus) -> Result<()> {
        self.db.update("consoles", id, None::<&ConsoleSpec>, Some(&status))
    }

    /// Delete a console
    pub fn delete_console(&self, id: &str) -> Result<bool> {
        self.db.delete("consoles", id)
    }
}
