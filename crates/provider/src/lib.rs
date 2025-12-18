
//! InfraSim Terraform Provider
//!
//! This crate implements a Terraform provider for InfraSim using the
//! Terraform Plugin Protocol v6.

pub mod server;
pub mod provider;
pub mod resources;
pub mod schema;
pub mod state;
pub mod client;

mod generated {
    pub mod infrasim {
        include!("generated/infrasim.v1.rs");
    }
    pub mod tfplugin6 {
        include!("generated/tfplugin6.rs");
    }
}

pub use generated::infrasim;
pub use generated::tfplugin6;
