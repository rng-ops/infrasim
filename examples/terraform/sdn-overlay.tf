# InfraSim SDN Overlay Example
#
# This example creates a complete software-defined network topology:
# - Edge router with WAN/LAN interfaces
# - WireGuard VPN gateway with Tailscale discovery
# - Stateful firewall protecting DMZ
# - Internal workload VMs
#
# The topology uses qcow2 appliance images with cloud-init for configuration.

terraform {
  required_version = ">= 1.0.0"

  required_providers {
    infrasim = {
      source  = "local/infrasim/infrasim"
      version = "~> 0.1"
    }
  }
}

# ============================================================================
# Provider Configuration
# ============================================================================

provider "infrasim" {
  daemon_address = var.daemon_address
}

# ============================================================================
# Variables
# ============================================================================

variable "daemon_address" {
  description = "InfraSim daemon gRPC address"
  type        = string
  default     = "http://127.0.0.1:50051"
}

variable "image_path" {
  description = "Path to base qcow2 images"
  type        = string
  default     = "/var/lib/infrasim/images"
}

variable "wg_private_key" {
  description = "WireGuard private key for VPN gateway"
  type        = string
  sensitive   = true
}

variable "tailscale_auth_key" {
  description = "Tailscale auth key for control plane integration"
  type        = string
  sensitive   = true
  default     = ""
}

variable "external_peers" {
  description = "External WireGuard peers to connect"
  type = list(object({
    name        = string
    public_key  = string
    endpoint    = optional(string)
    allowed_ips = list(string)
  }))
  default = []
}

# ============================================================================
# Networks
# ============================================================================

# WAN Network - External facing, bridged to host network
resource "infrasim_network" "wan" {
  name = "sdn-wan"
  mode = "vmnet-bridged"
  cidr = "192.168.1.0/24"
  mtu  = 1500
}

# LAN Network - Internal trusted network
resource "infrasim_network" "lan" {
  name         = "sdn-lan"
  mode         = "nat"
  cidr         = "10.0.0.0/24"
  gateway      = "10.0.0.1"
  dhcp_enabled = true
  dhcp_start   = "10.0.0.100"
  dhcp_end     = "10.0.0.200"
  mtu          = 1500
}

# DMZ Network - Semi-trusted, for public-facing services
resource "infrasim_network" "dmz" {
  name         = "sdn-dmz"
  mode         = "nat"
  cidr         = "10.0.1.0/24"
  gateway      = "10.0.1.1"
  dhcp_enabled = true
  mtu          = 1500
}

# VPN Network - WireGuard overlay
resource "infrasim_network" "vpn" {
  name         = "sdn-vpn"
  mode         = "nat"
  cidr         = "10.200.0.0/24"
  gateway      = "10.200.0.1"
  dhcp_enabled = false
  mtu          = 1420  # WireGuard MTU
}

# ============================================================================
# Edge Router
# ============================================================================

resource "infrasim_vm" "router" {
  name   = "edge-router"
  cpus   = 2
  memory = 1024
  disk   = "${var.image_path}/alpine-router-aarch64.qcow2"

  # Multi-homed: WAN, LAN, DMZ
  network_ids = [
    infrasim_network.wan.id,
    infrasim_network.lan.id,
    infrasim_network.dmz.id,
  ]

  cloud_init = base64encode(<<-EOF
    #cloud-config
    hostname: edge-router
    
    write_files:
      # Enable IP forwarding
      - path: /etc/sysctl.d/99-router.conf
        content: |
          net.ipv4.ip_forward = 1
          net.ipv6.conf.all.forwarding = 1
      
      # NAT and routing rules
      - path: /etc/nftables.conf
        content: |
          #!/usr/sbin/nft -f
          
          table inet filter {
            chain input {
              type filter hook input priority 0;
              ct state established,related accept
              iif lo accept
              iif eth1 accept  # LAN trusted
              iif eth2 tcp dport {22, 80, 443} accept  # DMZ services
              tcp dport 22 accept
              drop
            }
            
            chain forward {
              type filter hook forward priority 0;
              ct state established,related accept
              iif eth1 accept  # LAN can forward anywhere
              iif eth2 oif eth0 accept  # DMZ to WAN
              drop
            }
            
            chain output {
              type filter hook output priority 0;
              accept
            }
          }
          
          table ip nat {
            chain postrouting {
              type nat hook postrouting priority 100;
              oif eth0 masquerade
            }
          }
    
    runcmd:
      - sysctl --system
      - nft -f /etc/nftables.conf
      - rc-update add nftables default
  EOF
  )

  labels = {
    role     = "router"
    tier     = "edge"
    infrasim = "sdn-overlay"
  }
}

