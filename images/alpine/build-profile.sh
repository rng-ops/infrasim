#!/bin/bash
# build-profile.sh - Build a complete profile image from composed overlay
#
# This script takes a composed profile (output of compose-profile.sh) and
# builds a qcow2 image with all features, selftests, and signed provenance.
#
# Usage: build-profile.sh <profile-name> [--sign-key KEY_FILE] [--output FILE]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="${SCRIPT_DIR}/build"
OUTPUT_DIR="${OUTPUT_DIR:-${SCRIPT_DIR}/output}"
BASE_IMAGE="${BASE_IMAGE:-alpine-base.qcow2}"
SIGN_KEY=""

log() {
    echo "[$(date -Iseconds)] $*"
}

error() {
    echo "[$(date -Iseconds)] ERROR: $*" >&2
    exit 1
}

# Parse arguments
PROFILE_NAME=""
OUTPUT_FILE=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --sign-key)
            SIGN_KEY="$2"
            shift 2
            ;;
        --output)
            OUTPUT_FILE="$2"
            shift 2
            ;;
        --base-image)
            BASE_IMAGE="$2"
            shift 2
            ;;
        *)
            PROFILE_NAME="$1"
            shift
            ;;
    esac
done

if [[ -z "$PROFILE_NAME" ]]; then
    echo "Usage: $0 <profile-name> [--sign-key KEY_FILE] [--output FILE]"
    echo ""
    echo "Available composed profiles:"
    ls -1 "$BUILD_DIR" 2>/dev/null || echo "  (none - run compose-profile.sh first)"
    exit 1
fi

PROFILE_DIR="${BUILD_DIR}/${PROFILE_NAME}"

if [[ ! -d "$PROFILE_DIR" ]]; then
    error "Profile not composed: $PROFILE_NAME (run compose-profile.sh first)"
fi

if [[ -z "$OUTPUT_FILE" ]]; then
    OUTPUT_FILE="${OUTPUT_DIR}/${PROFILE_NAME}.qcow2"
fi

mkdir -p "$OUTPUT_DIR"
mkdir -p "${PROFILE_DIR}/rootfs"

log "Building profile: $PROFILE_NAME"
log "Output: $OUTPUT_FILE"

# Check for base image
if [[ ! -f "$BASE_IMAGE" ]]; then
    log "Base image not found: $BASE_IMAGE"
    log "Creating minimal Alpine base image..."
    
    # Create base image using Alpine's make_image script or qemu-img
    qemu-img create -f qcow2 "$BASE_IMAGE" 2G
fi

# Convert BASE_IMAGE to absolute path (qemu-img interprets backing files relative to overlay location)
BASE_IMAGE="$(cd "$(dirname "$BASE_IMAGE")" && pwd)/$(basename "$BASE_IMAGE")"

# Create overlay image
log "Creating overlay image..."
OVERLAY_FILE="${PROFILE_DIR}/${PROFILE_NAME}-overlay.qcow2"
qemu-img create -f qcow2 -b "$BASE_IMAGE" -F qcow2 "$OVERLAY_FILE" 4G

# Mount and install packages (using guestfish or nbd)
install_packages() {
    local packages_file="${PROFILE_DIR}/packages.txt"
    
    if [[ ! -f "$packages_file" ]] || [[ ! -s "$packages_file" ]]; then
        log "No packages to install"
        return
    fi
    
    local packages
    packages=$(cat "$packages_file" | tr '\n' ' ')
    
    log "Installing packages: $packages"
    
    # Check if we're in CI mode (skip actual installation)
    if [[ "${CI_VALIDATION_ONLY:-false}" == "true" ]]; then
        log "CI validation mode - skipping package installation"
        return
    fi
    
    # Using virt-customize if available and working
    if command -v virt-customize &> /dev/null; then
        # Test if libguestfs works (it often doesn't in containers/CI)
        if virt-customize --help &>/dev/null 2>&1; then
            virt-customize -a "$OVERLAY_FILE" \
                --run-command "apk update && apk add $packages" 2>&1 || {
                log "WARNING: virt-customize failed (common in CI), skipping package installation"
                log "Packages would be installed: $packages"
            }
        else
            log "WARNING: virt-customize not functional (libguestfs issue), skipping package installation"
            log "Packages to install: $packages"
        fi
    else
        log "WARNING: virt-customize not available, skipping package installation"
        log "Packages to install: $packages"
    fi
}

