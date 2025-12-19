# InfraSim GitHub Self-Hosted Runners

This Terraform configuration provisions dedicated self-hosted GitHub Actions runners for building Alpine image variants.

## Overview

Each Alpine variant is built on its own runner with variant-specific tools pre-installed:

| Runner Label | Variant | Tools |
|--------------|---------|-------|
| `infrasim-runner-no-vpn` | no-vpn | Base tools only |
| `infrasim-runner-wireguard` | wireguard | WireGuard tools |
| `infrasim-runner-tailscale` | tailscale | Tailscale client |
| `infrasim-runner-dual-vpn` | dual-vpn | WireGuard + Tailscale + nftables |

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     GitHub Actions                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│   ┌─────────────────┐     ┌─────────────────┐                   │
│   │  Job: no-vpn    │     │  Job: wireguard │                   │
│   │  runs-on:       │     │  runs-on:       │                   │
│   │  infrasim-      │     │  infrasim-      │                   │
│   │  runner-no-vpn  │     │  runner-wg      │                   │
│   └────────┬────────┘     └────────┬────────┘                   │
│            │                       │                             │
│            ▼                       ▼                             │
│   ┌─────────────────────────────────────────┐                   │
│   │           InfraSim Network               │                   │
│   │           192.168.100.0/24               │                   │
│   └─────────────────────────────────────────┘                   │
│            │                       │                             │
│            ▼                       ▼                             │
│   ┌─────────────────┐     ┌─────────────────┐                   │
│   │  VM: runner-    │     │  VM: runner-    │                   │
│   │  no-vpn         │     │  wireguard      │                   │
│   │  ┌───────────┐  │     │  ┌───────────┐  │                   │
│   │  │ Actions   │  │     │  │ Actions   │  │                   │
│   │  │ Runner    │  │     │  │ Runner    │  │                   │
│   │  └───────────┘  │     │  └───────────┘  │                   │
│   └─────────────────┘     └─────────────────┘                   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Prerequisites

1. **InfraSim daemon running**
   ```bash
   infrasimd --foreground
   ```

2. **Base Alpine image built**
   ```bash
   cd ../../images/alpine
   ./build-alpine-qcow2.sh
   ```

3. **GitHub Personal Access Token** with `repo` and `admin:org` scopes
   ```bash
   export TF_VAR_github_token="ghp_xxxxxxxxxxxx"
   ```

## Usage

### Initialize Terraform

```bash
terraform init
```

### Preview changes

```bash
terraform plan \
  -var="github_repo=owner/repo" \
  -var="github_token=$GITHUB_TOKEN"
```

### Provision runners

```bash
terraform apply \
  -var="github_repo=owner/repo" \
  -var="github_token=$GITHUB_TOKEN"
```

### Verify runners

Check GitHub repository Settings → Actions → Runners to see the registered runners.

### Destroy runners

```bash
terraform destroy \
  -var="github_repo=owner/repo" \
  -var="github_token=$GITHUB_TOKEN"
```

## Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `github_token` | GitHub PAT or runner registration token | (required) |
| `github_repo` | Repository in owner/repo format | (required) |
| `daemon_address` | InfraSim daemon address | `http://127.0.0.1:50051` |
| `runner_base_image` | Base qcow2 image path | `../../../images/alpine/output/base.qcow2` |
| `runner_cpus` | CPUs per runner VM | 4 |
| `runner_memory` | Memory (MB) per runner | 4096 |
| `runner_disk_size` | Disk size (GB) per runner | 50 |
| `network_cidr` | Network CIDR for runners | `192.168.100.0/24` |

## Outputs

| Output | Description |
|--------|-------------|
| `runner_ids` | Map of variant → VM ID |
| `runner_names` | Map of variant → VM name |
| `runner_ips` | Map of variant → IP address |
| `runner_labels` | Map of variant → GitHub runner label |

## Security Considerations

1. **Token Storage**: The GitHub token is passed via cloud-init and should be treated as sensitive. Consider using HashiCorp Vault or similar.

2. **Network Isolation**: Runners are on a NAT network with limited external access.

3. **Runner Scope**: Runners are registered to a specific repository, not organization-wide.

4. **Ephemeral Runners**: For production, consider making runners ephemeral (self-destruct after one job).

## Troubleshooting

### Runner not appearing in GitHub

```bash
# Check runner VM status
infrasim vm list

# View runner logs
infrasim vm logs infrasim-runner-no-vpn
```

### Runner offline

```bash
# SSH to runner (if accessible)
ssh runner@<runner-ip>

# Check runner service
rc-service github-runner status

# View runner logs
tail -f /opt/actions-runner/_diag/*.log
```

### Re-register runner

```bash
# On the runner VM
cd /opt/actions-runner
./config.sh remove --token <REMOVE_TOKEN>
./configure-runner.sh
rc-service github-runner restart
```