# ============================================================================
# WireGuard VPN Gateway
# ============================================================================

resource "infrasim_vm" "vpn" {
  name   = "vpn-gateway"
  cpus   = 2
  memory = 512
  disk   = "${var.image_path}/alpine-wireguard-aarch64.qcow2"

  network_ids = [
    infrasim_network.lan.id,
    infrasim_network.vpn.id,
  ]

  cloud_init = base64encode(<<-EOF
    #cloud-config
    hostname: vpn-gateway
    
    write_files:
      - path: /etc/wireguard/wg0.conf
        permissions: '0600'
        content: |
          [Interface]
          PrivateKey = ${var.wg_private_key}
          Address = 10.200.0.1/24
          ListenPort = 51820
          
          PostUp = iptables -A FORWARD -i wg0 -j ACCEPT; iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
          PostDown = iptables -D FORWARD -i wg0 -j ACCEPT; iptables -t nat -D POSTROUTING -o eth0 -j MASQUERADE
          
          %{ for peer in var.external_peers }
          [Peer]
          # ${peer.name}
          PublicKey = ${peer.public_key}
          %{ if peer.endpoint != null }Endpoint = ${peer.endpoint}%{ endif }
          AllowedIPs = ${join(", ", peer.allowed_ips)}
          PersistentKeepalive = 25
          
          %{ endfor }
      
      # Tailscale for C2 overlay (optional)
      - path: /etc/tailscale-setup.sh
        permissions: '0755'
        content: |
          #!/bin/sh
          %{ if var.tailscale_auth_key != "" }
          tailscale up --authkey="${var.tailscale_auth_key}" \
            --advertise-routes=10.200.0.0/24,10.0.0.0/24 \
            --accept-routes \
            --hostname=vpn-gateway
          %{ else }
          echo "No Tailscale auth key provided, skipping"
          %{ endif }
    
    runcmd:
      - modprobe wireguard
      - wg-quick up wg0
      - rc-update add wg-quick.wg0 default
      - /etc/tailscale-setup.sh
  EOF
  )

  labels = {
    role     = "vpn"
    tier     = "core"
    infrasim = "sdn-overlay"
  }
}

# ============================================================================
# Firewall
# ============================================================================

resource "infrasim_vm" "firewall" {
  name   = "firewall"
  cpus   = 2
  memory = 1024
  disk   = "${var.image_path}/alpine-router-aarch64.qcow2"

  network_ids = [
    infrasim_network.lan.id,
    infrasim_network.dmz.id,
  ]

  cloud_init = base64encode(<<-EOF
    #cloud-config
    hostname: firewall
    
    write_files:
      - path: /etc/nftables.conf
        content: |
          #!/usr/sbin/nft -f
          
          # Strict firewall between LAN and DMZ
          table inet filter {
            # Rate limiting sets
            set rate_limit_ssh {
              type ipv4_addr
              flags dynamic,timeout
              timeout 1m
            }
            
            chain input {
              type filter hook input priority 0; policy drop;
              
              # Stateful tracking
              ct state invalid drop
              ct state established,related accept
              
              # Loopback
              iif lo accept
              
              # ICMP with rate limit
              ip protocol icmp icmp type echo-request limit rate 4/second accept
              
              # SSH with rate limiting (5 per minute per IP)
              tcp dport 22 ct state new add @rate_limit_ssh { ip saddr limit rate 5/minute } accept
              
              # Management from LAN only
              iif eth0 tcp dport {22, 80, 443} accept
              
              # Log drops
              log prefix "FIREWALL INPUT DROP: " flags all
            }
            
            chain forward {
              type filter hook forward priority 0; policy drop;
              
              ct state invalid drop
              ct state established,related accept
              
              # LAN to DMZ - allowed
              iif eth0 oif eth1 accept
              
              # DMZ to LAN - deny (DMZ is untrusted)
              iif eth1 oif eth0 drop
              
              # Log drops
              log prefix "FIREWALL FORWARD DROP: " flags all
            }
            
            chain output {
              type filter hook output priority 0; policy accept;
            }
          }
    
    runcmd:
      - sysctl -w net.ipv4.ip_forward=1
      - nft -f /etc/nftables.conf
      - rc-update add nftables default
  EOF
  )

  labels = {
    role     = "firewall"
    tier     = "core"
    infrasim = "sdn-overlay"
  }
}

