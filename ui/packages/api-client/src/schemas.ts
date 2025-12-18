import { z } from "zod";

export const daemonStatusSchema = z.object({
  running_vms: z.number(),
  total_vms: z.number(),
  memory_used_bytes: z.number(),
  disk_used_bytes: z.number(),
  store_path: z.string(),
  qemu_available: z.boolean(),
  qemu_version: z.string(),
  hvf_available: z.boolean(),
});

export const vmSchema = z.object({
  id: z.string(),
  name: z.string(),
  arch: z.string(),
  machine: z.string(),
  cpu_cores: z.number(),
  memory_mb: z.number(),
  state: z.string(),
  vnc_display: z.string(),
  uptime_seconds: z.number(),
  volume_ids: z.array(z.string()),
  network_ids: z.array(z.string()),
  created_at: z.number(),
  labels: z.record(z.string()),
});

export const networkSchema = z.object({
  id: z.string(),
  name: z.string(),
  mode: z.string(),
  cidr: z.string(),
  gateway: z.string(),
  dns: z.string(),
  dhcp_enabled: z.boolean(),
  mtu: z.number(),
  active: z.boolean(),
  bridge_interface: z.string(),
  connected_vms: z.number(),
  created_at: z.number(),
  labels: z.record(z.string()),
});

export const volumeSchema = z.object({
  id: z.string(),
  name: z.string(),
  kind: z.string(),
  format: z.string(),
  size_bytes: z.number(),
  actual_size: z.number(),
  local_path: z.string(),
  digest: z.string(),
  ready: z.boolean(),
  verified: z.boolean(),
  source: z.string(),
  created_at: z.number(),
  labels: z.record(z.string()),
});

export const applianceInstanceSchema = z.object({
  id: z.string(),
  name: z.string(),
  template_id: z.string(),
  created_at: z.number(),
  updated_at: z.number().optional(),
  status: z.string(),
  vm_id: z.string().nullable().optional(),
  network_ids: z.array(z.string()),
  volume_ids: z.array(z.string()),
  console_id: z.string().nullable().optional(),
  snapshot_ids: z.array(z.string()),
});

export const applianceTemplateSchema = z.object({
  id: z.string(),
  title: z.string(),
  description: z.string(),
  arch: z.string(),
  machine: z.string(),
  cpu_cores: z.number(),
  memory_mb: z.number(),
  compatibility_mode: z.boolean(),
  tags: z.array(z.string()),
  image: z.string().optional(),
  env: z.record(z.string()).optional(),
  ports: z.array(z.object({
    container_port: z.number(),
    host_port: z.number().optional(),
    protocol: z.string(),
    description: z.string(),
  })).optional(),
  networks: z.array(z.object({
    id: z.string(),
    mode: z.string(),
    cidr: z.string().optional(),
    gateway: z.string().optional(),
    dhcp: z.boolean(),
  })).optional(),
  volumes: z.array(z.object({
    id: z.string(),
    size_mb: z.number(),
    mount_path: z.string(),
    kind: z.string(),
  })).optional(),
  tools: z.array(z.object({
    name: z.string(),
    version: z.string().optional(),
    purpose: z.string(),
  })).optional(),
});

export const snapshotSchema = z.object({
  id: z.string(),
  name: z.string(),
  vm_id: z.string(),
  include_memory: z.boolean(),
  include_disk: z.boolean(),
  description: z.string(),
  complete: z.boolean(),
  disk_snapshot_path: z.string(),
  memory_snapshot_path: z.string(),
  digest: z.string(),
  size_bytes: z.number(),
  encrypted: z.boolean(),
  created_at: z.number(),
  labels: z.record(z.string()),
});

export const terraformSchema = z.object({
  appliance_id: z.string(),
  terraform_hcl: z.string(),
});

export const aiDefineResponseSchema = z.object({
  intent: z.string(),
  appliance_template: z.unknown().optional(),
  networks: z.array(z.unknown()),
  volumes: z.array(z.unknown()),
  tools: z.array(z.unknown()),
  terraform_hcl: z.string(),
  notes: z.string(),
});

export const evidenceResponseSchema = z.object({
  digest: z.string(),
  signature: z.string(),
  public_key: z.string(),
  manifest: z.unknown().optional(),
});

export const attestationSchema = z.object({
  id: z.string().optional(),
  vm_id: z.string().optional(),
  digest: z.string().optional(),
  signature: z.string().optional(),
  created_at: z.number().optional(),
  attestation_type: z.string().optional(),
  host_provenance: z.object({
    qemu_version: z.string(),
    qemu_args: z.array(z.string()),
    base_image_hash: z.string(),
    volume_hashes: z.record(z.string()),
    macos_version: z.string(),
    cpu_model: z.string(),
    hvf_enabled: z.boolean(),
    hostname: z.string(),
    timestamp: z.number(),
  }).optional(),
  error: z.string().optional(),
});

// ============================================================================
// Virtual Filesystem Schemas (Terraform-addressable)
// ============================================================================

