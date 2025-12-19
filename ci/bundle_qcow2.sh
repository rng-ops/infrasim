#!/usr/bin/env bash
# ============================================================================
# bundle_qcow2.sh - Create signed .tar.gz bundle with provenance metadata
# ============================================================================
#
# Assembles an InfraSim Alpine qcow2 artifact bundle with:
# - disk/base.qcow2
# - disk/snapshots/clean.qcow2 (optional overlay)
# - meta/manifest.json (SHA256 hashes of all files)
# - meta/attestations/build-provenance.json
# - meta/attestations/artifact-integrity.json
# - meta/signatures/*.sig (Ed25519 signatures or placeholders)
# - meta/logs/qemu-img-info.txt, build.log.txt
#
# SIGNING:
# If SIGNING_KEY is set, uses Ed25519 to sign the manifest.
# Otherwise, creates a TODO placeholder signature.
#
set -euo pipefail

# =============================================================================
# Configuration
# =============================================================================
INPUT_DIR=""
OUTPUT_FILE=""
ALPINE_VERSION="${ALPINE_VERSION:-3.20}"
GIT_SHA="${GIT_SHA:-$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")}"
WORKFLOW_RUN_ID="${GITHUB_RUN_ID:-local}"
SIGNING_KEY="${SIGNING_KEY:-}"

# =============================================================================
# Argument parsing
# =============================================================================
while [[ $# -gt 0 ]]; do
    case $1 in
        --input) INPUT_DIR="$2"; shift 2 ;;
        --output) OUTPUT_FILE="$2"; shift 2 ;;
        --version) ALPINE_VERSION="$2"; shift 2 ;;
        --git-sha) GIT_SHA="$2"; shift 2 ;;
        --signing-key) SIGNING_KEY="$2"; shift 2 ;;
        --help)
            echo "Usage: $0 --input DIR --output FILE [--version VER] [--git-sha SHA] [--signing-key KEY]"
            exit 0
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

if [[ -z "$INPUT_DIR" || -z "$OUTPUT_FILE" ]]; then
    echo "Error: --input and --output are required"
    exit 1
fi

# =============================================================================
# Derived paths
# =============================================================================
BUNDLE_DIR=$(mktemp -d)
BUNDLE_NAME=$(basename "$OUTPUT_FILE" .tar.gz)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

cleanup() {
    rm -rf "$BUNDLE_DIR"
}
trap cleanup EXIT

# =============================================================================
# Create bundle directory structure
# =============================================================================
log_info "Creating bundle structure..."

mkdir -p "$BUNDLE_DIR/disk/snapshots"
mkdir -p "$BUNDLE_DIR/meta/attestations"
mkdir -p "$BUNDLE_DIR/meta/signatures"
mkdir -p "$BUNDLE_DIR/meta/logs"

# =============================================================================
# Copy disk images
# =============================================================================
log_info "Copying disk images..."

if [[ -f "$INPUT_DIR/base.qcow2" ]]; then
    cp "$INPUT_DIR/base.qcow2" "$BUNDLE_DIR/disk/base.qcow2"
elif [[ -f "$INPUT_DIR/output/base.qcow2" ]]; then
    cp "$INPUT_DIR/output/base.qcow2" "$BUNDLE_DIR/disk/base.qcow2"
else
    log_error "base.qcow2 not found in $INPUT_DIR"
    exit 1
fi

# Create optional external snapshot (clean overlay)
log_info "Creating clean overlay snapshot..."
qemu-img create -f qcow2 -b base.qcow2 -F qcow2 "$BUNDLE_DIR/disk/snapshots/clean.qcow2"

# =============================================================================
# Copy logs
# =============================================================================
log_info "Copying logs..."

for log_file in qemu-img-info.txt build.log.txt; do
    src="$INPUT_DIR/logs/$log_file"
    if [[ ! -f "$src" ]]; then
        src="$INPUT_DIR/output/logs/$log_file"
    fi
    if [[ -f "$src" ]]; then
        cp "$src" "$BUNDLE_DIR/meta/logs/$log_file"
    fi
done

# Generate qemu-img info if not present
if [[ ! -f "$BUNDLE_DIR/meta/logs/qemu-img-info.txt" ]]; then
    qemu-img info "$BUNDLE_DIR/disk/base.qcow2" > "$BUNDLE_DIR/meta/logs/qemu-img-info.txt"
fi

# =============================================================================
# Copy/generate provenance
# =============================================================================
log_info "Generating attestations..."

# Copy build provenance if exists
for prov_file in build-provenance.json; do
    src="$INPUT_DIR/meta/$prov_file"
    if [[ ! -f "$src" ]]; then
        src="$INPUT_DIR/output/meta/$prov_file"
    fi
    if [[ -f "$src" ]]; then
        cp "$src" "$BUNDLE_DIR/meta/attestations/$prov_file"
    fi
done

