//! MDM mobileconfig profile generation and signing.
//!
//! This module provides:
//! - Self-signed CA and signing certificate generation
//! - .mobileconfig XML profile generation for VPN, WiFi, and network bridge configs
//! - PKCS#7 (CMS) signing of profiles for iOS/macOS
//! - Webhook delivery of signed profiles

use anyhow::{anyhow, Result};
use plist::Dictionary;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose,
    IsCa, KeyPair as RcgenKeyPair, KeyUsagePurpose, SanType, PKCS_RSA_SHA256,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

/// Configuration for the MDM signing chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MdmConfig {
    /// Organization name for certificates
    pub org_name: String,
    /// Domain for the organization (e.g., "example.com")
    pub domain: String,
    /// Path to store certificates
    pub cert_store_path: PathBuf,
}

impl Default for MdmConfig {
    fn default() -> Self {
        Self {
            org_name: "InfraSim".to_string(),
            domain: "infrasim.local".to_string(),
            cert_store_path: PathBuf::from("/tmp/infrasim-mdm"),
        }
    }
}

/// Signing chain: root CA -> intermediate -> signing cert
#[derive(Debug)]
pub struct SigningChain {
    pub root_ca_cert_pem: String,
    pub root_ca_key_pem: String,
    pub signing_cert_pem: String,
    pub signing_key_pem: String,
    /// Full chain (signing + root) for inclusion in signed payloads
    pub full_chain_pem: String,
}

impl SigningChain {
    /// Generate a new self-signed signing chain
    pub fn generate(config: &MdmConfig) -> Result<Self> {
        // Generate Root CA
        let mut root_params = CertificateParams::default();
        root_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        root_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        
        let mut root_dn = DistinguishedName::new();
        root_dn.push(DnType::OrganizationName, &config.org_name);
        root_dn.push(DnType::CommonName, format!("{} Root CA", config.org_name));
        root_params.distinguished_name = root_dn;
        
        // Use RSA for maximum iOS compatibility
        let root_key = RcgenKeyPair::generate_for(&PKCS_RSA_SHA256)?;
        let root_cert = root_params.self_signed(&root_key)?;

        // Generate Signing Certificate (signed by root)
        let mut sign_params = CertificateParams::default();
        sign_params.is_ca = IsCa::NoCa;
        sign_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        sign_params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::CodeSigning,
            ExtendedKeyUsagePurpose::EmailProtection,
        ];
        
        let mut sign_dn = DistinguishedName::new();
        sign_dn.push(DnType::OrganizationName, &config.org_name);
        sign_dn.push(DnType::CommonName, format!("{} Config Signing", config.org_name));
        sign_params.distinguished_name = sign_dn;
        sign_params.subject_alt_names = vec![
            SanType::DnsName(config.domain.clone().try_into().map_err(|e| anyhow!("Invalid domain: {:?}", e))?),
        ];

        let sign_key = RcgenKeyPair::generate_for(&PKCS_RSA_SHA256)?;
        let sign_cert = sign_params.signed_by(&sign_key, &root_cert, &root_key)?;

        let root_cert_pem = root_cert.pem();
        let root_key_pem = root_key.serialize_pem();
        let sign_cert_pem = sign_cert.pem();
        let sign_key_pem = sign_key.serialize_pem();

        let full_chain_pem = format!("{}\n{}", sign_cert_pem, root_cert_pem);

        Ok(Self {
            root_ca_cert_pem: root_cert_pem,
            root_ca_key_pem: root_key_pem,
            signing_cert_pem: sign_cert_pem,
            signing_key_pem: sign_key_pem,
            full_chain_pem,
        })
    }

    /// Load from disk or generate new
    pub async fn load_or_generate(config: &MdmConfig) -> Result<Self> {
        let root_cert_path = config.cert_store_path.join("root-ca.crt");
        let root_key_path = config.cert_store_path.join("root-ca.key");
        let sign_cert_path = config.cert_store_path.join("signing.crt");
        let sign_key_path = config.cert_store_path.join("signing.key");

        if root_cert_path.exists() && root_key_path.exists() 
            && sign_cert_path.exists() && sign_key_path.exists() 
        {
            info!("Loading existing MDM signing chain from {:?}", config.cert_store_path);
            let root_ca_cert_pem = tokio::fs::read_to_string(&root_cert_path).await?;
            let root_ca_key_pem = tokio::fs::read_to_string(&root_key_path).await?;
            let signing_cert_pem = tokio::fs::read_to_string(&sign_cert_path).await?;
            let signing_key_pem = tokio::fs::read_to_string(&sign_key_path).await?;
            let full_chain_pem = format!("{}\n{}", signing_cert_pem, root_ca_cert_pem);

            Ok(Self {
                root_ca_cert_pem,
                root_ca_key_pem,
                signing_cert_pem,
                signing_key_pem,
                full_chain_pem,
            })
        } else {
            info!("Generating new MDM signing chain");
            tokio::fs::create_dir_all(&config.cert_store_path).await?;
            
            let chain = Self::generate(config)?;
            
            tokio::fs::write(&root_cert_path, &chain.root_ca_cert_pem).await?;
            tokio::fs::write(&root_key_path, &chain.root_ca_key_pem).await?;
            tokio::fs::write(&sign_cert_path, &chain.signing_cert_pem).await?;
            tokio::fs::write(&sign_key_path, &chain.signing_key_pem).await?;
            
            info!("MDM signing chain saved to {:?}", config.cert_store_path);
            Ok(chain)
        }
    }
}

