# InfraSim Alpine Runner Example

This example demonstrates how to use InfraSim to create an Alpine Linux VM
with a software-defined NAT network for local testing and development.

## Prerequisites

1. **InfraSim daemon** running:
   ```bash
   infrasimd --foreground
   ```

2. **Alpine qcow2 image** built:
   ```bash
   cd images/alpine
   make build
   ```

3. **Terraform/OpenTofu** installed:
   ```bash
   brew install opentofu
   # or
   brew install terraform
   ```

4. **InfraSim Terraform provider** installed:
   ```bash
   cargo build --release -p infrasim-provider
   
   mkdir -p ~/.terraform.d/plugins/local/infrasim/infrasim/0.1.0/darwin_arm64
   cp target/release/terraform-provider-infrasim \
      ~/.terraform.d/plugins/local/infrasim/infrasim/0.1.0/darwin_arm64/
   ```

## Usage

### 1. Initialize Terraform

```bash
cd examples/terraform/alpine_runner
terraform init
# or: tofu init
```

### 2. Plan the deployment

```bash
terraform plan
```

### 3. Apply the configuration

```bash
terraform apply
```

This will:
- Create a NAT network (192.168.200.0/24)
- Create an Alpine VM attached to the network
- Boot the VM with cloud-init configuration

### 4. Verify the VM

```bash
# Get VM ID
VM_ID=$(terraform output -raw vm_id)

# Check status via CLI
infrasim vm get $VM_ID

# Open web console
infrasim console $VM_ID --open
```

### 5. Destroy when done

```bash
terraform destroy
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `daemon_address` | `http://127.0.0.1:50051` | InfraSim daemon address |
| `alpine_image` | `../../../images/alpine/output/base.qcow2` | Path to Alpine image |
| `vm_cpus` | `2` | Number of CPUs |
| `vm_memory` | `512` | Memory in MB |
| `network_cidr` | `192.168.200.0/24` | NAT network CIDR |

## What This Tests

1. **Network Creation**: Creates a software-defined NAT network
2. **VM Boot**: Boots the Alpine image with cloud-init
3. **Network Connectivity**: Pings the gateway from inside the VM
4. **Serial Console**: Outputs boot status to serial console

## Troubleshooting

### "Provider not found"

Ensure the Terraform provider is installed:
```bash
ls ~/.terraform.d/plugins/local/infrasim/infrasim/0.1.0/darwin_arm64/
```

### "Connection refused"

Ensure the daemon is running:
```bash
infrasimd --foreground
# or check logs
journalctl -u infrasimd
```

### "Image not found"

Build the Alpine image first:
```bash
cd images/alpine && make build
```

## Network Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Host (macOS)                            │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              InfraSim NAT Network                     │  │
│  │              192.168.200.0/24                         │  │
│  │                                                       │  │
│  │  ┌─────────────┐                                     │  │
│  │  │ Alpine VM   │                                     │  │
│  │  │ .100        │ ←── DHCP assigned                   │  │
│  │  └──────┬──────┘                                     │  │
│  │         │                                            │  │
│  │         ▼                                            │  │
│  │  ┌─────────────┐                                     │  │
│  │  │ Gateway     │                                     │  │
│  │  │ .1          │ ←── NAT to host                     │  │
│  │  └──────┬──────┘                                     │  │
│  │         │                                            │  │
│  └─────────┼────────────────────────────────────────────┘  │
│            │                                                │
│            ▼                                                │
│       Host Network                                          │
└─────────────────────────────────────────────────────────────┘
```

## Future Enhancements

- **LoRaWAN Integration**: The Alpine image includes telemetry agent stubs
  for future LoRaWAN gateway simulation. See `images/alpine/telemetry/`.

- **QoS Simulation**: Add latency/jitter/loss to test network conditions:
  ```hcl
  resource "infrasim_vm" "alpine_runner" {
    # ...
    qos_latency_ms = 50
    qos_jitter_ms  = 10
  }
  ```

- **Snapshots**: Create baseline snapshots for testing:
  ```hcl
  resource "infrasim_snapshot" "baseline" {
    vm_id          = infrasim_vm.alpine_runner.id
    name           = "baseline"
    include_memory = true
  }
  ```
