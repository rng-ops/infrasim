# InfraSim Alpine Runner Example
#
# This Terraform configuration creates an Alpine Linux VM with a
# software-defined NAT network for local testing.
#
# USAGE:
#   1. Ensure infrasimd is running: infrasimd --foreground
#   2. Build the Alpine image: cd images/alpine && make build
#   3. Initialize Terraform: terraform init
#   4. Apply: terraform apply
#
# NOTES:
# - This example aligns with the actual infrasim provider schema
# - SDN/NAT features use InfraSim's network resource
# - The VM boots from the locally-built Alpine qcow2

terraform {
  required_providers {
    infrasim = {
      source  = "local/infrasim/infrasim"
      version = "0.1.0"
    }
  }
}

# =============================================================================
# Provider Configuration
# =============================================================================

provider "infrasim" {
  daemon_address = var.daemon_address
}

# =============================================================================
# Variables
# =============================================================================

variable "daemon_address" {
  description = "InfraSim daemon gRPC address"
  type        = string
  default     = "http://127.0.0.1:50051"
}

variable "alpine_image" {
  description = "Path to Alpine qcow2 image"
  type        = string
  default     = "../../../images/alpine/output/base.qcow2"
}

variable "vm_cpus" {
  description = "Number of CPUs for the Alpine VM"
  type        = number
  default     = 2
}

variable "vm_memory" {
  description = "Memory in MB for the Alpine VM"
  type        = number
  default     = 512
}

variable "network_cidr" {
  description = "CIDR for the NAT network"
  type        = string
  default     = "192.168.200.0/24"
}

# =============================================================================
# NAT Network (Software-Defined)
# =============================================================================

resource "infrasim_network" "alpine_net" {
  name    = "alpine-runner-net"
  mode    = "nat"
  cidr    = var.network_cidr
  gateway = cidrhost(var.network_cidr, 1)
  
  dhcp_start = cidrhost(var.network_cidr, 100)
  dhcp_end   = cidrhost(var.network_cidr, 200)
  
  mtu = 1500
}

# =============================================================================
# Alpine VM
# =============================================================================

resource "infrasim_vm" "alpine_runner" {
  name   = "alpine-runner"
  cpus   = var.vm_cpus
  memory = var.vm_memory
  disk   = var.alpine_image
  
  network_id = infrasim_network.alpine_net.id
  
  # Cloud-init for boot verification
  cloud_init = base64encode(<<-EOF
    #cloud-config
    hostname: alpine-runner
    
    # Simple boot verification script
    runcmd:
      # Wait for network
      - sleep 5
      
      # Log network status
      - echo "=== NETWORK STATUS ===" > /var/log/boot-test.log
      - ip a >> /var/log/boot-test.log
      - ip r >> /var/log/boot-test.log
      
      # Test connectivity
      - |
        if ping -c 3 ${cidrhost(var.network_cidr, 1)}; then
          echo "CONNECTIVITY_OK" >> /var/log/boot-test.log
          echo "CONNECTIVITY_OK" > /dev/ttyS0
        else
          echo "CONNECTIVITY_FAILED" >> /var/log/boot-test.log
          echo "CONNECTIVITY_FAILED" > /dev/ttyS0
        fi
      
      # Signal boot complete
      - echo "BOOT_OK" > /dev/ttyS0
    
    final_message: "Alpine runner ready. Check /var/log/boot-test.log"
  EOF
  )
}

# =============================================================================
# Outputs
# =============================================================================

output "vm_id" {
  description = "ID of the Alpine VM"
  value       = infrasim_vm.alpine_runner.id
}

output "vm_name" {
  description = "Name of the Alpine VM"
  value       = infrasim_vm.alpine_runner.name
}

output "network_id" {
  description = "ID of the NAT network"
  value       = infrasim_network.alpine_net.id
}

output "network_cidr" {
  description = "CIDR of the NAT network"
  value       = infrasim_network.alpine_net.cidr
}

output "console_url" {
  description = "Web console URL for the VM"
  value       = infrasim_vm.alpine_runner.console_url
}

output "vnc_port" {
  description = "VNC port for the VM"
  value       = infrasim_vm.alpine_runner.vnc_port
}
