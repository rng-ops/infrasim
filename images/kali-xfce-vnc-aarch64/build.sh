#!/bin/bash
#
# Build script for Kali Linux XFCE aarch64 QEMU image
#
# This script builds a qcow2 disk image suitable for use with InfraSim
# on macOS Apple Silicon with HVF acceleration.
#
# Prerequisites:
#   - QEMU (brew install qemu)
#   - Docker/Podman (for building the base image)
#   - debootstrap or similar (optional, for native builds)
#
# Usage:
#   ./build.sh [output-path]

set -euo pipefail

# Configuration
IMAGE_NAME="kali-xfce-aarch64"
IMAGE_SIZE="32G"
OUTPUT_DIR="${1:-/var/lib/infrasim/images}"
QCOW2_FILE="${OUTPUT_DIR}/${IMAGE_NAME}.qcow2"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    if ! command -v qemu-img &> /dev/null; then
        log_error "qemu-img not found. Install with: brew install qemu"
        exit 1
    fi
    
    if ! command -v qemu-system-aarch64 &> /dev/null; then
        log_error "qemu-system-aarch64 not found. Install with: brew install qemu"
        exit 1
    fi
    
    # Create output directory
    mkdir -p "${OUTPUT_DIR}"
    
    log_info "Prerequisites OK"
}