/// VPN configuration for mobileconfig
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnConfig {
    pub display_name: String,
    pub server: String,
    pub vpn_type: VpnType,
    /// Optional: shared secret for IKEv2/IPSec
    pub shared_secret: Option<String>,
    /// Optional: username (can be left blank for cert-based)
    pub username: Option<String>,
    /// On-demand rules
    pub on_demand: bool,
    /// On-demand SSID match (connect when NOT on these networks)
    pub trusted_ssids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VpnType {
    IKEv2,
    WireGuard,
    IPSec,
}

/// Bridge/network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub name: String,
    pub subnet: String,
    pub gateway: String,
    pub dns_servers: Vec<String>,
    /// Allowed peer endpoints
    pub peers: Vec<PeerEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerEndpoint {
    pub name: String,
    pub endpoint: String,
    pub public_key: Option<String>,
    pub allowed_ips: Vec<String>,
}

/// Profile request for generating mobileconfig
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileRequest {
    pub display_name: String,
    pub description: Option<String>,
    pub organization: String,
    pub identifier: String,
    pub vpn: Option<VpnConfig>,
    pub bridges: Vec<BridgeConfig>,
}

/// Generate a mobileconfig profile (unsigned XML plist)
pub fn generate_mobileconfig(req: &ProfileRequest) -> Result<Vec<u8>> {
    let mut root = Dictionary::new();
    let profile_uuid = Uuid::new_v4().to_string().to_uppercase();
    
    root.insert("PayloadDisplayName".into(), plist::Value::String(req.display_name.clone()));
    root.insert("PayloadDescription".into(), plist::Value::String(
        req.description.clone().unwrap_or_else(|| format!("{} Configuration", req.display_name))
    ));
    root.insert("PayloadIdentifier".into(), plist::Value::String(req.identifier.clone()));
    root.insert("PayloadOrganization".into(), plist::Value::String(req.organization.clone()));
    root.insert("PayloadType".into(), plist::Value::String("Configuration".into()));
    root.insert("PayloadUUID".into(), plist::Value::String(profile_uuid.clone()));
    root.insert("PayloadVersion".into(), plist::Value::Integer(plist::Integer::from(1)));
    root.insert("PayloadRemovalDisallowed".into(), plist::Value::Boolean(false));

    let mut payloads: Vec<plist::Value> = Vec::new();

    // Add VPN payload if configured
    if let Some(vpn) = &req.vpn {
        let vpn_payload = build_vpn_payload(vpn, &req.identifier)?;
        payloads.push(plist::Value::Dictionary(vpn_payload));
    }

    // Add bridge/network payloads as DNS settings
    for bridge in &req.bridges {
        let dns_payload = build_dns_payload(bridge, &req.identifier)?;
        payloads.push(plist::Value::Dictionary(dns_payload));
    }

    root.insert("PayloadContent".into(), plist::Value::Array(payloads));

    let mut buf = Vec::new();
    plist::to_writer_xml(&mut buf, &plist::Value::Dictionary(root))?;
    
    Ok(buf)
}

fn build_vpn_payload(vpn: &VpnConfig, base_id: &str) -> Result<Dictionary> {
    let mut payload = Dictionary::new();
    let vpn_uuid = Uuid::new_v4().to_string().to_uppercase();
    
    payload.insert("PayloadDisplayName".into(), plist::Value::String(vpn.display_name.clone()));
    payload.insert("PayloadIdentifier".into(), plist::Value::String(format!("{}.vpn", base_id)));
    payload.insert("PayloadType".into(), plist::Value::String("com.apple.vpn.managed".into()));
    payload.insert("PayloadUUID".into(), plist::Value::String(vpn_uuid));
    payload.insert("PayloadVersion".into(), plist::Value::Integer(plist::Integer::from(1)));
    payload.insert("UserDefinedName".into(), plist::Value::String(vpn.display_name.clone()));

    match vpn.vpn_type {
        VpnType::IKEv2 => {
            payload.insert("VPNType".into(), plist::Value::String("IKEv2".into()));
            
            let mut ikev2 = Dictionary::new();
            ikev2.insert("RemoteAddress".into(), plist::Value::String(vpn.server.clone()));
            ikev2.insert("RemoteIdentifier".into(), plist::Value::String(vpn.server.clone()));
            ikev2.insert("LocalIdentifier".into(), plist::Value::String(
                vpn.username.clone().unwrap_or_else(|| "client".into())
            ));
            ikev2.insert("AuthenticationMethod".into(), plist::Value::String(
                if vpn.shared_secret.is_some() { "SharedSecret".into() } else { "Certificate".into() }
            ));
            if let Some(secret) = &vpn.shared_secret {
                ikev2.insert("SharedSecret".into(), plist::Value::String(secret.clone()));
            }
            ikev2.insert("ExtendedAuthEnabled".into(), plist::Value::Boolean(false));
            ikev2.insert("IKESecurityAssociationParameters".into(), plist::Value::Dictionary({
                let mut sa = Dictionary::new();
                sa.insert("EncryptionAlgorithm".into(), plist::Value::String("AES-256-GCM".into()));
                sa.insert("IntegrityAlgorithm".into(), plist::Value::String("SHA2-256".into()));
                sa.insert("DiffieHellmanGroup".into(), plist::Value::Integer(plist::Integer::from(20)));
                sa
            }));
            ikev2.insert("ChildSecurityAssociationParameters".into(), plist::Value::Dictionary({
                let mut sa = Dictionary::new();
                sa.insert("EncryptionAlgorithm".into(), plist::Value::String("AES-256-GCM".into()));
                sa.insert("IntegrityAlgorithm".into(), plist::Value::String("SHA2-256".into()));
                sa.insert("DiffieHellmanGroup".into(), plist::Value::Integer(plist::Integer::from(20)));
                sa
            }));
            
            payload.insert("IKEv2".into(), plist::Value::Dictionary(ikev2));
        }
        VpnType::WireGuard => {
            // WireGuard requires a different payload type (newer iOS)
            payload.insert("VPNType".into(), plist::Value::String("VPN".into()));
            payload.insert("VPNSubType".into(), plist::Value::String("com.wireguard.ios".into()));
            // WireGuard config would typically be embedded differently
        }
        VpnType::IPSec => {
            payload.insert("VPNType".into(), plist::Value::String("IPSec".into()));
            
            let mut ipsec = Dictionary::new();
            ipsec.insert("RemoteAddress".into(), plist::Value::String(vpn.server.clone()));
            if let Some(secret) = &vpn.shared_secret {
                ipsec.insert("SharedSecret".into(), plist::Value::String(secret.clone()));
            }
            ipsec.insert("AuthenticationMethod".into(), plist::Value::String("SharedSecret".into()));
            
            payload.insert("IPSec".into(), plist::Value::Dictionary(ipsec));
        }
    }

    // On-demand configuration
    if vpn.on_demand {
        payload.insert("OnDemandEnabled".into(), plist::Value::Integer(plist::Integer::from(1)));
        
        let mut rules: Vec<plist::Value> = Vec::new();
        
        // Disconnect when on trusted SSIDs
        if !vpn.trusted_ssids.is_empty() {
            let mut disconnect_rule = Dictionary::new();
            disconnect_rule.insert("Action".into(), plist::Value::String("Disconnect".into()));
            disconnect_rule.insert("SSIDMatch".into(), plist::Value::Array(
                vpn.trusted_ssids.iter().map(|s| plist::Value::String(s.clone())).collect()
            ));
            rules.push(plist::Value::Dictionary(disconnect_rule));
        }
        
        // Connect otherwise
        let mut connect_rule = Dictionary::new();
        connect_rule.insert("Action".into(), plist::Value::String("Connect".into()));
        rules.push(plist::Value::Dictionary(connect_rule));
        
        payload.insert("OnDemandRules".into(), plist::Value::Array(rules));
    }

    Ok(payload)
}

fn build_dns_payload(bridge: &BridgeConfig, base_id: &str) -> Result<Dictionary> {
    let mut payload = Dictionary::new();
    let dns_uuid = Uuid::new_v4().to_string().to_uppercase();
    
    payload.insert("PayloadDisplayName".into(), plist::Value::String(format!("{} DNS", bridge.name)));
    payload.insert("PayloadIdentifier".into(), plist::Value::String(format!("{}.dns.{}", base_id, bridge.name.to_lowercase().replace(' ', "-"))));
    payload.insert("PayloadType".into(), plist::Value::String("com.apple.dnsSettings.managed".into()));
    payload.insert("PayloadUUID".into(), plist::Value::String(dns_uuid));
    payload.insert("PayloadVersion".into(), plist::Value::Integer(plist::Integer::from(1)));
    
    // DNS servers
    payload.insert("DNSSettings".into(), plist::Value::Dictionary({
        let mut dns = Dictionary::new();
        dns.insert("DNSProtocol".into(), plist::Value::String("Cleartext".into()));
        dns.insert("ServerAddresses".into(), plist::Value::Array(
            bridge.dns_servers.iter().map(|s| plist::Value::String(s.clone())).collect()
        ));
        dns
    }));

    Ok(payload)
}

/// Sign a mobileconfig with PKCS#7/CMS
/// Note: Full CMS signing is complex. For now, we provide the unsigned profile
/// and the signing chain for manual signing or use openssl.
pub fn sign_mobileconfig_openssl_command(
    unsigned_path: &str,
    signed_path: &str,
    cert_path: &str,
    key_path: &str,
    chain_path: &str,
) -> String {
    format!(
        "openssl smime -sign -signer {} -inkey {} -certfile {} -nodetach -outform der -in {} -out {}",
        cert_path, key_path, chain_path, unsigned_path, signed_path
    )
}

/// MDM state manager
pub struct MdmManager {
    pub config: MdmConfig,
    pub chain: Arc<RwLock<Option<SigningChain>>>,
    pub bridges: Arc<RwLock<Vec<BridgeConfig>>>,
    pub vpn_configs: Arc<RwLock<Vec<VpnConfig>>>,
}

impl MdmManager {
    pub fn new(config: MdmConfig) -> Self {
        Self {
            config,
            chain: Arc::new(RwLock::new(None)),
            bridges: Arc::new(RwLock::new(Vec::new())),
            vpn_configs: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn init(&self) -> Result<()> {
        let chain = SigningChain::load_or_generate(&self.config).await?;
        *self.chain.write().await = Some(chain);
        Ok(())
    }

    pub async fn get_root_ca_pem(&self) -> Option<String> {
        self.chain.read().await.as_ref().map(|c| c.root_ca_cert_pem.clone())
    }

    pub async fn add_bridge(&self, bridge: BridgeConfig) {
        self.bridges.write().await.push(bridge);
    }

    pub async fn add_vpn(&self, vpn: VpnConfig) {
        self.vpn_configs.write().await.push(vpn);
    }

    pub async fn list_bridges(&self) -> Vec<BridgeConfig> {
        self.bridges.read().await.clone()
    }

    pub async fn list_vpns(&self) -> Vec<VpnConfig> {
        self.vpn_configs.read().await.clone()
    }

    /// Generate a profile with all current configs
    pub async fn generate_profile(&self, name: &str) -> Result<Vec<u8>> {
        let bridges = self.bridges.read().await.clone();
        let vpns = self.vpn_configs.read().await.clone();
        
        let req = ProfileRequest {
            display_name: name.to_string(),
            description: Some(format!("{} network configuration", name)),
            organization: self.config.org_name.clone(),
            identifier: format!("{}.profile.{}", self.config.domain, name.to_lowercase().replace(' ', "-")),
            vpn: vpns.first().cloned(),
            bridges,
        };

        generate_mobileconfig(&req)
    }

    /// Get paths for signing
    pub fn signing_paths(&self) -> (PathBuf, PathBuf, PathBuf) {
        (
            self.config.cert_store_path.join("signing.crt"),
            self.config.cert_store_path.join("signing.key"),
            self.config.cert_store_path.join("root-ca.crt"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_mobileconfig() {
        let req = ProfileRequest {
            display_name: "Test Profile".into(),
            description: Some("A test profile".into()),
            organization: "Test Org".into(),
            identifier: "com.test.profile".into(),
            vpn: Some(VpnConfig {
                display_name: "Test VPN".into(),
                server: "vpn.example.com".into(),
                vpn_type: VpnType::IKEv2,
                shared_secret: Some("secret123".into()),
                username: Some("user".into()),
                on_demand: true,
                trusted_ssids: vec!["HomeWiFi".into()],
            }),
            bridges: vec![BridgeConfig {
                name: "Lab Network".into(),
                subnet: "10.0.0.0/24".into(),
                gateway: "10.0.0.1".into(),
                dns_servers: vec!["10.0.0.1".into(), "8.8.8.8".into()],
                peers: vec![],
            }],
        };

        let result = generate_mobileconfig(&req).unwrap();
        let xml = String::from_utf8(result).unwrap();
        assert!(xml.contains("Test Profile"));
        assert!(xml.contains("com.apple.vpn.managed"));
    }

    #[test]
    fn test_signing_chain_generation() {
        let config = MdmConfig::default();
        let chain = SigningChain::generate(&config).unwrap();
        assert!(chain.root_ca_cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(chain.signing_cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(chain.signing_key_pem.contains("BEGIN"));
    }
}