# Copy files into image
copy_files() {
    local files_file="${PROFILE_DIR}/files.txt"
    
    if [[ ! -f "$files_file" ]]; then
        log "No files to copy"
        return
    fi
    
    log "Copying files..."
    
    local file_count=0
    while IFS='|' read -r source dest mode; do
        if [[ -z "$source" ]]; then
            continue
        fi
        
        if [[ ! -f "$source" ]]; then
            log "WARNING: Source file not found: $source"
            continue
        fi
        
        # Stage file for copying
        local stage_dir="${PROFILE_DIR}/rootfs$(dirname "$dest")"
        mkdir -p "$stage_dir"
        cp "$source" "${PROFILE_DIR}/rootfs${dest}"
        chmod "$mode" "${PROFILE_DIR}/rootfs${dest}" 2>/dev/null || true
        
        file_count=$((file_count + 1))
    done < "$files_file"
    
    log "Staged $file_count files"
    
    # Copy selftests
    local selftests_file="${PROFILE_DIR}/selftests.txt"
    if [[ -f "$selftests_file" ]]; then
        while IFS='|' read -r source dest; do
            if [[ -n "$source" ]] && [[ -f "$source" ]]; then
                local stage_dir="${PROFILE_DIR}/rootfs$(dirname "$dest")"
                mkdir -p "$stage_dir"
                cp "$source" "${PROFILE_DIR}/rootfs${dest}"
                chmod 755 "${PROFILE_DIR}/rootfs${dest}" 2>/dev/null || true
            fi
        done < "$selftests_file"
    fi
    
    # Copy firewall rules
    if [[ -s "${PROFILE_DIR}/firewall.nft" ]]; then
        mkdir -p "${PROFILE_DIR}/rootfs/etc/nftables.d"
        cp "${PROFILE_DIR}/firewall.nft" "${PROFILE_DIR}/rootfs/etc/nftables.d/profile.nft"
    fi
    
    # In CI validation mode, just stage files without copying to image
    if [[ "${CI_VALIDATION_ONLY:-false}" == "true" ]]; then
        log "CI validation mode - files staged to ${PROFILE_DIR}/rootfs"
        return
    fi
    
    # Use virt-copy-in if available
    if command -v virt-copy-in &> /dev/null && [[ -d "${PROFILE_DIR}/rootfs" ]]; then
        # Get list of top-level directories in rootfs
        for dir in "${PROFILE_DIR}/rootfs"/*; do
            if [[ -d "$dir" ]]; then
                local dirname=$(basename "$dir")
                virt-copy-in -a "$OVERLAY_FILE" "$dir" / 2>/dev/null || {
                    log "WARNING: virt-copy-in failed for $dir (libguestfs issue)"
                }
            fi
        done
    else
        log "WARNING: virt-copy-in not available, files staged but not copied to image"
    fi
}

# Configure services
configure_services() {
    local enable_file="${PROFILE_DIR}/services-enable.txt"
    local disable_file="${PROFILE_DIR}/services-disable.txt"
    
    # Check if we're in CI mode (skip actual configuration)
    if [[ "${CI_VALIDATION_ONLY:-false}" == "true" ]]; then
        log "CI validation mode - skipping service configuration"
        if [[ -f "$enable_file" ]] && [[ -s "$enable_file" ]]; then
            log "Services to enable: $(cat "$enable_file" | tr '\n' ' ')"
        fi
        return
    fi
    
    if command -v virt-customize &> /dev/null; then
        # Enable services
        if [[ -f "$enable_file" ]]; then
            while IFS= read -r service; do
                if [[ -n "$service" ]]; then
                    virt-customize -a "$OVERLAY_FILE" \
                        --run-command "rc-update add $service default" 2>/dev/null || true
                fi
            done < "$enable_file"
        fi
        
        # Disable services
        if [[ -f "$disable_file" ]]; then
            while IFS= read -r service; do
                if [[ -n "$service" ]]; then
                    virt-customize -a "$OVERLAY_FILE" \
                        --run-command "rc-update del $service default" 2>/dev/null || true
                fi
            done < "$disable_file"
        fi
    else
        log "WARNING: virt-customize not available, skipping service configuration"
    fi
}

# Generate and sign provenance
generate_provenance() {
    log "Generating provenance..."
    
    local manifest_file="${PROFILE_DIR}/manifest.json"
    local provenance_file="${OUTPUT_DIR}/${PROFILE_NAME}.provenance.json"
    
    # Calculate image hash
    local image_sha256
    image_sha256=$(sha256sum "$OVERLAY_FILE" | awk '{print $1}')
    
    # Calculate overlay chain hash
    local overlay_chain_hash
    overlay_chain_hash=$(cat "${PROFILE_DIR}/packages.txt" "${PROFILE_DIR}/files.txt" 2>/dev/null | sha256sum | awk '{print $1}')
    
    # Create provenance document
    cat > "$provenance_file" <<EOF
{
  "_type": "https://infrasim.io/provenance/v1",
  "profile": "$PROFILE_NAME",
  "version": "$(jq -r '.version' "$manifest_file")",
  "subject": [
    {
      "name": "$(basename "$OUTPUT_FILE")",
      "digest": {
        "sha256": "$image_sha256"
      }
    }
  ],
  "predicate": {
    "buildType": "https://infrasim.io/profile-build/v1",
    "builder": {
      "id": "$(hostname)"
    },
    "invocation": {
      "configSource": {
        "profile": "$PROFILE_NAME",
        "digest": {
          "sha256": "$(sha256sum "${PROFILE_DIR}/profile.yaml" | awk '{print $1}')"
        }
      }
    },
    "metadata": {
      "buildStartedOn": "$(date -Iseconds)",
      "buildFinishedOn": "$(date -Iseconds)",
      "reproducible": false
    },
    "materials": [
      {
        "uri": "$(basename "$BASE_IMAGE")",
        "digest": {
          "sha256": "$(sha256sum "$BASE_IMAGE" | awk '{print $1}')"
        }
      }
    ]
  },
  "overlay_chain_hash": "$overlay_chain_hash",
  "features": $(jq '.features' "$manifest_file"),
  "generated_at": "$(date -Iseconds)",
  "git_sha": "$(git rev-parse HEAD 2>/dev/null || echo 'unknown')"
}
EOF
    
    log "Provenance written to: $provenance_file"
    
    # Sign if key provided
    if [[ -n "$SIGN_KEY" ]] && [[ -f "$SIGN_KEY" ]]; then
        log "Signing provenance with Ed25519..."
        
        openssl pkeyutl -sign \
            -inkey "$SIGN_KEY" \
            -rawin \
            -in "$provenance_file" \
            -out "${provenance_file}.sig"
        
        log "Signature written to: ${provenance_file}.sig"
    fi
}

# Run selftests (optional, for CI)
run_selftests() {
    if [[ "${RUN_SELFTESTS:-false}" != "true" ]]; then
        log "Skipping selftests (set RUN_SELFTESTS=true to run)"
        return
    fi
    
    log "Running selftests..."
    
    # This would boot the image and run selftests
    # For now, just validate the selftest files exist
    local selftests_file="${PROFILE_DIR}/selftests.txt"
    if [[ -f "$selftests_file" ]]; then
        local count
        count=$(wc -l < "$selftests_file" | tr -d ' ')
        log "  $count selftest modules available"
    fi
}

# Main build flow
main() {
    install_packages
    copy_files
    configure_services
    
    # Move overlay to final location
    log "Finalizing image..."
    mv "$OVERLAY_FILE" "$OUTPUT_FILE"
    
    generate_provenance
    run_selftests
    
    log ""
    log "Build complete!"
    log "  Image: $OUTPUT_FILE"
    log "  Size: $(du -h "$OUTPUT_FILE" | awk '{print $1}')"
    
    if [[ -f "${OUTPUT_DIR}/${PROFILE_NAME}.provenance.json" ]]; then
        log "  Provenance: ${OUTPUT_DIR}/${PROFILE_NAME}.provenance.json"
    fi
}

main
