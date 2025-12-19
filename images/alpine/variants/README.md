# Alpine Image Variants

This directory contains build configurations for four Alpine Linux image variants with different VPN/networking configurations:

| Variant | VPN Stack | Use Case |
|---------|-----------|----------|
| `no-vpn` | None | Baseline image for isolated/air-gapped environments |
| `wireguard` | WireGuard mesh | Peer-to-peer encrypted VPN mesh between nodes |
| `tailscale` | Tailscale | Centralized control plane, telemetry, node management |
| `dual-vpn` | WireGuard + Tailscale | Full stack: WireGuard for data plane, Tailscale for C2 |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Build Pipeline                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   ┌───────────────┐   ┌───────────────┐   ┌───────────────┐         │
│   │  no-vpn       │   │  wireguard    │   │  tailscale    │         │
│   │  Runner       │   │  Runner       │   │  Runner       │         │
│   └───────┬───────┘   └───────┬───────┘   └───────┬───────┘         │
│           │                   │                   │                  │
│           ▼                   ▼                   ▼                  │
│   ┌───────────────┐   ┌───────────────┐   ┌───────────────┐         │
│   │ alpine-base   │   │ alpine-wg     │   │ alpine-ts     │         │
│   │ .qcow2        │   │ .qcow2        │   │ .qcow2        │         │
│   └───────┬───────┘   └───────┬───────┘   └───────┬───────┘         │
│           │                   │                   │                  │
│           └───────────────────┴───────────────────┘                  │
│                               │                                      │
│                               ▼                                      │
│                    ┌────────────────────┐                            │
│                    │   dual-vpn Runner   │                            │
│                    └──────────┬─────────┘                            │
│                               │                                      │
│                               ▼                                      │
│                    ┌────────────────────┐                            │
│                    │ alpine-dual.qcow2  │                            │
│                    │ (WG + Tailscale)   │                            │
│                    └────────────────────┘                            │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Variant Details

### no-vpn

Base Alpine image with:
- Network tools (iproute2, iptables, nftables)
- SSH server
- Cloud-init support
- Telemetry agent stub

No VPN software installed. Suitable for:
- Air-gapped environments
- Internal isolated networks
- Base image for custom VPN solutions

### wireguard

Includes everything from `no-vpn` plus:
- WireGuard tools (`wireguard-tools`)
- Pre-configured mesh VPN template
- Peer discovery via DNS-SD
- Automatic key rotation support

Use cases:
- Peer-to-peer encrypted mesh
- High-performance VPN tunnels
- Self-sovereign network overlay

### tailscale

Includes everything from `no-vpn` plus:
- Tailscale client
- Integration with Tailscale control plane
- ACL support
- File sharing and SSH via Tailscale

Use cases:
- Centralized node management
- Zero-trust networking
- Telemetry collection (like Docker Swarm)
- Easy onboarding of new nodes

### dual-vpn

Combines both `wireguard` and `tailscale`:
- Tailscale for command & control (C2)
- WireGuard for data plane VPN
- Separation of control and data traffic
- Enhanced security for hostile territory deployment

Use cases:
- Military/intelligence deployments
- Multi-layer security requirements
- Compliance scenarios requiring traffic separation

## Building

Each variant is built via GitHub Actions on a dedicated self-hosted runner:

```bash
# Trigger specific variant build
gh workflow run build-alpine-variants.yml \
  -f variant=wireguard \
  -f alpine_version=3.20

# Build all variants
gh workflow run build-alpine-variants.yml \
  -f variant=all
```

## Configuration Files

Each variant has a configuration directory:

```
variants/
├── no-vpn/
│   └── config.yaml
├── wireguard/
│   ├── config.yaml
│   ├── wg0.conf.template
│   └── peer-discovery.sh
├── tailscale/
│   ├── config.yaml
│   └── tailscale-up.sh
└── dual-vpn/
    ├── config.yaml
    ├── wg0.conf.template
    └── tailscale-up.sh
```

## Overlays

qcow2 overlay chain:

```
base.qcow2 (shared base)
    │
    ├── overlay-no-vpn.qcow2
    │
    ├── overlay-wireguard.qcow2
    │
    ├── overlay-tailscale.qcow2
    │
    └── overlay-dual-vpn.qcow2
```

Each overlay contains only the variant-specific changes, minimizing storage and enabling efficient updates.
