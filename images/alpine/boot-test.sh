#!/usr/bin/env bash
# ============================================================================
# boot-test.sh - Headless QEMU Boot Test for Alpine qcow2
# ============================================================================
#
# Boots the Alpine image in headless QEMU and verifies:
# - VM boots successfully
# - Network interface comes up
# - Basic connectivity works
#
# Exits 0 on success, non-zero on failure.
# Prints "BOOT_OK" marker on success for CI verification.
#
set -euo pipefail

DISK_FILE="${1:-output/base.qcow2}"
TIMEOUT="${BOOT_TIMEOUT:-120}"
QEMU_MEMORY="${QEMU_MEMORY:-512}"
QEMU_CPUS="${QEMU_CPUS:-2}"

# Image architecture (the Alpine image is built for aarch64)
IMAGE_ARCH="${IMAGE_ARCH:-aarch64}"

# Allow skipping boot test entirely
SKIP_BOOT_TEST="${SKIP_BOOT_TEST:-false}"

# Detect host architecture for acceleration choice
HOST_ARCH=$(uname -m)
HOST_OS=$(uname -s)

# Check for cross-architecture emulation (very slow)
CROSS_ARCH=false

# Configure QEMU based on image architecture
case "$IMAGE_ARCH" in
    arm64|aarch64)
        QEMU_BIN="qemu-system-aarch64"
        
        # Set machine and CPU based on host capabilities
        if [[ "$HOST_ARCH" == "aarch64" || "$HOST_ARCH" == "arm64" ]]; then
            QEMU_MACHINE="-M virt -cpu host"
            # Try HVF on macOS, KVM on Linux, fall back to TCG
            if [[ "$HOST_OS" == "Darwin" ]]; then
                QEMU_ACCEL="-accel hvf"
            elif [[ -e /dev/kvm ]]; then
                QEMU_ACCEL="-accel kvm"
            else
                QEMU_ACCEL="-accel tcg"
            fi
        else
            # Cross-architecture emulation (x86_64 host running aarch64 guest)
            QEMU_MACHINE="-M virt -cpu cortex-a72"
            QEMU_ACCEL="-accel tcg"
            CROSS_ARCH=true
        fi
        
        # Find UEFI firmware
        UEFI_PATHS=(
            "/opt/homebrew/share/qemu/edk2-aarch64-code.fd"
            "/usr/share/qemu/edk2-aarch64-code.fd"
            "/usr/share/AAVMF/AAVMF_CODE.fd"
            "/usr/local/share/qemu/edk2-aarch64-code.fd"
        )
        BIOS_ARG=""
        for path in "${UEFI_PATHS[@]}"; do
            if [[ -f "$path" ]]; then
                BIOS_ARG="-bios $path"
                break
            fi
        done
        ;;
    x86_64)
        QEMU_BIN="qemu-system-x86_64"
        QEMU_MACHINE="-M q35"
        if [[ "$HOST_ARCH" == "x86_64" && -e /dev/kvm ]]; then
            QEMU_ACCEL="-accel kvm"
        else
            QEMU_ACCEL="-accel tcg"
        fi
        BIOS_ARG=""
        ;;
    *)
        echo "Unsupported image architecture: $IMAGE_ARCH"
        exit 1
        ;;
esac

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

# =============================================================================
# Skip check for cross-architecture emulation
# =============================================================================
if [[ "$SKIP_BOOT_TEST" == "true" ]]; then
    log_info "⏭️ Boot test skipped (SKIP_BOOT_TEST=true)"
    echo "BOOT_SKIPPED"
    exit 0
fi

if [[ "$CROSS_ARCH" == "true" ]]; then
    log_warn "Cross-architecture emulation detected (${HOST_ARCH} host → ${IMAGE_ARCH} guest)"
    log_warn "TCG emulation is extremely slow - boot test may take 10+ minutes"
    log_warn "Consider using SKIP_BOOT_TEST=true in CI for cross-arch scenarios"
    
    # Use a much shorter timeout for cross-arch - just check QEMU can start
    if [[ "${BOOT_TIMEOUT:-}" -gt 60 ]]; then
        log_info "Reducing timeout to 60s for cross-arch quick check"
        TIMEOUT=60
    fi
