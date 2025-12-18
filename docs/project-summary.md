# InfraSim Project Summary

## Overview

InfraSim is a secure virtual appliance management platform that combines:

1. **Meshnet Console** - WebAuthn passkey authentication with WireGuard mesh networking
2. **VM Appliance Management** - QEMU-based virtual machine provisioning and lifecycle
3. **Memory Acquisition & Provenance** - Forensic memory dumps with cryptographic attestation
4. **P2P Weight Sharing** - WebTorrent-style streaming for ML model weights and gradients

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Meshnet Console                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │   Identity   │  │     Mesh     │  │     Appliances       │  │
│  │  (WebAuthn)  │  │  (WireGuard) │  │  (QEMU + Provenance) │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│                         Backend (Rust)                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │  infrasim-   │  │  infrasim-   │  │    infrasim-         │  │
│  │    web       │  │   daemon     │  │    common            │  │
│  │  (Axum API)  │  │ (QEMU/gRPC)  │  │  (DB/Crypto/QMP)     │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│                      Infrastructure Layer                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │   SQLite     │  │   QEMU/KVM   │  │     WireGuard        │  │
│  │  (State DB)  │  │    (VMs)     │  │   (Mesh Network)     │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## Core Features

### 1. Meshnet Console (Primary UI)

The main user interface focused on three cards:

| Card | Purpose |
|------|---------|
| **Identity** | WebAuthn passkey registration, handle → subdomain + Matrix ID |
| **Mesh** | WireGuard peer management, config generation, key rotation |
| **Appliances** | VM template creation, archive generation, deployment |

**Authentication Flow:**
```
User → WebAuthn Passkey → Handle Selection → Identity Provisioning
                                                    ↓
                              ┌─────────────────────────────────┐
                              │ • Subdomain: handle.base.domain │
                              │ • Matrix ID: @handle:matrix.dom │
                              │ • S3 Storage: allocated bucket  │
                              └─────────────────────────────────┘
```

### 2. VM Appliance Management

QEMU-based virtual machine lifecycle:

- **Templates**: Pre-configured base images (Alpine, Kali, Ubuntu)
- **Provisioning**: Cloud-init for automated setup
- **Snapshots**: Point-in-time state capture
- **Live Migration**: Move running VMs between hosts

**Appliance Archive Format:**
```
appliance-{id}.tar.gz
├── disk.qcow2           # VM disk image
├── cloud-init/
│   ├── user-data        # Cloud-init configuration
│   └── meta-data        # Instance metadata
├── wireguard/
│   └── wg0.conf         # Pre-configured mesh peer
├── terraform/
│   └── main.tf          # Infrastructure-as-code
└── manifest.json        # Archive metadata + hashes
```

### 3. Memory Acquisition & Provenance

Forensic memory capture with cryptographic attestation:

```
VM Running → QMP dump-guest-memory → Raw Memory Dump
                                           ↓
                              ┌─────────────────────────┐
                              │ SHA-256 Hash            │
                              │ Timestamp               │
                              │ VM Metadata             │
                              │ Operator Identity       │
                              └─────────────────────────┘
                                           ↓
                              Signed Attestation Record
```

**Provenance Chain:**
1. Memory dump captured via QEMU QMP
2. SHA-256 hash computed
3. Attestation record created with operator identity
4. Record signed with operator's key
5. Stored in append-only audit log

### 4. P2P Weight Sharing (Planned)

WebTorrent-style streaming for ML model distribution:

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Seeder    │────▶│   Tracker   │◀────│   Leecher   │
│ (Has Model) │     │  (DHT/WS)   │     │(Needs Model)│
└─────────────┘     └─────────────┘     └─────────────┘
       │                                       │
       └───────── WebRTC Data Channel ─────────┘
                         │
              ┌──────────────────────┐
              │ Streaming Chunks:    │
              │ • Weights (quantized)│
              │ • Gradients          │
              │ • Activations        │
              └──────────────────────┘
