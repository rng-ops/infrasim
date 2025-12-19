# InfraSim GitHub Self-Hosted Runners
# ====================================
#
# This Terraform configuration provisions self-hosted GitHub Actions runners
# for building Alpine image variants. Each variant gets a dedicated runner
# with appropriate tools pre-installed.
#
# RUNNERS:
#   - infrasim-runner-no-vpn:     Base runner for no-vpn variant
#   - infrasim-runner-wireguard:  Runner with WireGuard tools
#   - infrasim-runner-tailscale:  Runner with Tailscale
#   - infrasim-runner-dual-vpn:   Runner with both WireGuard and Tailscale
#
# USAGE:
#   1. Set required variables (github_token, github_repo)
#   2. terraform init
#   3. terraform apply
#
# NOTES:
#   - Runners are provisioned as VMs via the infrasim provider
#   - Each runner self-registers with GitHub Actions
#   - Runners are labeled for variant-specific jobs

terraform {
  required_version = ">= 1.0.0"
  
  required_providers {
    infrasim = {
      source  = "local/infrasim/infrasim"
      version = ">= 0.1.0"
    }
  }
}

# =============================================================================
# Variables
# =============================================================================

variable "github_token" {
  description = "GitHub personal access token or runner registration token"
  type        = string
  sensitive   = true
}

variable "github_repo" {
  description = "GitHub repository (owner/repo format)"
  type        = string
}

variable "daemon_address" {
  description = "InfraSim daemon gRPC address"
  type        = string
  default     = "http://127.0.0.1:50051"
}

variable "runner_base_image" {
  description = "Base qcow2 image for runners"
  type        = string
  default     = "../../../images/alpine/output/base.qcow2"
}

variable "runner_cpus" {
  description = "CPUs per runner"
  type        = number
  default     = 4
}

variable "runner_memory" {
  description = "Memory in MB per runner"
  type        = number
  default     = 4096
}

variable "runner_disk_size" {
  description = "Disk size in GB per runner"
  type        = number
  default     = 50
}

variable "network_cidr" {
  description = "CIDR for runner network"
  type        = string
  default     = "192.168.100.0/24"
}

# =============================================================================
# Provider Configuration
# =============================================================================

provider "infrasim" {
  daemon_address = var.daemon_address
}

# =============================================================================
# Locals
# =============================================================================

locals {
  # Runner variants and their specific configurations
  variants = {
    "no-vpn" = {
      label   = "infrasim-runner-no-vpn"
      packages = []
      description = "Runner for no-vpn variant (base image)"
    }
    "wireguard" = {
      label   = "infrasim-runner-wireguard"
      packages = ["wireguard-tools"]
      description = "Runner for wireguard variant"
    }
    "tailscale" = {
      label   = "infrasim-runner-tailscale"
      packages = ["tailscale"]
      description = "Runner for tailscale variant"
    }
    "dual-vpn" = {
      label   = "infrasim-runner-dual-vpn"
      packages = ["wireguard-tools", "tailscale", "nftables"]
      description = "Runner for dual-vpn variant"
    }
  }
  
  # Common packages for all runners
  common_packages = [
    "qemu-utils",
    "jq",
    "curl",
    "wget",
    "git",
    "bash",
    "docker",
    "containerd"
  ]
  
  # GitHub runner version
  runner_version = "2.311.0"
}

# =============================================================================
# Shared Network
# =============================================================================

resource "infrasim_network" "runners" {
  name    = "github-runners"
  mode    = "nat"
  cidr    = var.network_cidr
  gateway = cidrhost(var.network_cidr, 1)
  
  dhcp_start = cidrhost(var.network_cidr, 10)
  dhcp_end   = cidrhost(var.network_cidr, 50)
  
  mtu = 1500
}

# =============================================================================
# Runner VMs
# =============================================================================

resource "infrasim_vm" "runner" {
  for_each = local.variants
  
  name   = each.value.label
  cpus   = var.runner_cpus
  memory = var.runner_memory
  disk   = var.runner_base_image
  
  network_id = infrasim_network.runners.id
  
  # Cloud-init for runner setup
  cloud_init = base64encode(templatefile("${path.module}/templates/runner-cloud-init.yaml", {
    runner_name    = each.value.label
    runner_labels  = each.value.label
    github_repo    = var.github_repo
    github_token   = var.github_token
    runner_version = local.runner_version
    packages       = concat(local.common_packages, each.value.packages)
    variant        = each.key
    description    = each.value.description
  }))
  
  # Metadata tags
  tags = {
    "infrasim.io/role"      = "github-runner"
    "infrasim.io/variant"   = each.key
    "infrasim.io/runner"    = each.value.label
  }
}

# =============================================================================
# Outputs
# =============================================================================

output "runner_ids" {
  description = "IDs of the runner VMs"
  value = {
    for k, v in infrasim_vm.runner : k => v.id
  }
}

output "runner_names" {
  description = "Names of the runner VMs"
  value = {
    for k, v in infrasim_vm.runner : k => v.name
  }
}

output "runner_ips" {
  description = "IP addresses of the runner VMs"
  value = {
    for k, v in infrasim_vm.runner : k => v.ip_address
  }
}

output "network_id" {
  description = "ID of the runners network"
  value       = infrasim_network.runners.id
}

output "runner_labels" {
  description = "GitHub runner labels"
  value = {
    for k, v in local.variants : k => v.label
  }
}
