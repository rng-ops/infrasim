# Infrasim: Technical Documentation for APART AI Security Fellowship

**Version:** 0.1.0  
**Date:** December 2024  
**Status:** Experimental / Research Infrastructure

---

## 1. Executive Summary

### What Infrasim Actually Is

Infrasim is a **QEMU orchestration platform** for macOS Apple Silicon that provides:

1. **Terraform-style IaC** for virtual machine lifecycle management
2. **Hardware-accelerated VMs** via HVF (Hypervisor.framework) for near-native ARM64 performance
3. **Host-level attestation** with Ed25519-signed provenance reports
4. **Content-addressed storage** for disk images with SHA256-based integrity verification
5. **QoS simulation** for network conditions (latency, jitter, packet loss, bandwidth)

### What Problem It Solves for AI Security Research

Infrasim addresses a critical gap in AI security research infrastructure: **reproducible, attestable execution environments**. When running AI model evaluations, red-team exercises, or containment tests, researchers need to:

- **Reproduce environments exactly** — Infrasim hashes disk images and signs execution provenance
- **Isolate experiments** — Full VM isolation with HVF, not container namespaces
- **Simulate real-world conditions** — QoS profiles for testing adversarial network scenarios
- **Capture and restore state** — Memory and disk snapshots for replay analysis
- **Declare infrastructure as code** — Terraform provider for version-controlled experiment configurations

### Why APART Fellows Should Care

1. **Shared Research Infrastructure**: Multiple thesis projects can run on a common VM orchestration layer without stepping on each other's environments
2. **Evidence Generation**: Attestation reports provide cryptographic evidence of experiment conditions for peer review
3. **Rapid Iteration**: Terraform + snapshots enable quick experiment reset and variant testing
4. **Cross-Domain Applicability**: Supports containment testing, eval harnessing, network simulation, and more

**Honest Assessment**: Infrasim is *research infrastructure*, not production-hardened software. Security guarantees are aspirational in several areas (see Section 7). The attestation system provides *host provenance*, not hardware-rooted trust.

---

## 2. System Overview

### Core Components

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          User Interfaces                                 │
├────────────────┬────────────────┬───────────────┬───────────────────────┤
│   Terraform    │      CLI       │  Web Console  │      gRPC Client      │
│   Provider     │   (infrasim)   │   (noVNC)     │      (direct)         │
└───────┬────────┴───────┬────────┴───────┬───────┴──────────┬────────────┘
        │ gRPC           │ gRPC           │ HTTP/WS          │ gRPC
        │                │                │                  │
┌───────▼────────────────▼────────────────▼──────────────────▼────────────┐
│                        infrasimd (Daemon)                                │
├──────────────┬──────────────┬──────────────┬──────────────┬─────────────┤
│    gRPC      │    State     │  Reconciler  │   Attestation│    QMP      │
│   Server     │   Manager    │    Loop      │   Provider   │   Client    │
├──────────────┴──────────────┴──────────────┴──────────────┴─────────────┤
│                      SQLite + Content-Addressed Store                    │
├─────────────────────────────────────────────────────────────────────────┤
│                        QEMU Process Manager                              │
└───────────────────────────────────┬─────────────────────────────────────┘
                                    │
              ┌─────────────────────┼─────────────────────┐
              │                     │                     │
       ┌──────▼──────┐       ┌──────▼──────┐       ┌──────▼──────┐
       │   QEMU VM   │       │   QEMU VM   │       │   QEMU VM   │
       │  (aarch64)  │       │  (aarch64)  │       │  (aarch64)  │
       └──────┬──────┘       └──────┬──────┘       └──────┬──────┘
              │                     │                     │
              └─────────────────────┼─────────────────────┘
                                    │
                           ┌────────▼────────┐
                           │  HVF (macOS)    │
                           │  Hardware Accel │
                           └─────────────────┘
```

### Component Responsibilities

| Component | Crate | Responsibility |
|-----------|-------|----------------|
| **infrasimd** | `daemon` | Central daemon: gRPC API, state reconciliation, QEMU lifecycle |
| **infrasim** | `cli` | User-facing CLI for VM/network/snapshot management |
| **terraform-provider-infrasim** | `provider` | Terraform Plugin Protocol v6 implementation |
| **infrasim-web** | `web` | Web console: noVNC proxy, REST API, optional MDM profiles |
| **infrasim-common** | `common` | Shared: types, crypto, CAS, QMP client, attestation, traffic shaping |

### Execution Flow: VM Creation

```
1. Terraform apply (or CLI command)
         │
