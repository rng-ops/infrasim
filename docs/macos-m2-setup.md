# macOS M2 Setup Guide

This guide covers setting up InfraSim on macOS with Apple Silicon (M1/M2/M3/M4).

## Prerequisites

### 1. Install Homebrew

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

### 2. Install QEMU

```bash
brew install qemu
```

Verify installation:
```bash
qemu-system-aarch64 --version
# QEMU emulator version 8.x.x
```

### 3. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### 4. Install Protocol Buffers

```bash
brew install protobuf
```

### 5. Install OpenTofu (or Terraform)

```bash
brew install opentofu
# Or: brew install terraform
```

## Building InfraSim

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

## Installing the Terraform Provider

```bash
# Create provider directory
PROVIDER_DIR=~/.terraform.d/plugins/local/infrasim/infrasim/0.1.0/darwin_arm64
mkdir -p "$PROVIDER_DIR"

# Copy provider binary
cp target/release/terraform-provider-infrasim "$PROVIDER_DIR/"

# Make executable
chmod +x "$PROVIDER_DIR/terraform-provider-infrasim"
```

## Verify HVF Support

Apple's Hypervisor.framework (HVF) provides hardware-accelerated virtualization:

```bash
# Check if HVF is available
sysctl kern.hv_support
# kern.hv_support: 1

# Test with QEMU
qemu-system-aarch64 -accel help
# Accelerators supported in QEMU binary:
# hvf
# tcg
```

## UEFI Firmware

QEMU requires UEFI firmware for ARM64 VMs:

```bash
# Verify firmware location
ls /opt/homebrew/share/qemu/edk2-aarch64-code.fd
```

If not present, reinstall QEMU:
```bash
brew reinstall qemu
```

## Creating Data Directory

```bash
# Create InfraSim data directory
sudo mkdir -p /var/lib/infrasim/{images,volumes,snapshots,state}
sudo chown -R $(whoami) /var/lib/infrasim
```

## Running the Daemon

### Foreground Mode (Development)

```bash
infrasimd --foreground
```

### Background Mode

```bash
infrasimd
```

### With Custom Settings

```bash
infrasimd \
  --config /etc/infrasim/config.toml \
  --data-dir /var/lib/infrasim \
  --grpc-addr 127.0.0.1:50051 \
  --web-addr 127.0.0.1:8080
```

## Building VM Images

### Download Pre-built Image

```bash
# Example: Debian ARM64 cloud image
curl -L -o /var/lib/infrasim/images/debian-arm64.qcow2 \
  https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-arm64.qcow2
```

### Build Kali Linux Image

```bash
cd images/kali-xfce-vnc-aarch64
chmod +x build.sh
./build.sh /var/lib/infrasim/images
```

## Quick Test

```bash
# Start daemon
infrasimd --foreground &

# Create a network
infrasim network create --name test --cidr 192.168.100.0/24

# List networks
infrasim network list

# Check daemon status
infrasim status
```

## Terraform Example

```bash
cd examples/terraform

# Initialize
tofu init

# Plan
tofu plan

# Apply
tofu apply

# Get console URL
tofu output console_url

# Destroy
tofu destroy
```

## Troubleshooting

### HVF Not Available

```
error: HVF acceleration not available
```

**Solution:** Ensure you're on macOS 11+ with Apple Silicon, and no other
hypervisors are running (Parallels, VMware, etc. may conflict).

### QEMU Crashes

```
qemu-system-aarch64: Hypervisor.framework error: HV_ERROR
```

**Solution:** Check system integrity protection isn't blocking HVF:
```bash
csrutil status
```

### Permission Denied

```
error: Cannot bind to /var/lib/infrasim
```

**Solution:**
```bash
sudo chown -R $(whoami) /var/lib/infrasim
```

### VNC Connection Refused

The VM may still be booting. Wait 30-60 seconds for cloud-init to complete.

Check VM status:
```bash
infrasim vm list
infrasim vm get <vm-id>
```

### Firewall Blocking

macOS firewall may block QEMU networking:

1. Open System Preferences → Security & Privacy → Firewall
2. Click "Firewall Options..."
3. Add qemu-system-aarch64 and allow incoming connections

## Performance Tuning

### Optimal VM Settings

```hcl
resource "infrasim_vm" "optimized" {
  name   = "high-perf"
  cpus   = 8        # Match host cores for best performance
  memory = 8192     # Sufficient RAM for workload
  
  # Use virtio for best I/O performance (InfraSim default)
}
```

### Disk Performance

For best disk I/O:
- Use qcow2 format with virtio-blk
- Preallocate disk space for write-heavy workloads
- Consider using NVMe passthrough for databases

## Next Steps

- [Create your first VM with Terraform](../examples/terraform/README.md)
- [Build custom images](../images/kali-xfce-vnc-aarch64/README.md)
- [CLI Reference](cli-reference.md)
- [API Documentation](api-reference.md)
