# InfraSim Control Plane Architecture

## Overview

The InfraSim Control Plane provides a secure, distributed command-and-control infrastructure for managing InfraSim nodes across networks, including hostile or isolated environments. It uses **Tailscale** as the control plane overlay, keeping it completely separate from the data plane (VM traffic via WireGuard).

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        CONTROL PLANE (Tailscale)                        │
│                                                                         │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐               │
│  │  Operator   │────▶│  Build CI   │────▶│   Worker    │               │
│  │  Machine    │     │   Server    │     │   Nodes     │               │
│  └─────────────┘     └─────────────┘     └─────────────┘               │
│        │                   │                   │                        │
│        ▼                   ▼                   ▼                        │
│   infrasim control    GitHub Actions     infrasim daemon                │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                    ════════════════╪═══════════════
                                    │
┌─────────────────────────────────────────────────────────────────────────┐
│                        DATA PLANE (WireGuard/QEMU)                      │
│                                                                         │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐               │
│  │    VMs      │◀───▶│   Router    │◀───▶│    VMs      │               │
│  │  (site A)   │     │   Firewall  │     │  (site B)   │               │
│  └─────────────┘     └─────────────┘     └─────────────┘               │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Why Two Planes?

1. **Security Isolation**: Control traffic (commands, logs, artifacts) is isolated from VM traffic
2. **Hostile Environments**: Tailscale works through NAT, firewalls, and restrictive networks
3. **Hotswappable**: The WireGuard data plane can be reconfigured without affecting C2
4. **Audit Trail**: Control plane operations are logged separately from VM network traffic

## Components

### 1. Control Plane (Tailscale)

The control plane handles:
- **Node discovery** - Finding and connecting to InfraSim worker nodes
- **Artifact distribution** - Pushing qcow2 images, configs, and binaries
- **Build orchestration** - Triggering and monitoring remote builds
- **Log aggregation** - Streaming logs from remote pipeline runs
- **Peering coordination** - Setting up WireGuard peers between nodes

### 2. Data Plane (WireGuard/QEMU)

The data plane handles:
- **VM-to-VM traffic** - Direct communication between virtual machines
- **Network appliances** - Routers, firewalls, VPN gateways (as VMs)
- **SDN topology** - Software-defined network overlays
- **Traffic shaping** - QoS simulation for WAN emulation

## CLI Commands

### Control Plane Commands

```bash
# Connect to Tailscale network
infrasim control login --auth-key tskey-xxx --hostname infrasim-operator

# Check control plane status
infrasim control status

# List all connected nodes
infrasim control nodes
infrasim control nodes --tag infrasim-worker --all

# Deploy image to target nodes
infrasim control deploy ./alpine-image.tar.gz --targets node1,node2
infrasim control deploy ./image.qcow2 --targets tag:worker --terraform ./sdn.tf

# Stream logs from remote build
infrasim control logs node1 --follow

# List builds across nodes
infrasim control builds --status running

# Push artifact to nodes
infrasim control push ./artifact.tar.gz --targets node1,node2 --verify

# Pull artifact from node
infrasim control pull build-abc123 --node node1 --dest ./output/

# Peer with another InfraSim network
infrasim control peer other-network.ts.net --accept-routes 10.0.0.0/8

# Manage exit nodes
infrasim control exit-node list
infrasim control exit-node use node1
```

### Pipeline Commands

```bash
# List available pipelines
infrasim pipeline list

# Trigger a build
infrasim pipeline trigger image-snapshot --tag v1.0.0 --wait
infrasim pipeline trigger sdn-overlay --node remote-builder --param IMAGE=alpine

# Check build status
infrasim pipeline status abc123

# Stream build logs
infrasim pipeline logs abc123 --follow
infrasim pipeline logs abc123 --stage build-alpine

# List/download artifacts
infrasim pipeline artifacts abc123
infrasim pipeline artifacts abc123 --download ./output/

# View provenance chain
infrasim pipeline provenance abc123 --verify

# Cancel/retry builds
infrasim pipeline cancel abc123
infrasim pipeline retry abc123 --failed-only
```

### SDN Commands

