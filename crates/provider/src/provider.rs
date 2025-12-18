//! InfraSim Terraform Provider Implementation
//!
//! Implements the Terraform Plugin Protocol v6 Provider service.

use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tracing::{info, error, debug};

use crate::generated::tfplugin6::*;
use crate::generated::tfplugin6::provider_server::Provider;
use crate::client::DaemonClient;
use crate::schema;
use crate::state::{
    DynamicValue as LocalDynamicValue, decode_dynamic_value, encode_dynamic_value,
    get_string_attr,
};
use crate::resources::{Resource, network::NetworkResource, vm::VmResource, volume::VolumeResource, snapshot::SnapshotResource};

/// InfraSim Terraform Provider
pub struct InfraSimProvider {
    /// Client for communicating with the daemon
    client: Arc<RwLock<Option<DaemonClient>>>,
    /// Daemon address
    daemon_addr: Arc<RwLock<String>>,
}

impl InfraSimProvider {
    pub async fn new() -> anyhow::Result<Self> {
        Ok(Self {
            client: Arc::new(RwLock::new(None)),
            daemon_addr: Arc::new(RwLock::new("http://127.0.0.1:50051".to_string())),
        })
    }

    async fn get_client(&self) -> Result<DaemonClient, Status> {
        let addr = self.daemon_addr.read().await.clone();
        DaemonClient::connect(&addr).await
            .map_err(|e| Status::unavailable(format!("Cannot connect to daemon: {}", e)))
    }
}

#[tonic::async_trait]
impl Provider for InfraSimProvider {
    async fn get_provider_schema(
        &self,
        _request: Request<get_provider_schema::Request>,
    ) -> Result<Response<get_provider_schema::Response>, Status> {
        info!("GetProviderSchema called");

        let response = get_provider_schema::Response {
            provider: Some(schema::provider_schema()),
            resource_schemas: vec![
                ("infrasim_network".to_string(), schema::network_schema()),
                ("infrasim_vm".to_string(), schema::vm_schema()),
                ("infrasim_volume".to_string(), schema::volume_schema()),
                ("infrasim_snapshot".to_string(), schema::snapshot_schema()),
            ].into_iter().collect(),
            data_source_schemas: std::collections::HashMap::new(),
            diagnostics: vec![],
            provider_meta: None,
            server_capabilities: Some(ServerCapabilities {
                plan_destroy: true,
                get_provider_schema_optional: false,
                move_resource_state: false,
            }),
            functions: std::collections::HashMap::new(),
        };

        Ok(Response::new(response))
    }

    async fn validate_provider_config(
        &self,
        request: Request<validate_provider_config::Request>,
    ) -> Result<Response<validate_provider_config::Response>, Status> {
        debug!("ValidateProviderConfig called");

        let response = validate_provider_config::Response {
            diagnostics: vec![],
        };

        Ok(Response::new(response))
    }

    async fn validate_resource_config(
        &self,
        request: Request<validate_resource_config::Request>,
    ) -> Result<Response<validate_resource_config::Response>, Status> {
        debug!("ValidateResourceConfig called for {}", request.get_ref().type_name);

        let response = validate_resource_config::Response {
            diagnostics: vec![],
        };

        Ok(Response::new(response))
    }

    async fn validate_data_resource_config(
        &self,
        _request: Request<validate_data_resource_config::Request>,
    ) -> Result<Response<validate_data_resource_config::Response>, Status> {
        Ok(Response::new(validate_data_resource_config::Response {
            diagnostics: vec![],
        }))
    }

    async fn upgrade_resource_state(
        &self,
        request: Request<upgrade_resource_state::Request>,
    ) -> Result<Response<upgrade_resource_state::Response>, Status> {
        debug!("UpgradeResourceState called");

        // For now, return the state as-is
        let req = request.into_inner();
        
        Ok(Response::new(upgrade_resource_state::Response {
            upgraded_state: req.raw_state.map(|rs| DynamicValue {
                msgpack: rs.json, // Pass through
                json: vec![],
            }),
            diagnostics: vec![],
        }))
    }

