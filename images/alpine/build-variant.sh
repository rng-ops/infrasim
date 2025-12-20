#!/usr/bin/env bash
# ============================================================================
# build-variant.sh - Build Alpine Image Variants
# ============================================================================
#
# Builds a specific Alpine Linux variant with VPN configuration.
# This script extends the base Alpine image with variant-specific packages
# and configurations.
#
# USAGE:
#   ./build-variant.sh --variant=wireguard
#   ./build-variant.sh --variant=tailscale --output=output/alpine-tailscale.qcow2
#   ./build-variant.sh --variant=dual-vpn --base=output/base.qcow2
#
# VARIANTS:
#   no-vpn     - Base image without VPN software
#   wireguard  - WireGuard mesh VPN
#   tailscale  - Tailscale control plane
#   dual-vpn   - WireGuard + Tailscale (data + control separation)
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VARIANT=""
BASE_IMAGE=""
OUTPUT_FILE=""
ALPINE_VERSION="${ALPINE_VERSION:-3.20}"
CREATE_OVERLAY=true

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }
log_step()  { echo -e "${BLUE}[STEP]${NC} $*"; }

# =============================================================================
# Argument parsing
# =============================================================================
while [[ $# -gt 0 ]]; do
    case $1 in
        --variant=*) VARIANT="${1#*=}"; shift ;;
        --variant) VARIANT="$2"; shift 2 ;;
        --base=*) BASE_IMAGE="${1#*=}"; shift ;;
        --base) BASE_IMAGE="$2"; shift 2 ;;
        --output=*) OUTPUT_FILE="${1#*=}"; shift ;;
        --output) OUTPUT_FILE="$2"; shift 2 ;;
        --no-overlay) CREATE_OVERLAY=false; shift ;;
        --help)
            echo "Usage: $0 --variant=VARIANT [--base=BASE_IMAGE] [--output=OUTPUT_FILE]"
            echo ""
            echo "Variants:"
            echo "  no-vpn     Base image without VPN"
            echo "  wireguard  WireGuard mesh VPN"
            echo "  tailscale  Tailscale control plane"
            echo "  dual-vpn   WireGuard + Tailscale"
            exit 0
            ;;
        *) log_error "Unknown option: $1"; exit 1 ;;
    esac
done

# Validate variant
if [[ -z "$VARIANT" ]]; then
    log_error "Variant is required. Use --variant=<name>"
    exit 1
fi

VARIANT_DIR="$SCRIPT_DIR/variants/$VARIANT"
if [[ ! -d "$VARIANT_DIR" ]]; then
    log_error "Unknown variant: $VARIANT"
    log_error "Available variants: no-vpn, wireguard, tailscale, dual-vpn"
    exit 1
fi

# Defaults
if [[ -z "$BASE_IMAGE" ]]; then
    BASE_IMAGE="$SCRIPT_DIR/output/base.qcow2"
fi

if [[ -z "$OUTPUT_FILE" ]]; then
    OUTPUT_FILE="$SCRIPT_DIR/output/alpine-${VARIANT}.qcow2"
fi

OUTPUT_DIR="$(dirname "$OUTPUT_FILE")"
OVERLAY_FILE="${OUTPUT_DIR}/overlay-${VARIANT}.qcow2"
META_DIR="${OUTPUT_DIR}/meta"
LOGS_DIR="${OUTPUT_DIR}/logs"
WORK_DIR="${OUTPUT_DIR}/work-${VARIANT}"

# =============================================================================
# Validate prerequisites
# =============================================================================
check_prerequisites() {
    log_step "Checking prerequisites..."
    
    local missing=()
    for cmd in qemu-img jq yq; do
        if ! command -v "$cmd" &>/dev/null; then
            missing+=("$cmd")
        fi
    done
    
    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required commands: ${missing[*]}"
        log_error "Install with: brew install qemu jq yq"
        exit 1
    fi
    
    if [[ ! -f "$BASE_IMAGE" ]]; then
        log_error "Base image not found: $BASE_IMAGE"
        log_error "Run build-alpine-qcow2.sh first to create the base image"
        exit 1
    fi
    
    log_info "Prerequisites OK"
}

# =============================================================================
# Setup directories
# =============================================================================
setup_directories() {
    log_step "Setting up directories..."
    mkdir -p "$OUTPUT_DIR" "$META_DIR" "$LOGS_DIR" "$WORK_DIR"
}

# =============================================================================
# Read variant configuration
# =============================================================================
read_variant_config() {
    log_step "Reading variant configuration..."
    
    local config_file="$VARIANT_DIR/config.yaml"
    if [[ ! -f "$config_file" ]]; then
        log_error "Variant config not found: $config_file"
        exit 1
    fi
    
    # Export variant config values
    VARIANT_DESC=$(yq -r '.description // ""' "$config_file")
    VARIANT_PACKAGES=$(yq -r '.packages | join(" ")' "$config_file")
    VARIANT_SERVICES=$(yq -r '.services | join(" ")' "$config_file")
    VARIANT_DISABLED_SERVICES=$(yq -r '.services_disabled | join(" ")' "$config_file" 2>/dev/null || echo "")
    
    log_info "Variant: $VARIANT"
    log_info "Description: $VARIANT_DESC"
    log_info "Packages: $VARIANT_PACKAGES"
    log_info "Services: $VARIANT_SERVICES"
}

