# InfraSim Terraform Provider - Variables
#
# Configurable variables for the example infrastructure

variable "network_cidr" {
  description = "CIDR block for the lab network"
  type        = string
  default     = "192.168.100.0/24"
}

variable "attacker_cpus" {
  description = "Number of CPUs for the attacker VM"
  type        = number
  default     = 4
}

variable "attacker_memory" {
  description = "Memory in MB for the attacker VM"
  type        = number
  default     = 4096
}

variable "target_count" {
  description = "Number of target VMs to create"
  type        = number
  default     = 3
}

variable "target_cpus" {
  description = "Number of CPUs for each target VM"
  type        = number
  default     = 2
}

variable "target_memory" {
  description = "Memory in MB for each target VM"
  type        = number
  default     = 2048
}

variable "kali_image" {
  description = "Path to the Kali Linux disk image"
  type        = string
  default     = "/var/lib/infrasim/images/kali-xfce-aarch64.qcow2"
}

variable "target_image" {
  description = "Path to the target VM disk image"
  type        = string
  default     = "/var/lib/infrasim/images/debian-aarch64.qcow2"
}

variable "qos_enabled" {
  description = "Enable QoS simulation on target VMs"
  type        = bool
  default     = true
}

variable "qos_latency_ms" {
  description = "Simulated network latency in milliseconds"
  type        = number
  default     = 50
}

variable "qos_jitter_ms" {
  description = "Simulated network jitter in milliseconds"
  type        = number
  default     = 10
}

variable "qos_loss_percent" {
  description = "Simulated packet loss percentage"
  type        = number
  default     = 0.5
}

variable "qos_bandwidth_mbps" {
  description = "Bandwidth limit in Mbps"
  type        = number
  default     = 100
}

variable "daemon_address" {
  description = "InfraSim daemon address"
  type        = string
  default     = "http://127.0.0.1:50051"
}

variable "create_snapshots" {
  description = "Create baseline snapshots"
  type        = bool
  default     = true
}