2. gRPC CreateVM → infrasimd
         │
3. StateManager: persist to SQLite
         │
4. Reconciler detects pending VM (every 5s)
         │
5. VolumePreparer: verify/prepare disk images
         │
6. QemuLauncher.build_args(): construct command line
         │
7. spawn qemu-system-aarch64 -accel hvf ...
         │
8. wait_for_qmp(): connect to QMP socket
         │
9. Update status → Running
         │
10. Generate attestation report (optional)
```

### State Management

- **Persistence**: SQLite with WAL mode (`~/.infrasim/state.db`)
- **Runtime State**: In-memory HashMap for running VM processes
- **Reconciliation**: 5-second loop comparing desired vs actual state
- **Key Storage**: Ed25519 signing key at `~/.infrasim/keys/signing.key`

---

## 3. Threat Model & Security Posture

### What Infrasim Is Designed To Mitigate

Based on implementation analysis:

1. **Experiment Tampering**
   - Disk images are hashed (SHA256) before VM launch
   - Attestation reports are signed (Ed25519) and timestamped
   - Volume integrity verification supports signed manifests

2. **Environment Reproducibility Disputes**
   - Host provenance captures: QEMU version, args, macOS version, CPU model, HVF status
   - Content-addressed storage deduplicates and verifies images

3. **Guest Escape Containment** (partial)
   - Full VM isolation via QEMU + HVF (stronger than containers)
   - No virtio-fs or shared memory by default
   - Network isolation via QEMU user-mode networking

4. **Unauthorized VM Access**
   - VNC bound to localhost by default
   - Optional token-based console auth
   - gRPC API on localhost only

### What Threats Are Explicitly Out of Scope

1. **Hardware-Rooted Attestation**
   - No TPM, SEV-SNP, or TDX integration (stubs exist, not implemented)
   - Attestation is *host provenance*, not cryptographic proof of execution
   - A compromised macOS host can forge attestation reports

2. **Multi-Tenant Security**
   - No resource quotas or isolation between VMs beyond what QEMU provides
   - Single-user design; daemon runs with user privileges

3. **Network Exfiltration**
   - QoS profiles affect traffic shaping, not content filtering
   - No firewall rules or IDS integration

4. **Malicious Disk Images**
   - Integrity verification checks hashes, not content safety
   - A correctly-hashed malicious image will pass verification

5. **Supply Chain Attacks**
   - QEMU binary integrity not verified
   - Rust dependencies not audited for this release

### Trust Assumptions

| Entity | Trust Level | Justification |
|--------|-------------|---------------|
| Host macOS | **Fully Trusted** | Daemon runs as user, HVF under macOS control |
| QEMU Binary | **Trusted** | Assumed correct; path configurable |
| Disk Images | **Verified** | SHA256 checked; content untrusted |
| Guest OS | **Untrusted** | Treated as potentially adversarial |
| Network | **Untrusted** | User-mode NAT; no host exposure by default |
| Researcher | **Trusted** | Full API access; can create/destroy VMs |

---

## 4. Provenance, Reproducibility, and Evidence

### Artifacts Produced

1. **Attestation Reports** (per VM)
   ```json
   {
     "id": "uuid",
     "vm_id": "uuid",
     "host_provenance": {
       "qemu_version": "8.2.0",
       "qemu_args": ["qemu-system-aarch64", "-accel", "hvf", ...],
       "base_image_hash": "sha256:abc123...",
       "volume_hashes": {"vol-1": "sha256:..."},
       "macos_version": "14.2",
       "cpu_model": "Apple M2 Pro",
       "hvf_enabled": true,
       "hostname": "researcher-mbp.local",
       "timestamp": 1702900000
     },
     "digest": "sha256:...",
     "signature": "base64:...",
     "attestation_type": "host_provenance"
   }
   ```

2. **Content-Addressed Objects**
   - Stored at `~/.infrasim/cas/objects/sha256/<prefix>/<digest>`
   - Automatic deduplication
   - Integrity verified on read

3. **Snapshots**
   - Memory dumps (via QMP `dump-guest-memory`)
   - Disk snapshots (qcow2 internal snapshots)
   - Stored per-run in `~/.infrasim/cas/runs/<run-id>/`

4. **Run Manifests** (struct defined, not fully integrated)
   ```rust
   struct RunManifest {
       vm_config_digest: String,
       image_digests: HashMap<String, String>,
       volume_digests: HashMap<String, String>,
       benchmark_suite_digest: Option<String>,
       attestation_digest: Option<String>,
       timestamp: i64,
   }
   ```

### What Is Logged/Hashed/Signed

| Data | Hashed | Signed | Logged | Notes |
|------|--------|--------|--------|-------|
| Disk images | ✅ SHA256 | ❌ | ✅ | Via CAS |
| QEMU args | ❌ | ✅ (in report) | ✅ | Part of attestation |
| Host info | ❌ | ✅ (in report) | ✅ | macOS, QEMU, HVF |
| Attestation report | ✅ SHA256 | ✅ Ed25519 | ✅ | Full chain |
| Memory snapshots | ✅ | ❌ | ✅ | Optional |
| Benchmark results | ✅ | ✅ | ✅ | Scaffold only |

### Realistic Integrity Claims

**What you CAN claim:**
- "This disk image had hash X at VM launch time"
- "This attestation report was signed by key Y at time T"
- "The QEMU command line included arguments Z"

**What you CANNOT claim:**
- "The VM executed exactly as described" (no hardware attestation)
- "No modifications occurred during execution" (host is trusted)
- "Results are bit-for-bit reproducible" (HVF timing non-deterministic)

### Provenance Gaps

- Memory encryption not implemented (scaffold for SEV-SNP/TDX exists)
- No chain-of-custody for the signing key
- Benchmark runs not fully integrated with attestation
- No reproducible builds for Infrasim itself

---

## 5. Adapter / Integration Model

### How Researchers Integrate Their Projects

#### Option 1: Terraform (Recommended)

Create `main.tf`:
```hcl
terraform {
  required_providers {
    infrasim = {
      source  = "local/infrasim/infrasim"
      version = "0.1.0"
    }
  }
}