```bash
# Create network appliances
infrasim sdn create edge-router --kind router --network wan,lan
infrasim sdn create vpn-gw --kind vpn --wg-address 10.200.0.1/24 --wg-port 51820
infrasim sdn create firewall --kind firewall --network lan,dmz

# List appliances
infrasim sdn list
infrasim sdn list --kind vpn --network lan

# Configure WireGuard peering
infrasim sdn peer vpn-gw remote-vpn --allowed-ips 10.100.0.0/24 --tailscale

# Deploy topology from Terraform
infrasim sdn deploy ./terraform/ --auto-approve
infrasim sdn deploy ./sdn-overlay.tf --var wg_private_key=xxx

# Generate Terraform from topology
infrasim sdn terraform my-topology --output ./generated/

# Visualize topology
infrasim sdn graph my-topology --format ascii
infrasim sdn graph my-topology --format mermaid --output topology.md
```

## Architecture Deep Dive

### Node Registration

When a worker node starts with Tailscale:

```bash
# On worker node
tailscale up --authkey=$TS_AUTHKEY \
  --advertise-tags=tag:infrasim-worker \
  --hostname=worker-$(hostname) \
  --accept-routes
```

The node becomes discoverable via:
```bash
# On operator machine
infrasim control nodes --tag infrasim-worker
```

### Artifact Distribution

Artifacts are transferred using Tailscale's secure file sharing:

```
Operator                        Worker Node
    │                               │
    │  infrasim control push        │
    │  artifact.tar.gz              │
    │───────────────────────────────▶
    │   [Tailscale File CP]         │
    │                               │
    │                               │  Verify SHA256
    │                               │  Extract to /var/lib/infrasim/
    │                               │
    │◀──────────────────────────────│
    │   [Transfer Complete]         │
    │                               │
```

### Build Pipeline Flow

```
1. Operator triggers build
   ┌────────────────────────────────────────────┐
   │ infrasim pipeline trigger image-snapshot   │
   │   --node=remote-builder --tag=v1.0.0       │
   └────────────────────────────────────────────┘
                      │
                      ▼
2. Request routed via Tailscale to remote node
   ┌────────────────────────────────────────────┐
   │ POST http://remote-builder:50051/pipeline  │
   └────────────────────────────────────────────┘
                      │
                      ▼
3. Remote node executes pipeline
   ┌────────────────────────────────────────────┐
   │ • Checkout code                            │
   │ • Build InfraSim binaries                  │
   │ • Build qcow2 image                        │
   │ • Verify boot in QEMU                      │
   │ • Create snapshot bundle                   │
   │ • Attach provenance                        │
   └────────────────────────────────────────────┘
                      │
                      ▼
4. Artifacts pushed back via Tailscale
   ┌────────────────────────────────────────────┐
   │ tailscale file cp artifact.tar.gz operator │
   └────────────────────────────────────────────┘
```

### SDN Topology Deployment

```
1. Define topology in Terraform
   ┌────────────────────────────────────────────┐
   │ examples/terraform/sdn-overlay.tf          │
   └────────────────────────────────────────────┘
                      │
                      ▼
2. Deploy to local or remote node
   ┌────────────────────────────────────────────┐
   │ infrasim sdn deploy ./sdn-overlay.tf       │
   │   --node=remote-worker --auto-approve      │
   └────────────────────────────────────────────┘
                      │
                      ▼
3. InfraSim daemon creates resources
   ┌────────────────────────────────────────────┐
   │ • Create networks (WAN, LAN, DMZ, VPN)     │
   │ • Start router VM with nftables config    │
   │ • Start VPN gateway with WireGuard        │
   │ • Start firewall VM                       │
   │ • Start workload VMs                      │
   └────────────────────────────────────────────┘
                      │
                      ▼
4. Topology active
   ┌────────────────────────────────────────────┐
   │   ┌─────┐                                  │
   │   │ WAN │──────┐                           │
   │   └─────┘      │                           │
   │           ┌────▼────┐                      │
   │           │ Router  │                      │
   │           └────┬────┘                      │
   │       ┌────────┴────────┐                  │
   │   ┌───▼───┐        ┌────▼───┐              │
   │   │  LAN  │        │  DMZ   │              │
   │   └───┬───┘        └────┬───┘              │
   │       │                 │                  │
   │   ┌───▼───┐        ┌────▼───┐              │
   │   │  VPN  │        │Firewall│              │
   │   └───────┘        └────────┘              │
   └────────────────────────────────────────────┘
```