# =============================================================================
# Create qcow2 overlay
# =============================================================================
create_overlay() {
    log_step "Creating qcow2 overlay..."
    
    if [[ "$CREATE_OVERLAY" == "true" ]]; then
        # Create overlay backed by base image
        qemu-img create -f qcow2 -b "$BASE_IMAGE" -F qcow2 "$OVERLAY_FILE" 2>&1 | tee -a "${LOGS_DIR}/build-${VARIANT}.log"
        log_info "Overlay created: $OVERLAY_FILE"
    fi
    
    # Copy base to output (we'll modify it)
    cp "$BASE_IMAGE" "$OUTPUT_FILE"
    log_info "Working image: $OUTPUT_FILE"
}

# =============================================================================
# Generate cloud-init for variant
# =============================================================================
generate_cloud_init() {
    log_step "Generating cloud-init for $VARIANT..."
    
    local ci_dir="${WORK_DIR}/cloud-init"
    mkdir -p "$ci_dir"
    
    # Generate packages section based on variant
    local packages_yaml=""
    if [[ -n "$VARIANT_PACKAGES" ]]; then
        packages_yaml="packages:"
        for pkg in $VARIANT_PACKAGES; do
            packages_yaml="$packages_yaml
  - $pkg"
        done
    fi
    
    # Generate services runcmd
    local services_runcmd=""
    for svc in $VARIANT_SERVICES; do
        services_runcmd="$services_runcmd
  - rc-update add $svc default
  - rc-service $svc start || true"
    done
    
    # Variant-specific content
    local variant_write_files=""
    local variant_runcmd=""
    
    case "$VARIANT" in
        no-vpn)
            variant_write_files=""
            variant_runcmd=""
            ;;
        wireguard)
            # Copy WireGuard configs
            if [[ -f "$VARIANT_DIR/wg0.conf.template" ]]; then
                cp "$VARIANT_DIR/wg0.conf.template" "$ci_dir/"
            fi
            if [[ -f "$VARIANT_DIR/peer-discovery.sh" ]]; then
                cp "$VARIANT_DIR/peer-discovery.sh" "$ci_dir/"
            fi
            
            variant_write_files="
  - path: /etc/wireguard/wg0.conf.template
    permissions: '0600'
    content: |
$(sed 's/^/      /' "$VARIANT_DIR/wg0.conf.template")
  
  - path: /opt/infrasim/wireguard/peer-discovery.sh
    permissions: '0755'
    content: |
$(sed 's/^/      /' "$VARIANT_DIR/peer-discovery.sh")"

            variant_runcmd="
  - mkdir -p /opt/infrasim/wireguard
  - chmod +x /opt/infrasim/wireguard/peer-discovery.sh"
            ;;
        tailscale)
            if [[ -f "$VARIANT_DIR/tailscale-up.sh" ]]; then
                cp "$VARIANT_DIR/tailscale-up.sh" "$ci_dir/"
            fi
            
            variant_write_files="
  - path: /opt/infrasim/tailscale/tailscale-up.sh
    permissions: '0755'
    content: |
$(sed 's/^/      /' "$VARIANT_DIR/tailscale-up.sh")
  
  - path: /etc/infrasim/tailscale/config.json
    permissions: '0600'
    content: |
      {
        \"advertise_exit_node\": false,
        \"accept_dns\": true,
        \"accept_routes\": true,
        \"ssh\": true,
        \"tags\": [\"tag:infrasim\", \"tag:alpine\", \"tag:tailscale\"]
      }"

            variant_runcmd="
  - mkdir -p /opt/infrasim/tailscale /etc/infrasim/tailscale
  - chmod +x /opt/infrasim/tailscale/tailscale-up.sh
  - /opt/infrasim/tailscale/tailscale-up.sh || true"
            ;;
        dual-vpn)
            # Copy all configs from both wireguard and tailscale variants
            local wg_dir="$SCRIPT_DIR/variants/wireguard"
            local ts_dir="$SCRIPT_DIR/variants/tailscale"
            
            variant_write_files="
  - path: /etc/wireguard/wg0.conf.template
    permissions: '0600'
    content: |
$(sed 's/^/      /' "$wg_dir/wg0.conf.template")
  
  - path: /opt/infrasim/wireguard/peer-discovery.sh
    permissions: '0755'
    content: |
$(sed 's/^/      /' "$wg_dir/peer-discovery.sh")
  
  - path: /opt/infrasim/tailscale/tailscale-up.sh
    permissions: '0755'
    content: |
$(sed 's/^/      /' "$ts_dir/tailscale-up.sh")
  
  - path: /opt/infrasim/network/policy-routing.sh
    permissions: '0755'
    content: |
