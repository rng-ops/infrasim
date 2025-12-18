//! Cryptographic utilities for InfraSim
//!
//! Provides Ed25519 signing/verification and key management.

use crate::{Error, Result};
use ed25519_dalek::{
    Signature, Signer as DalekSigner, SigningKey, Verifier as DalekVerifier, VerifyingKey,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

/// Ed25519 key pair for signing
#[derive(Clone)]
pub struct KeyPair {
    signing_key: SigningKey,
}

impl KeyPair {
    /// Generate a new random key pair
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Load key pair from file
    pub async fn load(path: impl AsRef<Path>) -> Result<Self> {
        let data = fs::read(path).await?;
        if data.len() != 32 {
            return Err(Error::Crypto("Invalid key length".to_string()));
        }
        let bytes: [u8; 32] = data.try_into().map_err(|_| {
            Error::Crypto("Invalid key length".to_string())
        })?;
        let signing_key = SigningKey::from_bytes(&bytes);
        Ok(Self { signing_key })
    }

    /// Save key pair to file
    pub async fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        fs::write(path, self.signing_key.to_bytes()).await?;
        Ok(())
    }

    /// Get the public key bytes
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Get the public key as hex
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public_key_bytes())
    }

    /// Get the verifying key
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }
}

impl std::fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyPair")
            .field("public_key", &self.public_key_hex())
            .finish()
    }
}

/// Trait for types that can sign data
pub trait Signer {
    /// Sign the given data
    fn sign(&self, data: &[u8]) -> Vec<u8>;
}

impl Signer for KeyPair {
    fn sign(&self, data: &[u8]) -> Vec<u8> {
        self.signing_key.sign(data).to_bytes().to_vec()
    }
}

/// Trait for types that can verify signatures
pub trait Verifier {
    /// Verify a signature
    fn verify(&self, data: &[u8], signature: &[u8]) -> Result<()>;
}

impl Verifier for VerifyingKey {
    fn verify(&self, data: &[u8], signature: &[u8]) -> Result<()> {
        if signature.len() != 64 {
            return Err(Error::Crypto("Invalid signature length".to_string()));
        }
        let sig_bytes: [u8; 64] = signature.try_into().map_err(|_| {
            Error::Crypto("Invalid signature length".to_string())
        })?;
        let sig = Signature::from_bytes(&sig_bytes);
        DalekVerifier::verify(self, data, &sig)?;
        Ok(())
    }
}

impl Verifier for KeyPair {
    fn verify(&self, data: &[u8], signature: &[u8]) -> Result<()> {
        Verifier::verify(&self.verifying_key(), data, signature)
    }
}

/// Create a verifying key from raw bytes
pub fn verifying_key_from_bytes(bytes: &[u8]) -> Result<VerifyingKey> {
    if bytes.len() != 32 {
        return Err(Error::Crypto("Invalid public key length".to_string()));
    }
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
        Error::Crypto("Invalid public key length".to_string())
    })?;
    VerifyingKey::from_bytes(&bytes).map_err(|e| Error::Crypto(e.to_string()))
}

/// Signed data wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedData<T> {
    pub data: T,
    #[serde(with = "hex_bytes")]
    pub signature: Vec<u8>,
    pub signer_public_key: String,
}

impl<T: Serialize> SignedData<T> {
    /// Create new signed data
    pub fn new(data: T, signer: &KeyPair) -> Result<Self> {
        let serialized = serde_json::to_vec(&data)?;
        let signature = signer.sign(&serialized);
        Ok(Self {
            data,
            signature,
            signer_public_key: signer.public_key_hex(),
        })
    }

    /// Verify the signature
    pub fn verify(&self) -> Result<()>
    where
        T: Serialize,
    {
        let public_key_bytes = hex::decode(&self.signer_public_key)
            .map_err(|e| Error::Crypto(format!("Invalid public key hex: {}", e)))?;
        let verifying_key = verifying_key_from_bytes(&public_key_bytes)?;
        let serialized = serde_json::to_vec(&self.data)?;
        Verifier::verify(&verifying_key, &serialized, &self.signature)
    }
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        hex::decode(&s).map_err(serde::de::Error::custom)
    }
}

/// Weight manifest for LLM weight volumes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightManifest {
    pub model_name: String,
    pub version: String,
    pub weight_files: Vec<WeightFile>,
    pub total_size: u64,
    pub format: String,
    pub expected_mount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightFile {
    pub path: String,
    pub digest: String,
    pub size: u64,
}

impl WeightManifest {
    /// Compute the manifest digest
    pub fn digest(&self) -> Result<String> {
        use sha2::{Digest, Sha256};
        let serialized = serde_json::to_vec(self)?;
        let mut hasher = Sha256::new();
        hasher.update(&serialized);
        Ok(hex::encode(hasher.finalize()))
    }

    /// Sign the manifest
    pub fn sign(&self, signer: &KeyPair) -> Result<SignedData<Self>> {
        SignedData::new(self.clone(), signer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let kp = KeyPair::generate();
        assert_eq!(kp.public_key_bytes().len(), 32);
    }

    #[test]
    fn test_sign_verify() {
        let kp = KeyPair::generate();
        let data = b"test message";
        let signature = kp.sign(data);
        assert!(kp.verify(data, &signature).is_ok());
    }

    #[test]
    fn test_signed_data() {
        let kp = KeyPair::generate();
        let data = "test data".to_string();
        let signed = SignedData::new(data.clone(), &kp).unwrap();
        assert!(signed.verify().is_ok());
    }

    #[test]
    fn test_tampered_signature() {
        let kp = KeyPair::generate();
        let data = b"test message";
        let mut signature = kp.sign(data);
        signature[0] ^= 0xff; // Tamper with signature
        assert!(kp.verify(data, &signature).is_err());
    }
}
