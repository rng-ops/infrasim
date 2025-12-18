# InfraSim Terraform Provider Example
#
# This example demonstrates how to use InfraSim to create
# a network of Kali Linux VMs with browser-based VNC access.

terraform {
  required_providers {
    infrasim = {
      source  = "local/infrasim/infrasim"
      version = "0.1.0"
    }
  }
}

# Configure the InfraSim provider
provider "infrasim" {
  # Daemon address (default: http://127.0.0.1:50051)
  daemon_address = "http://127.0.0.1:50051"
}

# Create a NAT network for the VMs
resource "infrasim_network" "lab" {
  name    = "pentest-lab"
  mode    = "nat"
  cidr    = "192.168.100.0/24"
  gateway = "192.168.100.1"
  
  dhcp_start = "192.168.100.100"
  dhcp_end   = "192.168.100.200"
  
  mtu = 1500
}

# Create the main Kali attacker VM
resource "infrasim_vm" "kali_attacker" {
  name    = "kali-attacker"
  cpus    = 4
  memory  = 4096  # 4GB RAM
  disk    = "/var/lib/infrasim/images/kali-xfce-aarch64.qcow2"
  
  network_id = infrasim_network.lab.id
  
  # Cloud-init configuration (base64 encoded)
  cloud_init = base64encode(<<-EOF
    #cloud-config
    hostname: kali-attacker
    users:
      - name: kali
        sudo: ALL=(ALL) NOPASSWD:ALL
        shell: /bin/bash
        lock_passwd: false
        passwd: $6$rounds=4096$salt$hashedpassword
    packages:
      - nmap
      - metasploit-framework
      - burpsuite
      - wireshark
    runcmd:
      - systemctl enable ssh
      - systemctl start ssh
  EOF
  )
}

# Create target VMs with QoS simulation
resource "infrasim_vm" "target" {
  count = 3
  
  name    = "target-${count.index + 1}"
  cpus    = 2
  memory  = 2048  # 2GB RAM
  disk    = "/var/lib/infrasim/images/debian-aarch64.qcow2"
  
  network_id = infrasim_network.lab.id
  
  # Simulate WAN latency and packet loss
  qos_latency_ms     = 50
  qos_jitter_ms      = 10
  qos_loss_percent   = 0.5
  qos_bandwidth_mbps = 100
  
  cloud_init = base64encode(<<-EOF
    #cloud-config
    hostname: target-${count.index + 1}
    packages:
      - apache2
      - mysql-server
      - php
    runcmd:
      - systemctl enable apache2
      - systemctl start apache2
  EOF
  )
}

# Create a snapshot of the attacker VM
resource "infrasim_snapshot" "kali_baseline" {
  name           = "baseline"
  vm_id          = infrasim_vm.kali_attacker.id
  include_memory = true
  description    = "Clean baseline before testing"
  
  depends_on = [infrasim_vm.kali_attacker]
}

# Outputs
output "attacker_console_url" {
  description = "Web console URL for the attacker VM"
  value       = infrasim_vm.kali_attacker.console_url
}

output "attacker_vnc_port" {
  description = "VNC port for the attacker VM"
  value       = infrasim_vm.kali_attacker.vnc_port
}

output "target_console_urls" {
  description = "Web console URLs for target VMs"
  value       = [for vm in infrasim_vm.target : vm.console_url]
}

output "network_cidr" {
  description = "Network CIDR"
  value       = infrasim_network.lab.cidr
}