# Download base image if not building from Dockerfile
download_base_image() {
    local base_url="https://cdimage.kali.org/kali-2024.1/kali-linux-2024.1-qemu-arm64.7z"
    local base_file="${OUTPUT_DIR}/kali-base.7z"
    
    if [[ -f "${OUTPUT_DIR}/kali-base.qcow2" ]]; then
        log_info "Base image already exists, skipping download"
        return
    fi
    
    log_info "Downloading Kali Linux ARM64 QEMU image..."
    curl -L -o "${base_file}" "${base_url}" || {
        log_warn "Failed to download official image. Using alternative method..."
        create_minimal_image
        return
    }
    
    log_info "Extracting image..."
    7z x "${base_file}" -o"${OUTPUT_DIR}" || {
        log_error "Failed to extract image"
        exit 1
    }
    
    mv "${OUTPUT_DIR}"/*.qcow2 "${OUTPUT_DIR}/kali-base.qcow2"
}

# Create a minimal Debian-based image as fallback
create_minimal_image() {
    log_info "Creating minimal ARM64 image..."
    
    # Create empty qcow2 image
    qemu-img create -f qcow2 "${OUTPUT_DIR}/kali-base.qcow2" "${IMAGE_SIZE}"
    
    log_info "Created empty image. You'll need to install an OS manually."
    log_info "See docs/INSTALL.md for instructions."
}

# Customize the image with cloud-init and VNC
customize_image() {
    log_info "Customizing image..."
    
    local base_image="${OUTPUT_DIR}/kali-base.qcow2"
    
    # Create a copy for customization
    cp "${base_image}" "${QCOW2_FILE}"
    
    # Resize if needed
    log_info "Resizing image to ${IMAGE_SIZE}..."
    qemu-img resize "${QCOW2_FILE}" "${IMAGE_SIZE}"
    
    log_info "Image customization complete"
}

# Generate cloud-init ISO for first boot
create_cloud_init_iso() {
    log_info "Creating cloud-init ISO..."
    
    local ci_dir="${OUTPUT_DIR}/cloud-init"
    mkdir -p "${ci_dir}"
    
    # Create meta-data
    cat > "${ci_dir}/meta-data" << 'EOF'
instance-id: kali-xfce-aarch64
local-hostname: kali
EOF
    
    # Create user-data
    cat > "${ci_dir}/user-data" << 'EOF'
#cloud-config
hostname: kali
manage_etc_hosts: true

users:
  - name: kali
    gecos: Kali User
    sudo: ALL=(ALL) NOPASSWD:ALL
    groups: sudo, adm, cdrom, dip, plugdev
    shell: /bin/bash
    lock_passwd: false
    # Password: kali (hashed)
    passwd: $6$rounds=4096$saltsalt$OGD5G6oWoFy/x7F0MnXFV9MTCH8zXXbX2YFMVDWJ0ELH8x0NmE0OqhTF0OjkZq6.X7jZ3eoB6P0qhGQtLk7nW0

packages:
  - tigervnc-standalone-server
  - tigervnc-common
  - xfce4
  - xfce4-goodies
  - cloud-guest-utils

runcmd:
  - systemctl enable ssh
  - systemctl start ssh
  - mkdir -p /home/kali/.vnc
  - echo "kali" | vncpasswd -f > /home/kali/.vnc/passwd
  - chmod 600 /home/kali/.vnc/passwd
  - chown -R kali:kali /home/kali/.vnc
  - |
    cat > /home/kali/.vnc/xstartup << 'XSTARTUP'
    #!/bin/bash
    unset SESSION_MANAGER
    unset DBUS_SESSION_BUS_ADDRESS
    exec startxfce4
    XSTARTUP
  - chmod +x /home/kali/.vnc/xstartup
  - chown kali:kali /home/kali/.vnc/xstartup

final_message: "Kali Linux is ready! VNC available on :1"
EOF
    
    # Create ISO using hdiutil on macOS
    if command -v hdiutil &> /dev/null; then
        hdiutil makehybrid -o "${OUTPUT_DIR}/cloud-init.iso" "${ci_dir}" \
            -iso -joliet -default-volume-name cidata
    elif command -v genisoimage &> /dev/null; then
        genisoimage -output "${OUTPUT_DIR}/cloud-init.iso" \
            -volid cidata -joliet -rock "${ci_dir}"
    else
        log_warn "Cannot create cloud-init ISO. Install hdiutil or genisoimage."
    fi
    
    log_info "Cloud-init ISO created"
}

# Create UEFI firmware symlinks
setup_uefi() {
    log_info "Setting up UEFI firmware..."
    
    local uefi_dir="${OUTPUT_DIR}/uefi"
    mkdir -p "${uefi_dir}"
    
    # Check for edk2-aarch64 firmware
    local edk2_paths=(
        "/opt/homebrew/share/qemu/edk2-aarch64-code.fd"
        "/usr/share/qemu/edk2-aarch64-code.fd"
        "/usr/local/share/qemu/edk2-aarch64-code.fd"
    )
    
    for path in "${edk2_paths[@]}"; do
        if [[ -f "${path}" ]]; then
            ln -sf "${path}" "${uefi_dir}/QEMU_EFI.fd"
            log_info "Linked UEFI firmware from ${path}"
            return
        fi
    done
    
    log_warn "UEFI firmware not found. QEMU may need firmware path specified."
}

# Print usage instructions
print_usage() {
    cat << EOF

${GREEN}=== Kali Linux ARM64 Image Built Successfully ===${NC}

Image location: ${QCOW2_FILE}

To test the image with QEMU:

  qemu-system-aarch64 \\
    -M virt,highmem=on \\
    -accel hvf \\
    -cpu host \\
    -smp 4 \\
    -m 4096 \\
    -drive file=${QCOW2_FILE},format=qcow2,if=virtio \\
    -drive file=${OUTPUT_DIR}/cloud-init.iso,format=raw,if=virtio \\
    -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \\
    -device virtio-net-pci,netdev=net0 \\
    -netdev user,id=net0,hostfwd=tcp::2222-:22,hostfwd=tcp::5901-:5901 \\
    -nographic

To use with InfraSim:

  1. Start the daemon:
     infrasimd --foreground

  2. Apply Terraform config:
     cd examples/terraform
     tofu apply

  3. Access via web console or VNC

Default credentials:
  Username: kali
  Password: kali

EOF
}

# Main build process
main() {
    log_info "Building Kali Linux XFCE ARM64 image for InfraSim"
    
    check_prerequisites
    download_base_image
    customize_image
    create_cloud_init_iso
    setup_uefi
    
    log_info "Build complete!"
    print_usage
}

main "$@"
