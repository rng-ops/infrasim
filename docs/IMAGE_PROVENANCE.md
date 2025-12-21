# Image Snapshot Provenance

This document describes how InfraSim image snapshots include build provenance and how the versioning system works.

## Overview

Every image snapshot built through the CI pipeline includes:

1. **InfraSim Version Pin** - The exact version/commit of InfraSim used
2. **Binary Hashes** - SHA256 of the InfraSim binaries
3. **Dependency Graph** - Full cargo metadata with all dependencies
4. **Image Hashes** - SHA256 of the qcow2 disk image
5. **Build Metadata** - Timestamps, runner info, GitHub run ID

## Automatic Tagging

Successful image builds automatically create Git tags with the format:

```
image-{type}-{YYYYMMDD}-{infrasim_short}-{image_short}
```

Example: `image-alpine-20241219-dev-a1b2-c3d4e5f6`

This allows you to:
- Trace any image back to the exact InfraSim version
- Reproduce builds with the same dependency graph
- Audit supply chain for any deployed image

## Provenance Files

Each image bundle includes these attestation files:

### `meta/image-provenance.json`

The unified provenance document linking the image to InfraSim:

```json
{
  "image": {
    "type": "alpine",
    "version": "3.20",
    "architecture": "aarch64",
    "sha256": "abc123..."
  },
  "infrasim": {
    "version": "v0.1.0-dev-a1b2c3d",
    "sha256": "def456...",
    "dep_graph_sha256": "789ghi..."
  },
  "build": {
    "timestamp": "2024-12-19T10:30:00Z",
    "git_commit": "abc123def456...",
    "git_ref": "refs/heads/main",
    "github_run_id": "12345678",
    "snapshot_version": "dev-20241219-a1b2c3d"
  },
  "analysis": {
    "risk_score": 0,
    "cycle_count": 0
  }
}
```

### `meta/attestations/infrasim-build.json`

Detailed InfraSim build information:

```json
{
  "version": "v0.1.0-dev-a1b2c3d",
  "git_commit": "abc123def456789...",
  "git_ref": "refs/heads/main",
  "build_timestamp": "2024-12-19T10:25:00Z",
  "github_run_id": "12345678",
  "rust_version": "rustc 1.75.0",
  "cargo_version": "cargo 1.75.0",
  "binaries": {
    "infrasimd": { "sha256": "..." },
    "infrasim": { "sha256": "..." }
  },
  "dependencies": {
    "graph_sha256": "...",
    "package_count": 350,
    "has_duplicates": false
  }
}
```

### `meta/attestations/cargo-metadata.json`

The complete `cargo metadata` output, allowing full reconstruction of the dependency graph at build time.

## Verification

### Verify Bundle Integrity

```bash
# Download the bundle and checksum
curl -LO https://github.com/rng-ops/infrasim/releases/download/image-alpine-20241219-.../infrasim-alpine-....tar.gz
curl -LO https://github.com/rng-ops/infrasim/releases/download/image-alpine-20241219-.../infrasim-alpine-....tar.gz.sha256

# Verify bundle
shasum -a 256 -c infrasim-alpine-*.tar.gz.sha256
```

### Verify Image Integrity

```bash
# Extract and verify image hash from provenance
tar -xzf infrasim-alpine-*.tar.gz
IMAGE_SHA256=$(jq -r '.image.sha256' meta/image-provenance.json)
echo "${IMAGE_SHA256}  disk/base.qcow2" | shasum -a 256 -c
```

### Verify InfraSim Version

```bash
# Check which InfraSim version was used
jq '.infrasim' meta/image-provenance.json

# Compare with running version
infrasim --version
```

### Audit Dependency Graph

```bash
# View the full dependency graph
jq '.packages[] | {name, version, source}' meta/attestations/cargo-metadata.json

# Check for specific package version
jq '.packages[] | select(.name == "tokio") | {name, version}' meta/attestations/cargo-metadata.json
```

## CI Workflow

The image build process:

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  Build InfraSim │────▶│  Build Image    │────▶│  Verify Boot    │
│  & Analyze Deps │     │  + Provenance   │     │  (QEMU test)    │
└─────────────────┘     └─────────────────┘     └─────────────────┘
         │                       │                       │
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│ infrasim-build  │     │ alpine-build-*  │     │ boot_status     │
│ .json           │     │ artifact        │     │ output          │
└─────────────────┘     └─────────────────┘     └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 ▼
                    ┌─────────────────────┐
                    │  Create Snapshot    │
                    │  Bundle + Tag       │
                    └─────────────────────┘
                                 │
                                 ▼
                    ┌─────────────────────┐
                    │  Deploy Release     │
                    │  (on success)       │
                    └─────────────────────┘
```

## Triggers

The image snapshot workflow runs on:

| Trigger | Description | Creates Tag |
|---------|-------------|-------------|
| `push` to `main` (images/**) | Image file changes | ✅ Yes |
| `push` tag `v*` | Version release | ✅ Yes (uses tag) |
| `push` tag `image-*` | Manual image tag | ✅ Yes (uses tag) |
| Schedule (weekly) | Nightly builds | ✅ Yes |
| `workflow_dispatch` | Manual trigger | Configurable |

## Using with ISVM

To use a specific InfraSim version with an image:

```bash
# Get the InfraSim version from the image provenance
INFRASIM_VERSION=$(curl -s https://github.com/rng-ops/infrasim/releases/download/image-alpine-20241219-.../infrasim-build.json | jq -r '.version')

# Install that version
isvm install ${INFRASIM_VERSION}
isvm use ${INFRASIM_VERSION}

# Now you're using the exact InfraSim version the image was built with
infrasim --version
```

## Supply Chain Security

This provenance system enables:

1. **Reproducibility** - Given a tag, you can rebuild with the same dependencies
2. **Auditability** - Full dependency graph is archived with each image
3. **Traceability** - Any image can be traced to its source commit
4. **Verification** - Cryptographic hashes verify integrity

The dependency analysis also flags:
- Dependency cycles
- Vendor concentration risks
- Suspicious patterns (typosquatting, unusual pins)
