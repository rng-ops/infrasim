#!/bin/bash
# ============================================================================
# verify-signature.sh - Ed25519 Signature Verification
# ============================================================================
#
# Verifies Ed25519 signatures for node descriptors, peer rosters, and manifests.
# Uses libsodium via Python or openssl depending on availability.
#
# CRITICAL: This is the trust gate. All peer additions MUST pass through
# signature verification. Discovery mechanisms (mDNS, rendezvous) are
# convenience only and provide no trust.
#
# Usage:
#   verify-signature.sh --data <file> --sig <signature_file> --pubkey <pubkey_file>
#   verify-signature.sh --descriptor <node-descriptor.json>
#   verify-signature.sh --roster <peer-roster.json>
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TRUST_DIR="${TRUST_DIR:-/etc/infrasim/trust}"
LOG_FILE="/var/log/infrasim/verify.log"

# Ensure log directory exists
mkdir -p "$(dirname "$LOG_FILE")"

log() {
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*" | tee -a "$LOG_FILE"
}

log_error() {
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] ERROR: $*" | tee -a "$LOG_FILE" >&2
}

# =============================================================================
# Ed25519 verification using Python + libsodium (nacl)
# =============================================================================
verify_ed25519_python() {
    local data_file="$1"
    local sig_file="$2"
    local pubkey_file="$3"
    
    python3 << EOF
import sys
import base64
import json

try:
    from nacl.signing import VerifyKey
    from nacl.exceptions import BadSignature
except ImportError:
    print("ERROR: pynacl not installed", file=sys.stderr)
    sys.exit(2)

# Read public key (base64 or raw)
with open("$pubkey_file", "rb") as f:
    pubkey_data = f.read().strip()
    
# Try base64 decode
try:
    if len(pubkey_data) == 32:
        pubkey_bytes = pubkey_data
    else:
        pubkey_bytes = base64.b64decode(pubkey_data)
except:
    pubkey_bytes = pubkey_data

if len(pubkey_bytes) != 32:
    print(f"ERROR: Invalid public key length: {len(pubkey_bytes)}", file=sys.stderr)
    sys.exit(1)

# Read signature (base64)
with open("$sig_file", "rb") as f:
    sig_data = f.read().strip()
try:
    sig_bytes = base64.b64decode(sig_data)
except:
    sig_bytes = sig_data

if len(sig_bytes) != 64:
    print(f"ERROR: Invalid signature length: {len(sig_bytes)}", file=sys.stderr)
    sys.exit(1)

# Read data
with open("$data_file", "rb") as f:
    data = f.read()

# Verify
try:
    verify_key = VerifyKey(pubkey_bytes)
    verify_key.verify(data, sig_bytes)
    print("VERIFIED")
    sys.exit(0)
except BadSignature:
    print("INVALID", file=sys.stderr)
    sys.exit(1)
except Exception as e:
    print(f"ERROR: {e}", file=sys.stderr)
    sys.exit(1)
EOF
}

# =============================================================================
# Ed25519 verification using openssl (fallback)
# =============================================================================
verify_ed25519_openssl() {
    local data_file="$1"
    local sig_file="$2"
    local pubkey_file="$3"
    
    # OpenSSL expects PEM format for Ed25519
    # Convert raw pubkey to PEM if needed
    local pubkey_pem
    pubkey_pem=$(mktemp)
    
    if head -1 "$pubkey_file" | grep -q "BEGIN"; then
        cp "$pubkey_file" "$pubkey_pem"
    else
        # Convert raw to PEM (Ed25519 public key)
        local raw_key
        raw_key=$(base64 -d "$pubkey_file" 2>/dev/null || cat "$pubkey_file")
        cat > "$pubkey_pem" << PEMEOF
-----BEGIN PUBLIC KEY-----
$(echo -n "$raw_key" | base64)
-----END PUBLIC KEY-----
PEMEOF
    fi
    
    # Convert base64 signature to binary
    local sig_bin
    sig_bin=$(mktemp)
    base64 -d "$sig_file" > "$sig_bin" 2>/dev/null || cp "$sig_file" "$sig_bin"
    
    # Verify
    if openssl pkeyutl -verify -pubin -inkey "$pubkey_pem" \
        -sigfile "$sig_bin" -in "$data_file" -rawin 2>/dev/null; then
        echo "VERIFIED"
        rm -f "$pubkey_pem" "$sig_bin"
        return 0
    else
        echo "INVALID" >&2
        rm -f "$pubkey_pem" "$sig_bin"
        return 1
    fi
}

