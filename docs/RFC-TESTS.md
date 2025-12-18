# RFC-TESTS: InfraSim Verification & Certification Test Suite

**Document Version:** 1.0  
**Status:** DRAFT  
**Date:** 2024-12-15  
**Target Audience:** Verifiers, Auditors, LLM Assistants (ChatGPT, Claude, etc.)

---

## Abstract

This document defines a comprehensive test suite for verifying InfraSim - a Terraform-compatible QEMU platform for macOS Apple Silicon. It provides structured checksheets for:

1. **Cryptographic Provenance Verification** - Proving artifact authenticity
2. **End-to-End Functional Tests** - Full system validation
3. **Terraform/Kubernetes Equivalence Tests** - IaC compatibility verification
4. **Software-Defined Device Tests** - Virtual device simulation accuracy
5. **Memory Snapshot Integrity Tests** - QCOW2/snapshot verification

The test suite is designed so that an independent verifier (human or LLM) can systematically validate the system's correctness and security properties.

---

## Table of Contents

1. [System Overview](#1-system-overview)
2. [What Has Been Built](#2-what-has-been-built)
3. [Current Implementation Status](#3-current-implementation-status)
4. [Cryptographic Provenance Verification](#4-cryptographic-provenance-verification)
5. [End-to-End Test Suite](#5-end-to-end-test-suite)
6. [Terraform Equivalence Tests](#6-terraform-equivalence-tests)
7. [Kubernetes Equivalence Tests](#7-kubernetes-equivalence-tests)
8. [Software-Defined Device Tests](#8-software-defined-device-tests)
9. [Snapshot & Memory Integrity Tests](#9-snapshot--memory-integrity-tests)
10. [QoS & Network Simulation Tests](#10-qos--network-simulation-tests)
11. [Security Verification Tests](#11-security-verification-tests)
12. [Verification Checksheet](#12-verification-checksheet)
13. [Test Execution Guide](#13-test-execution-guide)

---

## 1. System Overview

### 1.1 Purpose

InfraSim provides:
- **QEMU Orchestration** on macOS Apple Silicon with HVF acceleration
- **Terraform Provider** implementing Plugin Protocol v6
- **Cryptographic Attestation** with Ed25519 signatures
- **Content-Addressed Storage** with SHA-256 deduplication
- **QoS Simulation** (latency, jitter, packet loss, bandwidth shaping)
- **Software-Defined Devices** (LoRaWAN simulation)
- **Memory/Disk Snapshots** with optional encryption

### 1.2 Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        User Interfaces                               │
├─────────────────┬─────────────────┬─────────────────┬───────────────┤
│   Terraform     │    CLI          │   Web Console   │   gRPC API    │
│   Provider      │  (infrasim)     │   (noVNC)       │   (direct)    │
└───────┬─────────┴───────┬─────────┴───────┬─────────┴───────┬───────┘
        │                 │                 │                 │
        └─────────────────┴─────────────────┴─────────────────┘
                                   │
                                   ▼ gRPC (port 50051)
        ┌──────────────────────────────────────────────────────────────┐
        │                      infrasimd (Daemon)                       │
        ├───────────────┬───────────────┬───────────────┬──────────────┤
        │ StateManager  │ QemuLauncher  │ Reconciler    │ Attestation  │
        │ (SQLite+RAM)  │ (QEMU spawn)  │ (Drift loop)  │ (Ed25519)    │
        ├───────────────┴───────────────┴───────────────┴──────────────┤
        │                     Content-Addressed Store                   │
        │                        (SHA-256 CAS)                          │
        └──────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
        ┌──────────────────────────────────────────────────────────────┐
        │                   QEMU Processes                              │
        │  ┌─────────┐  ┌─────────┐  ┌─────────┐                       │
        │  │  VM 1   │  │  VM 2   │  │  VM N   │                       │
        │  │(HVF/TCG)│  │(HVF/TCG)│  │(HVF/TCG)│                       │
        │  └─────────┘  └─────────┘  └─────────┘                       │
        └──────────────────────────────────────────────────────────────┘
```

---

## 2. What Has Been Built

### 2.1 Core Binaries

| Binary | Description | Size | Status |
|--------|-------------|------|--------|
| `infrasim` | CLI for VM management | ~3.1MB | ✅ Compiles |
| `infrasimd` | gRPC daemon | ~4.9MB | ✅ Compiles |
| `terraform-provider-infrasim` | Terraform Plugin v6 | ~3.0MB | ✅ Compiles |

### 2.2 Crate Structure

```
infrasim/
├── crates/
│   ├── common/        # Shared types, crypto, CAS, DB, QMP, attestation
│   ├── daemon/        # gRPC server, QEMU launcher, state management
│   ├── provider/      # Terraform Plugin Protocol v6 implementation
│   ├── cli/           # Command-line interface
│   └── web/           # REST/WebSocket server (noVNC proxy)
├── proto/
│   ├── infrasim.proto # gRPC API definition
│   └── tfplugin6.proto # Terraform Plugin Protocol
└── examples/
    └── terraform/     # Sample Terraform configurations
```

### 2.3 Implemented Features

#### Cryptographic Subsystem
- **Ed25519 Key Generation** (`crates/common/src/crypto.rs`)
- **Attestation Reports** (`crates/common/src/attestation.rs`)
- **SHA-256 Content Hashing** (`crates/common/src/cas.rs`)
- **Signed Data Wrapper** with JSON serialization

#### Storage Subsystem
- **Content-Addressed Store** with deduplication
- **SQLite Persistence** with WAL mode
- **QCOW2 Volume Management** with overlay support
- **Encrypted Memory Snapshots** (XOR placeholder - needs ChaCha20-Poly1305)

#### VM Management
- **QEMU Process Spawning** with HVF acceleration
- **QMP Protocol** for VM control
- **VNC Console** access
- **State Reconciliation** loop

#### Networking
- **User-mode Networking** (default)
- **vmnet Support** (bridged/shared)
- **QoS Traffic Shaping** (latency, jitter, loss, bandwidth)
- **LoRaWAN Simulation** scaffold

#### Terraform Integration
- **Plugin Protocol v6** implementation
- **Resources:** `infrasim_vm`, `infrasim_network`, `infrasim_volume`, `infrasim_snapshot`
- **State Encoding:** MessagePack + JSON

---

## 3. Current Implementation Status

### 3.1 Working Components

| Component | Status | Unit Tests | Integration Tests |
|-----------|--------|------------|-------------------|
| Ed25519 Crypto | ✅ Working | ✅ 4 tests | ⬜ Needed |
| CAS Store | ✅ Working | ✅ 3 tests | ⬜ Needed |
| SQLite Database | ✅ Working | ✅ 1 test | ⬜ Needed |
| Traffic Shaper | ✅ Working | ✅ 5 tests | ⬜ Needed |
| QMP Client | ✅ Working | ✅ 3 tests | ⬜ Needed |
| Attestation | ✅ Working | ✅ 3 tests | ⬜ Needed |
| VNC Proxy | ✅ Compiles | ✅ 1 test | ⬜ Needed |
| gRPC Server | ✅ Compiles | ⬜ Needed | ⬜ Needed |
| Terraform Provider | ✅ Compiles | ⬜ Needed | ⬜ Needed |
| CLI | ✅ Compiles | ⬜ Needed | ⬜ Needed |

### 3.2 Known Gaps

1. **vTPM Attestation** - Scaffold only, not implemented
2. **SEV-SNP / TDX** - Placeholder documentation only (not applicable to macOS)
3. **Memory Encryption** - Uses XOR placeholder, needs proper crypto
4. **Integration Tests** - None exist yet
5. **E2E Tests** - Not implemented

---

## 4. Cryptographic Provenance Verification

### 4.1 Artifact Chain of Trust

```
┌─────────────────────────────────────────────────────────────────────┐
│                    BUILD ARTIFACT PROVENANCE                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  1. Source Code ──────────────────────────────────────────────────► │
│     │                                                                │
│     ▼ SHA-256 (git commit)                                          │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │ git commit: abc123...                                         │   │
│  │ Cargo.lock: deterministic dependencies                        │   │
│  └──────────────────────────────────────────────────────────────┘   │
│     │                                                                │
│     ▼ cargo build --release                                         │
│  2. Binary Artifacts ─────────────────────────────────────────────► │
│     │                                                                │
│     ▼ SHA-256 (per binary)                                          │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │ infrasim-{VERSION}-{TARGET}.tar.gz                            │   │
│  │   └── SHA256: <checksum>                                      │   │
│  │ infrasimd-{VERSION}-{TARGET}.tar.gz                           │   │
│  │   └── SHA256: <checksum>                                      │   │
│  │ terraform-provider-infrasim-{VERSION}-{TARGET}.tar.gz         │   │
│  │   └── SHA256: <checksum>                                      │   │
│  └──────────────────────────────────────────────────────────────┘   │
│     │                                                                │
│     ▼ manifest.json                                                  │
│  3. Build Manifest ───────────────────────────────────────────────► │
│     │                                                                │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │ {                                                             │   │
│  │   "version": "v0.1.0",                                       │   │
│  │   "build_date": "2024-12-15T...",                            │   │
│  │   "target": "aarch64-apple-darwin",                          │   │
│  │   "artifacts": { ... checksums ... }                         │   │
│  │ }                                                             │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### 4.2 Runtime Attestation Chain

```
┌─────────────────────────────────────────────────────────────────────┐
│                    VM RUNTIME ATTESTATION                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  1. Volume Preparation ───────────────────────────────────────────► │
│     │                                                                │
│     ▼ SHA-256 per volume                                            │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │ Volume: boot-disk                                             │   │
│  │   source: kali-aarch64.qcow2                                  │   │
│  │   digest: sha256:abc123...                                    │   │
│  │   verified: true                                              │   │
│  └──────────────────────────────────────────────────────────────┘   │
│     │                                                                │
│     ▼ VM Launch                                                      │
│  2. Host Provenance Collection ───────────────────────────────────► │
│     │                                                                │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │ HostProvenance {                                              │   │
│  │   qemu_version: "8.2.0",                                     │   │
│  │   qemu_args: ["-machine", "virt", ...],                      │   │
│  │   base_image_hash: "sha256:abc123...",                       │   │
│  │   volume_hashes: { "disk0": "sha256:..." },                  │   │
│  │   macos_version: "14.2",                                     │   │
│  │   cpu_model: "Apple M2",                                     │   │
│  │   hvf_enabled: true,                                         │   │
│  │   hostname: "build-host",                                    │   │
│  │   timestamp: 1702656000                                      │   │
│  │ }                                                             │   │
│  └──────────────────────────────────────────────────────────────┘   │
│     │                                                                │
│     ▼ SHA-256 + Ed25519 Sign                                         │
│  3. Attestation Report ───────────────────────────────────────────► │
│     │                                                                │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │ AttestationReport {                                           │   │
│  │   id: "uuid-...",                                            │   │
│  │   vm_id: "vm-123",                                           │   │
│  │   host_provenance: { ... },                                  │   │
│  │   digest: "sha256:provenance_hash",                          │   │
│  │   signature: "<Ed25519 signature>",                          │   │
│  │   attestation_type: "host_provenance"                        │   │
│  │ }                                                             │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### 4.3 Verification Tests

#### TEST-CRYPTO-001: Ed25519 Key Generation
```
GIVEN: No existing key pair
WHEN: KeyPair::generate() is called
THEN:
  - Public key is 32 bytes
  - Private key is 32 bytes
  - Different calls produce different keys
```

#### TEST-CRYPTO-002: Sign and Verify
```
GIVEN: A generated key pair and message "test data"
WHEN: Message is signed and then verified
THEN:
  - Signature is 64 bytes
  - Verification succeeds with correct key
  - Verification fails with different key
```

#### TEST-CRYPTO-003: Tamper Detection
```
GIVEN: A signed message
WHEN: Signature is modified (any bit flip)
THEN: Verification fails with crypto error
```

#### TEST-CRYPTO-004: Attestation Report Generation
```
GIVEN: A VM with associated volumes
WHEN: generate_report() is called
THEN:
  - Report contains valid UUID
  - Digest is SHA-256 of HostProvenance
  - Signature is valid Ed25519 over digest
  - All volume hashes are included
```

#### TEST-CRYPTO-005: Attestation Report Verification
```
GIVEN: A generated attestation report
WHEN: verify_report() is called
THEN:
  - Digest is recomputed and matches
  - Signature verifies correctly
  - Returns true for valid report
```

#### TEST-CRYPTO-006: CAS Integrity Verification
```
GIVEN: Data stored in CAS
WHEN: File content is modified on disk
THEN:
  - get() returns IntegrityError
  - Error message includes expected vs actual digest
```

---

## 5. End-to-End Test Suite

### 5.1 Daemon Lifecycle Tests

#### TEST-E2E-001: Daemon Startup
```
GIVEN: No running daemon
WHEN: infrasimd is started with valid config
THEN:
  - gRPC server listens on configured port
  - Health endpoint returns healthy=true
  - Database is initialized
  - CAS directories are created
```

#### TEST-E2E-002: Daemon Shutdown
```
GIVEN: Running daemon with active VMs
WHEN: SIGTERM is sent
THEN:
  - All VMs receive graceful shutdown signal
  - State is persisted to database
  - gRPC server stops accepting connections
  - Exit code is 0
```

### 5.2 VM Lifecycle Tests

#### TEST-E2E-010: Create VM
```
GIVEN: Running daemon
WHEN: CreateVM RPC is called with valid spec
THEN:
  - VM is created with PENDING state
  - Unique ID is assigned
  - VM is persisted to database
```

#### TEST-E2E-011: Start VM
```
GIVEN: Created VM with boot disk
WHEN: StartVM RPC is called
THEN:
  - QEMU process is spawned
  - QMP socket is created
  - VNC display is allocated
  - State transitions to RUNNING
  - PID is recorded in status
```

#### TEST-E2E-012: Stop VM
```
GIVEN: Running VM
WHEN: StopVM RPC is called (force=false)
THEN:
  - ACPI shutdown is sent via QMP
  - QEMU process exits within timeout
  - State transitions to STOPPED
```

#### TEST-E2E-013: Force Stop VM
```
GIVEN: Running VM that ignores ACPI
WHEN: StopVM RPC is called (force=true)
THEN:
  - SIGKILL is sent to QEMU process
  - Process terminates immediately
  - State transitions to STOPPED
```

#### TEST-E2E-014: Delete VM
```
GIVEN: Stopped VM
WHEN: DeleteVM RPC is called
THEN:
  - VM is removed from database
  - QMP socket is cleaned up
  - Resources are released
```

#### TEST-E2E-015: Delete Running VM (force=false)
```
GIVEN: Running VM
WHEN: DeleteVM RPC is called without force
THEN:
  - Error is returned: "VM must be stopped first"
  - VM remains running
```

#### TEST-E2E-016: Delete Running VM (force=true)
```
GIVEN: Running VM
WHEN: DeleteVM RPC is called with force=true
THEN:
  - VM is stopped forcefully
  - VM is deleted
  - All resources are cleaned up
```

### 5.3 Volume Lifecycle Tests

#### TEST-E2E-020: Create Volume from Source
```
GIVEN: A QCOW2 image file
WHEN: CreateVolume RPC is called with source path
THEN:
  - Volume is registered
  - SHA-256 digest is computed
  - Local path is recorded
  - ready=true in status
```

#### TEST-E2E-021: Create Overlay Volume
```
GIVEN: An existing base volume
WHEN: CreateVolume RPC is called with overlay=true
THEN:
  - New QCOW2 is created with backing_file
  - Writes go to overlay only
  - Base image is unchanged
```

#### TEST-E2E-022: Volume Integrity Check
```
GIVEN: Volume with integrity config (expected_digest)
WHEN: Volume is prepared for VM
THEN:
  - Actual digest is computed
  - Digest matches expected
  - verified=true in status
```

#### TEST-E2E-023: Volume Integrity Failure
```
GIVEN: Volume with wrong expected_digest
WHEN: VM attempts to start with this volume
THEN:
  - Start fails with IntegrityError
  - VM remains in PENDING state
  - Error message includes digest mismatch
```

### 5.4 Snapshot Tests

#### TEST-E2E-030: Create Disk Snapshot
```
GIVEN: Running VM with disk
WHEN: CreateSnapshot RPC is called (include_memory=false)
THEN:
  - QCOW2 internal snapshot is created
  - Snapshot path is recorded
  - Digest is computed
```

#### TEST-E2E-031: Create Memory Snapshot
```
GIVEN: Running VM
WHEN: CreateSnapshot RPC is called (include_memory=true)
THEN:
  - VM is paused
  - Memory state is dumped
  - Disk snapshot is created
  - VM is resumed
  - Both paths recorded in status
```

#### TEST-E2E-032: Restore Snapshot
```
GIVEN: VM with existing snapshot
WHEN: RestoreSnapshot RPC is called
THEN:
  - VM is stopped if running
  - Disk is reverted to snapshot
  - Memory is restored (if memory snapshot)
  - VM state matches snapshot state
```

### 5.5 Network Tests

#### TEST-E2E-040: Create User-Mode Network
```
GIVEN: No existing network
WHEN: CreateNetwork RPC is called with mode=USER
THEN:
  - Network is registered
  - Default gateway is configured
  - DHCP range is set
```

#### TEST-E2E-041: Attach VM to Network
```
GIVEN: Running VM and existing network
WHEN: VM spec includes network_id
THEN:
  - QEMU has virtio-net device
  - Guest can obtain DHCP address
  - Guest can reach gateway
```

---

## 6. Terraform Equivalence Tests

### 6.1 Provider Initialization

#### TEST-TF-001: Provider Handshake
```
GIVEN: terraform-provider-infrasim binary
WHEN: Terraform initializes provider
THEN:
  - Provider outputs handshake to stdout
  - gRPC server starts on random port
  - Terraform receives proto version
```

#### TEST-TF-002: Provider Configuration
```
GIVEN: Provider block with daemon_address
WHEN: Provider configures
THEN:
  - Provider connects to daemon
  - Health check succeeds
  - Ready for resource operations
```

### 6.2 Resource CRUD Equivalence

#### TEST-TF-010: infrasim_vm Create
```
GIVEN: Terraform config:
  resource "infrasim_vm" "test" {
    name = "test-vm"
    cpu_cores = 2
    memory_mb = 2048
  }
WHEN: terraform apply
THEN:
  - Equivalent to: infrasim vm create --name test-vm --cpu 2 --memory 2048
  - VM appears in: infrasim vm list
  - State file contains VM ID
```

#### TEST-TF-011: infrasim_vm Update
```
GIVEN: Existing VM with cpu_cores=2
WHEN: Config changed to cpu_cores=4 and terraform apply
THEN:
  - VM is stopped if running
  - CPU count is updated
  - VM can be restarted with new config
```

#### TEST-TF-012: infrasim_vm Delete
```
GIVEN: Existing VM managed by Terraform
WHEN: terraform destroy
THEN:
  - Equivalent to: infrasim vm delete <id>
  - VM process is terminated
  - Resources are cleaned up
  - State file no longer contains VM
```

#### TEST-TF-013: infrasim_network Lifecycle
```
GIVEN: Network resource in Terraform config
WHEN: terraform apply / terraform destroy
THEN:
  - Create: Equivalent to CLI network create
  - Read: State matches daemon state
  - Delete: Network is removed
```

#### TEST-TF-014: infrasim_volume Lifecycle
```
GIVEN: Volume resource with source path
WHEN: terraform apply
THEN:
  - Volume is created with digest
  - Digest stored in state
  - Integrity verified on subsequent applies
```

#### TEST-TF-015: infrasim_snapshot Lifecycle
```
GIVEN: Snapshot resource referencing VM
WHEN: terraform apply
THEN:
  - Snapshot is created via daemon
  - Snapshot ID stored in state
  - Restore possible via data source
```

### 6.3 Dependency Ordering

#### TEST-TF-020: Volume Before VM
```
GIVEN: Config with volume and VM using volume
WHEN: terraform apply
THEN:
  - Volume is created first
  - VM references volume by ID
  - terraform graph shows correct dependency
```

#### TEST-TF-021: Network Before VM
```
GIVEN: Config with network and VM on network
WHEN: terraform apply
THEN:
  - Network is created first
  - VM is attached to network
  - Deletion reverses order
```

### 6.4 State Consistency

#### TEST-TF-030: Import Existing VM
```
GIVEN: VM created via CLI
WHEN: terraform import infrasim_vm.test <vm-id>
THEN:
  - State file populated with VM details
  - terraform plan shows no changes
```

#### TEST-TF-031: Drift Detection
```
GIVEN: Terraform-managed VM
WHEN: VM modified directly via CLI
THEN:
  - terraform plan shows drift
  - Refresh updates state to match reality
```

---

## 7. Kubernetes Equivalence Tests

### 7.1 Conceptual Mapping

| InfraSim Concept | Kubernetes Equivalent |
|------------------|----------------------|
| VM | Pod |
| Network | NetworkPolicy + Service |
| Volume | PersistentVolume |
| Snapshot | VolumeSnapshot |
| QoS Profile | ResourceQuota + LimitRange |
| Reconciler | Controller |
| Desired State | Spec |
| Actual State | Status |

### 7.2 Declarative State Tests

#### TEST-K8S-001: Declarative VM Spec
```
GIVEN: VM spec with desired state
WHEN: Spec is applied (create/update)
THEN:
  - Reconciler works toward desired state
  - Status reflects actual state
  - Status converges to spec
```

#### TEST-K8S-002: Reconciliation Loop
```
GIVEN: Running reconciler
WHEN: VM process crashes externally
THEN:
  - Status updated to reflect crash
  - (If restart policy set) VM is restarted
  - Event is logged
```

#### TEST-K8S-003: Label Selector
```
GIVEN: Multiple VMs with different labels
WHEN: ListVMs called with label_selector
THEN:
  - Only matching VMs returned
  - Matches Kubernetes label selector semantics
```

### 7.3 Resource Model Tests

#### TEST-K8S-010: ResourceMeta Structure
```
GIVEN: Any resource (VM, Network, Volume)
THEN: ResourceMeta contains:
  - id: Unique identifier
  - name: Human-readable name
  - labels: Key-value metadata
  - annotations: Extended metadata
  - created_at: Creation timestamp
  - updated_at: Last modification
  - generation: Incremented on spec change
```

#### TEST-K8S-011: Spec/Status Separation
```
GIVEN: VM resource
THEN:
  - Spec: Desired configuration (user-defined)
  - Status: Actual state (system-managed)
  - Users modify spec, system updates status
```

---

## 8. Software-Defined Device Tests

### 8.1 LoRaWAN Simulation

#### TEST-DEV-001: LoRa Device Registration
```
GIVEN: VM and LoRa device spec
WHEN: CreateLoRaDevice RPC is called
THEN:
  - Device is registered with VM
  - Region config applied (EU868/US915)
  - Device EUI assigned
```

#### TEST-DEV-002: LoRa Packet Transmission
```
GIVEN: Active LoRa device
WHEN: Packet is transmitted
THEN:
  - Time-on-air calculated correctly
  - Spreading factor affects duration
  - Path loss simulation applied
  - RSSI/SNR values realistic
```

#### TEST-DEV-003: LoRa Loss Simulation
```
GIVEN: LoRa device with loss_rate=0.1
WHEN: 1000 packets transmitted
THEN:
  - Approximately 10% are lost
  - Loss is random (not deterministic)
```

### 8.2 QoS Profile Tests

#### TEST-DEV-010: Latency Injection
```
GIVEN: QoS profile with latency_ms=50
WHEN: Packet traverses shaped interface
THEN:
  - Packet delayed by ~50ms
  - RTT shows +50ms compared to baseline
```

#### TEST-DEV-011: Jitter Simulation
```
GIVEN: QoS profile with jitter_ms=10
WHEN: Multiple packets traverse
THEN:
  - Delays vary by 0-10ms randomly
  - Standard deviation approximates expected jitter
```

#### TEST-DEV-012: Packet Loss
```
GIVEN: QoS profile with loss_percent=5.0
WHEN: 1000 packets traverse
THEN:
  - Approximately 50 packets dropped
  - Distribution is random
```

#### TEST-DEV-013: Bandwidth Limiting
```
GIVEN: QoS profile with rate_limit_mbps=10
WHEN: Sustained throughput test
THEN:
  - Throughput converges to 10 Mbps
  - Token bucket correctly shapes traffic
  - Burst allowed up to burst_size_kb
```

#### TEST-DEV-014: Packet Padding
```
GIVEN: QoS profile with packet_padding_bytes=64
WHEN: Packet is transmitted
THEN:
  - Wire size increased by 64 bytes
  - Padding is null bytes
```

---

## 9. Snapshot & Memory Integrity Tests

### 9.1 QCOW2 Snapshot Tests

#### TEST-SNAP-001: QCOW2 Snapshot Creation
```
GIVEN: Running VM with QCOW2 disk
WHEN: Internal snapshot requested
THEN:
  - QEMU creates snapshot via QMP
  - Snapshot appears in qemu-img info
  - VM continues running
```

#### TEST-SNAP-002: QCOW2 Snapshot Restore
```
GIVEN: VM with existing snapshot
WHEN: Restore requested
THEN:
  - QEMU loads snapshot via QMP
  - Disk state reverted
  - All changes since snapshot are lost
```

### 9.2 Memory Snapshot Tests

#### TEST-SNAP-010: Memory Dump
```
GIVEN: Running VM with 2GB RAM
WHEN: Memory snapshot requested
THEN:
  - VM is paused during dump
  - Memory file is ~2GB (possibly compressed)
  - VM is resumed after dump
```

#### TEST-SNAP-011: Memory Restore
```
GIVEN: Memory snapshot file
WHEN: Memory restore requested
THEN:
  - VM loads memory state
  - Execution continues from snapshot point
  - In-flight operations resume correctly
```

### 9.3 Encrypted Snapshot Tests

#### TEST-SNAP-020: Encrypted Memory Snapshot
```
GIVEN: VM and encryption key
WHEN: Encrypted snapshot created
THEN:
  - Memory is encrypted before storage
  - File extension indicates encryption
  - Raw file is not readable as memory
```

#### TEST-SNAP-021: Encrypted Snapshot Restore
```
GIVEN: Encrypted memory snapshot and correct key
WHEN: Restore requested
THEN:
  - Memory is decrypted
  - VM resumes correctly
```

#### TEST-SNAP-022: Wrong Key Rejection
```
GIVEN: Encrypted snapshot and wrong key
WHEN: Restore attempted
THEN:
  - Decryption fails or produces garbage
  - VM does not start with corrupted state
  - Error indicates key mismatch
```

---

## 10. QoS & Network Simulation Tests

### 10.1 Traffic Shaper Unit Tests

#### TEST-QOS-001: No Shaping Baseline
```
GIVEN: Default QoS profile (all zeros)
WHEN: Packet processed
THEN: ShapingDecision::Send (no delay)
```

#### TEST-QOS-002: Pure Latency
```
GIVEN: QoS with latency_ms=100, all else zero
WHEN: Packet processed
THEN: ShapingDecision::Delay(100ms)
```

#### TEST-QOS-003: 100% Packet Loss
```
GIVEN: QoS with loss_percent=100.0
WHEN: Packet processed
THEN: ShapingDecision::Drop (always)
```

### 10.2 Network Integration Tests

#### TEST-NET-001: User-Mode NAT
```
GIVEN: VM with user-mode network
WHEN: Guest pings external IP
THEN:
  - NAT translation occurs
  - Response received
  - Round-trip time reasonable
```

#### TEST-NET-002: Port Forwarding
```
GIVEN: User-mode network with hostfwd=tcp::2222-:22
WHEN: SSH to localhost:2222
THEN:
  - Connection forwarded to guest:22
  - SSH session established
```

---

## 11. Security Verification Tests

### 11.1 Cryptographic Security

#### TEST-SEC-001: Key Entropy
```
GIVEN: Multiple key generations
WHEN: Keys analyzed
THEN:
  - Keys are cryptographically random
  - No patterns detectable
  - Entropy source is OS CSPRNG
```

#### TEST-SEC-002: Signature Non-Repudiation
```
GIVEN: Signed attestation report
WHEN: Signature verified with public key
THEN:
  - Only holder of private key could sign
  - Signature binds to exact content
```

### 11.2 Isolation Tests

#### TEST-SEC-010: VM Process Isolation
```
GIVEN: Two running VMs
WHEN: VMs examined
THEN:
  - Separate QEMU processes
  - Separate memory spaces
  - No shared file descriptors
```

#### TEST-SEC-011: Network Isolation
```
GIVEN: VMs on different networks
WHEN: VM1 attempts to reach VM2
THEN:
  - Connection fails
  - No route between networks
```

### 11.3 Integrity Tests

#### TEST-SEC-020: CAS Immutability
```
GIVEN: Object stored in CAS
WHEN: Object retrieved
THEN:
  - Content matches original exactly
  - Any modification detected
```

#### TEST-SEC-021: Database Integrity
```
GIVEN: SQLite database with WAL
WHEN: Crash during write
THEN:
  - Database recovers to consistent state
  - No partial writes
```

---

## 12. Verification Checksheet

### 12.1 Pre-Verification Checklist

```markdown
□ Build artifacts available (binaries, tarballs)
□ SHA256 checksums present
□ manifest.json present
□ Test environment prepared (macOS ARM64)
□ QEMU installed (qemu-system-aarch64)
□ Test disk images available
```

### 12.2 Cryptographic Provenance Checksheet

| Check ID | Description | Method | Expected | Actual | Pass |
|----------|-------------|--------|----------|--------|------|
| CP-001 | Verify binary checksum | `shasum -a 256 -c *.sha256` | Match | | □ |
| CP-002 | Verify manifest structure | `jq . manifest.json` | Valid JSON | | □ |
| CP-003 | Generate test attestation | `infrasim attestation get <vm>` | Report generated | | □ |
| CP-004 | Verify attestation signature | See verification code | Signature valid | | □ |
| CP-005 | CAS put/get round-trip | Run CAS tests | Integrity preserved | | □ |
| CP-006 | Tamper detection | Modify CAS object, get | Error returned | | □ |

### 12.3 Functional Checksheet

| Check ID | Description | Command | Expected | Actual | Pass |
|----------|-------------|---------|----------|--------|------|
| FN-001 | Daemon starts | `infrasimd --foreground` | Listening on 50051 | | □ |
| FN-002 | Health check | `grpcurl ... GetHealth` | healthy=true | | □ |
| FN-003 | Create VM | `infrasim vm create ...` | VM created | | □ |
| FN-004 | List VMs | `infrasim vm list` | VM appears | | □ |
| FN-005 | Start VM | `infrasim vm start <id>` | State=RUNNING | | □ |
| FN-006 | VNC accessible | Connect to VNC port | Display shown | | □ |
| FN-007 | Stop VM | `infrasim vm stop <id>` | State=STOPPED | | □ |
| FN-008 | Delete VM | `infrasim vm delete <id>` | VM removed | | □ |
| FN-009 | Create volume | `infrasim volume create ...` | Digest computed | | □ |
| FN-010 | Create snapshot | `infrasim snapshot create ...` | Snapshot stored | | □ |

### 12.4 Terraform Checksheet

| Check ID | Description | Command | Expected | Actual | Pass |
|----------|-------------|---------|----------|--------|------|
| TF-001 | Provider init | `terraform init` | Provider installed | | □ |
| TF-002 | Plan | `terraform plan` | Resources shown | | □ |
| TF-003 | Apply | `terraform apply` | Resources created | | □ |
| TF-004 | State matches | Compare CLI output | Identical | | □ |
| TF-005 | Destroy | `terraform destroy` | Resources deleted | | □ |
| TF-006 | Import | `terraform import ...` | State populated | | □ |

### 12.5 Security Checksheet

| Check ID | Description | Method | Expected | Actual | Pass |
|----------|-------------|--------|----------|--------|------|
| SC-001 | Ed25519 key gen | Run crypto tests | Keys generated | | □ |
| SC-002 | Sign/verify | Run crypto tests | Signatures valid | | □ |
| SC-003 | Tamper reject | Run tamper test | Verification fails | | □ |
| SC-004 | VM isolation | Check processes | Separate PIDs | | □ |
| SC-005 | CAS integrity | Run integrity test | Errors on tamper | | □ |

---

## 13. Test Execution Guide

### 13.1 Running Unit Tests

```bash
# All unit tests
cargo test --all

# Specific crate
cargo test -p infrasim-common

# With output
cargo test -- --nocapture

# Specific test
cargo test test_sign_verify
```

### 13.2 Running Integration Tests

```bash
# Start daemon first
infrasimd --foreground &

# Run integration tests (when implemented)
cargo test --test integration

# Or use make
make integration-test
```

### 13.3 Manual E2E Verification

```bash
# 1. Build and install
./build.sh
sudo make install

# 2. Start daemon
infrasimd --config ~/.config/infrasim/config.toml &

# 3. Verify health
infrasim status

# 4. Create test resources
infrasim network create --name test-net --cidr 192.168.100.0/24
infrasim volume create --name boot --source /path/to/image.qcow2
infrasim vm create --name test-vm --cpu 2 --memory 2048 --boot-disk boot

# 5. Start VM
infrasim vm start test-vm

# 6. Verify attestation
infrasim attestation get test-vm

# 7. Cleanup
infrasim vm delete test-vm --force
infrasim volume delete boot
infrasim network delete test-net
```

### 13.4 Terraform E2E Verification

```bash
# 1. Prepare Terraform config
cd examples/terraform/

# 2. Initialize
terraform init

# 3. Validate
terraform validate

# 4. Plan
terraform plan

# 5. Apply
terraform apply -auto-approve

# 6. Verify
infrasim vm list
infrasim network list

# 7. Destroy
terraform destroy -auto-approve
```

### 13.5 Verification Script

```bash
#!/bin/bash
# verify-infrasim.sh - Automated verification script

set -e

echo "=== InfraSim Verification Suite ==="
echo

# Phase 1: Checksums
echo "Phase 1: Verifying checksums..."
for f in dist/*.sha256; do
    if shasum -a 256 -c "$f"; then
        echo "  ✅ $(basename "$f" .sha256)"
    else
        echo "  ❌ $(basename "$f" .sha256)"
        exit 1
    fi
done

# Phase 2: Unit tests
echo
echo "Phase 2: Running unit tests..."
cargo test --all 2>&1 | tail -5

# Phase 3: Crypto tests
echo
echo "Phase 3: Crypto verification..."
cargo test -p infrasim-common crypto -- --nocapture

# Phase 4: CAS tests
echo
echo "Phase 4: CAS integrity tests..."
cargo test -p infrasim-common cas -- --nocapture

echo
echo "=== Verification Complete ==="
```

---

## Appendix A: Test Data Requirements

### A.1 Test Images

| Image | Architecture | Size | Purpose |
|-------|--------------|------|---------|
| `debian-aarch64.qcow2` | ARM64 | ~2GB | General testing |
| `alpine-aarch64.qcow2` | ARM64 | ~100MB | Minimal testing |
| `kali-aarch64.qcow2` | ARM64 | ~8GB | Security lab testing |

### A.2 Test Keys

```
# Generate test keypair
infrasim key generate --output /tmp/test-key

# Public key (share)
cat /tmp/test-key.pub

# Use for attestation verification
infrasim attestation verify --public-key /tmp/test-key.pub
```

---

## Appendix B: Signature Verification Code

```rust
use infrasim_common::crypto::{verifying_key_from_bytes, Verifier};

fn verify_attestation(report: &AttestationReport, public_key_hex: &str) -> bool {
    // Decode public key
    let public_key_bytes = hex::decode(public_key_hex).expect("Invalid hex");
    let verifying_key = verifying_key_from_bytes(&public_key_bytes).expect("Invalid key");
    
    // Recompute digest
    let provenance_json = serde_json::to_vec(&report.host_provenance).unwrap();
    let mut hasher = sha2::Sha256::new();
    hasher.update(&provenance_json);
    let computed_digest = hex::encode(hasher.finalize());
    
    // Verify digest matches
    if computed_digest != report.digest {
        return false;
    }
    
    // Verify signature
    verifying_key.verify(report.digest.as_bytes(), &report.signature).is_ok()
}
```

---

## Appendix C: Future Test Requirements

### C.1 vTPM Attestation (When Implemented)
- TPM2_Quote generation
- PCR extension verification
- Remote attestation flow

### C.2 Confidential Computing (Linux Hosts Only)
- SEV-SNP attestation
- TDX attestation
- Memory encryption verification

### C.3 Full LoRaWAN Tests
- End-to-end device simulation
- Gateway integration
- Network server protocol

---

## Document Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2024-12-15 | GitHub Copilot | Initial RFC |

---

**END OF DOCUMENT**