## Security Model

### Control Plane Security (Tailscale)

- **Zero Trust**: No open ports, all traffic encrypted
- **Identity-Based**: Nodes authenticated via Tailscale identity
- **ACLs**: Fine-grained access control via Tailscale ACLs
- **Audit Logs**: All control plane operations logged

Example Tailscale ACL:
```json
{
  "acls": [
    {
      "action": "accept",
      "src": ["tag:infrasim-operator"],
      "dst": ["tag:infrasim-worker:*"]
    },
    {
      "action": "accept",
      "src": ["tag:infrasim-worker"],
      "dst": ["tag:infrasim-ci:*"]
    }
  ],
  "tagOwners": {
    "tag:infrasim-operator": ["admin@example.com"],
    "tag:infrasim-worker": ["admin@example.com"],
    "tag:infrasim-ci": ["admin@example.com"]
  }
}
```

### Data Plane Security (WireGuard)

- **Separate Keys**: VM traffic uses different WireGuard keys
- **Network Isolation**: VMs cannot access Tailscale control plane
- **Firewall Rules**: nftables rules enforced in router/firewall VMs
- **Hotswappable**: Data plane keys can be rotated without C2 disruption

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `TS_AUTHKEY` | Tailscale auth key for node registration | - |
| `TS_CONTROL_URL` | Custom Tailscale control server | (default) |
| `INFRASIM_DAEMON_ADDR` | Local daemon address | `http://127.0.0.1:50051` |
| `INFRASIM_WORKER_TAGS` | Tags for this worker node | `infrasim-worker` |

## Use Cases

### 1. Distributed Penetration Testing Lab

Deploy attack/target VMs across multiple locations:

```bash
# Deploy attacker VM to local node
infrasim sdn create attacker --kind custom --network vpn

# Deploy targets to remote workers
infrasim control deploy ./target-image.qcow2 \
  --targets tag:worker-emea,tag:worker-apac \
  --terraform ./target-network.tf

# Connect via WireGuard for testing
infrasim sdn peer attacker remote-targets --tailscale
```

### 2. Hostile Network Deployment

Deploy InfraSim workers in restricted networks:

```bash
# On restricted machine (outbound HTTPS only works)
tailscale up --authkey=$KEY --hostname=hostile-worker

# From operator (anywhere)
infrasim control deploy ./mission-image.qcow2 --targets hostile-worker
infrasim control logs hostile-worker --follow
```

### 3. Multi-Region SDN

Build a global software-defined network:

```bash
# Deploy regional gateways
infrasim control deploy ./vpn-gateway.qcow2 --targets us-east,eu-west,ap-south

# Peer the gateways via WireGuard
infrasim sdn peer us-east-vpn eu-west-vpn --allowed-ips 10.1.0.0/16
infrasim sdn peer eu-west-vpn ap-south-vpn --allowed-ips 10.2.0.0/16
infrasim sdn peer ap-south-vpn us-east-vpn --allowed-ips 10.3.0.0/16
```

## Troubleshooting

### Control Plane Issues

```bash
# Check Tailscale status
tailscale status
tailscale netcheck

# Verify node connectivity
tailscale ping worker-node

# Check InfraSim control plane
infrasim control status
```

### Data Plane Issues

```bash
# Check WireGuard status
wg show

# Verify QEMU networking
infrasim vm get vm-id --json | jq '.status.qmp_socket'

# Check SDN appliance logs
infrasim sdn logs router-id --follow
```

## Related Documentation

- [IMAGE_PROVENANCE.md](./IMAGE_PROVENANCE.md) - Image build provenance chain
- [WEB_WORKFLOW.md](./WEB_WORKFLOW.md) - Web console workflows
- [SECURITY_CONSIDERATIONS.md](./SECURITY_CONSIDERATIONS.md) - Security architecture
- [README_MESHNET_MVP.md](./README_MESHNET_MVP.md) - WireGuard mesh networking
