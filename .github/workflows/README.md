# GitHub Actions CI/CD

This directory contains GitHub Actions workflows for InfraSim.

## Workflows

| Workflow | File | Triggers | Description |
|----------|------|----------|-------------|
| **Build and Release** | `build.yml` | Push to main/develop, tags, PRs | Main build pipeline for binaries |
| **Tests** | `tests.yml` | Push to main/develop, PRs | Unit and integration tests |
| **Build Images** | `build-images.yml` | Push (images/*), PRs | Alpine qcow2 image builds |
| **Snapshots** | `snapshots.yml` | Tags (v*), weekly schedule | Versioned image releases |

## Test Workflows

### `tests.yml`

Runs the full test suite:

```yaml
# Triggered on every push to main/develop
on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]
```

**Jobs:**
1. **unit-tests** - All crate unit tests (macOS M1)
2. **integration-tests** - E2E tests: `attestation_tamper`, `network_qos`
3. **expensive-tests** - Ignored tests (on demand or main branch)

**Manual trigger with expensive tests:**
```bash
gh workflow run tests.yml -f run_expensive_tests=true
```

### Integration Tests

| Test | Description | Status |
|------|-------------|--------|
| `attestation_tamper.rs` | Verifies tamper detection in attestation reports | ✅ Always runs |
| `network_qos.rs` | Validates QoS latency effects | ✅ Always runs |
| `determinism_build.rs` | Reproducible build verification | ⏸️ `#[ignore]` - expensive |
| `terraform_idempotency.rs` | Terraform plan idempotency | ⏸️ `#[ignore]` - needs infra |

## Image Workflows

### `build-images.yml`

Builds Alpine qcow2 images on changes to `images/alpine/**`.

**Jobs:**
1. **build-alpine** - Build the qcow2 image
2. **boot-test** - Headless QEMU verification
3. **bundle** - Create signed artifact bundle

### `snapshots.yml`

Creates versioned snapshot releases.

**Triggers:**
- **Tag push (`v*`)** - Creates a GitHub Release
- **Weekly schedule** - Rolling `latest` snapshot
- **Manual dispatch** - Ad-hoc builds

**Manual trigger:**
```bash
# Test build
gh workflow run snapshots.yml -f snapshot_type=test

# Nightly build
gh workflow run snapshots.yml -f snapshot_type=nightly

# Release build (or just push a tag)
gh workflow run snapshots.yml -f snapshot_type=release
```

## Artifact Structure

Image bundles are structured as:

```
infrasim-alpine-v1.0.0.tar.gz
├── disk/
│   ├── base.qcow2              # Main disk image
│   └── snapshots/
│       └── clean.qcow2         # Clean overlay snapshot
├── meta/
│   ├── manifest.json           # SHA256 checksums
│   ├── attestations/
│   │   ├── build-provenance.json
│   │   └── artifact-integrity.json
│   ├── signatures/
│   │   ├── manifest.sig
│   │   └── signature-info.json
│   └── logs/
│       ├── build.log.txt
│       └── qemu-img-info.txt
└── README.md
```

## Required Secrets

| Secret | Used By | Description |
|--------|---------|-------------|
| `GITHUB_TOKEN` | All workflows | Auto-provided by GitHub |
| `SIGNING_KEY` | snapshots.yml | Ed25519 private key (optional) |

## Local Testing

Test workflows locally with [act](https://github.com/nektos/act):

```bash
# Run tests workflow
act push -W .github/workflows/tests.yml

# Run snapshots workflow (dry run)
act workflow_dispatch -W .github/workflows/snapshots.yml \
  -e '{"inputs":{"snapshot_type":"test"}}'
```

## Releasing

1. **Create a tag:**
   ```bash
   git tag -s v1.0.0 -m "Release v1.0.0"
   git push origin v1.0.0
   ```

2. **Workflows triggered:**
   - `build.yml` - Builds binaries, creates GitHub Release
   - `snapshots.yml` - Builds image, attaches to Release

3. **Artifacts:**
   - `infrasim-v1.0.0-aarch64-apple-darwin.tar.gz` (binaries)
   - `infrasim-alpine-v1.0.0.tar.gz` (image bundle)
