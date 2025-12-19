#!/bin/sh
# apply-peers.sh - WireGuard peer admission with Ed25519 signature verification
# 
# Security Model:
# - Peers are initially added with narrow AllowedIPs (/128 or /32)
# - Full AllowedIPs are only applied after signature verification
# - Node descriptors must be signed by a trusted signer
# - Prevents rogue peers from claiming wide address ranges
#
# Usage: apply-peers.sh add|remove|verify|sync <peer-descriptor.json>

set -eu

PEERS_DIR="${PEERS_DIR:-/etc/wireguard/peers}"
TRUSTED_SIGNERS_DIR="${TRUSTED_SIGNERS_DIR:-/etc/infrasim/trusted-signers}"
WG_INTERFACE="${WG_INTERFACE:-wg0}"
NARROW_ADMISSION="${NARROW_ADMISSION:-true}"

log() {
    logger -t "wg-peers" -p daemon.info "$*"
    echo "[$(date -Iseconds)] $*"
}

error() {
    logger -t "wg-peers" -p daemon.err "$*"
    echo "[$(date -Iseconds)] ERROR: $*" >&2
}

# Verify peer descriptor signature using Ed25519
verify_peer_signature() {
    local descriptor_file="$1"
    local sig_file="${descriptor_file}.sig"
    
    if [ ! -f "$sig_file" ]; then
        error "Signature file not found: $sig_file"
        return 1
    fi
    
    # Extract signer ID from descriptor
    local signer_id
    signer_id=$(jq -r '.attestation.signer // empty' "$descriptor_file")
    
    if [ -z "$signer_id" ]; then
        error "No signer ID in descriptor"
        return 1
    fi
    
    # Find trusted signer public key
    local pubkey_file="${TRUSTED_SIGNERS_DIR}/${signer_id}.pub"
    
    if [ ! -f "$pubkey_file" ]; then
        error "Untrusted signer: $signer_id (no public key found)"
        return 1
    fi
    
    # Verify signature using openssl
    if openssl pkeyutl -verify \
        -pubin -inkey "$pubkey_file" \
        -sigfile "$sig_file" \
        -rawin -in "$descriptor_file" 2>/dev/null; then
        log "Signature verified for signer: $signer_id"
        return 0
    else
        error "Signature verification failed for signer: $signer_id"
        return 1
    fi
}

# Extract WireGuard config from peer descriptor
extract_wg_config() {
    local descriptor_file="$1"
    local narrow="$2"
    
    local wg_pubkey
    local wg_endpoint
    local wg_allowed_ips
    local wg_ipv4
    local wg_ipv6
    local preshared_key
    local keepalive
    
    wg_pubkey=$(jq -r '.identity.wg_public_key // empty' "$descriptor_file")
    wg_endpoint=$(jq -r '.endpoints.wireguard.endpoint // empty' "$descriptor_file")
    wg_ipv4=$(jq -r '.endpoints.wireguard.ipv4 // empty' "$descriptor_file")
    wg_ipv6=$(jq -r '.endpoints.wireguard.ipv6 // empty' "$descriptor_file")
    preshared_key=$(jq -r '.endpoints.wireguard.preshared_key // empty' "$descriptor_file")
    keepalive=$(jq -r '.endpoints.wireguard.persistent_keepalive // 25' "$descriptor_file")
    
    if [ -z "$wg_pubkey" ]; then
        error "No WireGuard public key in descriptor"
        return 1
    fi
    
    # Determine AllowedIPs based on admission mode
    if [ "$narrow" = "true" ]; then
        # Narrow admission: only allow exact IPs until verified
        wg_allowed_ips=""
        if [ -n "$wg_ipv4" ]; then
            wg_allowed_ips="${wg_ipv4}/32"
        fi
        if [ -n "$wg_ipv6" ]; then
            if [ -n "$wg_allowed_ips" ]; then
                wg_allowed_ips="${wg_allowed_ips},${wg_ipv6}/128"
            else
                wg_allowed_ips="${wg_ipv6}/128"
            fi
        fi
    else
        # Full admission: use declared AllowedIPs
        wg_allowed_ips=$(jq -r '.endpoints.wireguard.allowed_ips // [] | join(",")' "$descriptor_file")
        
        # Fallback to narrow if no allowed_ips declared
        if [ -z "$wg_allowed_ips" ]; then
            if [ -n "$wg_ipv4" ]; then
                wg_allowed_ips="${wg_ipv4}/32"
            fi
            if [ -n "$wg_ipv6" ]; then
                if [ -n "$wg_allowed_ips" ]; then
                    wg_allowed_ips="${wg_allowed_ips},${wg_ipv6}/128"
                else
                    wg_allowed_ips="${wg_ipv6}/128"
                fi
            fi
        fi
    fi
    
    if [ -z "$wg_allowed_ips" ]; then
        error "No AllowedIPs could be determined"
        return 1
    fi
    
    # Output peer config
    echo "PublicKey=$wg_pubkey"
    echo "AllowedIPs=$wg_allowed_ips"
    
    if [ -n "$wg_endpoint" ]; then
        echo "Endpoint=$wg_endpoint"
    fi
    
    if [ -n "$preshared_key" ]; then
        echo "PresharedKey=$preshared_key"
    fi
    
    if [ "$keepalive" -gt 0 ] 2>/dev/null; then
        echo "PersistentKeepalive=$keepalive"
    fi
}

