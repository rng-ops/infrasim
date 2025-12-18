# InfraSim Web Workflow

This document describes the complete web workflow for managing appliances, generating Terraform, and integrating with LLMs.

## Overview

The InfraSim web server provides a REST API for:

1. **Inventory Management** - View all images, volumes, snapshots, networks, and VMs
2. **Appliance Lifecycle** - Create, manage, export, import, and archive VM-based appliances
3. **Terraform Generation** - Generate HCL for network/volume/VM resources
4. **AI/LLM Integration** - Natural language → infrastructure definition via LangChain-style prompts
5. **Provenance/Evidence** - Signed attestation bundles for audit trails

## Authentication

See [WEB_AUTH.md](./WEB_AUTH.md) for JWT authentication configuration.

## Inventory API

### Daemon Status

```bash
GET /api/daemon/status
```

Returns daemon status including running VMs, disk usage, QEMU version, HVF availability.

### List Images (qcow2 disk volumes)

```bash
GET /api/images
```

Returns all disk images (volumes with format=qcow2 or raw):
```json
{
  "images": [
    {
      "id": "...",
      "name": "ubuntu-22.04-aarch64",
      "kind": "disk",
      "format": "qcow2",
      "size_bytes": 8589934592,
      "actual_size": 2147483648,
      "local_path": "/var/lib/infrasim/volumes/...",
      "digest": "sha256:...",
      "ready": true,
      "verified": true
    }
  ],
  "count": 1
}
```

### List Volumes

```bash
GET /api/volumes
GET /api/volumes/{volume_id}
```

### List Snapshots

```bash
GET /api/snapshots
GET /api/snapshots?vm_id={vm_id}
GET /api/snapshots/{snapshot_id}
```

### List Networks

```bash
GET /api/networks
GET /api/networks/{network_id}
```

### List VMs

```bash
GET /api/vms
GET /api/vms/{vm_id}
```

## Appliance API

### List Templates

```bash
GET /api/appliances/templates
```

Returns available appliance templates:
- `keycloak-aarch64` - Keycloak Identity Provider (quay.io/keycloak/keycloak:26.0)
- `pi-like-aarch64-desktop` - Raspberry Pi-like AArch64 desktop

### Create Appliance

```bash
POST /api/appliances
Content-Type: application/json

{
  "name": "my-keycloak",
  "template_id": "keycloak-aarch64",
  "auto_start": true
}
```

This will:
1. Create networks defined in the template
2. Create volumes defined in the template
3. Create the VM via daemon gRPC
4. Start the VM (if `auto_start` is true)
5. Create a VNC/web console for the VM

### List Appliances

```bash
GET /api/appliances
```

### Get Terraform for Appliance

```bash
GET /api/appliances/{appliance_id}/terraform
```

Returns generated Terraform HCL including:
- `infrasim_network` resources
- `infrasim_volume` resources
- `infrasim_vm` resource
- `infrasim_console` resource

### Boot Appliance

```bash
POST /api/appliances/{appliance_id}/boot
```

Starts the VM if stopped, returns the boot plan.

### Stop Appliance

```bash
POST /api/appliances/{appliance_id}/stop
Content-Type: application/json

{
  "force": false
}
```

### Get Detailed Appliance View

```bash
GET /api/appliances/{appliance_id}
```

Returns comprehensive appliance details including:
- Instance metadata (id, name, status, template_id)
- Template definition
- VM details (state, VNC display, uptime)
- Network configurations (CIDR, gateway, mode, connected VMs)
- Volume details (paths, sizes, digests)
- Snapshots (disk/memory paths, sizes)
- Generated Terraform HCL
- Export bundle for backup/restore

Response structure:
```json
{
  "instance": { "id": "...", "name": "...", "status": "running", ... },
  "template": { "id": "keycloak-aarch64", ... },
  "vm": { "state": "running", "vnc_display": ":0", "uptime_seconds": 3600, ... },
  "networks": [{ "id": "...", "mode": "user", "cidr": "10.0.2.0/24", ... }],
  "volumes": [{ "id": "...", "local_path": "/var/lib/infrasim/volumes/...", ... }],
  "snapshots": [{ "id": "...", "disk_snapshot_path": "...", ... }],
  "terraform_hcl": "...",
  "export_bundle": { ... }
}
```

### Export Appliance