# =============================================================================
# Verify function - tries Python first, falls back to OpenSSL
# =============================================================================
verify_signature() {
    local data_file="$1"
    local sig_file="$2"
    local pubkey_file="$3"
    
    if [[ ! -f "$data_file" ]]; then
        log_error "Data file not found: $data_file"
        return 1
    fi
    
    if [[ ! -f "$sig_file" ]]; then
        log_error "Signature file not found: $sig_file"
        return 1
    fi
    
    if [[ ! -f "$pubkey_file" ]]; then
        log_error "Public key file not found: $pubkey_file"
        return 1
    fi
    
    # Try Python/nacl first
    if python3 -c "import nacl" 2>/dev/null; then
        if verify_ed25519_python "$data_file" "$sig_file" "$pubkey_file"; then
            log "Verified: $data_file (using nacl)"
            return 0
        else
            log_error "Verification failed: $data_file"
            return 1
        fi
    fi
    
    # Fallback to OpenSSL
    if command -v openssl &>/dev/null; then
        if verify_ed25519_openssl "$data_file" "$sig_file" "$pubkey_file"; then
            log "Verified: $data_file (using openssl)"
            return 0
        else
            log_error "Verification failed: $data_file"
            return 1
        fi
    fi
    
    log_error "No Ed25519 verification method available"
    return 2
}

# =============================================================================
# Verify node descriptor
# =============================================================================
verify_descriptor() {
    local descriptor_file="$1"
    local sig_file="${descriptor_file%.json}.sig"
    
    if [[ ! -f "$sig_file" ]]; then
        sig_file="${descriptor_file}.sig"
    fi
    
    # Find trust root
    local trust_root=""
    for key in "$TRUST_DIR"/*.pub "$TRUST_DIR"/root.pub; do
        if [[ -f "$key" ]]; then
            trust_root="$key"
            break
        fi
    done
    
    if [[ -z "$trust_root" ]]; then
        log_error "No trust root found in $TRUST_DIR"
        return 1
    fi
    
    verify_signature "$descriptor_file" "$sig_file" "$trust_root"
}

# =============================================================================
# Verify peer roster
# =============================================================================
verify_roster() {
    local roster_file="$1"
    local sig_file="${roster_file%.json}.sig"
    
    if [[ ! -f "$sig_file" ]]; then
        sig_file="${roster_file}.sig"
    fi
    
    # Rosters may be signed by control plane key
    local trust_key=""
    for key in "$TRUST_DIR"/control.pub "$TRUST_DIR"/root.pub "$TRUST_DIR"/*.pub; do
        if [[ -f "$key" ]]; then
            trust_key="$key"
            break
        fi
    done
    
    if [[ -z "$trust_key" ]]; then
        log_error "No trust key found"
        return 1
    fi
    
    verify_signature "$roster_file" "$sig_file" "$trust_key"
}

# =============================================================================
# Main
# =============================================================================
main() {
    local mode=""
    local data_file=""
    local sig_file=""
    local pubkey_file=""
    
    while [[ $# -gt 0 ]]; do
        case $1 in
            --data) data_file="$2"; shift 2 ;;
            --sig) sig_file="$2"; shift 2 ;;
            --pubkey) pubkey_file="$2"; shift 2 ;;
            --descriptor) mode="descriptor"; data_file="$2"; shift 2 ;;
            --roster) mode="roster"; data_file="$2"; shift 2 ;;
            --help)
                echo "Usage: $0 [options]"
                echo ""
                echo "Options:"
                echo "  --data <file> --sig <sig> --pubkey <key>  Verify arbitrary file"
                echo "  --descriptor <file>                        Verify node descriptor"
                echo "  --roster <file>                            Verify peer roster"
                exit 0
                ;;
            *) log_error "Unknown option: $1"; exit 1 ;;
        esac
    done
    
    case "$mode" in
        descriptor)
            verify_descriptor "$data_file"
            ;;
        roster)
            verify_roster "$data_file"
            ;;
        "")
            if [[ -n "$data_file" && -n "$sig_file" && -n "$pubkey_file" ]]; then
                verify_signature "$data_file" "$sig_file" "$pubkey_file"
            else
                log_error "Must specify --data, --sig, and --pubkey, or use --descriptor/--roster"
                exit 1
            fi
            ;;
    esac
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
