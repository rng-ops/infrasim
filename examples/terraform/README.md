# InfraSim Terraform Example

This directory contains example Terraform configurations for InfraSim.

## Prerequisites

1. Install InfraSim daemon and CLI:
   ```bash
   cargo install --path crates/daemon
   cargo install --path crates/cli
   ```

2. Install the Terraform provider:
   ```bash
   cargo build --release -p infrasim-provider
   
   # Create provider directory
   mkdir -p ~/.terraform.d/plugins/local/infrasim/infrasim/0.1.0/darwin_arm64
   
   # Copy provider binary
   cp target/release/terraform-provider-infrasim \
      ~/.terraform.d/plugins/local/infrasim/infrasim/0.1.0/darwin_arm64/
   ```

3. Start the InfraSim daemon:
   ```bash
   infrasimd --foreground
   ```

4. Download or build VM images:
   ```bash
   # Build the Kali image (see images/kali-xfce-vnc-aarch64/)
   cd images/kali-xfce-vnc-aarch64
   ./build.sh
   ```

## Usage

1. Initialize Terraform:
   ```bash
   terraform init
   # Or with OpenTofu:
   tofu init
   ```

2. Preview the changes:
   ```bash
   terraform plan
   ```

3. Apply the configuration:
   ```bash
   terraform apply
   ```

4. Access the VMs:
   - Open the web console URLs from the output
   - Or use the CLI: `infrasim console <vm-id> --open`

5. Destroy when done:
   ```bash
   terraform destroy
   ```

## Configuration

See `variables.tf` for configurable options:

| Variable | Description | Default |
|----------|-------------|---------|
| `network_cidr` | Lab network CIDR | `192.168.100.0/24` |
| `attacker_cpus` | Attacker VM CPUs | `4` |
| `attacker_memory` | Attacker VM memory (MB) | `4096` |
| `target_count` | Number of target VMs | `3` |
| `qos_enabled` | Enable QoS simulation | `true` |
| `qos_latency_ms` | Simulated latency | `50` |

## Resources Created

- 1x NAT network for lab isolation
- 1x Kali Linux attacker VM with XFCE desktop
- 3x Target VMs with web services
- 1x Snapshot for baseline state

## Browser Console

Access VMs through your browser at the console URLs. The web console uses
noVNC for full graphical access without any client installation required.