    async fn configure_provider(
        &self,
        request: Request<configure_provider::Request>,
    ) -> Result<Response<configure_provider::Response>, Status> {
        info!("ConfigureProvider called");

        let req = request.into_inner();
        
        if let Some(config) = req.config {
            if let Ok(value) = decode_dynamic_value(&config.msgpack) {
                let addr = get_string_attr(&value, "daemon_address");
                if !addr.is_empty() {
                    *self.daemon_addr.write().await = addr;
                }
            }
        }

        // Test connection
        let addr = self.daemon_addr.read().await.clone();
        info!("Connecting to daemon at {}", addr);

        match DaemonClient::connect(&addr).await {
            Ok(client) => {
                *self.client.write().await = Some(client);
                info!("Connected to daemon successfully");
            }
            Err(e) => {
                error!("Failed to connect to daemon: {}", e);
                return Ok(Response::new(configure_provider::Response {
                    diagnostics: vec![Diagnostic {
                        severity: diagnostic::Severity::Error as i32,
                        summary: "Failed to connect to InfraSim daemon".to_string(),
                        detail: format!("Could not connect to {}: {}", addr, e),
                        attribute: None,
                    }],
                }));
            }
        }

        Ok(Response::new(configure_provider::Response {
            diagnostics: vec![],
        }))
    }

    async fn read_resource(
        &self,
        request: Request<read_resource::Request>,
    ) -> Result<Response<read_resource::Response>, Status> {
        let req = request.into_inner();
        info!("ReadResource called for {}", req.type_name);

        let mut client = self.get_client().await?;

        let current_state = req.current_state
            .and_then(|s| decode_dynamic_value(&s.msgpack).ok())
            .unwrap_or_default();

        let new_state = match req.type_name.as_str() {
            "infrasim_network" => NetworkResource::read(&mut client, &current_state).await,
            "infrasim_vm" => VmResource::read(&mut client, &current_state).await,
            "infrasim_volume" => VolumeResource::read(&mut client, &current_state).await,
            "infrasim_snapshot" => SnapshotResource::read(&mut client, &current_state).await,
            _ => return Err(Status::not_found(format!("Unknown resource type: {}", req.type_name))),
        };

        match new_state {
            Ok(state) => {
                let encoded = encode_dynamic_value(&state)
                    .map_err(|e| Status::internal(format!("Failed to encode state: {}", e)))?;

                Ok(Response::new(read_resource::Response {
                    new_state: Some(DynamicValue {
                        msgpack: encoded,
                        json: vec![],
                    }),
                    diagnostics: vec![],
                    private: vec![],
                    deferred: None,
                }))
            }
            Err(_e) => {
                // Resource not found - return null state
                Ok(Response::new(read_resource::Response {
                    new_state: None,
                    diagnostics: vec![],
                    private: vec![],
                    deferred: None,
                }))
            }
        }
    }

    async fn plan_resource_change(
        &self,
        request: Request<plan_resource_change::Request>,
    ) -> Result<Response<plan_resource_change::Response>, Status> {
        let req = request.into_inner();
        debug!("PlanResourceChange called for {}", req.type_name);

        // For planning, we generally return the proposed new state
        let proposed = req.proposed_new_state.clone();

        Ok(Response::new(plan_resource_change::Response {
            planned_state: proposed,
            requires_replace: vec![],
            planned_private: vec![],
            diagnostics: vec![],
            legacy_type_system: false,
            deferred: None,
        }))
    }

    async fn apply_resource_change(
        &self,
        request: Request<apply_resource_change::Request>,
    ) -> Result<Response<apply_resource_change::Response>, Status> {
        let req = request.into_inner();
        info!("ApplyResourceChange called for {}", req.type_name);

        let mut client = self.get_client().await?;

        let prior_state = req.prior_state
            .and_then(|s| decode_dynamic_value(&s.msgpack).ok());
        
        let planned_state = req.planned_state
            .and_then(|s| decode_dynamic_value(&s.msgpack).ok());

        let result = match (prior_state.as_ref(), planned_state.as_ref()) {
            // Create
            (None, Some(planned)) | (Some(LocalDynamicValue::Null), Some(planned)) => {
                match req.type_name.as_str() {
                    "infrasim_network" => NetworkResource::create(&mut client, planned).await,
                    "infrasim_vm" => VmResource::create(&mut client, planned).await,
                    "infrasim_volume" => VolumeResource::create(&mut client, planned).await,
                    "infrasim_snapshot" => SnapshotResource::create(&mut client, planned).await,
                    _ => return Err(Status::not_found(format!("Unknown resource type: {}", req.type_name))),
                }
            }
            // Delete
            (Some(prior), None) | (Some(prior), Some(LocalDynamicValue::Null)) => {
                let delete_result = match req.type_name.as_str() {
                    "infrasim_network" => NetworkResource::delete(&mut client, prior).await,
                    "infrasim_vm" => VmResource::delete(&mut client, prior).await,
                    "infrasim_volume" => VolumeResource::delete(&mut client, prior).await,
                    "infrasim_snapshot" => SnapshotResource::delete(&mut client, prior).await,
                    _ => return Err(Status::not_found(format!("Unknown resource type: {}", req.type_name))),
                };
                
                delete_result.map(|_| LocalDynamicValue::Null)
            }
            // Update
            (Some(prior), Some(planned)) => {
                match req.type_name.as_str() {
                    "infrasim_network" => NetworkResource::update(&mut client, prior, planned).await,
                    "infrasim_vm" => VmResource::update(&mut client, prior, planned).await,
                    "infrasim_volume" => VolumeResource::update(&mut client, prior, planned).await,
                    "infrasim_snapshot" => SnapshotResource::update(&mut client, prior, planned).await,
                    _ => return Err(Status::not_found(format!("Unknown resource type: {}", req.type_name))),
                }
            }
            // No change
            (None, None) => Ok(LocalDynamicValue::Null),
        };

        match result {
            Ok(new_state) => {
                let encoded = encode_dynamic_value(&new_state)
                    .map_err(|e| Status::internal(format!("Failed to encode state: {}", e)))?;

                Ok(Response::new(apply_resource_change::Response {
                    new_state: Some(DynamicValue {
                        msgpack: encoded,
                        json: vec![],
                    }),
                    private: vec![],
                    diagnostics: vec![],
                    legacy_type_system: false,
                }))
            }
            Err(e) => {
                Ok(Response::new(apply_resource_change::Response {
                    new_state: None,
                    private: vec![],
                    diagnostics: vec![Diagnostic {
                        severity: diagnostic::Severity::Error as i32,
                        summary: "Failed to apply resource change".to_string(),
                        detail: e.to_string(),
                        attribute: None,
                    }],
                    legacy_type_system: false,
                }))
            }
        }
    }