# ============================================================================
# Workload VMs
# ============================================================================

# Web server in DMZ
resource "infrasim_vm" "webserver" {
  name   = "webserver"
  cpus   = 2
  memory = 2048
  disk   = "${var.image_path}/alpine-aarch64.qcow2"

  network_ids = [infrasim_network.dmz.id]

  # Apply QoS simulation
  qos_latency_ms     = 5
  qos_bandwidth_mbps = 100

  cloud_init = base64encode(<<-EOF
    #cloud-config
    hostname: webserver
    
    packages:
      - nginx
      - php81
      - php81-fpm
    
    write_files:
      - path: /var/www/localhost/htdocs/index.html
        content: |
          <!DOCTYPE html>
          <html>
          <head><title>InfraSim SDN Demo</title></head>
          <body>
            <h1>InfraSim SDN Overlay</h1>
            <p>This server is running in the DMZ network.</p>
            <p>Protected by software-defined firewall.</p>
          </body>
          </html>
    
    runcmd:
      - rc-update add nginx default
      - rc-service nginx start
  EOF
  )

  labels = {
    role     = "webserver"
    tier     = "dmz"
    infrasim = "sdn-overlay"
  }
}

# Internal workstation in LAN
resource "infrasim_vm" "workstation" {
  name   = "workstation"
  cpus   = 4
  memory = 4096
  disk   = "${var.image_path}/kali-xfce-aarch64.qcow2"

  network_ids = [
    infrasim_network.lan.id,
    infrasim_network.vpn.id,  # Also on VPN network for remote access
  ]

  cloud_init = base64encode(<<-EOF
    #cloud-config
    hostname: workstation
    
    users:
      - name: operator
        sudo: ALL=(ALL) NOPASSWD:ALL
        shell: /bin/bash
        groups: [sudo, netdev]
    
    packages:
      - nmap
      - wireshark
      - tcpdump
    
    runcmd:
      - systemctl enable ssh
      - systemctl start ssh
  EOF
  )

  labels = {
    role     = "workstation"
    tier     = "lan"
    infrasim = "sdn-overlay"
  }
}

# ============================================================================
# Snapshots
# ============================================================================

resource "infrasim_snapshot" "baseline" {
  name           = "sdn-baseline"
  vm_id          = infrasim_vm.router.id
  include_memory = false
  description    = "Clean baseline of SDN overlay topology"

  depends_on = [
    infrasim_vm.router,
    infrasim_vm.vpn,
    infrasim_vm.firewall,
  ]
}

# ============================================================================
# Outputs
# ============================================================================

output "topology" {
  description = "SDN topology summary"
  value = {
    networks = {
      wan = infrasim_network.wan.id
      lan = infrasim_network.lan.id
      dmz = infrasim_network.dmz.id
      vpn = infrasim_network.vpn.id
    }
    appliances = {
      router   = infrasim_vm.router.id
      vpn      = infrasim_vm.vpn.id
      firewall = infrasim_vm.firewall.id
    }
    workloads = {
      webserver   = infrasim_vm.webserver.id
      workstation = infrasim_vm.workstation.id
    }
  }
}

output "router_console" {
  description = "Router web console URL"
  value       = infrasim_vm.router.console_url
}

output "vpn_endpoint" {
  description = "VPN gateway WireGuard endpoint"
  value       = "${infrasim_vm.vpn.ip_address}:51820"
}

output "webserver_url" {
  description = "DMZ webserver URL"
  value       = "http://${infrasim_vm.webserver.ip_address}/"
}

output "network_cidrs" {
  description = "Network CIDRs for reference"
  value = {
    wan = infrasim_network.wan.cidr
    lan = infrasim_network.lan.cidr
    dmz = infrasim_network.dmz.cidr
    vpn = infrasim_network.vpn.cidr
  }
}