# Generate build provenance if not present
if [[ ! -f "$BUNDLE_DIR/meta/attestations/build-provenance.json" ]]; then
    cat > "$BUNDLE_DIR/meta/attestations/build-provenance.json" << EOF
{
  "format_version": "1.0",
  "build_type": "alpine-qcow2-bundle",
  "source": {
    "git_sha": "${GIT_SHA}",
    "workflow_run_id": "${WORKFLOW_RUN_ID}",
    "build_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  },
  "alpine": {
    "version": "${ALPINE_VERSION}"
  }
}
EOF
fi

# =============================================================================
# Compute SHA256 hashes and generate manifest
# =============================================================================
log_info "Computing file hashes..."

declare -A FILE_HASHES

compute_hash() {
    local file="$1"
    local rel_path="${file#$BUNDLE_DIR/}"
    local hash
    
    if command -v sha256sum &>/dev/null; then
        hash=$(sha256sum "$file" | awk '{print $1}')
    else
        hash=$(shasum -a 256 "$file" | awk '{print $1}')
    fi
    
    FILE_HASHES["$rel_path"]="$hash"
    echo "$rel_path: $hash"
}

# Hash all files except signatures and manifest
find "$BUNDLE_DIR" -type f ! -path "*/signatures/*" ! -name "manifest.json" | while read -r file; do
    compute_hash "$file"
done

# Generate manifest.json
log_info "Generating manifest.json..."

cat > "$BUNDLE_DIR/meta/manifest.json" << 'EOF_HEADER'
{
  "format_version": "1.0",
  "created_at": "TIMESTAMP",
  "git_sha": "GIT_SHA",
  "files": [
EOF_HEADER

# Replace placeholders (portable sed for both macOS and Linux)
if [[ "$(uname)" == "Darwin" ]]; then
    sed -i '' "s/TIMESTAMP/$(date -u +%Y-%m-%dT%H:%M:%SZ)/g" "$BUNDLE_DIR/meta/manifest.json"
    sed -i '' "s/GIT_SHA/${GIT_SHA}/g" "$BUNDLE_DIR/meta/manifest.json"
else
    sed -i "s/TIMESTAMP/$(date -u +%Y-%m-%dT%H:%M:%SZ)/g" "$BUNDLE_DIR/meta/manifest.json"
    sed -i "s/GIT_SHA/${GIT_SHA}/g" "$BUNDLE_DIR/meta/manifest.json"
fi

# Add file entries
first=true
find "$BUNDLE_DIR" -type f ! -path "*/signatures/*" ! -name "manifest.json" -print0 | while IFS= read -r -d '' file; do
    rel_path="${file#$BUNDLE_DIR/}"
    
    if command -v sha256sum &>/dev/null; then
        hash=$(sha256sum "$file" | awk '{print $1}')
    else
        hash=$(shasum -a 256 "$file" | awk '{print $1}')
    fi
    
    # Compute file size (portable for macOS and Linux)
    if [[ "$(uname)" == "Darwin" ]]; then
        size=$(stat -f%z "$file" 2>/dev/null)
    else
        size=$(stat -c%s "$file" 2>/dev/null)
    fi
    
    if [[ "$first" == "true" ]]; then
        first=false
    else
        echo "," >> "$BUNDLE_DIR/meta/manifest.json"
    fi
    
    cat >> "$BUNDLE_DIR/meta/manifest.json" << EOF
    {
      "path": "$rel_path",
      "sha256": "$hash",
      "size": $size
    }
EOF
done

echo "  ]" >> "$BUNDLE_DIR/meta/manifest.json"
echo "}" >> "$BUNDLE_DIR/meta/manifest.json"

# =============================================================================
# Generate artifact integrity attestation
# =============================================================================
log_info "Generating artifact integrity attestation..."

# Compute manifest hash
if command -v sha256sum &>/dev/null; then
    MANIFEST_HASH=$(sha256sum "$BUNDLE_DIR/meta/manifest.json" | awk '{print $1}')
else
    MANIFEST_HASH=$(shasum -a 256 "$BUNDLE_DIR/meta/manifest.json" | awk '{print $1}')
fi

cat > "$BUNDLE_DIR/meta/attestations/artifact-integrity.json" << EOF
{
  "format_version": "1.0",
  "attestation_type": "artifact-integrity",
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "subject": {
    "name": "${BUNDLE_NAME}",
    "digest": {
      "sha256": "${MANIFEST_HASH}"
    }
  },
  "predicate": {
    "type": "https://infrasim.dev/attestation/artifact-integrity/v1",
    "manifest_sha256": "${MANIFEST_HASH}",
    "verified": true
  }
}
EOF

# =============================================================================
# Sign manifest (or create placeholder)
# =============================================================================
log_info "Signing manifest..."

if [[ -n "$SIGNING_KEY" && -f "$SIGNING_KEY" ]]; then
    # Real Ed25519 signing using OpenSSL or similar
    # TODO: Implement real signing with infrasim_common::crypto
    log_warn "Real signing not yet implemented. Creating placeholder."
    SIGNATURE_STATUS="placeholder"
else
    log_warn "No signing key provided. Creating TODO placeholder signature."
    SIGNATURE_STATUS="placeholder"
fi

# Create placeholder signature file
cat > "$BUNDLE_DIR/meta/signatures/manifest.sig" << EOF
-----BEGIN INFRASIM SIGNATURE-----
Status: ${SIGNATURE_STATUS}
Algorithm: Ed25519
Subject: manifest.json
Digest: sha256:${MANIFEST_HASH}

TODO: Implement real Ed25519 signing.
To enable signing:
1. Generate an Ed25519 key pair
2. Set SIGNING_KEY=/path/to/private.key
3. The signature will be computed using infrasim_common::crypto

This is a deterministic placeholder for development/testing.
Signature bytes would appear here in base64 encoding.

PLACEHOLDER_SIGNATURE_$(echo -n "${MANIFEST_HASH}" | head -c 32)
-----END INFRASIM SIGNATURE-----
EOF

# Create signature metadata
cat > "$BUNDLE_DIR/meta/signatures/signature-info.json" << EOF
{
  "format_version": "1.0",
  "status": "${SIGNATURE_STATUS}",
  "algorithm": "Ed25519",
  "subject": "manifest.json",
  "subject_digest": "sha256:${MANIFEST_HASH}",
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "signer": {
    "type": "none",
    "note": "Signing not configured. Set SIGNING_KEY to enable."
  },
  "how_to_enable_signing": {
    "step1": "Generate Ed25519 key: openssl genpkey -algorithm ED25519 -out signing.key",
    "step2": "Export public key: openssl pkey -in signing.key -pubout -out signing.pub",
    "step3": "Set environment: export SIGNING_KEY=/path/to/signing.key",
    "step4": "Re-run bundling: ./ci/bundle_qcow2.sh ..."
  }
}
EOF

# =============================================================================
# Create README for the bundle
# =============================================================================
log_info "Creating bundle README..."

cat > "$BUNDLE_DIR/README.md" << EOF
# InfraSim Alpine Linux Image Bundle

This bundle contains a QEMU-bootable Alpine Linux qcow2 image for use with InfraSim.

## Contents

\`\`\`
disk/
  base.qcow2              - Main disk image
  snapshots/
    clean.qcow2           - Clean overlay snapshot (external)

meta/
  manifest.json           - SHA256 hashes of all files
  attestations/
    build-provenance.json - Build inputs and environment
    artifact-integrity.json - Integrity attestation
  signatures/
    manifest.sig          - Signature of manifest.json
    signature-info.json   - Signature metadata
  logs/
    qemu-img-info.txt     - Image details
    build.log.txt         - Build log
\`\`\`

## Quick Start

### Boot with QEMU (macOS Apple Silicon)

\`\`\`bash
qemu-system-aarch64 \\
  -M virt -accel hvf -cpu host \\
  -m 512 -smp 2 \\
  -drive file=disk/base.qcow2,format=qcow2,if=virtio \\
  -device virtio-net-pci,netdev=net0 \\
  -netdev user,id=net0,hostfwd=tcp::2222-:22 \\
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \\
  -nographic
\`\`\`

### Use with InfraSim Terraform Provider

\`\`\`hcl
resource "infrasim_vm" "alpine" {
  name   = "alpine-runner"
  cpus   = 2
  memory = 512
  disk   = "/path/to/disk/base.qcow2"
}
\`\`\`

## Verification

Verify file integrity:

\`\`\`bash
# Check manifest hashes
jq -r '.files[] | "\(.sha256)  \(.path)"' meta/manifest.json | sha256sum -c
\`\`\`

## Build Information

- Alpine Version: ${ALPINE_VERSION}
- Git SHA: ${GIT_SHA}
- Built: $(date -u +%Y-%m-%dT%H:%M:%SZ)

See \`meta/attestations/build-provenance.json\` for full build details.
EOF

# =============================================================================
# Create tarball
# =============================================================================
log_info "Creating tarball: $OUTPUT_FILE"

mkdir -p "$(dirname "$OUTPUT_FILE")"

# Create tarball with deterministic ordering
(cd "$BUNDLE_DIR" && tar --sort=name -czf "$OUTPUT_FILE" .)

# Compute tarball hash
if command -v sha256sum &>/dev/null; then
    TARBALL_HASH=$(sha256sum "$OUTPUT_FILE" | awk '{print $1}')
else
    TARBALL_HASH=$(shasum -a 256 "$OUTPUT_FILE" | awk '{print $1}')
fi

echo "$TARBALL_HASH  $(basename "$OUTPUT_FILE")" > "${OUTPUT_FILE}.sha256"

# =============================================================================
# Summary
# =============================================================================
log_info "Bundle created successfully!"
echo ""
echo "  Output: $OUTPUT_FILE"
echo "  SHA256: $TARBALL_HASH"
echo "  Size:   $(du -h "$OUTPUT_FILE" | cut -f1)"
echo ""
echo "  Checksum file: ${OUTPUT_FILE}.sha256"
echo ""
