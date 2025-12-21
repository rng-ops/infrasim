# Alpine qcow2 Image Artifact Format

This document describes the structure and verification of InfraSim Alpine Linux
qcow2 image artifacts.

## Artifact Bundle Structure

The artifact is distributed as a `.tar.gz` file with the following structure:

```
infrasim-alpine-<gitsha>.tar.gz
├── disk/
│   ├── base.qcow2              # Main QEMU disk image
│   └── snapshots/
│       └── clean.qcow2         # External overlay snapshot (clean state)
├── meta/
│   ├── manifest.json           # SHA256 checksums of all files
│   ├── attestations/
│   │   ├── build-provenance.json    # Build inputs and environment
│   │   └── artifact-integrity.json  # Integrity attestation
│   ├── signatures/
│   │   ├── manifest.sig        # Ed25519 signature of manifest
│   │   └── signature-info.json # Signature metadata
│   └── logs/
│       ├── build.log.txt       # Build output log
│       └── qemu-img-info.txt   # Image metadata
└── README.md                   # Usage instructions
```

## Verification

### 1. Verify tarball checksum

```bash
# Download both files
wget https://github.com/rng-ops/infrasim/releases/download/v0.1.0/infrasim-alpine-abc1234.tar.gz
wget https://github.com/rng-ops/infrasim/releases/download/v0.1.0/infrasim-alpine-abc1234.tar.gz.sha256

# Verify checksum
shasum -a 256 -c infrasim-alpine-abc1234.tar.gz.sha256
```

### 2. Verify internal manifest

```bash
# Extract the bundle
tar -xzf infrasim-alpine-abc1234.tar.gz

# Verify each file against manifest
jq -r '.files[] | "\(.sha256)  \(.path)"' meta/manifest.json > checksums.txt
shasum -a 256 -c checksums.txt
```

### 3. Verify signature (when signing is enabled)

```bash
# Check signature status
cat meta/signatures/signature-info.json

# If signed, verify with public key
# (Future: use infrasim CLI)
infrasim verify --manifest meta/manifest.json --signature meta/signatures/manifest.sig
```

## Build Provenance

The `meta/attestations/build-provenance.json` file contains:

```json
{
  "format_version": "1.0",
  "build_type": "alpine-qcow2",
  "source": {
    "git_sha": "abc1234...",
    "git_branch": "main",
    "build_date": "2024-01-15T10:30:00Z"
  },
  "alpine": {
    "version": "3.20",
    "arch": "aarch64",
    "mirror": "https://dl-cdn.alpinelinux.org/alpine"
  },
  "packages": [
    "alpine-base",
    "bash",
    "curl",
    "ca-certificates",
    "iproute2",
    "iptables",
    "nftables",
    "openssh-client",
    "tcpdump",
    "jq",
    "python3"
  ],
  "inputs": {
    "minirootfs_url": "https://...",
    "minirootfs_sha256": "..."
  }
}
```

## Booting the Image

### QEMU (macOS Apple Silicon with HVF)

```bash
qemu-system-aarch64 \
  -M virt \
  -accel hvf \
  -cpu host \
  -m 512 \
  -smp 2 \
  -drive file=disk/base.qcow2,format=qcow2,if=virtio \
  -device virtio-net-pci,netdev=net0 \
  -netdev user,id=net0,hostfwd=tcp::2222-:22 \
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -nographic
```

### QEMU (Linux x86_64 with KVM, emulating ARM64)

```bash
qemu-system-aarch64 \
  -M virt \
  -cpu cortex-a72 \
  -m 512 \
  -smp 2 \
  -drive file=disk/base.qcow2,format=qcow2,if=virtio \
  -device virtio-net-pci,netdev=net0 \
  -netdev user,id=net0,hostfwd=tcp::2222-:22 \
  -bios /usr/share/qemu-efi-aarch64/QEMU_EFI.fd \
  -nographic
```

### InfraSim with Terraform

```hcl
resource "infrasim_vm" "alpine" {
  name   = "alpine-runner"
  cpus   = 2
  memory = 512
  disk   = "/path/to/disk/base.qcow2"
  
  network_id = infrasim_network.mynet.id
}
```

## Using External Snapshots

The `disk/snapshots/clean.qcow2` is an external overlay that references the
base image. This allows you to:

1. Boot from a clean state without modifying the base image
2. Discard changes by deleting the overlay
3. Create multiple independent snapshots

```bash
# Boot from clean overlay (changes won't affect base)
qemu-system-aarch64 \
  ... \
  -drive file=disk/snapshots/clean.qcow2,format=qcow2,if=virtio
```

## Reproducibility

This image is built with the following reproducibility measures:

1. **Pinned Alpine version**: The exact Alpine release is specified
2. **Pinned package list**: All packages are explicitly listed
3. **Pinned mirror**: The Alpine mirror URL is recorded
4. **Normalized timestamps**: `SOURCE_DATE_EPOCH` is used where possible
5. **Recorded inputs**: All input checksums are in the provenance file

### Limitations

Full bit-for-bit reproducibility is not yet guaranteed due to:

- QEMU disk image creation may include timestamps
- Cloud-init generates unique instance IDs
- Some packages may have non-deterministic builds

Future enhancements will address these limitations.

## Telemetry Agent (LoRaWAN Placeholder)

The image includes a telemetry agent stub for future LoRaWAN integration:

- **Config**: `/etc/infrasim/telemetry/config.json`
- **Script**: `/opt/infrasim/telemetry/telemetry-agent.sh`
- **Log**: `/var/log/infrasim-telemetry.log`

This is a simulation/logging placeholder only. It does NOT include real SDR
drivers or radio transmission code.

## Default Credentials

| Username | Password |
|----------|----------|
| infrasim | infrasim |
| root     | (disabled) |

## Included Tools

- **Networking**: iproute2, iptables, nftables, tcpdump
- **Utilities**: bash, curl, wget, jq, python3
- **SSH**: openssh-client, openssh-server
- **Cloud**: cloud-init

## Security Considerations

1. **Change default passwords** before using in production
2. **Verify checksums** before deploying
3. **Review provenance** to ensure build integrity
4. **Disable SSH password auth** if using in untrusted environments

## Building Locally

```bash
cd images/alpine
make build       # Build the qcow2
make boot-test   # Verify it boots
make bundle      # Create the .tar.gz bundle
```

See `images/alpine/README.md` for detailed build instructions.
