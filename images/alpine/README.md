# InfraSim Alpine Linux Image

Reproducible QEMU-bootable Alpine Linux qcow2 image for InfraSim with a
modular feature overlay system for building specialized profiles.

## Quick Start

```bash
# Build a profile using the feature overlay system
./compose-profile.sh wg-mesh-ipv6
./build-profile.sh wg-mesh-ipv6 --sign-key /path/to/key

# Or use legacy build
make build
```

## Feature Overlay System (New)

The feature overlay system provides modular, composable image building:

```
features/           # Modular capabilities
├── base-minimal/   # Core system, cloud-init, selftest
├── vpn-wireguard/  # WireGuard mesh networking
├── vpn-tailscale/  # Tailscale managed networking
├── rendezvous-ipv6/# IPv6 peer discovery
├── control-mtls/   # mTLS control plane
├── discovery-bonjour/ # mDNS/Bonjour (LAN)
└── wan-nat/        # NAT traversal

profiles/           # Profile compositions
├── no-vpn-minimal.yaml    # Base only
├── wg-mesh-ipv6.yaml      # WireGuard + IPv6 rendezvous
├── ts-managed.yaml        # Tailscale managed
├── dual-vpn-separated.yaml # Both with policy routing
├── wg-bonjour.yaml        # WireGuard + mDNS
└── ts-mtls.yaml           # Tailscale + mTLS
```

See [Feature Overlay Documentation](docs/FEATURE_OVERLAYS.md) for details.

## Features

- **Minimal Alpine Linux** (3.19, ~150MB compressed)
- **QEMU-bootable** with virtio drivers
- **Network tools**: iproute2, iptables, nftables, tcpdump
- **VPN options**: WireGuard, Tailscale, or both
- **IPv6 Rendezvous**: Peer discovery without multicast
- **Ed25519 Signatures**: Cryptographic peer verification
- **Selftest Framework**: Built-in validation
- **Signed Provenance**: in-toto attestations
- **Utilities**: bash, curl, jq, python3
- **Cloud-init** for automated configuration

## Prerequisites

- QEMU (`brew install qemu`)
- curl, jq
- For macOS: HVF acceleration available

## Build Options

```bash
# Custom Alpine version
ALPINE_VERSION=3.19 make build

# Custom image size
IMAGE_SIZE=4G make build

# Custom output location
./build-alpine-qcow2.sh --output /path/to/image.qcow2
```

## Output Structure

```
output/
├── base.qcow2           # Main disk image
├── logs/
│   ├── build.log.txt    # Build output
│   └── qemu-img-info.txt
├── meta/
│   ├── build-provenance.json
│   └── minirootfs.sha256
└── work/                # Build artifacts (can be deleted)
```

## Reproducibility

The build process is designed for reproducibility:

1. Alpine version is pinned
2. Package list is explicit
3. Mirror URL is recorded
4. All input checksums are saved
5. `SOURCE_DATE_EPOCH` normalizes timestamps

See `output/meta/build-provenance.json` for full build details.

## Telemetry Agent

The image includes a placeholder telemetry agent for future LoRaWAN integration:

```
telemetry/
├── lorawan-config.json   # Configuration template
└── telemetry-agent.sh    # Stub agent script
```

**IMPORTANT**: This is simulation/logging only. No real SDR or radio code.

## Testing with QEMU

```bash
# macOS Apple Silicon (HVF)
qemu-system-aarch64 \
  -M virt -accel hvf -cpu host \
  -m 512 -smp 2 \
  -drive file=output/base.qcow2,format=qcow2,if=virtio \
  -device virtio-net-pci,netdev=net0 \
  -netdev user,id=net0,hostfwd=tcp::2222-:22 \
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -nographic

# Connect via SSH (after boot)
ssh -p 2222 infrasim@localhost
```

## Using with InfraSim

See `examples/terraform/alpine_runner/` for a complete example.

```hcl
resource "infrasim_vm" "alpine" {
  name   = "alpine-runner"
  cpus   = 2
  memory = 512
  disk   = "path/to/base.qcow2"
}
```

## Default Credentials

| User | Password |
|------|----------|
| infrasim | infrasim |

## CI/CD

The image is built automatically by GitHub Actions:

- Workflow: `.github/workflows/build-images.yml`
- Artifacts: Available in GitHub Releases
- Bundle: `infrasim-alpine-<gitsha>.tar.gz`

## Files

| File | Description |
|------|-------------|
| `build-alpine-qcow2.sh` | Main build script |
| `boot-test.sh` | Headless QEMU boot test |
| `Makefile` | Build automation |
| `telemetry/` | LoRaWAN agent stub |
