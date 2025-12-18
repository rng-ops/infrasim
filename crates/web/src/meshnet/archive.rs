//! Archive builder utilities
//!
//! Re-exports the archive functionality from the appliance module.

pub use crate::meshnet::appliance::ApplianceService;

/// Deterministic manifest hashing for reproducible builds
pub fn compute_manifest_hash(entries: &[(String, String)]) -> String {
    use sha2::{Sha256, Digest};
    
    let mut hasher = Sha256::new();
    
    // Sort entries for deterministic ordering
    let mut sorted: Vec<_> = entries.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    
    for (path, hash) in sorted {
        hasher.update(path.as_bytes());
        hasher.update(b":");
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }
    
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_hash_deterministic() {
        let entries1 = vec![
            ("a.txt".to_string(), "hash1".to_string()),
            ("b.txt".to_string(), "hash2".to_string()),
        ];
        let entries2 = vec![
            ("b.txt".to_string(), "hash2".to_string()),
            ("a.txt".to_string(), "hash1".to_string()),
        ];
        
        // Order shouldn't matter
        assert_eq!(
            compute_manifest_hash(&entries1),
            compute_manifest_hash(&entries2)
        );
    }
    
    #[test]
    fn test_manifest_hash_changes() {
        let entries1 = vec![
            ("a.txt".to_string(), "hash1".to_string()),
        ];
        let entries2 = vec![
            ("a.txt".to_string(), "hash2".to_string()),
        ];
        
        // Different hashes should produce different manifest hash
        assert_ne!(
            compute_manifest_hash(&entries1),
            compute_manifest_hash(&entries2)
        );
    }
}