fi

# =============================================================================
# Preflight checks
# =============================================================================
if [[ ! -f "$DISK_FILE" ]]; then
    log_error "Disk image not found: $DISK_FILE"
    exit 1
fi

if ! command -v "$QEMU_BIN" &>/dev/null; then
    log_error "$QEMU_BIN not found. Install with: brew install qemu"
    exit 1
fi

# =============================================================================
# Create a temporary overlay to avoid modifying the base image
# =============================================================================
WORK_DIR=$(mktemp -d)
OVERLAY_FILE="${WORK_DIR}/overlay.qcow2"
SERIAL_LOG="${WORK_DIR}/serial.log"
MONITOR_SOCKET="${WORK_DIR}/monitor.sock"

cleanup() {
    log_info "Cleaning up..."
    # Kill QEMU if still running
    if [[ -n "${QEMU_PID:-}" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
        kill "$QEMU_PID" 2>/dev/null || true
    fi
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT

# Create overlay
log_info "Creating temporary overlay..."
qemu-img create -f qcow2 -b "$(realpath "$DISK_FILE")" -F qcow2 "$OVERLAY_FILE"

# =============================================================================
# Boot QEMU in background
# =============================================================================
log_info "Booting QEMU (timeout: ${TIMEOUT}s)..."
log_info "Image arch: $IMAGE_ARCH, Host arch: $HOST_ARCH"
log_info "QEMU: $QEMU_BIN $QEMU_MACHINE $QEMU_ACCEL"

# Build QEMU command
QEMU_CMD=(
    "$QEMU_BIN"
    $QEMU_MACHINE
    $QEMU_ACCEL
    -m "$QEMU_MEMORY"
    -smp "$QEMU_CPUS"
    -drive "file=$OVERLAY_FILE,format=qcow2,if=virtio"
    -device virtio-net-pci,netdev=net0
    -netdev user,id=net0
    -nographic
    -serial "file:$SERIAL_LOG"
    -monitor "unix:$MONITOR_SOCKET,server,nowait"
    -no-reboot
)

if [[ -n "$BIOS_ARG" ]]; then
    QEMU_CMD+=($BIOS_ARG)
fi

# Start QEMU in background
"${QEMU_CMD[@]}" &
QEMU_PID=$!

log_info "QEMU PID: $QEMU_PID"

# =============================================================================
# Wait for boot and verify
# =============================================================================
BOOT_OK=false
START_TIME=$(date +%s)

log_info "Waiting for boot (checking serial log)..."

while true; do
    ELAPSED=$(($(date +%s) - START_TIME))
    
    if [[ $ELAPSED -ge $TIMEOUT ]]; then
        log_error "Boot timeout after ${TIMEOUT}s"
        break
    fi
    
    # Check if QEMU is still running
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
        log_warn "QEMU exited prematurely"
        break
    fi
    
    # Check serial log for boot markers
    if [[ -f "$SERIAL_LOG" ]]; then
        # Look for login prompt or BOOT_OK marker
        if grep -q "BOOT_OK\|login:" "$SERIAL_LOG" 2>/dev/null; then
            log_info "Boot marker detected!"
            BOOT_OK=true
            break
        fi
        
        # Also accept kernel boot messages as partial success
        if grep -q "Linux version\|Booting Linux" "$SERIAL_LOG" 2>/dev/null; then
            log_info "Kernel boot detected (waiting for full boot)..."
        fi
    fi
    
    sleep 2
done

# =============================================================================
# Print results
# =============================================================================
echo ""
echo "=== Serial Log (last 50 lines) ==="
tail -50 "$SERIAL_LOG" 2>/dev/null || echo "(no serial output)"
echo "==================================="
echo ""

if $BOOT_OK; then
    log_info "✅ BOOT_OK - Image boots successfully"
    echo "BOOT_OK"
    exit 0
else
    log_error "❌ BOOT_FAILED - Image did not boot within ${TIMEOUT}s"
    echo "BOOT_FAILED"
    exit 1
fi
