# Feature Overlay System Documentation

This document describes the feature overlay system for building Alpine-based
infrasim images with modular, composable capabilities.

## Overview

The feature overlay system replaces the previous hard-coded variants with:

1. **Features**: Modular capability overlays (VPN, discovery, security)
2. **Profiles**: Compositions of features with specific configurations
3. **Schemas**: JSON schemas for validation
4. **Selftests**: Built-in validation for each feature
5. **Provenance**: Signed attestations for builds

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Profile                                  │
│  (e.g., wg-mesh-ipv6.yaml)                                      │
├─────────────────────────────────────────────────────────────────┤
│ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐                 │
│ │base-minimal │ │vpn-wireguard│ │rendezvous-  │                 │
│ │             │ │             │ │ipv6         │                 │
│ │ - cloud-init│ │ - wg-tools  │ │ - rendezvous│                 │
│ │ - selftest  │ │ - apply-    │ │   daemon    │                 │
│ │ - ssh       │ │   peers.sh  │ │ - HMAC addr │                 │
│ │ - nftables  │ │ - narrow    │ │   derivation│                 │
│ │             │ │   admission │ │             │                 │
│ └─────────────┘ └─────────────┘ └─────────────┘                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Composed Output                             │
│  packages.txt, files.txt, firewall.nft, selftests.txt           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                       qcow2 Image                                │
│  + provenance.json + signature                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Security Model

### Core Principles

1. **Discovery is convenience, identity is cryptographic**
   - Rendezvous/mDNS help nodes find each other
   - Ed25519 signatures verify who nodes actually are

2. **Narrow admission by default**
   - New peers get minimal AllowedIPs (/32 or /128)
   - Full ranges only after signature verification

3. **Signed provenance**
   - All builds produce signed attestations
   - Chain of custody from source to image

### Peer Admission Flow

```
  New Peer Discovered
         │
         ▼
┌──────────────────┐
│ Add with narrow  │
│ AllowedIPs:      │
│ - IPv4: /32      │
│ - IPv6: /128     │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ Verify Ed25519   │──────▶ FAIL: Remove peer
│ signature        │
└────────┬─────────┘
         │ PASS
         ▼
┌──────────────────┐
│ Widen AllowedIPs │
│ to declared      │
│ ranges           │
└──────────────────┘
```

### Trust Model

```
┌─────────────────────────────────────────────────────────────────┐
│                      Trusted Signers                             │
│  /etc/infrasim/trusted-signers/                                 │
│  ├── control-plane.pub                                          │
│  ├── build-system.pub                                           │
│  └── admin.pub                                                  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ Verifies
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Node Descriptors                              │
│  - node_id                                                       │
│  - identity (wg_public_key, ed25519_public_key)                 │
│  - endpoints (rendezvous_ipv6, wireguard, control_mtls)         │
│  - attestation (signature by trusted signer)                    │
└─────────────────────────────────────────────────────────────────┘
```

## IPv6 Rendezvous Protocol

The IPv6 rendezvous protocol provides peer discovery without LAN multicast.

### How It Works

1. **Shared Secret**: All mesh nodes share a `mesh_secret`
2. **Epoch/Slot Timing**: Time divided into epochs (e.g., 60s) with slots
3. **Address Derivation**: `addr = fe80::HMAC(secret, epoch || slot)[0:8]`
4. **Discovery Window**: Nodes bind to derived address during their slot
5. **Descriptor Exchange**: Node descriptors broadcast on slot

### Timing Diagram

```
Epoch N                                    Epoch N+1
├────────┬────────┬────────┬────────┼────────┬────────
│ Slot 0 │ Slot 1 │ Slot 2 │ Slot 3 │ Slot 0 │ ...
│ addr_0 │ addr_1 │ addr_2 │ addr_3 │ addr_0'│
└────────┴────────┴────────┴────────┴────────┴────────

For each slot:
1. Derive IPv6 address
2. Add address to interface
3. Broadcast descriptor
4. Listen for peer descriptors
5. Remove address
```

### Configuration

```yaml
rendezvous-ipv6:
  mesh_secret: "shared-secret-32-bytes"
  epoch_seconds: 60
  slots_per_epoch: 4
  slot_duration_ms: 500
```

## Feature Reference

### base-minimal

**Purpose**: Core system foundation

**Packages**:
- alpine-base, busybox, openrc
- openssh, cloud-init
- iproute2, iptables, nftables
- python3, curl, jq

**Key Files**:
- `/usr/local/bin/verify-signature.sh` - Ed25519 verification
- `/etc/infrasim/node-descriptor.json` - Node identity (template)

**Selftests**:
- Network interfaces up
- Services running
- IPv6 enabled
- Firewall loaded
- SSH keys secure

### vpn-wireguard

**Purpose**: WireGuard mesh networking

**Packages**: wireguard-tools

**Key Files**:
- `/usr/local/bin/apply-peers.sh` - Peer admission with verification
- `/usr/local/bin/wg-keygen.sh` - Key generation
- `/etc/wireguard/wg0.conf.template` - Interface template

**Config Options**:
```yaml
vpn-wireguard:
  listen_port: 51820
  network_cidr: "10.100.0.0/16"
  narrow_admission: true  # Recommended
  persistent_keepalive: 25
```

