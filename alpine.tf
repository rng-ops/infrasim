# Terraform for appliance: my-alpine-rpi (template: alpine-rpi-aarch64)
terraform {
  required_providers {
    infrasim = {
      source  = "infrasim/infrasim"
      version = ">= 0.1.0"
    }
  }
}

provider "infrasim" {
  endpoint = "http://host.docker.internal:50051"
}

resource "infrasim_network" "default" {
  name         = "default"
  mode         = "user"
  cidr         = "10.0.2.0/24"
  gateway      = "10.0.2.2"
  dhcp_enabled = true
}

resource "infrasim_volume" "root" {
  name      = "root"
  size_mb   = 2048
  kind      = "disk"
}

resource "infrasim_volume" "data" {
  name      = "data"
  size_mb   = 1024
  kind      = "disk"
}

resource "infrasim_vm" "my-alpine-rpi" {
  name             = "my-alpine-rpi"
  arch             = "aarch64"
  machine          = "raspi3"
  cpu_cores        = 4
  memory_mb        = 1024
  compatibility_mode = false
  network_ids      = [infrasim_network.default.id]
  volume_ids       = [infrasim_volume.root.id, infrasim_volume.data.id]
}