# Add peer with narrow admission
add_peer_narrow() {
    local descriptor_file="$1"
    local node_id
    
    node_id=$(jq -r '.node_id // empty' "$descriptor_file")
    
    if [ -z "$node_id" ]; then
        error "No node_id in descriptor"
        return 1
    fi
    
    log "Adding peer $node_id with narrow AllowedIPs"
    
    # Extract narrow config
    local wg_config
    wg_config=$(extract_wg_config "$descriptor_file" "true")
    
    if [ -z "$wg_config" ]; then
        return 1
    fi
    
    # Apply to WireGuard interface
    local pubkey
    pubkey=$(echo "$wg_config" | grep "^PublicKey=" | cut -d= -f2)
    
    local allowed_ips
    allowed_ips=$(echo "$wg_config" | grep "^AllowedIPs=" | cut -d= -f2)
    
    local endpoint
    endpoint=$(echo "$wg_config" | grep "^Endpoint=" | cut -d= -f2-)
    
    local keepalive
    keepalive=$(echo "$wg_config" | grep "^PersistentKeepalive=" | cut -d= -f2)
    
    local psk
    psk=$(echo "$wg_config" | grep "^PresharedKey=" | cut -d= -f2)
    
    # Build wg set command
    local wg_args="$WG_INTERFACE peer $pubkey allowed-ips $allowed_ips"
    
    if [ -n "$endpoint" ]; then
        wg_args="$wg_args endpoint $endpoint"
    fi
    
    if [ -n "$keepalive" ]; then
        wg_args="$wg_args persistent-keepalive $keepalive"
    fi
    
    if [ -n "$psk" ]; then
        echo "$psk" | wg set $wg_args preshared-key /dev/stdin
    else
        wg set $wg_args
    fi
    
    # Store descriptor for later verification
    mkdir -p "$PEERS_DIR"
    cp "$descriptor_file" "${PEERS_DIR}/${node_id}.json"
    if [ -f "${descriptor_file}.sig" ]; then
        cp "${descriptor_file}.sig" "${PEERS_DIR}/${node_id}.json.sig"
    fi
    
    log "Peer $node_id added with AllowedIPs: $allowed_ips (pending verification)"
}

