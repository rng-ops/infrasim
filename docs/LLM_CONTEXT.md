# InfraSim - LLM Context Document

## Project Overview
InfraSim is a Terraform-compatible QEMU virtualization platform for macOS Apple Silicon. It enables running ARM64 VMs (including Raspberry Pi emulation) with hardware acceleration via HVF.

**Target**: macOS M1/M2/M3 | **Guest Arch**: aarch64 | **Hypervisor**: QEMU + HVF

## Architecture Summary

```
┌─────────────────────────────────────────────────────────────┐
│                    Terraform (HCL)                          │
│                         │                                    │
│                         ▼                                    │
│              terraform-provider-infrasim                     │
│                    (tfplugin6)                              │
│                         │                                    │
│                         ▼ gRPC                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │                    infrasimd                          │   │
│  │  ┌────────────┐ ┌──────────┐ ┌──────────────────┐    │   │
│  │  │ QemuLauncher│ │StateManager│ │VolumeIntegrity│    │   │
│  │  └────────────┘ └──────────┘ └──────────────────┘    │   │
│  │  ┌────────────┐ ┌──────────┐ ┌──────────────────┐    │   │
│  │  │ QMPClient  │ │Attestation│ │TrafficShaper   │    │   │
│  │  └────────────┘ └──────────┘ └──────────────────┘    │   │
│  └──────────────────────────────────────────────────────┘   │
│                         │                                    │
│                         ▼                                    │
│  ┌──────────────────────────────────────────────────────┐   │
│  │                    QEMU                               │   │
│  │  -machine virt -cpu cortex-a76 -accel hvf            │   │
│  │  VirtIO: blk, net, rng, console                      │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

## Crate Structure

### 1. infrasim-common (lib)
Core types, traits, and utilities shared across crates.

**Key Modules:**
- `types.rs`: Domain types (VM, Network, Volume, Snapshot, etc.) with Kubernetes-style meta/spec/status
- `cas.rs`: Content-Addressable Storage with Blake3 hashing
- `crypto.rs`: Ed25519 signing, X25519 key exchange, ChaCha20 encryption
- `db.rs`: SQLite-backed persistent storage via sled
- `qmp.rs`: QEMU Machine Protocol async client
- `attestation.rs`: vTPM attestation and provenance reporting
- `traffic_shaper.rs`: QoS with token bucket, LoRa modulation simulation

**Resource Types (meta/spec/status pattern):**
```rust
pub struct VM { meta: ResourceMeta, spec: VMSpec, status: VMStatus }
pub struct Network { meta: ResourceMeta, spec: NetworkSpec, status: NetworkStatus }
pub struct Volume { meta: ResourceMeta, spec: VolumeSpec, status: VolumeStatus }
pub struct Snapshot { meta: ResourceMeta, spec: SnapshotSpec, status: SnapshotStatus }
```

### 2. infrasim-daemon (bin: infrasimd)
gRPC daemon that orchestrates QEMU VMs.

**Key Modules:**
- `grpc.rs`: InfraSimDaemon service implementation (CRUD for all resources)
- `qemu.rs`: QemuLauncher, VolumePreparer - spawns QEMU processes
- `state.rs`: StateManager - in-memory + sqlite persistence
- `config.rs`: DaemonConfig from TOML

**gRPC Service (proto: infrasim.v1):**
```protobuf
service InfraSimDaemon {
  rpc CreateVM, GetVM, UpdateVM, DeleteVM, ListVMs, StartVM, StopVM
  rpc CreateNetwork, GetNetwork, DeleteNetwork, ListNetworks
  rpc CreateVolume, GetVolume, DeleteVolume, ListVolumes
  rpc CreateSnapshot, GetSnapshot, DeleteSnapshot, ListSnapshots, RestoreSnapshot
  rpc CreateConsole, GetConsole, DeleteConsole
  rpc CreateBenchmarkRun, GetBenchmarkRun, ListBenchmarkRuns
  rpc GetAttestation
  rpc GetHealth, GetDaemonStatus
}
```

### 3. infrasim-provider (bin: terraform-provider-infrasim)
Terraform Plugin Protocol v6 provider.

**Key Modules:**
- `provider.rs`: Implements tfplugin6::Provider trait
- `resources/`: vm.rs, network.rs, volume.rs, snapshot.rs
- `schema.rs`: Terraform schema generation
- `state.rs`: DynamicValue encode/decode (msgpack/json)
- `client.rs`: gRPC client to daemon

**Terraform Resources:**
```hcl
resource "infrasim_vm" "example" {
  name = "my-vm"
  cpu_cores = 4
  memory_mb = 4096
  boot_disk_id = infrasim_volume.disk.id
}
resource "infrasim_network" "private" { ... }
resource "infrasim_volume" "disk" { ... }
resource "infrasim_snapshot" "backup" { ... }
```

### 4. infrasim-cli (bin: infrasim)
Human-friendly CLI.

**Commands:**
```bash
infrasim vm create|list|get|start|stop|delete
infrasim network create|list|get|delete
infrasim volume create|list|get|delete
infrasim snapshot create|list|get|restore|delete
infrasim console <vm_id>
infrasim benchmark run|list|get
infrasim attestation get|verify
infrasim status|version
```

### 5. infrasim-web (lib)
REST/WebSocket API and VNC web proxy (axum-based).

**Endpoints:**
- `GET /api/vms`, `POST /api/vms`, `GET /api/vms/:id`
- `GET /api/networks`, `GET /api/volumes`, `GET /api/snapshots`
- `WS /ws/console/:vm_id` - WebSocket to VNC

## Proto Definitions

### Core Enums
```protobuf
enum VMState { UNSPECIFIED, PENDING, RUNNING, STOPPED, PAUSED, ERROR }
enum NetworkMode { UNSPECIFIED, USER, VMNET_SHARED, VMNET_BRIDGED }
enum VolumeKind { UNSPECIFIED, DISK, WEIGHTS }
```

### Key Messages
```protobuf
message ResourceMeta {
  string id = 1;
  string name = 2;
  map<string, string> labels = 3;
  int64 created_at = 4;
  int64 updated_at = 5;
}

