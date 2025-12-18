# InfraSim Architecture

This document describes the architecture of InfraSim, a Terraform-compatible
QEMU platform for macOS Apple Silicon.

## Overview

InfraSim follows a daemon-based architecture where a background service
manages QEMU virtual machines, and clients interact via gRPC.

```
┌────────────────────────────────────────────────────────────────────┐
│                           User Interface                            │
├──────────────┬──────────────┬──────────────┬───────────────────────┤
│   Terraform  │     CLI      │  Web Console │     Direct gRPC       │
│   Provider   │  (infrasim)  │   (noVNC)    │       Client          │
└──────┬───────┴──────┬───────┴──────┬───────┴───────────┬───────────┘
       │              │              │                   │
       │   gRPC       │   gRPC       │   HTTP/WS         │   gRPC
       │              │              │                   │
┌──────▼──────────────▼──────────────▼───────────────────▼───────────┐
│                        InfraSim Daemon (infrasimd)                  │
├─────────────┬─────────────┬─────────────┬─────────────┬────────────┤
│    gRPC     │   State     │ Reconciler  │    Web      │   QMP      │
│   Server    │  Manager    │    Loop     │   Server    │  Client    │
├─────────────┴─────────────┴─────────────┴─────────────┴────────────┤
│                         QEMU Process Manager                        │
└──────────────────────────────┬─────────────────────────────────────┘
                               │
        ┌──────────────────────┼──────────────────────┐
        │                      │                      │
┌───────▼───────┐      ┌───────▼───────┐      ┌───────▼───────┐
│   QEMU VM 1   │      │   QEMU VM 2   │      │   QEMU VM N   │
│   (aarch64)   │      │   (aarch64)   │      │   (aarch64)   │
└───────────────┘      └───────────────┘      └───────────────┘
        │                      │                      │
        └──────────────────────┼──────────────────────┘
                               │
                        ┌──────▼──────┐
                        │     HVF     │
                        │  (macOS)    │
                        └─────────────┘
```

## Components

### 1. Daemon (infrasimd)

The central process that:
- Listens for gRPC requests from clients
- Manages QEMU process lifecycle
- Handles state persistence
- Provides web console access
- Runs the reconciliation loop

**Key modules:**
- `grpc.rs` - gRPC service implementation
- `state.rs` - VM state tracking
- `qemu.rs` - QEMU process launcher
- `reconciler.rs` - Desired vs actual state reconciliation

### 2. Terraform Provider

Implements the Terraform Plugin Protocol v6:
- Translates Terraform resources to daemon API calls
- Handles state encoding/decoding
- Manages resource lifecycle (CRUD)

**Key modules:**
- `provider.rs` - Main provider implementation
- `resources/` - Resource implementations (vm, network, volume)
- `schema.rs` - Terraform schema definitions
- `state.rs` - State serialization

### 3. CLI (infrasim)

User-friendly command-line interface:
- Wraps gRPC calls in friendly commands
- Provides formatted output (table, JSON, YAML)
- Supports interactive features

**Commands:**
```
infrasim vm list|create|start|stop|delete
infrasim network list|create|delete
infrasim volume list|create|delete
infrasim console <vm-id>
infrasim snapshot create|restore|delete
infrasim attestation get|verify|export
```

### 4. Web Console

Browser-based VM access:
- Serves noVNC JavaScript client
- Proxies WebSocket to VNC connections
- Provides dashboard UI

**Flow:**
```
Browser → WebSocket → infrasim-web → TCP → QEMU VNC
```

### 5. Common Library

Shared functionality:
- Type definitions
- Cryptographic operations (Ed25519 signing)
- Content-addressed storage (SHA256)
- SQLite state persistence
- QMP (QEMU Machine Protocol) client
- Attestation collection

## Data Flow

### VM Creation

```
1. Terraform apply
   │
2. Provider: CreateVM RPC
   │
3. Daemon: Validate config
   │
4. Daemon: Store desired state
   │
5. Reconciler: Detect new VM
   │
6. QEMU Launcher: Build command line
   │
7. spawn qemu-system-aarch64
   │
8. QMP: Wait for VM ready
   │
9. Update state: Running
   │
10. Return to Terraform
```

### State Reconciliation

The reconciler runs continuously:

```
loop every 5 seconds:
    for each desired_vm in state:
        actual = check_qemu_process()
        if desired.state != actual.state:
            reconcile(desired, actual)
```

This ensures:
- Crashed VMs are restarted (if configured)
- Deleted VMs have processes cleaned up
- State stays synchronized with reality

## Storage Layout

```
/var/lib/infrasim/
├── config.toml           # Daemon configuration
├── state.db              # SQLite state database
├── keys/
│   ├── signing.key       # Ed25519 private key
│   └── signing.pub       # Ed25519 public key
├── images/
│   ├── kali-xfce-aarch64.qcow2
│   └── debian-arm64.qcow2
├── volumes/
│   └── <vm-id>/
│       ├── disk.qcow2
│       └── cloud-init.iso
└── snapshots/
    └── <snapshot-id>/
        ├── disk.qcow2
        └── memory.bin
```

## QEMU Configuration

InfraSim launches QEMU with optimal settings for Apple Silicon:

```bash
qemu-system-aarch64 \
  -name <vm-name> \
  -machine virt,highmem=on \
  -accel hvf \
  -cpu host \
  -smp <cpus> \
  -m <memory> \
  -drive file=<disk>,format=qcow2,if=virtio \
  -device virtio-net-pci,netdev=net0 \
  -netdev user,id=net0,hostfwd=... \
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -vnc 127.0.0.1:<port> \
  -qmp unix:/tmp/qmp-<id>.sock,server,wait=off \
  -monitor none \
  -serial none
```

Key options:
- `-accel hvf` - Hardware acceleration
- `-cpu host` - Native CPU passthrough
- `-machine virt,highmem=on` - ARM virt platform with large memory
- `-drive if=virtio` - High-performance virtio disk
- `-device virtio-net-pci` - High-performance network

## Security Model

### Attestation

Every VM operation can generate a signed attestation:

```json
{
  "vm_id": "abc123",
  "timestamp": "2024-01-15T10:30:00Z",
  "host_info": {
    "hostname": "macbook-pro.local",
    "os_version": "macOS 14.2",
    "kernel_version": "23.2.0",
    "qemu_version": "8.2.0",
    "hvf_enabled": true
  },
  "disk_info": {
    "sha256": "abc123...",
    "size_bytes": 34359738368,
    "path": "/var/lib/infrasim/images/kali.qcow2"
  },
  "signature": "base64-ed25519-signature"
}
```

### Content-Addressed Storage

Disk images are tracked by SHA256 hash:
- Enables deduplication
- Verifies image integrity
- Supports reproducible deployments

## Performance Considerations

### HVF Acceleration

Hypervisor.framework provides:
- Native instruction execution
- Hardware virtualization extensions
- Near-bare-metal performance

### I/O Performance

- virtio-blk for disk: 10-100x faster than emulated IDE
- virtio-net for network: Near-native throughput
- Shared memory for inter-VM communication (planned)

### Memory

- Large pages for reduced TLB misses
- Balloon driver for dynamic memory (planned)
- Memory deduplication via macOS (automatic)

## Future Enhancements

- [ ] Live migration between hosts
- [ ] GPU passthrough (Metal support)
- [ ] Container integration (podman/docker)
- [ ] Cluster mode with distributed state
- [ ] ARM32 guest support
- [ ] Integration with Apple Virtualization.framework