$(sed 's/^/      /' "$VARIANT_DIR/policy-routing.sh")
  
  - path: /etc/infrasim/network/isolation.json
    permissions: '0644'
    content: |
      {
        \"control_plane\": {
          \"interface\": \"tailscale0\",
          \"mark\": \"0x100\",
          \"table\": 100
        },
        \"data_plane\": {
          \"interface\": \"wg0\",
          \"mark\": \"0x200\",
          \"table\": 200
        },
        \"isolation_enabled\": true
      }"

            variant_runcmd="
  - mkdir -p /opt/infrasim/wireguard /opt/infrasim/tailscale /opt/infrasim/network /etc/infrasim/network
  - chmod +x /opt/infrasim/wireguard/peer-discovery.sh
  - chmod +x /opt/infrasim/tailscale/tailscale-up.sh
  - chmod +x /opt/infrasim/network/policy-routing.sh
  - /opt/infrasim/tailscale/tailscale-up.sh || true
  - /opt/infrasim/network/policy-routing.sh || true"
            ;;
    esac
    
    # Generate user-data
    cat > "${ci_dir}/user-data" << EOF
#cloud-config
# InfraSim Alpine Linux - Variant: $VARIANT
# Description: $VARIANT_DESC
# Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")

hostname: alpine-${VARIANT}
manage_etc_hosts: true

$packages_yaml

runcmd:
  # Enable variant services
$services_runcmd
$variant_runcmd
  
  # Signal boot complete
  - echo "BOOT_OK variant=$VARIANT" > /dev/ttyS0
  - echo "BOOT_OK variant=$VARIANT" >> /var/log/boot-status.log

write_files:
  - path: /etc/infrasim/variant
    permissions: '0644'
    content: |
      VARIANT=$VARIANT
      BUILD_DATE=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
      ALPINE_VERSION=$ALPINE_VERSION
$variant_write_files

final_message: "InfraSim Alpine ($VARIANT) ready. Boot time: \$UPTIME seconds."
EOF
    
    # Generate meta-data
    cat > "${ci_dir}/meta-data" << EOF
instance-id: infrasim-alpine-${VARIANT}
local-hostname: alpine-${VARIANT}
EOF
    
    # Create cloud-init ISO
    if command -v genisoimage &>/dev/null; then
        genisoimage -output "${WORK_DIR}/cloud-init-${VARIANT}.iso" \
            -volid cidata -joliet -rock "$ci_dir" 2>&1 | tee -a "${LOGS_DIR}/build-${VARIANT}.log"
    elif command -v hdiutil &>/dev/null; then
        hdiutil makehybrid -o "${WORK_DIR}/cloud-init-${VARIANT}.iso" "$ci_dir" \
            -iso -joliet -default-volume-name cidata 2>&1 | tee -a "${LOGS_DIR}/build-${VARIANT}.log"
    fi
    
    log_info "Cloud-init generated: ${WORK_DIR}/cloud-init-${VARIANT}.iso"
}

# =============================================================================
# Record variant provenance
# =============================================================================
record_provenance() {
    log_step "Recording variant provenance..."
    
    local git_sha
    git_sha=$(git rev-parse HEAD 2>/dev/null || echo "unknown")
    
    local base_hash
    if command -v sha256sum &>/dev/null; then
        base_hash=$(sha256sum "$BASE_IMAGE" | awk '{print $1}')
    else
        base_hash=$(shasum -a 256 "$BASE_IMAGE" | awk '{print $1}')
    fi
    
    cat > "${META_DIR}/variant-${VARIANT}-provenance.json" << EOF
{
  "format_version": "1.0",
  "build_type": "alpine-variant",
  "variant": "$VARIANT",
  "description": "$VARIANT_DESC",
  "base_image": {
    "path": "$BASE_IMAGE",
    "sha256": "$base_hash"
  },
  "builder": {
    "script": "build-variant.sh",
    "version": "1.0.0"
  },
  "source": {
    "git_sha": "$git_sha",
    "build_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  },
  "alpine": {
    "version": "$ALPINE_VERSION"
  },
  "packages": [
$(echo "$VARIANT_PACKAGES" | tr ' ' '\n' | sed 's/^/    "/;s/$/",/' | sed '$ s/,$//')
  ],
  "services": [
$(echo "$VARIANT_SERVICES" | tr ' ' '\n' | sed 's/^/    "/;s/$/",/' | sed '$ s/,$//')
  ],
  "output": {
    "image": "$OUTPUT_FILE",
    "overlay": "$OVERLAY_FILE"
  }
}
EOF
    
    log_info "Provenance recorded: ${META_DIR}/variant-${VARIANT}-provenance.json"
}

# =============================================================================
# Main
# =============================================================================
main() {
    log_info "Building Alpine Linux variant: $VARIANT"
    log_info "Base image: $BASE_IMAGE"
    log_info "Output: $OUTPUT_FILE"
    
    check_prerequisites
    setup_directories
    read_variant_config
    create_overlay
    generate_cloud_init
    record_provenance
    
    log_info "Build complete!"
    log_info "Variant image: $OUTPUT_FILE"
    if [[ "$CREATE_OVERLAY" == "true" ]]; then
        log_info "Overlay: $OVERLAY_FILE"
    fi
}

main "$@"
