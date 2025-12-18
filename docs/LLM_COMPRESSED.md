# InfraSim Compressed Context

## Identity
**InfraSim**: Terraform-compatible QEMU platform for macOS ARM64 (M1/M2/M3). Runs aarch64 VMs with HVF acceleration.

## Stack
Rust, tonic/gRPC, QEMU, Terraform Plugin Protocol v6, tokio async, axum web

## Crates & Binaries
```
infrasim-common     # types, crypto, cas, qmp, db, attestation
infrasim-daemon     # infrasimd binary, gRPC server, QemuLauncher
infrasim-provider   # terraform-provider-infrasim binary
infrasim-cli        # infrasim binary
infrasim-web        # REST/WebSocket lib
```

## Proto Types (K8s-style meta/spec/status)
```
VM       {meta, VMSpec{arch,machine,cpu,mem,volumes,networks,tpm}, VMStatus{state,pid,qmp}}
Network  {meta, NetworkSpec{mode,cidr,gw,dns,dhcp,mtu}, NetworkStatus{active}}
Volume   {meta, VolumeSpec{kind,source,integrity,ro,size,format}, VolumeStatus{ready,digest}}
Snapshot {meta, SnapshotSpec{vm_id,include_memory,disk,desc}, SnapshotStatus{complete,size}}
Console  {meta, ConsoleSpec{vm_id,vnc,web}, ConsoleStatus{active,url}}
```

## Enums
VMState: Unspecified|Pending|Running|Stopped|Paused|Error
NetworkMode: Unspecified|User|VmnetShared|VmnetBridged
VolumeKind: Unspecified|Disk|Weights

## gRPC Service (InfraSimDaemon)
Create/Get/Update/Delete/List for: VM, Network, Volume, Snapshot, Console, QoSProfile, BenchmarkRun
Start/Stop VM, RestoreSnapshot, GetAttestation, GetHealth

## Request Patterns
```rust
CreateVmRequest { name: String, spec: Option<VmSpec>, labels: HashMap }
GetVmRequest { id: String }
DeleteVmRequest { id: String, force: bool }
ListVMsRequest { label_selector: HashMap }
```

## Key Implementation
```rust
// QEMU launch
qemu-system-aarch64 -machine virt -cpu cortex-a76 -accel hvf -m {mem}M -smp {cpu}
  -drive file={disk},format=qcow2,if=virtio
  -chardev socket,id=mon,path={qmp},server=on -mon chardev=mon,mode=control

// QMP protocol for VM control
qmp.execute("stop"|"cont"|"system_powerdown"|"savevm"|"loadvm")

// Content integrity
Blake3(data) → digest, Ed25519::sign(digest) → signature
```

## CLI
```
infrasim vm create|list|get|start|stop|delete
infrasim network|volume|snapshot|console|benchmark|attestation
```

## Terraform
```hcl
resource "infrasim_vm" "x" { name="vm", cpu_cores=4, memory_mb=4096, boot_disk_id=vol.id }
resource "infrasim_network" "n" { mode="user", cidr="10.0.0.0/24" }
resource "infrasim_volume" "v" { kind="disk", size_bytes=10737418240 }
```

## Paths
```
proto/infrasim.proto, proto/tfplugin6.proto
crates/{common,daemon,provider,cli,web}/src/
examples/, docs/, images/
```

## Deps: tokio, tonic, prost, clap, axum, sled, rusqlite, blake3, ed25519-dalek, chacha20poly1305