provider "infrasim" {
  daemon_address = "http://127.0.0.1:9090"
}

resource "infrasim_volume" "model_weights" {
  name   = "llama-weights"
  source = "/path/to/llama.qcow2"
  
  integrity {
    scheme          = "sha256"
    expected_digest = "sha256:abc123..."
  }
}

resource "infrasim_vm" "eval_harness" {
  name         = "eval-runner"
  cpus         = 8
  memory       = 16384
  boot_disk_id = infrasim_volume.model_weights.id
  
  # Simulate high-latency network
  qos_profile_id = infrasim_qos_profile.satellite.id
}

resource "infrasim_qos_profile" "satellite" {
  name              = "satellite-link"
  latency_ms        = 600
  jitter_ms         = 50
  loss_percent      = 2.0
  rate_limit_mbps   = 10
}
```

#### Option 2: gRPC Direct

```rust
// Connect to daemon
let channel = tonic::transport::Channel::from_static("http://127.0.0.1:9090")
    .connect().await?;
let mut client = InfraSimDaemonClient::new(channel);

// Create VM
let response = client.create_vm(CreateVmRequest {
    name: "my-experiment".to_string(),
    spec: Some(VmSpec {
        cpu_cores: 4,
        memory_mb: 8192,
        boot_disk_id: "vol-123".to_string(),
        ..Default::default()
    }),
    labels: HashMap::new(),
}).await?;

// Get attestation
let attestation = client.get_attestation(GetAttestationRequest {
    vm_id: response.vm.unwrap().meta.unwrap().id,
}).await?;
```

#### Option 3: CLI Scripting

```bash
#!/bin/bash
# experiment-runner.sh

# Create volume with integrity check
infrasim volume create eval-disk \
  --source ./eval-image.qcow2 \
  --integrity sha256 \
  --expected-digest sha256:abc123

# Create and start VM
VM_ID=$(infrasim vm create eval-vm \
  --cpus 4 \
  --memory 8192 \
  --boot-disk eval-disk \
  --output json | jq -r '.meta.id')

infrasim vm start $VM_ID

# Wait for experiment to complete...

# Capture attestation
infrasim attestation get $VM_ID --output attestation.json

