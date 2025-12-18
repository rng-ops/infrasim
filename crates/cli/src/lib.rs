//! InfraSim CLI
//!
//! Command-line interface for managing InfraSim virtual machines,
//! networks, and volumes.

pub mod commands;
pub mod client;
pub mod output;

mod generated {
    include!("generated/infrasim.v1.rs");
}

pub use generated::*;
