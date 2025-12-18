//! InfraSim Web Console
//!
//! Provides a web-based console for accessing VMs via noVNC.

pub mod server;
pub mod vnc_proxy;
pub mod static_files;
pub mod mdm;
pub mod auth;
pub mod docker;
pub mod meshnet;

/// Generated gRPC client for InfraSim daemon.
pub mod generated {
    pub mod infrasim {
        include!("generated/infrasim.v1.rs");
    }
}

pub use server::WebServer;
pub use mdm::{MdmManager, MdmConfig, BridgeConfig, VpnConfig, VpnType, PeerEndpoint, ProfileRequest};
pub use auth::{AuthManager, AuthProviderConfig, Permission, Policy, PolicyEngine, Role};
pub use docker::{ContainerManager, ContainerImage, ApplianceBuildSpec, NetworkInterface, ImageOverlay};