### vpn-tailscale

**Purpose**: Tailscale managed networking

**Packages**: tailscale

**Key Files**:
- `/usr/local/bin/tailscale-up.sh` - Initialize and authenticate
- `/usr/local/bin/tailscale-check.sh` - Status checks

**Config Options**:
```yaml
vpn-tailscale:
  auth_key: "tskey-..."  # Required
  accept_routes: true
  exit_node: false
```

### rendezvous-ipv6

**Purpose**: IPv6 peer discovery

**Packages**: python3, py3-cryptography

**Key Files**:
- `/usr/local/bin/rendezvousd` - Discovery daemon
- `/etc/infrasim/rendezvous.conf` - Configuration

**Config Options**:
```yaml
rendezvous-ipv6:
  mesh_secret: "..."  # Required
  epoch_seconds: 60
  slots_per_epoch: 4
```

### control-mtls

**Purpose**: mTLS for control plane

**Packages**: openssl, ca-certificates

**Key Files**:
- `/usr/local/bin/mtls-setup.sh` - Certificate management
- `/usr/local/bin/verify-mtls.sh` - Verification script

### discovery-bonjour

**Purpose**: mDNS/Bonjour (LAN only)

**Packages**: avahi-daemon, dbus

**Key Files**:
- `/etc/avahi/services/infrasim.service` - Service definition

**Note**: Only works on local LAN segment

### wan-nat

**Purpose**: NAT traversal

**Packages**: stun-client

**Key Files**:
- `/usr/local/bin/nat-detect.sh` - NAT type detection

## Profile Reference

### no-vpn-minimal

Minimal base for development/testing.

```bash
./compose-profile.sh no-vpn-minimal
./build-profile.sh no-vpn-minimal
```

### wg-mesh-ipv6

Self-organizing WireGuard mesh with IPv6 rendezvous.

**Required**: `mesh_secret` in cloud-init

```yaml
#cloud-config
write_files:
  - path: /etc/infrasim/rendezvous.conf
    content: |
      mesh_secret=your-32-byte-secret
```

### ts-managed

Tailscale with managed control plane.

**Required**: `auth_key` in cloud-init

```yaml
#cloud-config
write_files:
  - path: /etc/infrasim/tailscale.conf
    content: |
      auth_key=tskey-auth-...
```

### dual-vpn-separated

Both VPNs with policy routing.

**Required**: `mesh_secret` + `auth_key`

### wg-bonjour

WireGuard with mDNS for LAN discovery.

### ts-mtls

Tailscale with mTLS control plane.

**Required**: `auth_key` + CA/client certificates

## Build Process

### 1. Compose Profile

```bash
./compose-profile.sh wg-mesh-ipv6
```

Outputs to `build/wg-mesh-ipv6/`:
- `packages.txt` - Merged package list
- `files.txt` - Files to copy
- `firewall.nft` - Merged firewall rules
- `selftests.txt` - Selftest modules
- `manifest.json` - Build manifest

### 2. Build Image

```bash
./build-profile.sh wg-mesh-ipv6 --sign-key /path/to/key
```

Outputs to `output/`:
- `wg-mesh-ipv6.qcow2` - Image
- `wg-mesh-ipv6.provenance.json` - Provenance
- `wg-mesh-ipv6.provenance.json.sig` - Signature

### 3. Test with Sidecar

```bash
# Run sidecar tests against deployed VM
docker run sidecar-control \
  --target 192.168.1.100 \
  --signing-key /path/to/key
```

## Extending the System

### Adding a Feature

1. Create directory: `features/<name>/`

2. Create `feature.yaml`:
```yaml
name: my-feature
version: 1.0.0
requires:
  - base-minimal
packages:
  - my-package
files:
  - source: files/my-script.sh
    destination: /usr/local/bin/my-script.sh
    mode: "0755"
firewall_fragment: |
  # nftables rules
selftest:
  - test_my_feature
```

3. Create files in `features/<name>/files/`

4. Create selftest in `features/<name>/selftest/test_*.py`

### Adding a Profile

1. Create `profiles/<name>.yaml`:
```yaml
name: my-profile
version: 1.0.0
base: alpine:3.19
features:
  - base-minimal
  - vpn-wireguard
  - my-feature
feature_config:
  my-feature:
    option: value
test_requirements:
  - boot_successful
  - my_feature_works
```

2. Validate: `./compose-profile.sh my-profile --validate-only`

## Troubleshooting

### Peer Not Admitted

1. Check signature: `verify-signature.sh node-descriptor /path/to/descriptor.json`
2. Check trusted signers: `ls /etc/infrasim/trusted-signers/`
3. Check WireGuard: `wg show wg0`
4. Check logs: `journalctl -u wg-quick@wg0`

### Rendezvous Not Working

1. Check service: `rc-service rendezvousd status`
2. Check config: `cat /etc/infrasim/rendezvous.conf`
3. Check IPv6: `ip -6 addr show`
4. Check mesh_secret is consistent across all nodes

### Selftest Failures

1. Run manually: `python3 /usr/share/infrasim/selftest/test_base.py`
2. Check specific test output for details
3. Review cloud-init logs: `cat /var/log/cloud-init-output.log`
