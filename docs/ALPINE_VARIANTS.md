# Alpine Image Variants Build Pipeline

This document describes the CI/CD pipeline for building the four Alpine Linux image variants with different VPN configurations.

## Overview

InfraSim produces four Alpine image variants to support different deployment scenarios:

| Variant | VPN Stack | Use Case |
|---------|-----------|----------|
| **no-vpn** | None | Air-gapped/isolated environments |
| **wireguard** | WireGuard mesh | Peer-to-peer encrypted VPN |
| **tailscale** | Tailscale | Centralized control plane (like Docker Swarm) |
| **dual-vpn** | WireGuard + Tailscale | Data/control plane separation for hostile territory |

## Architecture

### Build Pipeline Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         GitHub Actions Workflow                              │
│                       build-alpine-variants.yml                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  1. PROVISION RUNNERS (Terraform)                                           │
│     ┌──────────────────────────────────────────────────────────────────┐    │
│     │  examples/terraform/github-runners/                              │    │
│     │                                                                  │    │
│     │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐            │    │
│     │  │ no-vpn  │  │wireguard│  │tailscale│  │dual-vpn │            │    │
│     │  │ runner  │  │ runner  │  │ runner  │  │ runner  │            │    │
│     │  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘            │    │
│     │       │            │            │            │                   │    │
│     │       └────────────┴────────────┴────────────┘                   │    │
│     │                        │                                         │    │
│     │              Self-register with GitHub                           │    │
│     └──────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  2. BUILD BASE IMAGE (ubuntu-latest)                                        │
│     ┌──────────────────────────────────────────────────────────────────┐    │
│     │  images/alpine/build-alpine-qcow2.sh                             │    │
│     │                                                                  │    │
│     │  • Download Alpine minirootfs                                    │    │
│     │  • Create 2GB qcow2 image                                        │    │
│     │  • Generate cloud-init                                           │    │
│     │  • Record provenance                                             │    │
│     │                                                                  │    │
│     │  Output: base.qcow2 + meta/build-provenance.json                │    │
│     └──────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  3. BUILD VARIANTS (parallel, on self-hosted runners)                       │
│     ┌──────────────────────────────────────────────────────────────────┐    │
│     │                                                                  │    │
│     │  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐          │    │
│     │  │ no-vpn      │    │ wireguard   │    │ tailscale   │          │    │
│     │  │             │    │             │    │             │          │    │
│     │  │ base.qcow2  │    │ base.qcow2  │    │ base.qcow2  │          │    │
│     │  │     ↓       │    │     ↓       │    │     ↓       │          │    │
│     │  │ overlay-    │    │ overlay-    │    │ overlay-    │          │    │
│     │  │ no-vpn      │    │ wireguard   │    │ tailscale   │          │    │
│     │  │ .qcow2      │    │ .qcow2      │    │ .qcow2      │          │    │
│     │  └─────────────┘    └─────────────┘    └─────────────┘          │    │
│     │                                                                  │    │
│     │                     ┌─────────────┐                              │    │
│     │                     │ dual-vpn    │                              │    │
│     │                     │             │                              │    │
│     │                     │ base.qcow2  │                              │    │
│     │                     │     ↓       │                              │    │
│     │                     │ overlay-    │                              │    │
│     │                     │ dual-vpn    │                              │    │
│     │                     │ .qcow2      │                              │    │
│     │                     └─────────────┘                              │    │
│     │                                                                  │    │
│     └──────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  4. BUNDLE & RELEASE                                                        │
│     ┌──────────────────────────────────────────────────────────────────┐    │
│     │                                                                  │    │
│     │  infrasim-alpine-variants-{git_sha}.tar.gz                      │    │
│     │  ├── manifest.json                                              │    │
│     │  ├── no-vpn/                                                    │    │
│     │  │   ├── alpine-no-vpn.qcow2                                    │    │
│     │  │   └── overlay-no-vpn.qcow2                                   │    │
│     │  ├── wireguard/                                                 │    │
│     │  │   ├── alpine-wireguard.qcow2                                 │    │
│     │  │   └── overlay-wireguard.qcow2                                │    │
│     │  ├── tailscale/                                                 │    │
│     │  │   ├── alpine-tailscale.qcow2                                 │    │
│     │  │   └── overlay-tailscale.qcow2                                │    │
│     │  ├── dual-vpn/                                                  │    │
│     │  │   ├── alpine-dual-vpn.qcow2                                  │    │
│     │  │   └── overlay-dual-vpn.qcow2                                 │    │
│     │  └── meta/                                                      │    │
│     │      └── variant-*-provenance.json                              │    │
│     │                                                                  │    │
│     └──────────────────────────────────────────────────────────────────┘    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Self-Hosted Runners