export const filesystemTypeSchema = z.enum([
  "local",      // fs.local - Host-local storage
  "snapshot",   // fs.snapshot - Copy-on-write snapshot
  "ephemeral",  // fs.ephemeral - RAM-backed, lost on stop
  "network",    // fs.network - NFS/CIFS/iSCSI mount
  "physical",   // fs.physical - Direct block device passthrough
  "geobound",   // fs.geobound - Geo-fenced with destruction policy
]);

export const filesystemLifecycleSchema = z.enum([
  "pending",
  "creating",
  "ready",
  "attached",
  "detaching",
  "deleting",
  "error",
]);

export const geographicBoundsSchema = z.object({
  center_lat: z.number(),
  center_lon: z.number(),
  radius_km: z.number(),
  destruction_policy: z.enum(["wipe", "encrypt", "alert"]),
});

export const filesystemSchema = z.object({
  id: z.string(),
  name: z.string(),
  fs_type: filesystemTypeSchema,
  size_bytes: z.number(),
  format: z.string().optional(),
  mount_point: z.string().optional(),
  source_path: z.string().optional(),
  network_uri: z.string().optional(),
  geographic_bounds: geographicBoundsSchema.optional(),
  lifecycle: filesystemLifecycleSchema,
  attached_to: z.array(z.string()),
  labels: z.record(z.string()),
  created_at: z.string(),
  updated_at: z.string(),
});

export const filesystemSnapshotSchema = z.object({
  id: z.string(),
  filesystem_id: z.string(),
  name: z.string(),
  description: z.string().optional(),
  created_at: z.string(),
  size_bytes: z.number(),
  checksum: z.string().optional(),
});

// ============================================================================
// Resource Graph Schemas
// ============================================================================

export const resourceNodeSchema = z.object({
  id: z.string(),
  resource_type: z.string(),
  address: z.string(),
  status: z.string(),
  metadata: z.unknown(),
  position: z.object({
    x: z.number(),
    y: z.number(),
  }).optional(),
});

export const resourceEdgeSchema = z.object({
  id: z.string(),
  source: z.string(),
  target: z.string(),
  edge_type: z.string(),
  metadata: z.unknown(),
});

export const resourceGraphSchema = z.object({
  nodes: z.array(resourceNodeSchema),
  edges: z.array(resourceEdgeSchema),
  version: z.number(),
  generated_at: z.string(),
});

export const graphPlanResultSchema = z.object({
  plan_id: z.string(),
  additions: z.array(z.string()),
  modifications: z.array(z.string()),
  deletions: z.array(z.string()),
  valid: z.boolean(),
  errors: z.array(z.string()),
});

export const graphApplyResultSchema = z.object({
  success: z.boolean(),
  applied_operations: z.number(),
  errors: z.array(z.string()),
});

export const graphValidationResultSchema = z.object({
  valid: z.boolean(),
  errors: z.array(z.object({
    resource_id: z.string(),
    field: z.string(),
    message: z.string(),
  })),
  warnings: z.array(z.string()),
});

// ============================================================================
// UI Manifest Schema (Provenance)
// ============================================================================

export const uiManifestAssetSchema = z.object({
  path: z.string(),
  size_bytes: z.number(),
  sha256: z.string(),
});

export const uiManifestSchema = z.object({
  version: z.string(),
  build_timestamp: z.string(),
  git_commit: z.string().optional(),
  git_branch: z.string().optional(),
  assets: z.array(uiManifestAssetSchema),
});

// ============================================================================
// Type Exports
// ============================================================================

export type DaemonStatus = z.infer<typeof daemonStatusSchema>;
export type Vm = z.infer<typeof vmSchema>;
export type Network = z.infer<typeof networkSchema>;
export type Volume = z.infer<typeof volumeSchema>;
export type ApplianceInstance = z.infer<typeof applianceInstanceSchema>;
export type ApplianceTemplate = z.infer<typeof applianceTemplateSchema>;
export type Snapshot = z.infer<typeof snapshotSchema>;
export type Terraform = z.infer<typeof terraformSchema>;
export type AiDefine = z.infer<typeof aiDefineResponseSchema>;
export type Evidence = z.infer<typeof evidenceResponseSchema>;
export type Attestation = z.infer<typeof attestationSchema>;

// Filesystem types
export type FilesystemType = z.infer<typeof filesystemTypeSchema>;
export type FilesystemLifecycle = z.infer<typeof filesystemLifecycleSchema>;
export type GeographicBounds = z.infer<typeof geographicBoundsSchema>;
export type Filesystem = z.infer<typeof filesystemSchema>;
export type FilesystemSnapshot = z.infer<typeof filesystemSnapshotSchema>;

// Resource graph types
export type ResourceNode = z.infer<typeof resourceNodeSchema>;
export type ResourceEdge = z.infer<typeof resourceEdgeSchema>;
export type ResourceGraph = z.infer<typeof resourceGraphSchema>;
export type GraphPlanResult = z.infer<typeof graphPlanResultSchema>;
export type GraphApplyResult = z.infer<typeof graphApplyResultSchema>;
export type GraphValidationResult = z.infer<typeof graphValidationResultSchema>;

// UI manifest types
export type UiManifestAsset = z.infer<typeof uiManifestAssetSchema>;
export type UiManifest = z.infer<typeof uiManifestSchema>;
