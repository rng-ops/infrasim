# InfraSim Containerized GitHub Actions Runner

This directory contains the secure containerized GitHub Actions runner setup with HashiCorp Vault for secrets management.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Docker Network                            │
│                                                              │
│  ┌─────────────┐     ┌─────────────────────────────────┐   │
│  │   Vault     │     │      GitHub Actions Runner       │   │
│  │             │────▶│                                   │   │
│  │  Secrets:   │     │  - Fetches secrets from Vault   │   │
│  │  - GITHUB_  │     │  - Registers with GitHub        │   │
│  │    TOKEN    │     │  - Runs jobs in containers      │   │
│  │             │     │  - Docker-in-Docker via socket  │   │
│  └─────────────┘     └─────────────────────────────────┘   │
│        ▲                           │                        │
│        │                           ▼                        │
│   Port 8200                  /var/run/docker.sock           │
│   (UI access)                                               │
└─────────────────────────────────────────────────────────────┘
```

## Quick Start

### 1. Initialize with your GitHub token

```bash
export GITHUB_TOKEN=ghp_xxxx
./start-runner.sh --init
```

This will:
- Start HashiCorp Vault
- Store your GitHub token securely in Vault
- Build and start the runner container
- Register the runner with GitHub

### 2. Subsequent starts

Once initialized, just run:

```bash
./start-runner.sh
```

The runner will fetch the token from Vault automatically.

## Components

| Component | Description |
|-----------|-------------|
| `docker-compose.yml` | Docker Compose configuration |
| `Dockerfile.runner` | Runner container with Docker-in-Docker |
| `entrypoint.sh` | Runner startup script with Vault integration |
| `start-runner.sh` | Convenience script to start everything |
| `vault-config/` | Vault configuration files |

## Security Features

1. **Container Isolation**: Runner executes in a proper Docker container, not chroot jail
2. **Secrets in Vault**: GitHub token stored in HashiCorp Vault, not env vars
3. **Docker-in-Docker**: Builds run in nested containers
4. **No Host Access**: Runner cannot access host filesystem except via mounts

## Commands

```bash
# View runner logs
docker compose logs -f runner

# View Vault logs  
docker compose logs -f vault

# Access Vault UI
open http://localhost:8200
# Token: infrasim-dev-token

# Stop everything
docker compose down

# Stop and remove data
docker compose down -v

# Rebuild runner container
docker compose build runner

# Shell into runner
docker compose exec runner bash
```

## Updating GitHub Token

```bash
# Connect to Vault
docker compose exec vault vault kv put secret/github token="ghp_new_token"

# Restart runner to pick up new token
docker compose restart runner
```

## Troubleshooting

### Runner not connecting
```bash
# Check runner logs
docker compose logs runner

# Check Vault is healthy
docker compose exec vault vault status
```

### Docker-in-Docker issues
```bash
# Ensure Docker socket is mounted
docker compose exec runner docker ps
```

### Rebuild from scratch
```bash
docker compose down -v
docker compose build --no-cache
./start-runner.sh --init
```
