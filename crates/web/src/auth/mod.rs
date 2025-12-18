//! Modular authentication system with pluggable providers.
//!
//! Supports:
//! - Local TOTP (Google Authenticator compatible)
//! - WebAuthn (FIDO2 passkeys / hardware keys)
//! - OIDC (Keycloak, Auth0, etc.)
//!
//! All providers integrate with a unified RBAC system that can be
//! exported as Terraform resources for auditing.

pub mod provider;
pub mod rbac;
pub mod types;
// These modules require additional setup
// pub mod webauthn;
// pub mod oidc;
// pub mod middleware;

pub use provider::{AuthProvider, AuthProviderConfig, AuthManager, OidcConfig};
pub use rbac::{Role, Permission, Policy, PolicyEngine};
pub use types::*;