message VMSpec {
  string arch = 1;           // "aarch64"
  string machine = 2;        // "virt" or "raspi3b"
  int32 cpu_cores = 3;
  int64 memory_mb = 4;
  repeated string volume_ids = 5;
  repeated string network_ids = 6;
  string qos_profile_id = 7;
  bool enable_tpm = 8;
  string boot_disk_id = 9;
  map<string, string> extra_args = 10;
  bool compatibility_mode = 11;  // raspi3b compat
}

message IntegrityConfig {
  string scheme = 1;         // "ed25519-blake3"
  bytes public_key = 2;
  bytes signature = 3;
  string expected_digest = 4;
}

message VolumeSpec {
  VolumeKind kind = 1;
  string source = 2;
  IntegrityConfig integrity = 3;
  bool read_only = 4;
  int64 size_bytes = 5;
  string format = 6;         // "qcow2", "raw"
  bool overlay = 7;
}

message HostProvenance {
  string qemu_version = 1;
  repeated string qemu_args = 2;
  string base_image_hash = 3;
  map<string, string> volume_hashes = 4;
  string macos_version = 5;
  string cpu_model = 6;
  bool hvf_enabled = 7;
  string hostname = 8;
  int64 timestamp = 9;
}
```

## Key Implementation Patterns

### 1. QEMU Invocation
```rust
// QemuLauncher::launch_vm()
Command::new("qemu-system-aarch64")
    .args(["-machine", "virt", "-cpu", "cortex-a76", "-accel", "hvf"])
    .args(["-m", &format!("{}M", spec.memory_mb)])
    .args(["-smp", &spec.cpu_cores.to_string()])
    .args(["-drive", &format!("file={},format=qcow2,if=virtio", disk)])
    .args(["-netdev", "user,id=net0", "-device", "virtio-net-pci,netdev=net0"])
    .args(["-chardev", &format!("socket,id=mon,path={},server=on,wait=off", qmp)])
    .args(["-mon", "chardev=mon,mode=control"])
    .spawn()
```

### 2. QMP Communication
```rust
// QmpClient - QEMU Machine Protocol
async fn execute<R>(&mut self, cmd: &str, args: Option<Value>) -> Result<R>;
// Commands: query-status, stop, cont, system_powerdown, migrate, loadvm, savevm
```

### 3. Volume Integrity
```rust
// Blake3 content hash + Ed25519 signature
let digest = blake3::hash(data);
let signature = keypair.sign(digest.as_bytes());
// Verification on mount
```

### 4. State Persistence
```rust
// sled + SQLite hybrid
state.insert::<VMSpec, VMStatus>(id, spec, status)?;
let vm = state.get::<VM>(id)?;
```

## Build & Run

```bash
# Build
cargo build --release

# Run daemon
./target/release/infrasimd --config config.toml

# CLI
./target/release/infrasim vm list

# Terraform
terraform init
terraform apply
```

## Configuration (config.toml)
```toml
[daemon]
grpc_listen = "127.0.0.1:50051"
data_dir = "/var/lib/infrasim"
qemu_path = "/opt/homebrew/bin/qemu-system-aarch64"

[storage]
images_dir = "images"
volumes_dir = "volumes"
snapshots_dir = "snapshots"

[network]
default_mode = "user"
vmnet_interface = "bridge100"
```

## Dependencies Summary
- **Async**: tokio, async-trait
- **gRPC**: tonic, prost
- **Crypto**: ed25519-dalek, x25519-dalek, chacha20poly1305, blake3
- **Storage**: sled, rusqlite
- **Web**: axum, tower, hyper
- **CLI**: clap, tracing
- **Terraform**: tfplugin6 proto

## File Counts
- Rust files: ~50
- Proto files: 2 (infrasim.proto, tfplugin6.proto)
- Total LOC: ~15,000

## Extension Points
1. Add new NetworkMode (macvtap, etc.)
2. Implement GPU passthrough
3. Add cloud-init/user-data support
4. Extend attestation providers
5. Add custom benchmark suites