# Snapshot for later analysis
infrasim snapshot create post-eval --vm $VM_ID --include-memory
```

### Extension Points

| Extension Type | Location | Interface |
|----------------|----------|-----------|
| Custom VM images | `images/` directory | Shell scripts + Dockerfile |
| QoS profiles | Terraform or gRPC | `QosProfileSpec` struct |
| Benchmark suites | `benchmark_runs` table | gRPC (scaffold) |
| Volume providers | `VolumePreparer` | Rust trait (OCI stub exists) |
| Attestation types | `AttestationProvider` | Extend `generate_report()` |

### Constraints Adapters Must Respect

1. **ARM64 only**: Guest VMs must be aarch64 (HVF limitation)
2. **qcow2 format**: Disk images should be qcow2 for snapshot support
3. **No GPU passthrough**: HVF doesn't support Metal/GPU yet
4. **Localhost networking**: Default is user-mode NAT; no L2 bridging without vmnet
5. **Single-host**: No distributed/clustered mode

---

## 6. Example Research Use Cases

### 6.1 Containment Testing

**Scenario**: Test whether an AI agent can escape its sandbox or exfiltrate data.

**Infrasim Support**:
- Full VM isolation (not container namespaces)
- Network limited to user-mode NAT with port forwarding
- Snapshot before/after for forensic comparison
- QoS profiles can simulate air-gapped conditions (100% packet loss)

**Key Code**: `crates/daemon/src/qemu.rs` → `build_args()` for network config

### 6.2 Eval Harnessing

**Scenario**: Run standardized AI capability evaluations with reproducible environments.

**Infrasim Support**:
- Terraform defines exact VM config (version controlled)
- Volume integrity ensures correct model weights
- Attestation reports document execution environment
- Snapshots enable reset between eval runs

**Key Code**: `crates/common/src/attestation.rs` → `generate_report()`

### 6.3 Red-Team Replay

**Scenario**: Replay adversarial prompts against a model in a known-good state.

**Infrasim Support**:
- Create snapshot at "clean" state
- Inject adversarial inputs
- Restore snapshot and repeat with variants
- Compare memory dumps between runs

**Key Code**: `crates/daemon/src/qemu.rs` → `create_memory_snapshot()`, `restore_internal_snapshot()`

### 6.4 Network Adversarial Conditions

**Scenario**: Test model behavior under degraded network (slow, lossy, jittery).

**Infrasim Support**:
- QoS profiles define latency/jitter/loss/bandwidth
- Traffic shaper applies rules to VM traffic
- LoRaWAN simulator for IoT scenarios (scaffold)

**Key Code**: `crates/common/src/traffic_shaper.rs` → `TrafficShaper`, `ShapingDecision`

### 6.5 Inference Leakage Detection

**Scenario**: Detect side-channel leakage during model inference.

**Infrasim Support**:
- Memory snapshots can capture inference-time state
- QEMU provides some timing visibility
- Attestation timestamps correlate with external observations

**Limitation**: No hardware-level side-channel monitoring; HVF abstracts too much.

### 6.6 Multi-Model Interaction Testing

**Scenario**: Run multiple AI systems that communicate with each other.

**Infrasim Support**:
- Multiple VMs on same virtual network
- Port forwarding for inter-VM communication
- Independent snapshots per VM

**Key Code**: `crates/daemon/src/state.rs` → network management

### 6.7 Governance/Policy Experiments

**Scenario**: Test infrastructure-level governance controls.

**Infrasim Support**:
- MDM profile generation for device management testing
- Auth system with roles (admin, user)
- Appliance catalog for approved images
- Mesh networking with identity provisioning (experimental)

**Key Code**: `crates/web/src/mdm.rs`, `crates/web/src/meshnet/`

### 6.8 Long-Running Experiment Management

**Scenario**: Manage experiments that run for days/weeks.

**Infrasim Support**:
- Reconciler auto-restarts crashed VMs
- Persistent state in SQLite
- Console access via web UI
- Snapshot at checkpoints

**Key Code**: `crates/daemon/src/reconciler.rs` → `Reconciler::run()`

---

## 7. Limitations and Open Gaps

### Critical Limitations

| Limitation | Impact | Mitigation Path |
|------------|--------|-----------------|
| **No hardware attestation** | Cannot prove execution to third parties | Integrate vTPM/SEV-SNP on Linux servers |
| **macOS-only** | Limits deployment options | Linux port with KVM feasible |
| **Single-user design** | No multi-tenancy | Add namespace/quota support |
| **No GPU passthrough** | Cannot run GPU-accelerated inference | Wait for Apple virtualization improvements |
| **Benchmark system is scaffold** | Cannot run structured evals | Implement `CreateBenchmarkRun` |

### Technical Debt

1. **OCI registry pull not implemented** — volumes must be local files
2. **HTTP download not implemented** — same as above
3. **vTPM is scaffold only** — requires swtpm and Linux
4. **LoRa device management is scaffold** — not connected to actual simulation
5. **Memory encryption stubs** — SEV-SNP/TDX placeholders, not functional
6. **Web auth is TOTP only** — WebAuthn passkeys in meshnet only
7. **No rate limiting on gRPC API** — DoS possible locally

### Ambiguities

- **Reproducibility**: Not guaranteed due to HVF timing jitter
- **Attestation scope**: Covers host provenance, not in-guest execution
- **Snapshot consistency**: QEMU internal snapshots may have caveats
- **Key management**: Signing key generated once, no rotation

### Areas Requiring Future Work

1. **Formal threat model documentation**
2. **Reproducible build pipeline for Infrasim itself**
3. **Integration tests covering security properties**
4. **Hardware attestation on Linux (SEV-SNP, TDX)**
5. **Distributed mode for multi-host experiments**
6. **Audit logging for compliance**

---

## 8. Suggested Reading Order for Reviewers

### Quick Start (30 minutes)

1. **This document** — overview and context
2. **`README.md`** — installation and basic usage
3. **`docs/architecture.md`** — component diagram and flow

### Core Implementation (2-3 hours)

4. **`crates/common/src/types.rs`** — all data structures
5. **`crates/common/src/attestation.rs`** — provenance collection
6. **`crates/daemon/src/qemu.rs`** — QEMU launching logic
7. **`crates/daemon/src/reconciler.rs`** — state reconciliation
8. **`crates/daemon/src/grpc.rs`** — API implementation

### Security-Relevant Logic

9. **`crates/common/src/crypto.rs`** — Ed25519 signing
10. **`crates/common/src/cas.rs`** — content-addressed storage
11. **`crates/common/src/traffic_shaper.rs`** — QoS simulation
12. **`proto/infrasim.proto`** — full API schema

### Example Commands

```bash
# Build everything
cargo build --release