    async fn import_resource_state(
        &self,
        request: Request<import_resource_state::Request>,
    ) -> Result<Response<import_resource_state::Response>, Status> {
        let req = request.into_inner();
        info!("ImportResourceState called for {} with ID {}", req.type_name, req.id);

        let mut client = self.get_client().await?;

        // Create a minimal state with just the ID
        let initial_state = crate::state::make_state(vec![
            ("id", crate::state::string_value(&req.id)),
        ]);

        // Read the actual state
        let state = match req.type_name.as_str() {
            "infrasim_network" => NetworkResource::read(&mut client, &initial_state).await,
            "infrasim_vm" => VmResource::read(&mut client, &initial_state).await,
            "infrasim_volume" => VolumeResource::read(&mut client, &initial_state).await,
            "infrasim_snapshot" => SnapshotResource::read(&mut client, &initial_state).await,
            _ => return Err(Status::not_found(format!("Unknown resource type: {}", req.type_name))),
        };

        match state {
            Ok(s) => {
                let encoded = encode_dynamic_value(&s)
                    .map_err(|e| Status::internal(format!("Failed to encode state: {}", e)))?;

                Ok(Response::new(import_resource_state::Response {
                    imported_resources: vec![import_resource_state::ImportedResource {
                        type_name: req.type_name,
                        state: Some(DynamicValue {
                            msgpack: encoded,
                            json: vec![],
                        }),
                        private: vec![],
                    }],
                    diagnostics: vec![],
                    deferred: None,
                }))
            }
            Err(e) => {
                Ok(Response::new(import_resource_state::Response {
                    imported_resources: vec![],
                    diagnostics: vec![Diagnostic {
                        severity: diagnostic::Severity::Error as i32,
                        summary: "Failed to import resource".to_string(),
                        detail: e.to_string(),
                        attribute: None,
                    }],
                    deferred: None,
                }))
            }
        }
    }

    async fn move_resource_state(
        &self,
        _request: Request<move_resource_state::Request>,
    ) -> Result<Response<move_resource_state::Response>, Status> {
        Ok(Response::new(move_resource_state::Response {
            target_state: None,
            diagnostics: vec![],
        }))
    }

    async fn read_data_source(
        &self,
        _request: Request<read_data_source::Request>,
    ) -> Result<Response<read_data_source::Response>, Status> {
        Ok(Response::new(read_data_source::Response {
            state: None,
            diagnostics: vec![],
            deferred: None,
        }))
    }

    async fn get_functions(
        &self,
        _request: Request<get_functions::Request>,
    ) -> Result<Response<get_functions::Response>, Status> {
        Ok(Response::new(get_functions::Response {
            functions: std::collections::HashMap::new(),
            diagnostics: vec![],
        }))
    }

    async fn call_function(
        &self,
        _request: Request<call_function::Request>,
    ) -> Result<Response<call_function::Response>, Status> {
        Err(Status::unimplemented("Functions not implemented"))
    }

    async fn stop_provider(
        &self,
        _request: Request<stop_provider::Request>,
    ) -> Result<Response<stop_provider::Response>, Status> {
        info!("StopProvider called");
        Ok(Response::new(stop_provider::Response {
            error: String::new(),
        }))
    }
}
