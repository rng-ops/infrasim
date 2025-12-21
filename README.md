# InfraSim

[![Build Status](https://github.com/rng-ops/infrasim/actions/workflows/build.yml/badge.svg)](https://github.com/rng-ops/infrasim/actions/workflows/build.yml)
[![Tests](https://github.com/rng-ops/infrasim/actions/workflows/tests.yml/badge.svg)](https://github.com/rng-ops/infrasim/actions/workflows/tests.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**Terraform-Compatible QEMU Virtualization Platform for macOS Apple Silicon**

InfraSim is a production-ready virtualization platform that brings infrastructure-as-code to QEMU virtual machines on macOS. Designed specifically for Apple Silicon Macs, it provides HVF-accelerated ARM64 guests with full Terraform/OpenTofu provider support, cryptographic attestation, and software-defined networking capabilities.

---

## Table of Contents

- [Features](#features)
- [Architecture](#architecture)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [CLI Reference](#cli-reference)
- [VPN & Network Options](#vpn--network-options)
- [Alpine Image Profiles](#alpine-image-profiles)
- [CI/CD Integration](#cicd-integration)
- [Test Coverage](#test-coverage)
- [Dependencies](#dependencies)
- [Project Structure](#project-structure)
- [Configuration](#configuration)
- [Development](#development)
- [Maintenance](#maintenance)
- [License](#license)

---

## Features

### Core Virtualization
- **Native Performance** — HVF acceleration for near-native ARM64 VM performance on Apple Silicon
- **Terraform Compatible** — Full Terraform/OpenTofu provider (`terraform-provider-infrasim`)
- **Browser Console** — noVNC-based web console for graphical VM access
- **Snapshots** — Memory and disk snapshots for instant save/restore
- **QoS Simulation** — Latency, jitter, packet loss, and bandwidth shaping

### Security & Provenance
- **Cryptographic Attestation** — Ed25519 signed provenance reports for all builds
- **Content-Addressed Storage** — SHA256-based deduplication for disk images
- **Artifact Verification** — Inspect and verify build artifacts with tamper detection
- **mTLS Control Plane** — Optional mutual TLS for secure node communication

### Networking
- **Software-Defined Networking** — Router, firewall, VPN gateway, load balancer appliances
- **WireGuard Mesh** — Cryptographically verified peer admission with narrow AllowedIPs
- **Tailscale Integration** — Managed mesh networking with restrictive security defaults
- **IPv6 Rendezvous** — Epoch-based peer discovery without multicast dependency

### Build System
- **Feature Overlays** — Modular, composable image building system
- **Profile Composition** — Mix-and-match features for custom images
- **Signed Provenance** — In-toto style attestations for supply chain security

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Terraform / OpenTofu                         │
└─────────────────────────┬───────────────────────────────────────┘
                          │ gRPC (tfplugin6)
┌─────────────────────────▼───────────────────────────────────────┐
│                  terraform-provider-infrasim                     │
└─────────────────────────┬───────────────────────────────────────┘
                          │ gRPC
┌─────────────────────────▼───────────────────────────────────────┐
│                         infrasimd                                │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────────┐    │
│  │Reconciler│ │  State   │ │   QMP    │ │    Attestation   │    │
│  │   Loop   │ │ Manager  │ │  Client  │ │      Engine      │    │
│  └──────────┘ └──────────┘ └──────────┘ └──────────────────┘    │
└─────────────────────────┬───────────────────────────────────────┘
                          │ Process Management + QMP
┌─────────────────────────▼───────────────────────────────────────┐
│                   QEMU (qemu-system-aarch64)                     │
│                         -accel hvf                               │
└─────────────────────────┬───────────────────────────────────────┘
                          │ VNC (5900+)
┌─────────────────────────▼───────────────────────────────────────┐
│                       infrasim-web                               │
│                  (noVNC over WebSocket)                          │
└─────────────────────────────────────────────────────────────────┘
```

### Crates

| Crate | Binary | Description |
|-------|--------|-------------|
| `infrasim-daemon` | `infrasimd` | Background daemon managing QEMU processes, state, and reconciliation |
| `infrasim-cli` | `infrasim` | CLI for managing VMs, networks, volumes, attestation, SDN, and control plane |
| `infrasim-provider` | `terraform-provider-infrasim` | Terraform provider implementing tfplugin6 protocol |
| `infrasim-web` | `infrasim-web` | Web console server with noVNC integration and REST API |
| `infrasim-common` | — | Shared library: types, crypto, CAS, QMP, traffic shaping, pipeline analysis |
| `infrasim-e2e` | — | End-to-end integration tests |

---

## Installation

### Prerequisites

```bash
# Install QEMU (required)
brew install qemu

# Install Rust toolchain (if not installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install protobuf compiler (for gRPC code generation)
brew install protobuf
```

### Option 1: ISVM (Recommended)

ISVM (InfraSim Version Manager) provides nvm-style version management:

```bash
git clone https://github.com/rng-ops/infrasim.git
cd infrasim

# Install ISVM
./scripts/install-isvm.sh
source ~/.zshrc  # or ~/.bashrc

# Build and install
make build
isvm install

# Verify
infrasim --version
infrasimd --version
```

**ISVM Commands:**
| Command | Description |
|---------|-------------|
| `isvm install` | Install current build as a version |
| `isvm use <version>` | Switch to a specific version |
| `isvm list` | List installed versions |
| `isvm link` | Development mode (symlink to project binaries) |
| `isvm current` | Show current active version |

### Option 2: Manual Installation

```bash
git clone https://github.com/rng-ops/infrasim.git
cd infrasim

# Build release binaries
cargo build --release

# Install to ~/.local/bin (or adjust path)
cp target/release/infrasim ~/.local/bin/
cp target/release/infrasimd ~/.local/bin/
cp target/release/infrasim-web ~/.local/bin/
cp target/release/terraform-provider-infrasim ~/.local/bin/
```

---

## Quick Start

### 1. Start the Daemon

```bash
# Foreground (development)
infrasimd --foreground

# Background (production)
infrasimd
```

### 2. Create Resources via CLI

```bash
# Create a network
infrasim network create lab-network --cidr 192.168.100.0/24

# Create a VM
infrasim vm create my-vm \
  --cpus 4 \
  --memory 4096 \
  --disk /path/to/alpine.qcow2 \
  --network lab-network

# List VMs
infrasim vm list

# Access console
infrasim console my-vm
```

### 3. Use with Terraform

```hcl
terraform {
  required_providers {
    infrasim = {
      source  = "local/infrasim/infrasim"
      version = "0.1.0"
    }
  }
}

provider "infrasim" {
  daemon_address = "http://127.0.0.1:50051"
}

resource "infrasim_network" "lab" {
  name = "lab-network"
  cidr = "192.168.100.0/24"
}

resource "infrasim_vm" "workstation" {
  name       = "kali-workstation"
  cpus       = 4
  memory     = 4096
  disk       = "/var/lib/infrasim/images/kali-xfce-aarch64.qcow2"
  network_id = infrasim_network.lab.id

  # QoS simulation (optional)
  qos_latency_ms     = 50
  qos_jitter_ms      = 10
  qos_loss_percent   = 0.5
  qos_bandwidth_mbps = 100
}

output "console_url" {
  value = infrasim_vm.workstation.console_url
}
```

```bash
terraform init
terraform apply
open $(terraform output -raw console_url)
```

---

## CLI Reference

### Global Options

```bash
infrasim [OPTIONS] <COMMAND>

Options:
  --daemon-addr <URL>   Daemon address [default: http://127.0.0.1:50051]
  --format <FORMAT>     Output format: table, json, yaml [default: table]
  -v, --verbose         Enable verbose output
  -h, --help            Print help
  -V, --version         Print version
```

### Commands

| Command | Description |
|---------|-------------|
| `vm` | Manage virtual machines (create, list, start, stop, delete) |
| `network` | Manage virtual networks |
| `volume` | Manage disk volumes |
| `snapshot` | Create and restore snapshots |
| `console` | Access VM console (VNC) |
| `attestation` | View and verify cryptographic provenance |
| `artifact` | Inspect and verify build artifacts |
| `web` | Web server and UI management |
| `control` | Tailscale-based distributed node control plane |
| `pipeline` | Build pipeline management and analysis |
| `sdn` | Software-defined networking appliances and topologies |
| `benchmark` | Run performance benchmarks |
| `status` | Check daemon status |

### VM Management

```bash
# Create a VM
infrasim vm create <name> --cpus <n> --memory <mb> --disk <path> --network <name>

# List VMs
infrasim vm list

# Start/stop/restart
infrasim vm start <name>
infrasim vm stop <name>
infrasim vm restart <name>

# Delete
infrasim vm delete <name>

# Get details
infrasim vm get <name> --format json
```

### Attestation & Provenance

```bash
# Get attestation report for a VM
infrasim attestation get <vm-id>

# Verify signature
infrasim attestation verify <vm-id> --pubkey signing.pub

# Export for audit
infrasim attestation export <vm-id> --output report.json
```

### SDN Commands

```bash
# Create network appliances
infrasim sdn appliance create my-router --type router
infrasim sdn appliance create my-firewall --type firewall
infrasim sdn appliance create my-vpn --type vpn

# Manage mesh topologies
infrasim sdn mesh create production-mesh --peers node1,node2,node3
infrasim sdn mesh status production-mesh

# Apply Terraform topologies
infrasim sdn topology apply --file topology.tf
```

### Control Plane

```bash
# List nodes in Tailscale mesh
infrasim control nodes list

# Deploy image to remote node
infrasim control deploy <node> --image alpine-wg.qcow2

# Retrieve logs from node
infrasim control logs <node> --follow

# Execute command on node
infrasim control exec <node> -- infrasim vm list
```

### Artifact Inspection

```bash
# Inspect a qcow2 image
infrasim artifact inspect /path/to/image.qcow2

# Verify checksums
infrasim artifact verify /path/to/image.qcow2 --sha256 <expected>

# Check for tampering
infrasim artifact tamper-check /path/to/image.qcow2
```

---

## VPN & Network Options

InfraSim supports multiple VPN and networking configurations through the feature overlay system:

### WireGuard Mesh

Cryptographically verified peer-to-peer mesh:

```bash
# Build WireGuard profile
cd images/alpine
./build-profile.sh wg-mesh-ipv6

# Features included:
# - base-minimal: Core utilities, cloud-init, selftest framework
# - vpn-wireguard: WireGuard with Ed25519 peer verification
# - rendezvous-ipv6: Epoch-based IPv6 peer discovery
```

**Security Features:**
- Narrow AllowedIPs by default (`/32` IPv4, `/128` IPv6)
- Ed25519 signature verification before widening ranges
- Signed peer descriptors required for admission

### Tailscale Managed

Enterprise-ready managed mesh:

```bash
./build-profile.sh ts-managed

# Features included:
# - base-minimal
# - vpn-tailscale: Tailscale with restrictive defaults
```

**Security Defaults:**
- `accept-routes=false` — No automatic route injection
- `accept-dns=false` — No DNS override
- `exit-node` disabled by default
- Auth keys never stored in config files

### Dual VPN (WireGuard + Tailscale)

Separated tunnels with policy routing:

```bash
./build-profile.sh dual-vpn-separated

# Features:
# - vpn-wireguard on wg0 (data plane)
# - vpn-tailscale on tailscale0 (control plane)
# - Policy routing to separate traffic
```

### mTLS Control Plane

Add mutual TLS authentication:

```bash
./build-profile.sh ts-mtls

# Adds control-mtls feature:
# - Certificate chain verification
# - Client authentication required
# - 24-hour expiry checks
```

### Discovery Options

| Feature | Description | Multicast Required |
|---------|-------------|-------------------|
| `rendezvous-ipv6` | HMAC-derived link-local addresses | No |
| `discovery-bonjour` | mDNS/Avahi service advertisement | Yes |

---

## Alpine Image Profiles

### Available Profiles

| Profile | Features | Use Case |
|---------|----------|----------|
| `no-vpn-minimal` | base-minimal | Baseline testing |
| `wg-mesh-ipv6` | base + wireguard + rendezvous-ipv6 | Standalone WireGuard mesh |
| `ts-managed` | base + tailscale | Tailscale-managed infrastructure |
| `dual-vpn-separated` | base + wireguard + tailscale | Dual VPN with policy routing |
| `wg-bonjour` | base + wireguard + discovery-bonjour | WireGuard + LAN discovery |
| `ts-mtls` | base + tailscale + control-mtls | Tailscale + mTLS control plane |

### Available Features

| Feature | Description |
|---------|-------------|
| `base-minimal` | Cloud-init, selftest framework, Ed25519 verification, nftables |
| `vpn-wireguard` | WireGuard tools, peer admission with signature verification |
| `vpn-tailscale` | Tailscale daemon with security-hardened defaults |
| `rendezvous-ipv6` | Epoch/slot-based IPv6 peer discovery daemon |
| `control-mtls` | Mutual TLS for control plane connections |
| `discovery-bonjour` | Avahi/mDNS service discovery (optional) |
| `wan-nat` | NAT traversal with STUN probing |
| `wan-nat64` | NAT64 translation via TAYGA |

### Building Profiles

```bash
cd images/alpine

# Compose profile (generates merged config)
./compose-profile.sh wg-mesh-ipv6

# Build qcow2 image
./build-profile.sh wg-mesh-ipv6 \
  --base-image alpine-base.qcow2 \
  --output output/wg-mesh-ipv6.qcow2

# Run selftests
python3 features/vpn-wireguard/selftest/test_wireguard.py
```

### Creating Custom Profiles

```yaml
# profiles/my-custom.yaml
name: my-custom
description: Custom profile with WireGuard and mTLS
base: alpine:3.19

features:
  - base-minimal
  - vpn-wireguard
  - control-mtls

feature_config:
  vpn-wireguard:
    interface: wg0
    listen_port: 51820
  control-mtls:
    ca_path: /etc/infrasim/ca.crt
    verify_peer: true

test_requirements:
  minimum_memory_mb: 256
  requires_network: true
```

---

## CI/CD Integration

### GitHub Actions Workflows

| Workflow | Trigger | Description |
|----------|---------|-------------|
| `build.yml` | Push to main/develop, tags | Build release binaries, run tests |
| `tests.yml` | Push, PR to main | Unit tests, integration tests, attestation tests |
| `image-snapshots.yml` | Manual, scheduled | Build Alpine images with provenance |
| `build-alpine-profiles.yml` | Changes to images/alpine | Validate and build all profiles |
| `build-alpine-variants.yml` | Changes to variants | Legacy variant builds |
| `snapshots.yml` | Schedule | Periodic snapshot builds |

### Workflow Features

- **macOS M1 Runners** — Native ARM64 builds on `macos-14`
- **Cargo Caching** — Registry, index, and target directory caching
- **Provenance Generation** — SHA256 hashes and signed attestations
- **Artifact Upload** — Binaries, images, and provenance files
- **Matrix Builds** — Parallel profile building

### Local CI Reproduction

```bash
# Run the same tests as CI
cargo test --all

# Build all profiles
for profile in images/alpine/profiles/*.yaml; do
  name=$(basename "$profile" .yaml)
  ./images/alpine/build-profile.sh "$name"
done

# Validate workflows
ruby -ryaml -e "YAML.load_file('.github/workflows/build.yml')"
```

---

## Test Coverage

### Test Categories

| Category | Location | Description |
|----------|----------|-------------|
| Unit Tests | `crates/*/src/**/*.rs` | Per-module tests with `#[cfg(test)]` |
| Integration Tests | `crates/e2e/tests/` | End-to-end daemon tests |
| Feature Selftests | `images/alpine/features/*/selftest/` | In-image validation |

### Running Tests

```bash
# All unit tests
cargo test --all

# Specific crate
cargo test -p infrasim-common

# With output
cargo test -- --nocapture

# Ignored/expensive tests
cargo test -- --ignored

# E2E tests (requires running daemon)
cargo test -p infrasim-e2e
```

### Test Modules

**infrasim-common:**
- `artifact::tests` — SHA256 parsing, qcow2 header parsing, truncation detection
- `attestation::tests` — HVF check, attestation generation/verification
- `cas::tests` — Content-addressed storage put/get, deduplication, integrity
- `crypto::tests` — Ed25519 keypair generation, sign/verify, tamper detection
- `db::tests` — SQLite CRUD operations
- `pipeline::tests` — Dependency graph, cycle detection, network fingerprinting
- `qmp::tests` — QMP command serialization, response parsing
- `traffic_shaper::tests` — Latency shaping, packet loss, LoRa ToA

**Feature Selftests:**
- `test_base.py` — Cloud-init, packages, signature verification
- `test_wireguard.py` — Interface, peer admission, narrow AllowedIPs
- `test_tailscale.py` — Daemon status, accept-routes disabled, authkey leaks
- `test_rendezvous.py` — Daemon, slot scanning, address derivation
- `test_mtls.py` — Certificate chain, client auth, expiry
- `test_bonjour.py` — Avahi, service publication, reflector disabled
- `test_nat.py` — STUN probing, UPnP disabled
- `test_nat64.py` — TAYGA, NAT64 prefix, forwarding

---

## Dependencies

### Rust Crates

**Runtime:**
| Crate | Version | Purpose |
|-------|---------|---------|
| `tokio` | 1.35 | Async runtime |
| `tonic` | 0.11 | gRPC framework |
| `prost` | 0.12 | Protocol Buffers |
| `axum` | 0.7 | Web framework |
| `tower-http` | 0.5 | HTTP middleware |
| `rusqlite` | 0.31 | SQLite database |
| `clap` | 4.4 | CLI argument parsing |

**Serialization:**
| Crate | Version | Purpose |
|-------|---------|---------|
| `serde` | 1.0 | Serialization framework |
| `serde_json` | 1.0 | JSON support |
| `toml` | 0.8 | TOML config files |

**Cryptography:**
| Crate | Version | Purpose |
|-------|---------|---------|
| `ed25519-dalek` | 2.1 | Ed25519 signatures |
| `sha2` | 0.10 | SHA256 hashing |
| `rand` | 0.8 | Secure randomness |
| `hex` | 0.4 | Hex encoding |
| `base64` | 0.21 | Base64 encoding |

**Utilities:**
| Crate | Version | Purpose |
|-------|---------|---------|
| `anyhow` | 1.0 | Error handling |
| `thiserror` | 1.0 | Custom errors |
| `tracing` | 0.1 | Structured logging |
| `uuid` | 1.6 | UUID generation |
| `chrono` | 0.4 | Date/time |
| `nix` | 0.28 | Unix system calls |
| `ipnetwork` | 0.20 | IP address handling |

### System Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| QEMU | 8.0+ | Virtual machine hypervisor |
| protobuf | 3.x | Protocol buffer compiler |
| Rust | 1.75+ | Compiler toolchain |

### Python Dependencies (Selftests)

```
pyyaml
```

---

## Project Structure

```
infrasim/
├── Cargo.toml                 # Workspace manifest
├── Cargo.lock                 # Dependency lock file
├── Makefile                   # Build automation
├── build.sh                   # Build script
├── README.md                  # This file
│
├── crates/
│   ├── cli/                   # CLI binary (infrasim)
│   │   └── src/
│   │       ├── main.rs
│   │       ├── client.rs      # Daemon client
│   │       ├── output.rs      # Output formatting
│   │       └── commands/      # Subcommands
│   │           ├── vm.rs
│   │           ├── network.rs
│   │           ├── volume.rs
│   │           ├── snapshot.rs
│   │           ├── attestation.rs
│   │           ├── artifact.rs
│   │           ├── control.rs
│   │           ├── pipeline.rs
│   │           ├── sdn.rs
│   │           └── web.rs
│   │
│   ├── common/                # Shared library
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── artifact.rs    # Artifact inspection
│   │       ├── attestation.rs # Cryptographic attestation
│   │       ├── cas.rs         # Content-addressed storage
│   │       ├── crypto.rs      # Ed25519 signing
│   │       ├── db.rs          # SQLite wrapper
│   │       ├── error.rs       # Error types
│   │       ├── pipeline.rs    # Build pipeline analysis
│   │       ├── qmp.rs         # QEMU Machine Protocol
│   │       ├── traffic_shaper.rs # QoS simulation
│   │       └── types.rs       # Shared types
│   │
│   ├── daemon/                # Background daemon (infrasimd)
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config.rs      # Configuration
│   │       ├── grpc.rs        # gRPC server
│   │       ├── qemu.rs        # QEMU process management
│   │       ├── reconciler.rs  # State reconciliation
│   │       └── state.rs       # State management
│   │
│   ├── provider/              # Terraform provider
│   │   └── src/
│   │       └── main.rs
│   │
│   ├── web/                   # Web console server
│   │   └── src/
│   │       └── main.rs
│   │
│   └── e2e/                   # End-to-end tests
│       └── tests/
│
├── proto/
│   ├── infrasim.proto         # InfraSim gRPC definitions
│   └── tfplugin6.proto        # Terraform plugin protocol
│
├── images/
│   └── alpine/
│       ├── features/          # Feature overlays
│       │   ├── base-minimal/
│       │   ├── vpn-wireguard/
│       │   ├── vpn-tailscale/
│       │   ├── rendezvous-ipv6/
│       │   ├── control-mtls/
│       │   ├── discovery-bonjour/
│       │   ├── wan-nat/
│       │   └── wan-nat64/
│       ├── profiles/          # Profile compositions
│       ├── schemas/           # JSON schemas
│       ├── compose-profile.sh
│       └── build-profile.sh
│
├── docs/
│   ├── architecture.md
│   ├── api-reference.md
│   ├── FEATURE_OVERLAYS.md
│   ├── macos-m2-setup.md
│   └── ...
│
├── .github/
│   └── workflows/
│       ├── build.yml
│       ├── tests.yml
│       ├── image-snapshots.yml
│       └── build-alpine-profiles.yml
│
└── ui/                        # Web UI (TypeScript/React)
    ├── apps/console/
    └── packages/
```

---

## Configuration

### Daemon Configuration

```toml
# ~/.infrasim/config.toml

[daemon]
listen_address = "127.0.0.1:50051"
data_dir = "~/.infrasim"
log_level = "info"

[qemu]
binary = "/opt/homebrew/bin/qemu-system-aarch64"
accel = "hvf"
default_memory = 1024
default_cpus = 2

[web]
listen_address = "127.0.0.1:8080"
vnc_base_port = 5900

[attestation]
signing_key = "~/.infrasim/signing.key"
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `INFRASIM_DAEMON_ADDR` | Daemon gRPC address | `http://127.0.0.1:50051` |
| `INFRASIM_DATA_DIR` | Data directory | `~/.infrasim` |
| `INFRASIM_LOG_LEVEL` | Log level (trace, debug, info, warn, error) | `info` |
| `RUST_LOG` | Rust logging filter | — |

---

## Development

### Building from Source

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Build specific crate
cargo build -p infrasim-cli

# Check without building
cargo check --all
```

### Code Generation

Protocol buffer code is generated automatically via `build.rs`:

```bash
# Regenerate protobuf code
cargo build  # build.rs handles this

# Manual generation (if needed)
protoc --rust_out=crates/common/src/generated proto/infrasim.proto
```

### Debugging

```bash
# Run daemon with debug logging
RUST_LOG=debug cargo run -p infrasim-daemon -- --foreground

# Trace-level logging
RUST_LOG=trace infrasimd --foreground

# Specific module logging
RUST_LOG=infrasim_daemon::reconciler=debug infrasimd --foreground
```

### Code Style

```bash
# Format code
cargo fmt

# Lint
cargo clippy --all

# Fix warnings automatically
cargo fix --lib -p infrasim-common
```

---

## Maintenance

### Regular Tasks

| Task | Frequency | Command |
|------|-----------|---------|
| Update dependencies | Monthly | `cargo update` |
| Audit dependencies | Monthly | `cargo audit` |
| Check for outdated deps | Monthly | `cargo outdated` |
| Run full test suite | Before release | `cargo test --all` |
| Build release binaries | On tag | `cargo build --release` |

### Dependency Updates

```bash
# Check for outdated dependencies
cargo outdated

# Update all dependencies
cargo update

# Update specific dependency
cargo update -p tokio

# Audit for vulnerabilities
cargo audit
```

### Database Maintenance

```bash
# Database location
~/.infrasim/state.db

# Backup
cp ~/.infrasim/state.db ~/.infrasim/state.db.bak

# Vacuum (compact)
sqlite3 ~/.infrasim/state.db "VACUUM;"
```

### Log Management

```bash
# Log location (when running as service)
~/.infrasim/logs/

# View recent logs
tail -f ~/.infrasim/logs/daemon.log

# Rotate logs (if using logrotate)
logrotate /etc/logrotate.d/infrasim
```

### Troubleshooting

**Daemon won't start:**
```bash
# Check if already running
pgrep infrasimd

# Check port availability
lsof -i :50051

# Run in foreground for errors
infrasimd --foreground
```

**VM won't boot:**
```bash
# Check QEMU availability
which qemu-system-aarch64
qemu-system-aarch64 --version

# Check HVF support
sysctl kern.hv_support

# Check image validity
infrasim artifact inspect /path/to/image.qcow2
```

**Terraform provider issues:**
```bash
# Check provider binary
ls -la ~/.terraform.d/plugins/

# Enable debug logging
TF_LOG=DEBUG terraform apply
```

---

## License

Apache-2.0

---

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Code of Conduct

This project follows the [Contributor Covenant](https://www.contributor-covenant.org/) code of conduct.
