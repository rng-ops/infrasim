#!/bin/bash
# ============================================================================
# WireGuard Peer Discovery via DNS-SD
# ============================================================================
#
# This script discovers other WireGuard peers on the local network using
# DNS-SD (Avahi/mDNS). It's designed for mesh networks where peers can
# dynamically join and leave.
#
# How it works:
# 1. Advertises this node's WireGuard public key and endpoint via mDNS
# 2. Listens for other peers advertising the same service
# 3. Automatically adds discovered peers to the WireGuard interface
#
# Requirements:
# - avahi-daemon running
# - avahi-tools installed
# - WireGuard interface (wg0) configured
#
set -euo pipefail

# Configuration
SERVICE_TYPE="_wireguard._udp"
DOMAIN="local"
WG_INTERFACE="${WG_INTERFACE:-wg0}"
DISCOVERY_INTERVAL="${DISCOVERY_INTERVAL:-30}"
LOG_FILE="/var/log/wg-peer-discovery.log"
PEER_DIR="/var/lib/infrasim/wireguard/peers"
STATE_FILE="/var/lib/infrasim/wireguard/discovery-state.json"

# Ensure directories exist
mkdir -p "$PEER_DIR"
mkdir -p "$(dirname "$STATE_FILE")"

# Logging
log() {
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*" | tee -a "$LOG_FILE"
}

# Get this node's WireGuard public key
get_public_key() {
    wg show "$WG_INTERFACE" public-key 2>/dev/null || echo ""
}

# Get this node's WireGuard listen port
get_listen_port() {
    wg show "$WG_INTERFACE" listen-port 2>/dev/null || echo "51820"
}

# Get local IP address
get_local_ip() {
    ip -4 addr show scope global | grep inet | head -1 | awk '{print $2}' | cut -d/ -f1
}

# Register this node via Avahi
register_service() {
    local pubkey
    local port
    local ip
    
    pubkey=$(get_public_key)
    port=$(get_listen_port)
    ip=$(get_local_ip)
    
    if [[ -z "$pubkey" ]]; then
        log "ERROR: Cannot get WireGuard public key. Is $WG_INTERFACE configured?"
        return 1
    fi
    
    log "Registering WireGuard service: pubkey=$pubkey, endpoint=$ip:$port"
    
    # Create Avahi service file
    cat > /etc/avahi/services/wireguard.service << EOF
<?xml version="1.0" standalone='no'?>
<!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<service-group>
  <name replace-wildcards="yes">WireGuard on %h</name>
  <service>
    <type>$SERVICE_TYPE</type>
    <port>$port</port>
    <txt-record>pubkey=$pubkey</txt-record>
    <txt-record>ip=$ip</txt-record>
  </service>
</service-group>
EOF
    
    # Reload Avahi
    avahi-daemon --reload 2>/dev/null || true
    
    log "Service registered successfully"
}

# Discover peers via Avahi
discover_peers() {
    log "Discovering WireGuard peers..."
    
    local my_pubkey
    my_pubkey=$(get_public_key)
    
    # Browse for WireGuard services (timeout after 5 seconds)
    avahi-browse -t -r "$SERVICE_TYPE" -p 2>/dev/null | while IFS=';' read -r type iface proto name stype domain hostname address port txt; do
        # Skip non-resolution lines
        [[ "$type" != "=" ]] && continue
        
        # Parse TXT record for public key
        local peer_pubkey
        local peer_ip
        
        peer_pubkey=$(echo "$txt" | grep -oP 'pubkey=\K[^"]+' || true)
        peer_ip=$(echo "$txt" | grep -oP 'ip=\K[^"]+' || true)
        
        # Skip if no public key or same as ours
        [[ -z "$peer_pubkey" ]] && continue
        [[ "$peer_pubkey" == "$my_pubkey" ]] && continue
        
        # Use resolved address if ip not in TXT
        [[ -z "$peer_ip" ]] && peer_ip="$address"
        
        log "Discovered peer: $name at $peer_ip:$port (pubkey: ${peer_pubkey:0:8}...)"
        
        # Add peer if not already known
        add_peer "$peer_pubkey" "$peer_ip" "$port" "$name"
    done
}

# Add a discovered peer to WireGuard
add_peer() {
    local pubkey="$1"
    local ip="$2"
    local port="$3"
    local name="$4"
    
    # Check if peer already exists
    if wg show "$WG_INTERFACE" peers | grep -q "$pubkey"; then
        log "Peer ${pubkey:0:8}... already configured"
        return 0
    fi
    
    log "Adding peer: $name ($pubkey)"
    
    # Add peer to WireGuard
    wg set "$WG_INTERFACE" peer "$pubkey" \
        endpoint "$ip:$port" \
        allowed-ips "10.50.0.0/16" \
        persistent-keepalive 25
    
    # Save peer info
    cat > "$PEER_DIR/${pubkey:0:16}.json" << EOF
{
  "public_key": "$pubkey",
  "endpoint": "$ip:$port",
  "name": "$name",
  "discovered_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "allowed_ips": "10.50.0.0/16"
}
EOF
    
    log "Peer added successfully"
}

# Remove stale peers (not seen in recent discoveries)
cleanup_stale_peers() {
    # TODO: Implement stale peer cleanup
    # For now, peers are persistent
    :
}

# Main discovery loop
main() {
    log "Starting WireGuard peer discovery daemon"
    log "Interface: $WG_INTERFACE"
    log "Service type: $SERVICE_TYPE"
    log "Discovery interval: ${DISCOVERY_INTERVAL}s"
    
    # Wait for WireGuard interface
    local retries=0
    while ! wg show "$WG_INTERFACE" >/dev/null 2>&1; do
        log "Waiting for $WG_INTERFACE..."
        sleep 5
        ((retries++))
        if [[ $retries -gt 12 ]]; then
            log "ERROR: $WG_INTERFACE not available after 60s"
            exit 1
        fi
    done
    
    # Register our service
    register_service
    
    # Discovery loop
    while true; do
        discover_peers
        cleanup_stale_peers
        sleep "$DISCOVERY_INTERVAL"
    done
}

# Handle signals
trap 'log "Shutting down..."; exit 0' SIGTERM SIGINT

# Run if not sourced
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
