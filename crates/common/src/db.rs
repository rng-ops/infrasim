//! SQLite database for InfraSim state persistence

use crate::{Error, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};

/// Database wrapper for state persistence
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Expose the underlying connection for internal subsystems that need to manage
    /// their own tables within the shared state DB.
    pub fn connection(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }
}

impl Database {
    /// Open or create database at path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref())?;
        
        // Enable WAL mode for better concurrency
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        
        db.init_schema()?;
        
        info!("Opened database at {:?}", path.as_ref());
        Ok(db)
    }

    /// Open in-memory database (for testing)
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_schema()?;
        Ok(db)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock();
        
        conn.execute_batch(
            r#"
            -- VMs table
            CREATE TABLE IF NOT EXISTS vms (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                spec TEXT NOT NULL,
                status TEXT NOT NULL,
                labels TEXT NOT NULL DEFAULT '{}',
                annotations TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                generation INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_vms_name ON vms(name);

            -- Networks table
            CREATE TABLE IF NOT EXISTS networks (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                spec TEXT NOT NULL,
                status TEXT NOT NULL,
                labels TEXT NOT NULL DEFAULT '{}',
                annotations TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                generation INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_networks_name ON networks(name);

            -- QoS profiles table
            CREATE TABLE IF NOT EXISTS qos_profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                spec TEXT NOT NULL,
                labels TEXT NOT NULL DEFAULT '{}',
                annotations TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                generation INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_qos_profiles_name ON qos_profiles(name);

            -- Volumes table
            CREATE TABLE IF NOT EXISTS volumes (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                spec TEXT NOT NULL,
                status TEXT NOT NULL,
                labels TEXT NOT NULL DEFAULT '{}',
                annotations TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                generation INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_volumes_name ON volumes(name);

            -- Consoles table
            CREATE TABLE IF NOT EXISTS consoles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                spec TEXT NOT NULL,
                status TEXT NOT NULL,
                labels TEXT NOT NULL DEFAULT '{}',
                annotations TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                generation INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_consoles_name ON consoles(name);
            CREATE INDEX IF NOT EXISTS idx_consoles_vm ON consoles(json_extract(spec, '$.vm_id'));

            -- Snapshots table
            CREATE TABLE IF NOT EXISTS snapshots (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                spec TEXT NOT NULL,
                status TEXT NOT NULL,
                labels TEXT NOT NULL DEFAULT '{}',
                annotations TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                generation INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_snapshots_name ON snapshots(name);
            CREATE INDEX IF NOT EXISTS idx_snapshots_vm ON snapshots(json_extract(spec, '$.vm_id'));

            -- Appliance catalog (web-visible launchable entries)
            CREATE TABLE IF NOT EXISTS appliance_catalog (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                spec TEXT NOT NULL,
                status TEXT NOT NULL,
                labels TEXT NOT NULL DEFAULT '{}',
                annotations TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                generation INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_appliance_catalog_name ON appliance_catalog(name);

            -- Appliance events (audit trail / future indexing)
            CREATE TABLE IF NOT EXISTS appliance_events (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                spec TEXT NOT NULL,
                status TEXT NOT NULL,
                labels TEXT NOT NULL DEFAULT '{}',
                annotations TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                generation INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_appliance_events_name ON appliance_events(name);

            -- Benchmark runs table
            CREATE TABLE IF NOT EXISTS benchmark_runs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                spec TEXT NOT NULL,
                results TEXT NOT NULL DEFAULT '[]',
                receipt TEXT,
                attestation_id TEXT,
                labels TEXT NOT NULL DEFAULT '{}',
                annotations TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                generation INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_benchmark_runs_name ON benchmark_runs(name);
            CREATE INDEX IF NOT EXISTS idx_benchmark_runs_vm ON benchmark_runs(json_extract(spec, '$.vm_id'));

            -- Attestation reports table
            CREATE TABLE IF NOT EXISTS attestation_reports (
                id TEXT PRIMARY KEY,
                vm_id TEXT NOT NULL,
                host_provenance TEXT NOT NULL,
                digest TEXT NOT NULL,
                signature BLOB NOT NULL,
                created_at INTEGER NOT NULL,
                attestation_type TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_attestation_reports_vm ON attestation_reports(vm_id);

            -- LoRa devices table
            CREATE TABLE IF NOT EXISTS lora_devices (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                spec TEXT NOT NULL,
                status TEXT NOT NULL,
                labels TEXT NOT NULL DEFAULT '{}',
                annotations TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                generation INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_lora_devices_name ON lora_devices(name);
            CREATE INDEX IF NOT EXISTS idx_lora_devices_vm ON lora_devices(json_extract(spec, '$.vm_id'));

            -- Key-value store for misc state
            CREATE TABLE IF NOT EXISTS kv_store (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            "#,
        )?;
        
        debug!("Database schema initialized");
        Ok(())
    }

    // ========================================================================
    // Generic CRUD operations
    // ========================================================================

    /// Insert a resource
    pub fn insert<S: serde::Serialize, T: serde::Serialize>(
        &self,
        table: &str,
        id: &str,
        name: &str,
        spec: &S,
        status: &T,
        labels: &std::collections::HashMap<String, String>,
    ) -> Result<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp();
        
        conn.execute(
            &format!(
                "INSERT INTO {} (id, name, spec, status, labels, created_at, updated_at) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                table
            ),
            params![
                id,
                name,
                serde_json::to_string(spec)?,
                serde_json::to_string(status)?,
                serde_json::to_string(labels)?,
                now,
                now,
            ],
        )?;
        
        debug!("Inserted {} with id {}", table, id);
        Ok(())
    }

    /// Update a resource
    pub fn update<S: serde::Serialize, T: serde::Serialize>(
        &self,
        table: &str,
        id: &str,
        spec: Option<&S>,
        status: Option<&T>,
    ) -> Result<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp();
        
        if let Some(spec) = spec {
            conn.execute(
                &format!(
                    "UPDATE {} SET spec = ?1, updated_at = ?2, generation = generation + 1 WHERE id = ?3",
                    table
                ),
                params![serde_json::to_string(spec)?, now, id],
            )?;
        }
        
        if let Some(status) = status {
            conn.execute(
                &format!(
                    "UPDATE {} SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    table
                ),
                params![serde_json::to_string(status)?, now, id],
            )?;
        }
        
        debug!("Updated {} with id {}", table, id);
        Ok(())
    }

    /// Get a resource by ID
    pub fn get<S: serde::de::DeserializeOwned, T: serde::de::DeserializeOwned>(
        &self,
        table: &str,
        id: &str,
    ) -> Result<Option<ResourceRow<S, T>>> {
        let conn = self.conn.lock();
        
        let row = conn
            .query_row(
                &format!(
                    "SELECT id, name, spec, status, labels, annotations, created_at, updated_at, generation 
                     FROM {} WHERE id = ?1",
                    table
                ),
                params![id],
                |row| {
                    Ok(RawRow {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        spec: row.get(2)?,
                        status: row.get(3)?,
                        labels: row.get(4)?,
                        annotations: row.get(5)?,
                        created_at: row.get(6)?,
                        updated_at: row.get(7)?,
                        generation: row.get(8)?,
                    })
                },
            )
            .optional()?;
        
        match row {
            Some(raw) => Ok(Some(raw.parse()?)),
            None => Ok(None),
        }
    }

    /// Get a resource by name
    pub fn get_by_name<S: serde::de::DeserializeOwned, T: serde::de::DeserializeOwned>(
        &self,
        table: &str,
        name: &str,
    ) -> Result<Option<ResourceRow<S, T>>> {
        let conn = self.conn.lock();
        
        let row = conn
            .query_row(
                &format!(
                    "SELECT id, name, spec, status, labels, annotations, created_at, updated_at, generation 
                     FROM {} WHERE name = ?1",
                    table
                ),
                params![name],
                |row| {
                    Ok(RawRow {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        spec: row.get(2)?,
                        status: row.get(3)?,
                        labels: row.get(4)?,
                        annotations: row.get(5)?,
                        created_at: row.get(6)?,
                        updated_at: row.get(7)?,
                        generation: row.get(8)?,
                    })
                },
            )
            .optional()?;
        
        match row {
            Some(raw) => Ok(Some(raw.parse()?)),
            None => Ok(None),
        }
    }

    /// List all resources
    pub fn list<S: serde::de::DeserializeOwned, T: serde::de::DeserializeOwned>(
        &self,
        table: &str,
    ) -> Result<Vec<ResourceRow<S, T>>> {
        let conn = self.conn.lock();
        
        let mut stmt = conn.prepare(&format!(
            "SELECT id, name, spec, status, labels, annotations, created_at, updated_at, generation 
             FROM {} ORDER BY created_at DESC",
            table
        ))?;
        
        let rows = stmt.query_map([], |row| {
            Ok(RawRow {
                id: row.get(0)?,
                name: row.get(1)?,
                spec: row.get(2)?,
                status: row.get(3)?,
                labels: row.get(4)?,
                annotations: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                generation: row.get(8)?,
            })
        })?;
        
        let mut results = Vec::new();
        for row in rows {
            results.push(row?.parse()?);
        }
        
        Ok(results)
    }

    /// Delete a resource
    pub fn delete(&self, table: &str, id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let rows = conn.execute(
            &format!("DELETE FROM {} WHERE id = ?1", table),
            params![id],
        )?;
        
        if rows > 0 {
            debug!("Deleted {} with id {}", table, id);
        }
        
        Ok(rows > 0)
    }

    /// Check if a resource exists
    pub fn exists(&self, table: &str, id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {} WHERE id = ?1", table),
            params![id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Check if a name is taken
    pub fn name_exists(&self, table: &str, name: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {} WHERE name = ?1", table),
            params![name],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    // ========================================================================
    // Key-value store
    // ========================================================================

    /// Set a key-value pair
    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp();
        
        conn.execute(
            "INSERT OR REPLACE INTO kv_store (key, value, updated_at) VALUES (?1, ?2, ?3)",
            params![key, value, now],
        )?;
        
        Ok(())
    }

    /// Get a value by key
    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock();
        
        let value = conn
            .query_row(
                "SELECT value FROM kv_store WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        
        Ok(value)
    }

    /// Delete a key
    pub fn kv_delete(&self, key: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM kv_store WHERE key = ?1", params![key])?;
        Ok(())
    }
}

/// Raw database row before parsing
struct RawRow {
    id: String,
    name: String,
    spec: String,
    status: String,
    labels: String,
    annotations: String,
    created_at: i64,
    updated_at: i64,
    generation: i64,
}

impl RawRow {
    fn parse<S: serde::de::DeserializeOwned, T: serde::de::DeserializeOwned>(
        self,
    ) -> Result<ResourceRow<S, T>> {
        Ok(ResourceRow {
            id: self.id,
            name: self.name,
            spec: serde_json::from_str(&self.spec)?,
            status: serde_json::from_str(&self.status)?,
            labels: serde_json::from_str(&self.labels)?,
            annotations: serde_json::from_str(&self.annotations)?,
            created_at: self.created_at,
            updated_at: self.updated_at,
            generation: self.generation,
        })
    }
}

/// Parsed resource row
#[derive(Debug, Clone)]
pub struct ResourceRow<S, T> {
    pub id: String,
    pub name: String,
    pub spec: S,
    pub status: T,
    pub labels: std::collections::HashMap<String, String>,
    pub annotations: std::collections::HashMap<String, String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub generation: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestSpec {
        value: String,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestStatus {
        ready: bool,
    }

    #[test]
    fn test_crud() {
        let db = Database::open_memory().unwrap();
        
        // Create a test table
        {
            let conn = db.conn.lock();
            conn.execute_batch(
                r#"
                CREATE TABLE test_resources (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    spec TEXT NOT NULL,
                    status TEXT NOT NULL,
                    labels TEXT NOT NULL DEFAULT '{}',
                    annotations TEXT NOT NULL DEFAULT '{}',
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    generation INTEGER NOT NULL DEFAULT 1
                );
                "#,
            ).unwrap();
        }

        let spec = TestSpec { value: "test".to_string() };
        let status = TestStatus { ready: true };

        // Insert
        db.insert(
            "test_resources",
            "test-id",
            "test-name",
            &spec,
            &status,
            &std::collections::HashMap::new(),
        )
        .unwrap();

        // Get
        let row: ResourceRow<TestSpec, TestStatus> = db
            .get("test_resources", "test-id")
            .unwrap()
            .unwrap();
        assert_eq!(row.spec.value, "test");
        assert!(row.status.ready);

        // List
        let rows: Vec<ResourceRow<TestSpec, TestStatus>> = db.list("test_resources").unwrap();
        assert_eq!(rows.len(), 1);

        // Delete
        assert!(db.delete("test_resources", "test-id").unwrap());
        assert!(!db.exists("test_resources", "test-id").unwrap());
    }
}