# Verify and widen peer AllowedIPs
verify_and_widen_peer() {
    local descriptor_file="$1"
    local node_id
    
    node_id=$(jq -r '.node_id // empty' "$descriptor_file")
    
    if [ -z "$node_id" ]; then
        error "No node_id in descriptor"
        return 1
    fi
    
    log "Verifying peer $node_id for full AllowedIPs"
    
    # Verify signature
    if ! verify_peer_signature "$descriptor_file"; then
        error "Peer $node_id failed signature verification"
        return 1
    fi
    
    # Extract full config
    local wg_config
    wg_config=$(extract_wg_config "$descriptor_file" "false")
    
    if [ -z "$wg_config" ]; then
        return 1
    fi
    
    local pubkey
    pubkey=$(echo "$wg_config" | grep "^PublicKey=" | cut -d= -f2)
    
    local allowed_ips
    allowed_ips=$(echo "$wg_config" | grep "^AllowedIPs=" | cut -d= -f2)
    
    # Update peer with full AllowedIPs
    wg set "$WG_INTERFACE" peer "$pubkey" allowed-ips "$allowed_ips"
    
    # Mark as verified
    touch "${PEERS_DIR}/${node_id}.verified"
    
    log "Peer $node_id verified and widened to AllowedIPs: $allowed_ips"
}

# Remove peer
remove_peer() {
    local descriptor_file="$1"
    local node_id
    local pubkey
    
    node_id=$(jq -r '.node_id // empty' "$descriptor_file")
    pubkey=$(jq -r '.identity.wg_public_key // empty' "$descriptor_file")
    
    if [ -z "$pubkey" ]; then
        error "No WireGuard public key in descriptor"
        return 1
    fi
    
    log "Removing peer $node_id"
    
    wg set "$WG_INTERFACE" peer "$pubkey" remove
    
    # Clean up stored files
    if [ -n "$node_id" ]; then
        rm -f "${PEERS_DIR}/${node_id}.json"
        rm -f "${PEERS_DIR}/${node_id}.json.sig"
        rm -f "${PEERS_DIR}/${node_id}.verified"
    fi
    
    log "Peer $node_id removed"
}

# Sync all peers from descriptors directory
sync_peers() {
    local descriptors_dir="${1:-/var/lib/infrasim/peer-descriptors}"
    
    if [ ! -d "$descriptors_dir" ]; then
        log "No descriptors directory: $descriptors_dir"
        return 0
    fi
    
    log "Syncing peers from $descriptors_dir"
    
    local count=0
    for descriptor in "$descriptors_dir"/*.json; do
        if [ ! -f "$descriptor" ]; then
            continue
        fi
        
        # Skip signature files
        case "$descriptor" in
            *.sig) continue ;;
        esac
        
        local node_id
        node_id=$(jq -r '.node_id // empty' "$descriptor")
        
        if [ -z "$node_id" ]; then
            continue
        fi
        
        # Check if already added
        if [ -f "${PEERS_DIR}/${node_id}.json" ]; then
            # Check if needs verification
            if [ ! -f "${PEERS_DIR}/${node_id}.verified" ]; then
                if verify_and_widen_peer "$descriptor" 2>/dev/null; then
                    count=$((count + 1))
                fi
            fi
        else
            if add_peer_narrow "$descriptor" 2>/dev/null; then
                count=$((count + 1))
            fi
        fi
    done
    
    log "Synced $count peers"
}

# Main
case "${1:-}" in
    add)
        shift
        if [ -z "${1:-}" ]; then
            error "Usage: $0 add <peer-descriptor.json>"
            exit 1
        fi
        add_peer_narrow "$1"
        ;;
    
    verify)
        shift
        if [ -z "${1:-}" ]; then
            error "Usage: $0 verify <peer-descriptor.json>"
            exit 1
        fi
        verify_and_widen_peer "$1"
        ;;
    
    remove)
        shift
        if [ -z "${1:-}" ]; then
            error "Usage: $0 remove <peer-descriptor.json>"
            exit 1
        fi
        remove_peer "$1"
        ;;
    
    sync)
        shift
        sync_peers "${1:-}"
        ;;
    
    *)
        echo "Usage: $0 add|verify|remove|sync <peer-descriptor.json|directory>"
        echo ""
        echo "Commands:"
        echo "  add <descriptor>     Add peer with narrow AllowedIPs"
        echo "  verify <descriptor>  Verify signature and widen AllowedIPs"
        echo "  remove <descriptor>  Remove peer from WireGuard"
        echo "  sync [directory]     Sync all peers from descriptors directory"
        exit 1
        ;;
esac
