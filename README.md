# InfraSim

**Terraform-Compatible QEMU Platform for macOS Apple Silicon**

InfraSim is a production-ready virtualization platform that brings Terraform-style infrastructure-as-code to QEMU virtual machines on macOS. It's designed specifically for Apple Silicon Macs running ARM64 guests at native speed using HVF (Hypervisor.framework) acceleration.

## Features

- 🚀 **Native Performance** - HVF acceleration for near-native ARM64 VM performance
- 🔧 **Terraform Compatible** - Full Terraform/OpenTofu provider for infrastructure-as-code
- 🌐 **Browser Console** - noVNC-based web console for graphical VM access
- 📸 **Snapshots** - Memory and disk snapshots for instant restore
- 🔐 **Cryptographic Attestation** - Ed25519 signed provenance reports
- 📊 **QoS Simulation** - Latency, jitter, packet loss, and bandwidth shaping
- 🗄️ **Content-Addressed Storage** - SHA256-based deduplication

## Quick Start

### Prerequisites

```bash
# Install QEMU
brew install qemu

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Installation

```bash
# Clone the repository
git clone https://github.com/infrasim/infrasim.git
cd infrasim

# Build all components
cargo build --release

# Install binaries
cargo install --path crates/daemon
cargo install --path crates/cli
```

### Start the Daemon

```bash
# Run in foreground for development
infrasimd --foreground

# Or as a background service
infrasimd
```

### Use with Terraform

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
  name = "lab-ånetwork"
  cidr = "192.168.100.0/24"
}

resource "infrasim_vm" "kali" {
  name       = "kali-workstation"
  cpus       = 4
  memory     = 4096
  disk       = "/var/lib/infrasim/images/kali-xfce-aarch64.qcow2"
  network_id = infrasim_network.lab.id
}

output "console_url" {
  value = infrasim_vm.kali.console_url
}
```

```bash
# Apply configuration
terraform apply

# Access the VM console in your browser
open $(terraform output -raw console_url)
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Terraform / OpenTofu                     │
└─────────────────────────┬───────────────────────────────────┘
                          │ gRPC (tfplugin6)
┌─────────────────────────▼───────────────────────────────────┐
│                  terraform-provider-infrasim                 │
└─────────────────────────┬───────────────────────────────────┘
                          │ gRPC
┌─────────────────────────▼───────────────────────────────────┐
│                        infrasimd                             │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐    │
│  │Reconciler│ │  State   │ │   QMP    │ │  Attestation │    │
│  │   Loop   │ │ Manager  │ │  Client  │ │   Engine     │    │
│  └──────────┘ └──────────┘ └──────────┘ └──────────────┘    │
└─────────────────────────┬───────────────────────────────────┘
                          │ Process Management
┌─────────────────────────▼───────────────────────────────────┐
│                   QEMU (qemu-system-aarch64)                 │
│                        -accel hvf                            │
└─────────────────────────┬───────────────────────────────────┘
                          │ VNC (5900+)
┌─────────────────────────▼───────────────────────────────────┐
│                      Web Console                             │
│               (noVNC over WebSocket)                         │
└─────────────────────────────────────────────────────────────┘
```

## Components

| Component | Description |
|-----------|-------------|
| `infrasimd` | Background daemon managing QEMU processes |
| `infrasim` | CLI tool for managing VMs, networks, and volumes |
| `terraform-provider-infrasim` | Terraform provider binary |
| `infrasim-web` | Web console server with noVNC integration |
| `infrasim-common` | Shared library (types, crypto, storage) |

## Documentation

- [macOS M2 Setup Guide](docs/macos-m2-setup.md)
- [Architecture Overview](docs/architecture.md)
- [API Reference](docs/api-reference.md)
- [Terraform Provider](docs/terraform-provider.md)
- [Building Images](images/kali-xfce-vnc-aarch64/README.md)

## Resources

| Resource Type | Description |
|---------------|-------------|
| `infrasim_network` | Virtual network with NAT, bridge, or isolated modes |
| `infrasim_vm` | Virtual machine with optional QoS simulation |
| `infrasim_volume` | Disk volume (qcow2 or raw format) |
| `infrasim_snapshot` | Memory and/or disk snapshot |

## QoS Simulation

InfraSim can simulate real-world network conditions:

```hcl
resource "infrasim_vm" "remote_target" {
  name = "target-across-wan"
  # ...

  # Simulate 50ms latency with 10ms jitter
  qos_latency_ms     = 50
  qos_jitter_ms      = 10

  # 0.5% packet loss
  qos_loss_percent   = 0.5

  # 100 Mbps bandwidth limit
  qos_bandwidth_mbps = 100
}
```

## Attestation

Every VM includes cryptographic provenance:

```bash
# Get attestation report
infrasim attestation get <vm-id>

# Verify signature
infrasim attestation verify <vm-id> --pubkey signing.pub

# Export for audit
infrasim attestation export <vm-id> --output report.json
```

Reports include:
- Host system information (hostname, OS, kernel, QEMU version)
- HVF acceleration status
- Disk image SHA256 hash
- Ed25519 signature

## Development

```bash
# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run -p infrasim-daemon -- --foreground

# Generate protobuf code
cargo build  # build.rs handles this automatically
```

## License

MIT OR Apache-2.0

## Contributing

Contributions welcome! Please read the contributing guidelines first.