```bash
GET /api/appliances/{appliance_id}/export
```

Returns a signed JSON bundle containing all appliance configuration for backup:
```json
{
  "bundle": {
    "version": "1.0",
    "type": "infrasim_appliance_export",
    "exported_at": "2024-12-15T...",
    "appliance": { ... },
    "template": { ... },
    "vm_spec": { "arch": "aarch64", "machine": "virt", ... },
    "networks": [{ ... }],
    "volumes": [{ "name": "...", "digest": "sha256:...", ... }],
    "snapshots": [{ ... }],
    "terraform_hcl": "..."
  },
  "signature": "hex-encoded-ed25519-signature",
  "public_key": "hex-encoded-public-key"
}
```

### Import Appliance

```bash
POST /api/appliances/import
Content-Type: application/json

{
  "bundle": { ... },  // The export bundle from /export
  "new_name": "my-imported-keycloak"  // Optional
}
```

Creates a new appliance from an export bundle. Use `POST /api/appliances/{id}/boot` to launch.

### Archive Appliance

```bash
POST /api/appliances/{appliance_id}/archive
Content-Type: application/json

{
  "format": "json",           // "json", "tar.gz", or "zip"
  "include_memory": false,    // Include memory snapshots
  "include_all_snapshots": true
}
```

Returns an archive manifest with file paths for creating a backup:
```json
{
  "archive_id": "...",
  "format": "json",
  "manifest": { ... },
  "signature": "...",
  "public_key": "...",
  "files_to_archive": [
    "/var/lib/infrasim/volumes/xxx.qcow2",
    "/var/lib/infrasim/snapshots/yyy.qcow2"
  ]
}
```

### Get Attestation Report

```bash
GET /api/appliances/{appliance_id}/attestation
```

Returns the VM attestation report with host provenance:
```json
{
  "id": "...",
  "vm_id": "...",
  "digest": "sha256:...",
  "signature": "...",
  "created_at": 1702656000,
  "attestation_type": "host_provenance",
  "host_provenance": {
    "qemu_version": "8.2.0",
    "qemu_args": ["..."],
    "base_image_hash": "sha256:...",
    "macos_version": "14.0",
    "hvf_enabled": true,
    ...
  }
}
```

### Snapshot Appliance

```bash
POST /api/appliances/{appliance_id}/snapshot
Content-Type: application/json

{
  "name": "pre-upgrade-snapshot",
  "include_memory": false
}
```

Returns:
```json
{
  "snapshot_id": "...",
  "appliance_id": "...",
  "vm_id": "...",
  "name": "pre-upgrade-snapshot",
  "evidence": {
    "data": { ... },
    "signature": "hex-encoded-signature",
    "public_key": "hex-encoded-public-key"
  }
}
```

## AI/LLM Integration

### Natural Language → Infrastructure

```bash
POST /api/ai/define
Content-Type: application/json

{
  "prompt": "Create a Keycloak identity provider with persistent storage"
}
```

Supported intents (rule-based fallback):
- `keycloak`, `identity`, `sso`, `oauth`, `oidc` → Keycloak appliance
- `pi`, `raspberry`, `desktop`, `kali` → Pi-like desktop
- `nginx`, `reverse proxy`, `load balancer` → nginx tool
- `apache`, `httpd`, `web server` → Apache tool
- `postgres`, `postgresql`, `database` → PostgreSQL + volume
- `redis`, `cache` → Redis
- `storage`, `volume`, `disk`, `persistent` → Storage volume
- `network`, `bridge`, `nat`, `vlan` → Network definition
- `forwarder`, `haproxy`, `envoy` → TCP/HTTP forwarder
- `container`, `docker`, `podman` → Container runtime

### LLM Backend Configuration

Set environment variables to use an LLM backend instead of rule-based matching:

#### Ollama (Local LLM)

```bash
export INFRASIM_LLM_BACKEND=ollama
export INFRASIM_OLLAMA_URL=http://localhost:11434
export INFRASIM_OLLAMA_MODEL=llama3.2
```

#### vLLM Server

```bash
export INFRASIM_LLM_BACKEND=vllm
export INFRASIM_VLLM_URL=http://localhost:8000
export INFRASIM_VLLM_MODEL=default
```

#### OpenAI API

```bash
export INFRASIM_LLM_BACKEND=openai
export OPENAI_API_KEY=sk-...
export OPENAI_MODEL=gpt-4o-mini
```

