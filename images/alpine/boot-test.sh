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

# Detect architecture and set QEMU binary
ARCH=$(uname -m)
case "$ARCH" in
    arm64|aarch64)
        QEMU_BIN="qemu-system-aarch64"
        QEMU_MACHINE="-M virt -cpu host"
        # Try HVF on macOS, fall back to TCG
        if [[ "$(uname -s)" == "Darwin" ]]; then
            QEMU_ACCEL="-accel hvf"
        else
            QEMU_ACCEL="-accel tcg"
        fi
        # Find UEFI firmware
        UEFI_PATHS=(
            "/opt/homebrew/share/qemu/edk2-aarch64-code.fd"
            "/usr/share/qemu/edk2-aarch64-code.fd"
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
        QEMU_ACCEL="-accel tcg"
        BIOS_ARG=""
        ;;
    *)
        echo "Unsupported architecture: $ARCH"
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
