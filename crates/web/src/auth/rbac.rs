//! Role-Based Access Control (RBAC) system.
//!
//! Policies are defined as Terraform-addressable resources that can be
//! audited and tested.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A role that can be assigned to identities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// Unique role identifier (e.g., "admin", "operator", "viewer")
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description
    pub description: String,
    /// Permissions granted by this role
    pub permissions: Vec<String>,
    /// Parent roles (for inheritance)
    #[serde(default)]
    pub inherits: Vec<String>,
    /// Terraform resource address
    pub terraform_address: Option<String>,
}

/// A specific permission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    /// Unique permission identifier (e.g., "vm:create", "vm:delete")
    pub id: String,
    /// Resource type this permission applies to
    pub resource: String,
    /// Action (create, read, update, delete, execute)
    pub action: String,
    /// Optional scope constraints
    #[serde(default)]
    pub scope: Option<PermissionScope>,
}

/// Scope constraints for a permission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionScope {
    /// Workspace IDs this permission applies to (empty = all)
    #[serde(default)]
    pub workspaces: Vec<String>,
    /// Resource tags that must match
    #[serde(default)]
    pub tags: HashMap<String, String>,
    /// Time-based constraints (ISO8601 duration)
    pub valid_until: Option<String>,
}

/// A policy document (Terraform-addressable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    /// Policy ID
    pub id: String,
    /// Policy name
    pub name: String,
    /// Description
    pub description: String,
    /// Version for tracking changes
    pub version: String,
    /// Roles defined in this policy
    pub roles: Vec<Role>,
    /// Created timestamp
    pub created_at: i64,
    /// Last modified timestamp
    pub updated_at: i64,
    /// Terraform resource address
    pub terraform_address: String,
}

