# Meshnet Console MVP

A minimal WebAuthn-secured mesh networking console for InfraSim.

## Features

- **WebAuthn Passkeys Only** - No passwords. Secure, phishing-resistant authentication.
- **Identity Handle** - Choose a unique handle that becomes your subdomain (`handle.mesh.local`) and Matrix ID (`@handle:matrix.mesh.local`)
- **WireGuard Mesh** - Create peers, download `.conf` files for each device
- **Appliance Archives** - Generate downloadable bundles with disk.qcow2, mesh configs, and Terraform

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Meshnet Console MVP                       │
├─────────────────────────────────────────────────────────────┤
│  UI (React)                                                  │
│  ├─ MeshnetLogin.tsx    - WebAuthn registration/login        │
│  └─ MeshnetDashboard.tsx - 3-card layout (Identity/Mesh/App) │
├─────────────────────────────────────────────────────────────┤
│  API Routes (/api/meshnet/*)                                 │
│  ├─ auth/*         - WebAuthn challenges & verification      │
│  ├─ identity       - Handle creation & provisioning          │
│  ├─ mesh/peers     - WireGuard peer management               │
│  └─ appliances     - Archive generation & download           │
├─────────────────────────────────────────────────────────────┤
│  Backend Services                                            │
│  ├─ MeshnetDb      - SQLite storage via infrasim_common      │
│  ├─ IdentityService - Async provisioning (subdomain/Matrix)  │
│  ├─ WireGuardProvider - Key generation, config rendering     │
│  └─ ApplianceService - tar.gz archive with manifest          │
└─────────────────────────────────────────────────────────────┘
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `BASE_DOMAIN` | `mesh.local` | Base domain for subdomains |
| `MATRIX_DOMAIN` | `matrix.mesh.local` | Matrix homeserver domain |
| `DATA_DIR` | `~/.infrasim` | Data directory for archives |
| `WEBAUTHN_RP_ID` | `localhost` | WebAuthn Relying Party ID |
| `WEBAUTHN_RP_ORIGIN` | `http://localhost:8080` | WebAuthn Relying Party Origin |
| `WEBAUTHN_RP_NAME` | `Meshnet Console` | Display name in passkey prompts |
| `WG_GATEWAY_ENDPOINT` | `gateway.mesh.local:51820` | WireGuard gateway endpoint |
| `WG_GATEWAY_PUBLIC_KEY` | (generated) | Gateway public key |

## Quick Start

### 1. Build the backend

```bash
cd infrasim
cargo build -p infrasim-web
```

### 2. Install frontend dependencies

```bash
cd ui
pnpm install
```

Note: The MeshnetLogin component requires `@simplewebauthn/browser`:

```bash
cd ui/apps/console
pnpm add @simplewebauthn/browser
```

### 3. Build the frontend

```bash
cd ui
pnpm -r build
```

### 4. Run the server

```bash
# Set environment variables
export BASE_DOMAIN=mesh.local
export WEBAUTHN_RP_ID=localhost
export WEBAUTHN_RP_ORIGIN=http://localhost:8080

# Start the server
./target/debug/infrasim-web
```

### 5. Access the console

Open http://localhost:8080/meshnet in your browser.

## API Endpoints

### Authentication

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/meshnet/auth/register/options` | Get WebAuthn registration options |
| POST | `/api/meshnet/auth/register/verify` | Verify registration & create user |
| POST | `/api/meshnet/auth/login/options` | Get WebAuthn login options |
| POST | `/api/meshnet/auth/login/verify` | Verify login & create session |
| POST | `/api/meshnet/auth/logout` | Invalidate session |
| GET | `/api/meshnet/me` | Get current user, identity, status |

### Identity

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/meshnet/identity` | Create identity with handle |
| GET | `/api/meshnet/identity` | Get current identity |
| POST | `/api/meshnet/identity/provision` | Start async provisioning |
| GET | `/api/meshnet/identity/status` | Get provisioning status |

### Mesh

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/meshnet/mesh/peers` | Create new peer |
| GET | `/api/meshnet/mesh/peers` | List all peers |
| GET | `/api/meshnet/mesh/peers/:id` | Get peer details |
| GET | `/api/meshnet/mesh/peers/:id/config` | Download WireGuard .conf |
| POST | `/api/meshnet/mesh/peers/:id/revoke` | Revoke peer |
| POST | `/api/meshnet/mesh/rotate-keys` | Rotate all keys |

### Appliances

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/meshnet/appliances` | Create appliance |
| GET | `/api/meshnet/appliances` | List appliances |
| GET | `/api/meshnet/appliances/:id` | Get appliance details |
| GET | `/api/meshnet/appliances/:id/archive` | Download tar.gz |
| GET | `/api/meshnet/appliances/:id/terraform` | Get Terraform JSON |
| DELETE | `/api/meshnet/appliances/:id` | Delete appliance |

## Database Schema

All data is stored in SQLite via `infrasim_common::Database`.

Tables:
- `meshnet_users` - User records with WebAuthn user handles
- `meshnet_webauthn_credentials` - Passkey credentials
- `meshnet_identities` - Identity handles with provisioning status
- `meshnet_mesh_peers` - WireGuard peer configurations
- `meshnet_appliances` - Generated appliance archives
- `meshnet_challenges` - WebAuthn challenge state (TTL 5min)
- `meshnet_sessions` - Auth sessions (TTL 7 days)

## Archive Structure

When you download an appliance archive (`.tar.gz`), it contains:

```
appliance-{id}/
├── disk.qcow2                  # QEMU disk image (placeholder)
├── mesh/
│   └── {handle}-{peer}.conf    # WireGuard configs for each peer
├── terraform/
│   └── main.tf.json            # Terraform configuration
├── signatures/
│   ├── manifest.json           # File checksums
│   └── manifest.sig            # Manifest signature
└── README.md                   # Usage instructions
```

## Security Considerations

1. **WebAuthn Only** - No password fallback. Users must have a passkey-capable device.
2. **Handle Validation** - Handles are normalized to lowercase, validated against blocklist.
3. **Session Tokens** - SHA-256 hashed before storage. 7-day TTL.
4. **Challenge TTL** - WebAuthn challenges expire after 5 minutes.
5. **Private Keys** - WireGuard private keys are stored encrypted (TODO: implement KMS).

## Future Extensions

- **Tailscale Provider** - Add `TailscaleProvider` implementing `MeshProvider` trait
- **Real Provisioning** - Integrate with DNS API, Matrix homeserver, S3
- **Hosting Tab** - File upload to subdomain via WebDAV or S3
- **Multi-device** - Link multiple passkeys to one identity

## Testing

### Smoke Test

```bash
# 1. Start server
./target/debug/infrasim-web &

# 2. Check health
curl http://localhost:8080/api/health

# 3. Create a registration challenge (use browser for actual passkey)
curl -X POST http://localhost:8080/api/meshnet/auth/register/options \
  -H "Content-Type: application/json" \
  -d '{"handle": "testuser"}'
```

### Unit Tests

```bash
cargo test -p infrasim-web meshnet
```

## License

Same as InfraSim - see root LICENSE file.
