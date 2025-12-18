# InfraSim API Reference

This document describes the gRPC API for communicating with the InfraSim daemon.

## Overview

InfraSim uses gRPC for all daemon communication. The API is defined in
`proto/infrasim.proto` and includes operations for:

- Networks: Create, read, update, delete virtual networks
- VMs: Full lifecycle management of virtual machines
- Volumes: Disk image management
- Snapshots: Memory and disk snapshots
- Console: VNC/web console access
- Benchmarks: Performance testing
- Attestation: Cryptographic provenance

## Connection

Default endpoint: `grpc://127.0.0.1:50051`

Configure via:
- Daemon: `--grpc-addr` flag
- CLI: `--daemon-addr` flag
- Terraform: `daemon_address` provider attribute

## Service: InfraSimDaemon

### Network Operations

#### CreateNetwork

Create a new virtual network.

```protobuf
rpc CreateNetwork(CreateNetworkRequest) returns (CreateNetworkResponse);

message CreateNetworkRequest {
  NetworkConfig config = 1;
}

message NetworkConfig {
  string name = 1;
  NetworkMode mode = 2;    // NAT, BRIDGE, ISOLATED
  string cidr = 3;         // e.g., "192.168.100.0/24"
  string gateway = 4;
  string dhcp_range_start = 5;
  string dhcp_range_end = 6;
  repeated string dns_servers = 7;
  uint32 mtu = 8;
}
```

**Example (CLI):**
```bash
infrasim network create \
  --name lab-network \
  --mode nat \
  --cidr 192.168.100.0/24
```

#### GetNetwork

Get network details by ID.

```protobuf
rpc GetNetwork(GetNetworkRequest) returns (GetNetworkResponse);
```

#### ListNetworks

List all networks.

```protobuf
rpc ListNetworks(ListNetworksRequest) returns (ListNetworksResponse);
```

#### DeleteNetwork

Delete a network.

```protobuf
rpc DeleteNetwork(DeleteNetworkRequest) returns (DeleteNetworkResponse);

message DeleteNetworkRequest {
  string id = 1;
  bool force = 2;  // Delete even if VMs attached
}
```

---

### VM Operations

#### CreateVm

Create a new virtual machine.

```protobuf
rpc CreateVm(CreateVmRequest) returns (CreateVmResponse);

message VmConfig {
  string name = 1;
  uint32 cpus = 2;
  uint32 memory_mb = 3;
  string disk_image_path = 4;
  string network_id = 5;
  string cloud_init_data = 6;  // Base64-encoded
  ConsoleConfig console = 7;
  QosProfile qos = 8;
}

message ConsoleConfig {
  bool vnc_enabled = 1;
  uint32 vnc_port = 2;      // 0 = auto-assign
  string vnc_password = 3;
  bool websocket_enabled = 4;
  uint32 websocket_port = 5;
}

message QosProfile {
  uint32 latency_ms = 1;
  uint32 jitter_ms = 2;
  float loss_percent = 3;
  uint32 bandwidth_mbps = 4;
}
```

**Example (CLI):**
```bash
infrasim vm create \
  --name kali-workstation \
  --cpus 4 \
  --memory 4096 \
  --disk /var/lib/infrasim/images/kali.qcow2 \
  --network lab-network \
  --latency-ms 50 \
  --loss-percent 0.1
```

#### GetVm

Get VM details by ID.

```protobuf
rpc GetVm(GetVmRequest) returns (GetVmResponse);

message Vm {
  string id = 1;
  VmConfig config = 2;
  VmState state = 3;  // UNKNOWN, STOPPED, STARTING, RUNNING, STOPPING
  string ip_address = 4;
  string created_at = 5;
  string updated_at = 6;
}
```

#### ListVms

List all VMs.

```protobuf
rpc ListVms(ListVmsRequest) returns (ListVmsResponse);
```

#### StartVm

Start a stopped VM.

```protobuf
rpc StartVm(StartVmRequest) returns (StartVmResponse);
```

#### StopVm

Stop a running VM.

```protobuf
rpc StopVm(StopVmRequest) returns (StopVmResponse);

message StopVmRequest {
  string id = 1;
  bool force = 2;  // SIGKILL instead of graceful shutdown
}
```

#### DeleteVm

Delete a VM.

```protobuf
rpc DeleteVm(DeleteVmRequest) returns (DeleteVmResponse);
```

---

### Volume Operations

#### CreateVolume

Create a new disk volume.