### Why Self-Hosted Runners?

1. **Variant-Specific Tools**: Each runner has the exact tools needed for its variant
2. **Visibility**: Separate runners make build progress visible in GitHub Actions UI
3. **Isolation**: Build environments are isolated from each other
4. **Reproducibility**: Consistent build environment controlled via Terraform

### Runner Configuration

Runners are provisioned via Terraform in `examples/terraform/github-runners/`:

```hcl
# Each variant gets a dedicated runner VM
resource "infrasim_vm" "runner" {
  for_each = local.variants
  
  name   = each.value.label  # e.g., "infrasim-runner-wireguard"
  cpus   = 4
  memory = 4096
  
  cloud_init = templatefile("templates/runner-cloud-init.yaml", {
    runner_name    = each.value.label
    runner_labels  = each.value.label
    packages       = each.value.packages  # wireguard-tools, tailscale, etc.
    ...
  })
}
```

### Runner Labels

| Label | Variant | Installed Tools |
|-------|---------|-----------------|
| `infrasim-runner-no-vpn` | no-vpn | Base tools only |
| `infrasim-runner-wireguard` | wireguard | wireguard-tools |
| `infrasim-runner-tailscale` | tailscale | tailscale |
| `infrasim-runner-dual-vpn` | dual-vpn | wireguard-tools, tailscale, nftables |

## qcow2 Overlay System

### How Overlays Work

```
┌──────────────────────────────────────────────────────────────────┐
│                        Overlay Architecture                       │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                      base.qcow2                              │ │
│  │                                                              │ │
│  │  Alpine Linux 3.20                                           │ │
│  │  - Core packages                                             │ │
│  │  - SSH, networking                                           │ │
│  │  - Cloud-init                                                │ │
│  │  - Telemetry agent stub                                      │ │
│  │                                                              │ │
│  │  Size: ~500MB (compressed)                                   │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                              ▲                                    │
│                              │ backing_file                       │
│           ┌──────────────────┼──────────────────┐                │
│           │                  │                  │                 │
│           ▼                  ▼                  ▼                 │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐          │
│  │overlay-     │    │overlay-     │    │overlay-     │          │
│  │wireguard    │    │tailscale    │    │dual-vpn     │          │
│  │.qcow2       │    │.qcow2       │    │.qcow2       │          │
│  │             │    │             │    │             │          │
│  │Delta only:  │    │Delta only:  │    │Delta only:  │          │
│  │~50MB        │    │~80MB        │    │~120MB       │          │
│  └─────────────┘    └─────────────┘    └─────────────┘          │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
```

### Benefits

1. **Storage Efficiency**: Overlays contain only changes from base
2. **Fast Updates**: Update base once, all variants inherit changes
3. **Atomic Rollback**: Swap overlay files for instant rollback
4. **Delta Distribution**: Only transfer overlay for updates

### Creating Overlays

```bash
# Create overlay backed by base
qemu-img create -f qcow2 -b base.qcow2 -F qcow2 overlay-wireguard.qcow2

# Overlay inherits all base content
# Any writes go to overlay only
```

## Variant Details

### no-vpn

**Purpose**: Baseline image for isolated environments

**Packages**: None additional (base only)

**Use Cases**:
- Air-gapped networks
- Internal isolated testing
- Custom VPN solutions

### wireguard

**Purpose**: Peer-to-peer encrypted mesh VPN

**Packages**:
- `wireguard-tools` - WireGuard CLI
- `wireguard-lts` - Kernel module
- `avahi`, `avahi-tools` - DNS-SD for peer discovery
- `libqrencode` - QR code generation for mobile configs

**Features**:
- Automatic peer discovery via mDNS
- Key rotation support
- NAT traversal with keepalives

**Configuration**:
```bash
# /etc/wireguard/wg0.conf
[Interface]
PrivateKey = {generated}
Address = 10.50.X.Y/24
ListenPort = 51820

[Peer]
# Added dynamically via peer-discovery.sh
```

### tailscale

**Purpose**: Centralized control plane (like Docker Swarm)

**Packages**:
- `tailscale` - Tailscale client
- `tailscale-openrc` - OpenRC service

**Features**:
- Zero-config networking
- ACL-based access control
- Tailscale SSH
- File sharing
- Exit node support

**Configuration**:
```json
// /etc/infrasim/tailscale/config.json
{
  "advertise_exit_node": false,
  "accept_dns": true,
  "accept_routes": true,
  "ssh": true,
  "tags": ["tag:infrasim", "tag:alpine"]
}
```

