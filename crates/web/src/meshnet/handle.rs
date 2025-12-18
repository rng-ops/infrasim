//! Identity handle validation
//!
//! Handles must be:
//! - 3-32 characters
//! - Lowercase only
//! - [a-z0-9-] characters only
//! - No leading or trailing hyphens
//! - Not in the blocklist of reserved words

use std::collections::HashSet;
use once_cell::sync::Lazy;

/// Reserved handles that cannot be registered
static BLOCKLIST: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        // System/infrastructure
        "admin", "administrator", "root", "system", "sys",
        "support", "help", "helpdesk", "info", "contact",
        // Services
        "matrix", "element", "synapse", "homeserver",
        "www", "web", "api", "app", "apps",
        "mail", "email", "smtp", "imap", "pop",
        "dns", "ns", "ns1", "ns2", "nameserver",
        "ftp", "sftp", "ssh", "vpn", "wireguard", "wg",
        "git", "gitlab", "github", "svn",
        "ldap", "auth", "oauth", "sso", "login", "signin", "signup",
        // Networking
        "host", "localhost", "server", "node", "gateway", "router",
        "proxy", "nginx", "apache", "caddy",
        "mesh", "meshnet", "network", "net",
        // Storage
        "storage", "s3", "minio", "blob", "cdn", "static", "assets", "media",
        // Common subdomains
        "blog", "docs", "wiki", "forum", "status", "monitor",
        "dev", "staging", "prod", "production", "test", "testing",
        "demo", "preview", "beta", "alpha",
        // Reserved words
        "null", "undefined", "none", "void", "true", "false",
        "create", "delete", "update", "edit", "new",
        "user", "users", "account", "accounts", "profile", "profiles",
        "settings", "config", "configuration",
        // Security
        "security", "secure", "cert", "certificate", "ssl", "tls",
        "key", "keys", "token", "tokens", "secret", "secrets",
        // Abuse prevention
        "abuse", "spam", "phishing", "malware",
        "postmaster", "hostmaster", "webmaster",
        // Brand protection (customize as needed)
        "infrasim", "meshconsole", "meshnet-console",
    ]
    .into_iter()
    .collect()
});

/// Handle validation error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandleError {
    TooShort { min: usize, got: usize },
    TooLong { max: usize, got: usize },
    InvalidCharacter { position: usize, char: char },
    LeadingHyphen,
    TrailingHyphen,
    ConsecutiveHyphens,
    Reserved { handle: String },
}

impl std::fmt::Display for HandleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { min, got } => {
                write!(f, "Handle must be at least {} characters (got {})", min, got)
            }
            Self::TooLong { max, got } => {
                write!(f, "Handle must be at most {} characters (got {})", max, got)
            }
            Self::InvalidCharacter { position, char } => {
                write!(
                    f,
                    "Invalid character '{}' at position {}. Only lowercase letters, numbers, and hyphens are allowed.",
                    char, position
                )
            }
            Self::LeadingHyphen => {
                write!(f, "Handle cannot start with a hyphen")
            }
            Self::TrailingHyphen => {
                write!(f, "Handle cannot end with a hyphen")
            }
            Self::ConsecutiveHyphens => {
                write!(f, "Handle cannot contain consecutive hyphens")
            }
            Self::Reserved { handle } => {
                write!(f, "The handle '{}' is reserved and cannot be used", handle)
            }
        }
    }
}

impl std::error::Error for HandleError {}

/// Validate an identity handle
///
/// Returns Ok(normalized_handle) or Err(HandleError)
pub fn validate_handle(handle: &str) -> Result<String, HandleError> {
    const MIN_LEN: usize = 3;
    const MAX_LEN: usize = 32;

    // Normalize to lowercase
    let handle = handle.to_lowercase();
    let len = handle.len();

    // Length checks
    if len < MIN_LEN {
        return Err(HandleError::TooShort { min: MIN_LEN, got: len });
    }
    if len > MAX_LEN {
        return Err(HandleError::TooLong { max: MAX_LEN, got: len });
    }

    // Character validation
    for (i, c) in handle.chars().enumerate() {
        if !matches!(c, 'a'..='z' | '0'..='9' | '-') {
            return Err(HandleError::InvalidCharacter { position: i, char: c });
        }
    }

    // Hyphen rules
    if handle.starts_with('-') {
        return Err(HandleError::LeadingHyphen);
    }
    if handle.ends_with('-') {
        return Err(HandleError::TrailingHyphen);
    }
    if handle.contains("--") {
        return Err(HandleError::ConsecutiveHyphens);
    }

    // Blocklist check
    if BLOCKLIST.contains(handle.as_str()) {
        return Err(HandleError::Reserved { handle: handle.clone() });
    }

    Ok(handle)
}