impl Policy {
    /// Generate Terraform HCL for this policy
    pub fn to_terraform_hcl(&self) -> String {
        let mut hcl = format!(
            r#"# InfraSim RBAC Policy: {}
# Generated from policy version {}

resource "infrasim_rbac_policy" "{}" {{
  name        = "{}"
  description = "{}"
  version     = "{}"

"#,
            self.name, self.version, self.id, self.name, self.description, self.version
        );

        for role in &self.roles {
            hcl.push_str(&format!(
                r#"  role {{
    id          = "{}"
    name        = "{}"
    description = "{}"
    permissions = [{}]
{}  }}

"#,
                role.id,
                role.name,
                role.description,
                role.permissions
                    .iter()
                    .map(|p| format!("\"{}\"", p))
                    .collect::<Vec<_>>()
                    .join(", "),
                if role.inherits.is_empty() {
                    String::new()
                } else {
                    format!(
                        "    inherits    = [{}]\n",
                        role.inherits
                            .iter()
                            .map(|r| format!("\"{}\"", r))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            ));
        }

        hcl.push_str("}\n");
        hcl
    }
}

/// Policy engine for evaluating permissions
pub struct PolicyEngine {
    /// Loaded policies
    policies: Vec<Policy>,
    /// Compiled role -> permissions map
    role_permissions: HashMap<String, HashSet<String>>,
    /// All known permissions
    all_permissions: HashSet<String>,
}

impl PolicyEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            policies: Vec::new(),
            role_permissions: HashMap::new(),
            all_permissions: HashSet::new(),
        };
        engine.load_default_policy();
        engine
    }

    /// Load the default built-in policy
    fn load_default_policy(&mut self) {
        let default_policy = Policy {
            id: "default".to_string(),
            name: "Default InfraSim Policy".to_string(),
            description: "Built-in default RBAC policy".to_string(),
            version: "1.0.0".to_string(),
            roles: vec![
                Role {
                    id: "admin".to_string(),
                    name: "Administrator".to_string(),
                    description: "Full system access".to_string(),
                    permissions: vec!["*".to_string()],
                    inherits: vec![],
                    terraform_address: Some("infrasim_rbac_role.admin".to_string()),
                },
                Role {
                    id: "operator".to_string(),
                    name: "Operator".to_string(),
                    description: "Can manage VMs and appliances".to_string(),
                    permissions: vec![
                        "vm:create".to_string(),
                        "vm:read".to_string(),
                        "vm:update".to_string(),
                        "vm:start".to_string(),
                        "vm:stop".to_string(),
                        "vm:console".to_string(),
                        "appliance:read".to_string(),
                        "appliance:create".to_string(),
                        "appliance:boot".to_string(),
                        "appliance:stop".to_string(),
                        "image:read".to_string(),
                        "image:pull".to_string(),
                        "network:read".to_string(),
                        "config:read".to_string(),
                    ],
                    inherits: vec!["viewer".to_string()],
                    terraform_address: Some("infrasim_rbac_role.operator".to_string()),
                },
                Role {
                    id: "viewer".to_string(),
                    name: "Viewer".to_string(),
                    description: "Read-only access".to_string(),
                    permissions: vec![
                        "vm:read".to_string(),
                        "appliance:read".to_string(),
                        "image:read".to_string(),
                        "network:read".to_string(),
                        "config:read".to_string(),
                        "audit:read".to_string(),
                    ],
                    inherits: vec![],
                    terraform_address: Some("infrasim_rbac_role.viewer".to_string()),
                },
                Role {
                    id: "builder".to_string(),
                    name: "Image Builder".to_string(),
                    description: "Can build and manage images".to_string(),
                    permissions: vec![
                        "image:read".to_string(),
                        "image:pull".to_string(),
                        "image:build".to_string(),
                        "image:push".to_string(),
                        "image:delete".to_string(),
                        "overlay:create".to_string(),
                        "overlay:read".to_string(),
                        "overlay:delete".to_string(),
                    ],
                    inherits: vec!["viewer".to_string()],
                    terraform_address: Some("infrasim_rbac_role.builder".to_string()),
                },
            ],
            created_at: 0,
            updated_at: 0,
            terraform_address: "infrasim_rbac_policy.default".to_string(),
        };

        self.add_policy(default_policy);
    }

    /// Add a policy and recompile permissions
    pub fn add_policy(&mut self, policy: Policy) {
        // Register all permissions
        for role in &policy.roles {
            for perm in &role.permissions {
                self.all_permissions.insert(perm.clone());
            }
        }

        self.policies.push(policy);
        self.compile_permissions();
    }

    /// Compile role -> permission mappings with inheritance
    fn compile_permissions(&mut self) {
        self.role_permissions.clear();

        // First pass: direct permissions
        for policy in &self.policies {
            for role in &policy.roles {
                let perms = self
                    .role_permissions
                    .entry(role.id.clone())
                    .or_insert_with(HashSet::new);
                for perm in &role.permissions {
                    perms.insert(perm.clone());
                }
            }
        }

        // Second pass: resolve inheritance (simple single-level for now)
        let roles: Vec<_> = self.policies.iter().flat_map(|p| p.roles.clone()).collect();
        for role in &roles {
            for parent_id in &role.inherits {
                if let Some(parent_perms) = self.role_permissions.get(parent_id).cloned() {
                    if let Some(child_perms) = self.role_permissions.get_mut(&role.id) {
                        child_perms.extend(parent_perms);
                    }
                }
            }
        }
    }

    /// Get all permissions for a set of roles
    pub fn permissions_for_roles(&self, roles: &[String]) -> HashSet<String> {
        let mut perms = HashSet::new();
        for role in roles {
            if let Some(role_perms) = self.role_permissions.get(role) {
                perms.extend(role_perms.clone());
            }
        }
        perms
    }

    /// Check if a set of roles has a specific permission
    pub fn has_permission(&self, roles: &[String], permission: &str) -> bool {
        let perms = self.permissions_for_roles(roles);
        // Check for wildcard
        if perms.contains("*") {
            return true;
        }
        // Check exact match
        if perms.contains(permission) {
            return true;
        }
        // Check resource wildcard (e.g., "vm:*" matches "vm:create")
        if let Some((resource, _action)) = permission.split_once(':') {
            if perms.contains(&format!("{}:*", resource)) {
                return true;
            }
        }
        false
    }

    /// Export all policies as Terraform HCL
    pub fn export_terraform(&self) -> String {
        self.policies
            .iter()
            .map(|p| p.to_terraform_hcl())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get all defined roles
    pub fn roles(&self) -> Vec<&Role> {
        self.policies.iter().flat_map(|p| &p.roles).collect()
    }

    /// Get all defined permissions
    pub fn permissions(&self) -> Vec<String> {
        self.all_permissions.iter().cloned().collect()
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_inheritance() {
        let engine = PolicyEngine::new();
        
        // Admin should have everything
        assert!(engine.has_permission(&["admin".to_string()], "vm:create"));
        assert!(engine.has_permission(&["admin".to_string()], "anything:at:all"));
        
        // Operator should have VM permissions
        assert!(engine.has_permission(&["operator".to_string()], "vm:create"));
        assert!(engine.has_permission(&["operator".to_string()], "vm:read"));
        
        // Viewer should only have read permissions
        assert!(engine.has_permission(&["viewer".to_string()], "vm:read"));
        assert!(!engine.has_permission(&["viewer".to_string()], "vm:create"));
    }

    #[test]
    fn test_terraform_export() {
        let engine = PolicyEngine::new();
        let hcl = engine.export_terraform();
        assert!(hcl.contains("resource \"infrasim_rbac_policy\""));
        assert!(hcl.contains("admin"));
        assert!(hcl.contains("operator"));
        assert!(hcl.contains("viewer"));
    }
}
