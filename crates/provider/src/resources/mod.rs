//! Resource Implementations
//!
//! Implements the CRUD operations for each resource type.

pub mod network;
pub mod vm;
pub mod volume;
pub mod snapshot;

use anyhow::Result;
use crate::client::DaemonClient;
use crate::state::DynamicValue;

/// Trait for resource operations
#[async_trait::async_trait]
pub trait Resource {
    /// Resource type name
    fn type_name() -> &'static str;

    /// Create a new resource
    async fn create(client: &mut DaemonClient, config: &DynamicValue) -> Result<DynamicValue>;

    /// Read an existing resource
    async fn read(client: &mut DaemonClient, state: &DynamicValue) -> Result<DynamicValue>;

    /// Update an existing resource
    async fn update(client: &mut DaemonClient, state: &DynamicValue, config: &DynamicValue) -> Result<DynamicValue>;

    /// Delete a resource
    async fn delete(client: &mut DaemonClient, state: &DynamicValue) -> Result<()>;
}