### dual-vpn

**Purpose**: Maximum security with traffic isolation

**Packages**: All from wireguard + tailscale + nftables

**Architecture**:
```
┌─────────────────────────────────────────────────────────────┐
│                        dual-vpn Node                         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│   CONTROL PLANE (Tailscale)          DATA PLANE (WireGuard) │
│   ┌─────────────────────┐            ┌─────────────────────┐│
│   │ tailscale0          │            │ wg0                 ││
│   │                     │            │                     ││
│   │ • SSH access        │ ──────X──→ │ • VM traffic        ││
│   │ • Telemetry         │    Block   │ • Storage repl.     ││
│   │ • Management        │            │ • Live migration    ││
│   │ • File transfer     │ ←──X────── │                     ││
│   │                     │    Block   │                     ││
│   └─────────────────────┘            └─────────────────────┘│
│            │                                  │              │
│            │ Mark: 0x100                     │ Mark: 0x200  │
│            │ Table: 100                      │ Table: 200   │
│            │                                  │              │
│   ┌────────┴──────────────────────────────────┴────────┐    │
│   │              Policy-Based Routing (nftables)        │    │
│   └─────────────────────────────────────────────────────┘    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Traffic Isolation**:
- Control traffic (SSH, telemetry) → Tailscale only
- Data traffic (VMs, storage) → WireGuard only
- Cross-interface traffic blocked by nftables

## Triggering Builds

### Automatic Triggers

Builds run automatically on:
- Push to `main` or `develop` branches
- Pull requests to `main`
- Changes to `images/alpine/**`

### Manual Dispatch

```bash
# Build all variants
gh workflow run build-alpine-variants.yml \
  -f variant=all \
  -f alpine_version=3.20

# Build specific variant
gh workflow run build-alpine-variants.yml \
  -f variant=wireguard

# Build without self-hosted runners (use ubuntu-latest)
gh workflow run build-alpine-variants.yml \
  -f variant=all \
  -f use_self_hosted=false
```

### Via InfraSim CLI

```bash
# Trigger build via control plane
infrasim pipeline trigger \
  --workflow build-alpine-variants.yml \
  --input variant=wireguard

# View build status
infrasim pipeline list --workflow build-alpine-variants.yml

# Stream build logs
infrasim pipeline logs --run-id 12345 --follow
```

## Provenance & Attestation

Each build produces provenance metadata:

```json
// meta/variant-wireguard-provenance.json
{
  "format_version": "1.0",
  "build_type": "alpine-variant",
  "variant": "wireguard",
  "base_image": {
    "path": "base.qcow2",
    "sha256": "abc123..."
  },
  "source": {
    "git_sha": "abc123",
    "build_date": "2024-12-19T10:30:00Z"
  },
  "packages": [
    "wireguard-tools",
    "wireguard-lts",
    "avahi",
    "avahi-tools"
  ],
  "services": [
    "wireguard",
    "avahi-daemon"
  ]
}
```

## Local Development

### Build Base Image

```bash
cd images/alpine
./build-alpine-qcow2.sh --version 3.20 --output output/base.qcow2
```

### Build Variant

```bash
./build-variant.sh --variant=wireguard --base=output/base.qcow2
```

### Test Variant

```bash
# Boot the variant
qemu-system-aarch64 \
  -M virt -m 512 -cpu cortex-a57 \
  -drive file=output/alpine-wireguard.qcow2,if=virtio \
  -nographic

# Verify WireGuard is available
wg --version
```

## Troubleshooting

### Runner Not Picking Up Jobs

1. Check runner registration in GitHub Settings → Actions → Runners
2. Verify runner VM is running: `infrasim vm list`
3. Check runner logs: `infrasim vm logs infrasim-runner-wireguard`

### Overlay Creation Fails

1. Ensure base image exists: `ls images/alpine/output/base.qcow2`
2. Check qemu-img version: `qemu-img --version`
3. Verify disk space available

### Build Fails on Variant

1. Check variant config: `cat variants/wireguard/config.yaml`
2. Verify yq is installed: `yq --version`
3. Review build logs in GitHub Actions

## Related Documentation

- [CONTROL_PLANE.md](./CONTROL_PLANE.md) - Tailscale C2 architecture
- [IMAGE_PROVENANCE.md](./IMAGE_PROVENANCE.md) - Provenance and attestation
- [BUILD_PIPELINE.md](./BUILD_PIPELINE.md) - General build pipeline