```

**Design Goals:**
- Stream weights just-in-time as needed
- No full model download required
- Browser-based using WebRTC
- Mesh network for peer discovery

## Technology Stack

| Layer | Technology |
|-------|------------|
| Frontend | React 18, TypeScript, Vite |
| Backend | Rust, Axum, Tokio |
| Database | SQLite (rusqlite) |
| Auth | WebAuthn (webauthn-rs), Passkeys |
| VPN | WireGuard (x25519-dalek) |
| VMs | QEMU, libvirt, QMP |
| IaC | Terraform Provider |
| P2P | WebTorrent, WebRTC (planned) |

## Project Structure

```
infrasim/
├── crates/
│   ├── cli/          # Command-line interface
│   ├── common/       # Shared types, DB, crypto
│   ├── daemon/       # QEMU management, gRPC
│   ├── provider/     # Terraform provider
│   └── web/          # Axum web server
│       └── src/
│           ├── meshnet/    # Meshnet Console MVP
│           │   ├── db.rs       # SQLite schema
│           │   ├── handle.rs   # Handle validation
│           │   ├── identity.rs # Provisioning
│           │   ├── mesh.rs     # WireGuard provider
│           │   ├── appliance.rs# Archive generation
│           │   └── routes.rs   # API endpoints
│           └── server.rs   # Main server
├── ui/
│   └── apps/console/ # React frontend
│       └── src/
│           └── pages/
│               ├── MeshnetDashboard.tsx
│               └── MeshnetLogin.tsx
├── proto/            # gRPC definitions
└── docs/             # Documentation
```

## API Endpoints

### Meshnet API (`/api/meshnet/`)

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/auth/register/options` | WebAuthn registration options |
| POST | `/auth/register/verify` | Complete registration |
| POST | `/auth/login/options` | WebAuthn login options |
| POST | `/auth/login/verify` | Complete login |
| GET | `/me` | Current user + identity |
| POST | `/identity` | Create identity with handle |
| POST | `/identity/provision` | Start provisioning |
| GET | `/mesh/peers` | List mesh peers |
| POST | `/mesh/peers` | Create new peer |
| GET | `/mesh/peers/:id/config` | Download WireGuard config |
| GET | `/appliances` | List appliances |
| POST | `/appliances` | Create appliance |
| GET | `/appliances/:id/archive` | Download archive |

## Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `BASE_DOMAIN` | Base domain for subdomains | `example.com` |
| `MATRIX_DOMAIN` | Matrix homeserver domain | `matrix.example.com` |
| `WEBAUTHN_RP_ID` | WebAuthn relying party ID | `localhost` |
| `WEBAUTHN_RP_ORIGIN` | WebAuthn origin | `http://localhost:8080` |
| `WG_GATEWAY_ENDPOINT` | WireGuard gateway | `vpn.example.com:51820` |

## Running Locally

```bash
# 1. Build backend
cd infrasim
cargo build -p infrasim-web --release

# 2. Build frontend
cd ui/apps/console
pnpm install
pnpm build

# 3. Start server
export BASE_DOMAIN="localhost"
export MATRIX_DOMAIN="matrix.localhost"
export WEBAUTHN_RP_ID="localhost"
export WEBAUTHN_RP_ORIGIN="http://localhost:8080"
export WG_GATEWAY_ENDPOINT="127.0.0.1:51820"
./target/release/infrasim-web
```

## Security Model

1. **Authentication**: WebAuthn passkeys only (no passwords)
2. **Authorization**: Session tokens with 24h TTL
3. **Encryption**: WireGuard for mesh traffic, TLS for web
4. **Attestation**: SHA-256 hashes with signed provenance records
5. **Isolation**: QEMU VMs with separate network namespaces

## Future Work

- [ ] P2P weight streaming via WebTorrent
- [ ] Tailscale integration as alternative mesh provider
- [ ] GPU passthrough for ML workloads
- [ ] Federated identity across mesh nodes
- [ ] SLSA provenance for container images
- [ ] Memory forensics tooling integration
