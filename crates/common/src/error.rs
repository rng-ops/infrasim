//! Error types for InfraSim

use thiserror::Error;

/// Result type alias using InfraSim Error
pub type Result<T> = std::result::Result<T, Error>;

/// InfraSim error types
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("QEMU error: {0}")]
    Qemu(String),

    #[error("QMP error: {0}")]
    Qmp(String),

    #[error("Resource not found: {kind} with id {id}")]
    NotFound { kind: String, id: String },

    #[error("Resource already exists: {kind} with id {id}")]
    AlreadyExists { kind: String, id: String },

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Integrity verification failed: {0}")]
    IntegrityError(String),

    #[error("Attestation error: {0}")]
    AttestationError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Volume error: {0}")]
    VolumeError(String),

    #[error("Snapshot error: {0}")]
    SnapshotError(String),

    #[error("Benchmark error: {0}")]
    BenchmarkError(String),

    #[error("Console error: {0}")]
    ConsoleError(String),

    #[error("Invalid state transition: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("Operation timeout after {seconds}s")]
    Timeout { seconds: u64 },

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("HVF not available on this system")]
    HvfNotAvailable,

    #[error("QEMU not found at expected path")]
    QemuNotFound,

    #[error("Unsupported architecture: {0}")]
    UnsupportedArch(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<ed25519_dalek::SignatureError> for Error {
    fn from(e: ed25519_dalek::SignatureError) -> Self {
        Error::Crypto(e.to_string())
    }
}

impl From<Error> for tonic::Status {
    fn from(e: Error) -> Self {
        match e {
            Error::NotFound { kind, id } => {
                tonic::Status::not_found(format!("{} {} not found", kind, id))
            }
            Error::AlreadyExists { kind, id } => {
                tonic::Status::already_exists(format!("{} {} already exists", kind, id))
            }
            Error::InvalidConfig(msg) => tonic::Status::invalid_argument(msg),
            Error::PermissionDenied(msg) => tonic::Status::permission_denied(msg),
            Error::Timeout { seconds } => {
                tonic::Status::deadline_exceeded(format!("Operation timed out after {}s", seconds))
            }
            _ => tonic::Status::internal(e.to_string()),
        }
    }
}