The LLM is given a system prompt that instructs it to output JSON with:
- `intent` - The inferred action type
- `appliance_template_id` - Template to use (if applicable)
- `networks` - Network definitions
- `volumes` - Volume definitions
- `tools` - Software/tool definitions

## Keycloak Appliance

The Keycloak template provides:

### Image
```
quay.io/keycloak/keycloak:26.0
```

### Ports
- 8080: HTTP (Keycloak web UI)
- 8443: HTTPS
- 9000: Management

### Environment Variables
- `KC_BOOTSTRAP_ADMIN_USERNAME`: admin
- `KC_BOOTSTRAP_ADMIN_PASSWORD`: changeme (override in production!)

### Boot Plan
1. Create AArch64 VM
2. Pull Keycloak container image
3. Start Keycloak in dev mode (`start-dev`)
4. Wait for health endpoint (`/health/ready`)

### Generated Terraform

```hcl
terraform {
  required_providers {
    infrasim = {
      source  = "infrasim/infrasim"
      version = ">= 0.1.0"
    }
  }
}

provider "infrasim" {
  endpoint = "http://127.0.0.1:50051"
}

resource "infrasim_network" "mgmt" {
  name         = "mgmt"
  mode         = "user"
  cidr         = "10.0.2.0/24"
  gateway      = "10.0.2.2"
  dhcp_enabled = true
}

resource "infrasim_volume" "kc-data" {
  name      = "kc-data"
  size_mb   = 1024
  kind      = "disk"
}

resource "infrasim_vm" "keycloak-aarch64" {
  name       = "keycloak-aarch64"
  arch       = "aarch64"
  machine    = "virt"
  cpu_cores  = 2
  memory_mb  = 2048
  image      = "quay.io/keycloak/keycloak:26.0"
  
  network_ids = [infrasim_network.mgmt.id]
  volume_ids  = [infrasim_volume.kc-data.id]
}

resource "infrasim_console" "keycloak-console" {
  vm_id      = infrasim_vm.keycloak-aarch64.id
  enable_vnc = true
  vnc_port   = 5900
  enable_web = true
  web_port   = 6080
}
```

## Daemon gRPC Integration

The web server connects to the InfraSim daemon via gRPC for:

- `CreateVM` / `StartVM` / `StopVM` - VM lifecycle
- `CreateNetwork` / `DeleteNetwork` - Network management
- `CreateVolume` / `DeleteVolume` - Volume management
- `CreateConsole` - VNC/web console provisioning
- `CreateSnapshot` - VM snapshots for backup/restore
- `GetHealth` - Daemon health check

Configure the daemon endpoint:

```rust
WebServerConfig {
    daemon_addr: "http://127.0.0.1:50051".to_string(),
    auth: WebUiAuth::Jwt(JwtAuthConfig { ... }),
}
```

## Example Workflow

```bash
# 1. List available templates
curl http://localhost:8080/api/appliances/templates

# 2. Use AI to define infrastructure
curl -X POST http://localhost:8080/api/ai/define \
  -H "Content-Type: application/json" \
  -d '{"prompt": "I need a Keycloak server with 2GB storage"}'

# 3. Create the appliance
curl -X POST http://localhost:8080/api/appliances \
  -H "Content-Type: application/json" \
  -d '{"name": "my-keycloak", "template_id": "keycloak-aarch64"}'

# 4. Get Terraform for the appliance
curl http://localhost:8080/api/appliances/{id}/terraform

# 5. Create a snapshot before making changes
curl -X POST http://localhost:8080/api/appliances/{id}/snapshot \
  -H "Content-Type: application/json" \
  -d '{"name": "pre-config-snapshot"}'

# 6. Stop the appliance
curl -X POST http://localhost:8080/api/appliances/{id}/stop \
  -H "Content-Type: application/json" \
  -d '{}'
```

## Security Considerations

1. **Authentication**: Use JWT auth with approved issuers in production
2. **Keycloak Credentials**: Override `KC_BOOTSTRAP_ADMIN_PASSWORD` with a secure value
3. **Network Isolation**: Use `vmnet_bridged` only when needed; prefer `user` mode
4. **Snapshots**: Evidence bundles are Ed25519 signed for audit trails
5. **TLS**: Use HTTPS in production for both web server and daemon
