#!/bin/sh
# wg-keygen.sh - Generate WireGuard keypair and update node descriptor
#
# Usage: wg-keygen.sh [output-dir]

set -eu

OUTPUT_DIR="${1:-/etc/wireguard}"
DESCRIPTOR_FILE="${DESCRIPTOR_FILE:-/etc/infrasim/node-descriptor.json}"

log() {
    logger -t "wg-keygen" -p daemon.info "$*"
    echo "[$(date -Iseconds)] $*"
}

mkdir -p "$OUTPUT_DIR"
chmod 700 "$OUTPUT_DIR"

# Generate private key
PRIVATE_KEY=$(wg genkey)
echo "$PRIVATE_KEY" > "${OUTPUT_DIR}/privatekey"
chmod 600 "${OUTPUT_DIR}/privatekey"

# Generate public key
PUBLIC_KEY=$(echo "$PRIVATE_KEY" | wg pubkey)
echo "$PUBLIC_KEY" > "${OUTPUT_DIR}/publickey"
chmod 644 "${OUTPUT_DIR}/publickey"

log "Generated WireGuard keypair"
log "Public key: $PUBLIC_KEY"

# Update node descriptor if it exists
if [ -f "$DESCRIPTOR_FILE" ]; then
    # Create temporary file
    TMP_FILE=$(mktemp)
    
    # Update the wg_public_key field
    jq --arg pubkey "$PUBLIC_KEY" '.identity.wg_public_key = $pubkey' \
        "$DESCRIPTOR_FILE" > "$TMP_FILE"
    
    mv "$TMP_FILE" "$DESCRIPTOR_FILE"
    chmod 644 "$DESCRIPTOR_FILE"
    
    log "Updated node descriptor with WireGuard public key"
    
    # Remove old signature (needs re-signing)
    rm -f "${DESCRIPTOR_FILE}.sig"
    log "Note: Node descriptor needs re-signing"
fi

echo "$PUBLIC_KEY"
