# Kali Linux XFCE ARM64 Image for InfraSim

This directory contains build files for creating a Kali Linux ARM64 disk image
optimized for use with InfraSim on macOS Apple Silicon.

## Features

- **Kali Linux Rolling** - Latest security tools and packages
- **XFCE Desktop** - Lightweight desktop environment
- **VNC Server** - TigerVNC for remote graphical access
- **Cloud-init** - Automated configuration on boot
- **ARM64 Native** - Runs at native speed with HVF acceleration

## Quick Start

```bash
# Make the build script executable
chmod +x build.sh

# Build the image
./build.sh

# Or specify custom output directory
./build.sh /path/to/images
```

## Prerequisites

1. **QEMU** for image manipulation:
   ```bash
   brew install qemu
   ```

2. **7-Zip** (optional, for official Kali images):
   ```bash
   brew install p7zip
   ```

## Build Output

The build script creates:

| File | Description |
|------|-------------|
| `kali-xfce-aarch64.qcow2` | Main disk image (32GB sparse) |
| `cloud-init.iso` | Cloud-init configuration ISO |
| `uefi/QEMU_EFI.fd` | UEFI firmware symlink |

## Default Credentials

| Username | Password |
|----------|----------|
| kali | kali |

**Note:** Password can be changed via cloud-init user-data.

## Using with InfraSim

1. Build the image:
   ```bash
   ./build.sh /var/lib/infrasim/images
   ```

2. Reference in Terraform:
   ```hcl
   resource "infrasim_vm" "kali" {
     name   = "kali-workstation"
     cpus   = 4
     memory = 4096
     disk   = "/var/lib/infrasim/images/kali-xfce-aarch64.qcow2"
   }
   ```

3. Access via web console or VNC.

## Manual Testing

Test the image directly with QEMU:

```bash
qemu-system-aarch64 \
  -M virt,highmem=on \
  -accel hvf \
  -cpu host \
  -smp 4 \
  -m 4096 \
  -drive file=kali-xfce-aarch64.qcow2,format=qcow2,if=virtio \
  -drive file=cloud-init.iso,format=raw,if=virtio \
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -device virtio-net-pci,netdev=net0 \
  -netdev user,id=net0,hostfwd=tcp::2222-:22,hostfwd=tcp::5901-:5901 \
  -nographic
```

Then connect via SSH or VNC:
```bash
ssh -p 2222 kali@localhost
# Or VNC to localhost:5901
```

## Customization

### Adding Packages

Edit `cloud.cfg` or use cloud-init user-data:

```yaml
#cloud-config
packages:
  - metasploit-framework
  - burpsuite
  - zaproxy
```

### Changing Resolution

VNC resolution can be changed via environment variable:
```yaml
runcmd:
  - export VNC_GEOMETRY=2560x1440
  - /usr/local/bin/vnc-startup.sh
```

## Files

| File | Description |
|------|-------------|
| `Dockerfile` | Container build (for reference) |
| `build.sh` | Main build script |
| `cloud.cfg` | Cloud-init configuration |
| `entrypoint.sh` | Container entrypoint |
| `vnc-startup.sh` | VNC server startup script |

## Troubleshooting

### Image won't boot
- Ensure UEFI firmware is installed: `brew install qemu`
- Check firmware path: `/opt/homebrew/share/qemu/edk2-aarch64-code.fd`

### VNC connection refused
- Wait for cloud-init to complete (check with `cloud-init status`)
- Verify VNC is running: `ss -tlnp | grep 5901`

### Slow performance
- Ensure HVF acceleration is enabled (`-accel hvf`)
- Check you're using `virt` machine type with `highmem=on`