/// Check if a handle is available (not reserved)
/// This doesn't check database uniqueness, only the blocklist
pub fn is_handle_available(handle: &str) -> bool {
    validate_handle(handle).is_ok()
}

/// Suggest similar available handles based on a rejected one
pub fn suggest_handles(base: &str) -> Vec<String> {
    let base = base.to_lowercase();
    let mut suggestions = Vec::new();
    
    // Try adding numbers
    for i in 1..=5 {
        let suggestion = format!("{}{}", base, i);
        if validate_handle(&suggestion).is_ok() {
            suggestions.push(suggestion);
        }
    }
    
    // Try common suffixes
    for suffix in ["dev", "me", "io", "net", "hub"] {
        let suggestion = format!("{}-{}", base, suffix);
        if validate_handle(&suggestion).is_ok() {
            suggestions.push(suggestion);
        }
    }
    
    suggestions.truncate(5);
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_handles() {
        assert_eq!(validate_handle("alice").unwrap(), "alice");
        assert_eq!(validate_handle("Alice").unwrap(), "alice"); // normalized
        assert_eq!(validate_handle("alice123").unwrap(), "alice123");
        assert_eq!(validate_handle("alice-bob").unwrap(), "alice-bob");
        assert_eq!(validate_handle("a1b").unwrap(), "a1b");
        assert_eq!(validate_handle("my-cool-handle").unwrap(), "my-cool-handle");
    }

    #[test]
    fn test_too_short() {
        assert!(matches!(
            validate_handle("ab"),
            Err(HandleError::TooShort { min: 3, got: 2 })
        ));
        assert!(matches!(
            validate_handle("a"),
            Err(HandleError::TooShort { min: 3, got: 1 })
        ));
    }

    #[test]
    fn test_too_long() {
        let long_handle = "a".repeat(33);
        assert!(matches!(
            validate_handle(&long_handle),
            Err(HandleError::TooLong { max: 32, got: 33 })
        ));
    }

    #[test]
    fn test_invalid_characters() {
        assert!(matches!(
            validate_handle("alice_bob"),
            Err(HandleError::InvalidCharacter { position: 5, char: '_' })
        ));
        assert!(matches!(
            validate_handle("alice.bob"),
            Err(HandleError::InvalidCharacter { position: 5, char: '.' })
        ));
        assert!(matches!(
            validate_handle("alice@bob"),
            Err(HandleError::InvalidCharacter { position: 5, char: '@' })
        ));
        assert!(matches!(
            validate_handle("alice bob"),
            Err(HandleError::InvalidCharacter { position: 5, char: ' ' })
        ));
    }

    #[test]
    fn test_hyphen_rules() {
        assert!(matches!(
            validate_handle("-alice"),
            Err(HandleError::LeadingHyphen)
        ));
        assert!(matches!(
            validate_handle("alice-"),
            Err(HandleError::TrailingHyphen)
        ));
        assert!(matches!(
            validate_handle("alice--bob"),
            Err(HandleError::ConsecutiveHyphens)
        ));
    }

    #[test]
    fn test_reserved_handles() {
        assert!(matches!(
            validate_handle("admin"),
            Err(HandleError::Reserved { .. })
        ));
        assert!(matches!(
            validate_handle("root"),
            Err(HandleError::Reserved { .. })
        ));
        assert!(matches!(
            validate_handle("ADMIN"), // case insensitive
            Err(HandleError::Reserved { .. })
        ));
        assert!(matches!(
            validate_handle("matrix"),
            Err(HandleError::Reserved { .. })
        ));
        assert!(matches!(
            validate_handle("www"),
            Err(HandleError::Reserved { .. })
        ));
    }

    #[test]
    fn test_suggestions() {
        let suggestions = suggest_handles("admin");
        assert!(!suggestions.is_empty());
        for s in &suggestions {
            assert!(validate_handle(s).is_ok());
        }
    }
}
