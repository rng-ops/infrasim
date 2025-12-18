# InfraSim Terraform Example Outputs

output "summary" {
  description = "Summary of created resources"
  value = {
    network = {
      id      = infrasim_network.lab.id
      name    = infrasim_network.lab.name
      cidr    = infrasim_network.lab.cidr
      gateway = infrasim_network.lab.gateway
    }
    attacker = {
      id          = infrasim_vm.kali_attacker.id
      name        = infrasim_vm.kali_attacker.name
      console_url = infrasim_vm.kali_attacker.console_url
      vnc_port    = infrasim_vm.kali_attacker.vnc_port
      ip_address  = infrasim_vm.kali_attacker.ip_address
    }
    targets = [for idx, vm in infrasim_vm.target : {
      id          = vm.id
      name        = vm.name
      console_url = vm.console_url
      vnc_port    = vm.vnc_port
      ip_address  = vm.ip_address
    }]
  }
}

output "web_console_urls" {
  description = "All web console URLs"
  value = concat(
    [infrasim_vm.kali_attacker.console_url],
    [for vm in infrasim_vm.target : vm.console_url]
  )
}

output "quick_access" {
  description = "Quick access commands"
  value = <<-EOF
    
    =====================================
    InfraSim Lab Quick Access
    =====================================
    
    Attacker VM Console:
      ${infrasim_vm.kali_attacker.console_url}
    
    Target VM Consoles:
    ${join("\n    ", [for vm in infrasim_vm.target : vm.console_url])}
    
    VNC Access (via VNC client):
      vnc://127.0.0.1:${infrasim_vm.kali_attacker.vnc_port}
    
    CLI Commands:
      infrasim vm list
      infrasim console ${infrasim_vm.kali_attacker.id} --open
      infrasim attestation get ${infrasim_vm.kali_attacker.id}
    
  EOF
}