```protobuf
rpc CreateVolume(CreateVolumeRequest) returns (CreateVolumeResponse);

message VolumeConfig {
  string name = 1;
  string path = 2;          // Auto-assigned if empty
  uint64 size_bytes = 3;
  VolumeFormat format = 4;  // QCOW2, RAW
  string base_image = 5;    // For copy-on-write
}
```

**Example (CLI):**
```bash
infrasim volume create \
  --name data-disk \
  --size 100 \
  --format qcow2
```

#### GetVolume / ListVolumes / DeleteVolume

Similar to network operations.

---

### Snapshot Operations

#### CreateSnapshot

Create a point-in-time snapshot.

```protobuf
rpc CreateSnapshot(CreateSnapshotRequest) returns (CreateSnapshotResponse);

message SnapshotConfig {
  string vm_id = 1;
  string name = 2;
  bool include_memory = 3;  // Include RAM state
  string description = 4;
}
```

**Example (CLI):**
```bash
infrasim snapshot create my-vm \
  --name before-test \
  --memory \
  --description "Clean state before penetration test"
```

#### RestoreSnapshot

Restore a VM to a snapshot state.

```protobuf
rpc RestoreSnapshot(RestoreSnapshotRequest) returns (RestoreSnapshotResponse);
```

#### ListSnapshots / DeleteSnapshot

Similar to other operations.

---

### Console Operations

#### GetConsole

Get console access information.

```protobuf
rpc GetConsole(GetConsoleRequest) returns (GetConsoleResponse);

message GetConsoleResponse {
  string vnc_address = 1;   // e.g., "127.0.0.1:5901"
  string web_url = 2;       // e.g., "http://127.0.0.1:8080/console/abc123"
  string websocket_url = 3; // e.g., "ws://127.0.0.1:8080/ws/abc123"
}
```

**Example (CLI):**
```bash
# Print console URL
infrasim console my-vm

# Open in browser
infrasim console my-vm --open
```

---

### Benchmark Operations

#### RunBenchmark

Run performance benchmarks on a VM.

```protobuf
rpc RunBenchmark(RunBenchmarkRequest) returns (RunBenchmarkResponse);

message BenchmarkConfig {
  string vm_id = 1;
  string benchmark_type = 2;  // cpu, memory, disk, network, all
  uint32 duration_secs = 3;
  uint32 threads = 4;
}

message BenchmarkResult {
  string vm_id = 1;
  string benchmark_type = 2;
  uint32 duration_secs = 3;
  double cpu_score = 4;
  double memory_score = 5;
  double disk_read_mbps = 6;
  double disk_write_mbps = 7;
  double network_throughput_mbps = 8;
}
```

**Example (CLI):**
```bash
infrasim benchmark my-vm --type all --duration 60
```

---

### Attestation Operations

#### GetAttestation

Get cryptographic attestation for a VM.

```protobuf
rpc GetAttestation(GetAttestationRequest) returns (GetAttestationResponse);

message AttestationReport {
  string vm_id = 1;
  string timestamp = 2;
  HostInfo host_info = 3;
  DiskInfo disk_info = 4;
  string signature = 5;  // Ed25519 signature
}

message HostInfo {
  string hostname = 1;
  string os_version = 2;
  string kernel_version = 3;
  string qemu_version = 4;
  bool hvf_enabled = 5;
}

message DiskInfo {
  string sha256 = 1;
  uint64 size_bytes = 2;
  string path = 3;
}
```

**Example (CLI):**
```bash
# View attestation
infrasim attestation get my-vm

# Verify signature
infrasim attestation verify my-vm --pubkey /path/to/signing.pub

# Export to file
infrasim attestation export my-vm --output report.json
```

---

## Error Handling

All RPCs return standard gRPC status codes:

| Code | Description |
|------|-------------|
| `OK` | Success |
| `NOT_FOUND` | Resource not found |
| `ALREADY_EXISTS` | Resource already exists |
| `INVALID_ARGUMENT` | Invalid request parameters |
| `FAILED_PRECONDITION` | Operation not allowed in current state |
| `INTERNAL` | Internal server error |
| `UNAVAILABLE` | Daemon not running |

## Streaming

Future versions may include streaming RPCs for:
- Real-time VM state updates
- Console output streaming
- Log streaming

## Authentication

Currently, authentication is not implemented. The daemon binds to
localhost only by default. For production use, consider:

- TLS with client certificates
- mTLS authentication
- Integration with external auth systems

## Rate Limiting

No rate limiting is currently enforced. The daemon processes requests
sequentially for state consistency.
