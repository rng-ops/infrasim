//! InfraSim Common Library
//!
//! Shared types, utilities, and infrastructure for the InfraSim platform.

pub mod artifact;
pub mod cas;
pub mod crypto;
pub mod db;
pub mod error;
pub mod pipeline;
pub mod qmp;
pub mod types;
pub mod attestation;
pub mod traffic_shaper;

// Re-export commonly used types
pub use artifact::{ArtifactInspector, ArtifactInspectionReport};
pub use pipeline::{
    AnalysisReport, DependencyGraph, NetworkFingerprint, PipelineAnalyzer, TimingProbe,
};
pub use cas::ContentAddressedStore;
pub use crypto::{KeyPair, Signer, Verifier};
pub use db::Database;
pub use error::{Error, Result};
pub use types::*;

/// InfraSim version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default store path
pub fn default_store_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".infrasim")
}

/// Default socket path for the daemon
pub fn default_socket_path() -> std::path::PathBuf {
    default_store_path().join("daemon.sock")
}

/// Default database path
pub fn default_db_path() -> std::path::PathBuf {
    default_store_path().join("state.db")
}

/// Home directory helper
mod dirs {
    pub fn home_dir() -> Option<std::path::PathBuf> {
        std::env::var_os("HOME").map(std::path::PathBuf::from)
    }
}
