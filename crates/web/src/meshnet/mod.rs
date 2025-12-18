//! Meshnet Console MVP
//! 
//! This module implements a simplified WireGuard mesh networking console with:
//! - WebAuthn/passkey authentication (no passwords)
//! - Identity handle provisioning (subdomain, Matrix, storage)
//! - Mesh peer management with WireGuard configs
//! - Appliance archive generation
//!
//! The design supports future providers (Tailscale) via the MeshProvider trait.

pub mod db;
pub mod handle;
pub mod identity;
pub mod mesh;
pub mod appliance;
pub mod archive;
pub mod routes;

pub use db::MeshnetDb;
pub use handle::validate_handle;
pub use identity::{IdentityService, ProvisioningStatus};
pub use mesh::{MeshProvider, WireGuardProvider, MeshPeer, PeerStatus};
pub use appliance::ApplianceService;
pub use archive::compute_manifest_hash;
pub use routes::meshnet_router;