# Run daemon in foreground
RUST_LOG=debug ./target/release/infrasimd --foreground

# List VMs
./target/release/infrasim vm list

# Get attestation for a running VM
./target/release/infrasim attestation get <vm-id>

# Run unit tests
cargo test

# Run with Terraform
cd examples/terraform && terraform apply
```

### Key Directories

| Path | Contents |
|------|----------|
| `crates/daemon/src/` | Core daemon logic |
| `crates/common/src/` | Shared utilities, crypto, types |
| `crates/web/src/` | Web UI and REST API |
| `crates/provider/src/` | Terraform provider |
| `proto/` | gRPC/protobuf definitions |
| `docs/` | Additional documentation |
| `examples/terraform/` | Example Terraform configs |

---

## Appendix A: Attestation Report Schema

```rust
pub struct AttestationReport {
    pub id: String,                    // UUID
    pub vm_id: String,                 // Target VM
    pub host_provenance: HostProvenance,
    pub digest: String,                // SHA256 of provenance JSON
    pub signature: Vec<u8>,            // Ed25519 signature
    pub created_at: i64,               // Unix timestamp
    pub attestation_type: String,      // "host_provenance"
}

pub struct HostProvenance {
    pub qemu_version: String,
    pub qemu_args: Vec<String>,
    pub base_image_hash: String,
    pub volume_hashes: HashMap<String, String>,
    pub macos_version: String,
    pub cpu_model: String,
    pub hvf_enabled: bool,
    pub hostname: String,
    pub timestamp: i64,
}
```

## Appendix B: QoS Profile Parameters

```rust
pub struct QosProfileSpec {
    pub latency_ms: u32,           // Added delay
    pub jitter_ms: u32,            // Random variation
    pub loss_percent: f32,         // Packet drop rate
    pub rate_limit_mbps: u32,      // Bandwidth cap
    pub packet_padding_bytes: u32, // Traffic analysis resistance
    pub burst_shaping: bool,       // Token bucket bursting
    pub burst_size_kb: u32,        // Burst allowance
}
```

## Appendix C: Resource Types

| Resource | Terraform Type | gRPC Service |
|----------|----------------|--------------|
| Virtual Machine | `infrasim_vm` | `CreateVM`, `StartVM`, `StopVM` |
| Network | `infrasim_network` | `CreateNetwork`, `DeleteNetwork` |
| Volume | `infrasim_volume` | `CreateVolume`, `DeleteVolume` |
| QoS Profile | `infrasim_qos_profile` | `CreateQoSProfile` |
| Snapshot | `infrasim_snapshot` | `CreateSnapshot`, `RestoreSnapshot` |
| Console | `infrasim_console` | `CreateConsole`, `GetConsole` |

---

*Document prepared for APART AI Security Fellowship application review.*
