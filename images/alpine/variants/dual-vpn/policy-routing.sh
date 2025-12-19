#!/bin/bash
# ============================================================================
# Policy-Based Routing for Dual-VPN Configuration
# ============================================================================
#
# This script sets up policy-based routing to separate:
# - Control plane traffic (Tailscale) - management, telemetry, C2
# - Data plane traffic (WireGuard) - VM traffic, replication, migration
#
# The goal is complete traffic isolation between the two VPN interfaces
# for security purposes, especially in hostile territory deployments.
#
set -euo pipefail

LOG_FILE="/var/log/policy-routing.log"
CONFIG_FILE="/etc/infrasim/network/isolation.json"

# Logging
log() {
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*" | tee -a "$LOG_FILE"
}

# Read configuration
CONTROL_MARK="${CONTROL_MARK:-0x100}"
CONTROL_TABLE="${CONTROL_TABLE:-100}"
DATA_MARK="${DATA_MARK:-0x200}"
DATA_TABLE="${DATA_TABLE:-200}"

if [[ -f "$CONFIG_FILE" ]]; then
    CONTROL_MARK=$(jq -r '.control_plane.mark // "0x100"' "$CONFIG_FILE")
    CONTROL_TABLE=$(jq -r '.control_plane.table // 100' "$CONFIG_FILE")
    DATA_MARK=$(jq -r '.data_plane.mark // "0x200"' "$CONFIG_FILE")
    DATA_TABLE=$(jq -r '.data_plane.table // 200' "$CONFIG_FILE")
fi

# Wait for interfaces
wait_for_interfaces() {
    log "Waiting for VPN interfaces..."
    
    local retries=0
    while true; do
        local have_wg=false
        local have_ts=false
        
        ip link show wg0 >/dev/null 2>&1 && have_wg=true
        ip link show tailscale0 >/dev/null 2>&1 && have_ts=true
        
        if [[ "$have_wg" == "true" ]] && [[ "$have_ts" == "true" ]]; then
            log "Both interfaces are up"
            break
        fi
        
        ((retries++))
        if [[ $retries -gt 60 ]]; then
            log "WARNING: Timeout waiting for interfaces. Continuing with available interfaces."
            break
        fi
        
        sleep 2
    done
}

# Get interface IP
get_interface_ip() {
    local iface="$1"
    ip -4 addr show "$iface" 2>/dev/null | grep inet | head -1 | awk '{print $2}' | cut -d/ -f1
}

# Setup routing tables
setup_routing_tables() {
    log "Setting up routing tables..."
    
    # Add custom tables to rt_tables if not exists
    if ! grep -q "^$CONTROL_TABLE" /etc/iproute2/rt_tables 2>/dev/null; then
        echo "$CONTROL_TABLE control_plane" >> /etc/iproute2/rt_tables
    fi
    
    if ! grep -q "^$DATA_TABLE" /etc/iproute2/rt_tables 2>/dev/null; then
        echo "$DATA_TABLE data_plane" >> /etc/iproute2/rt_tables
    fi
    
    log "Routing tables configured"
}

# Setup control plane routing (Tailscale)
setup_control_plane() {
    log "Setting up control plane routing (Tailscale)..."
    
    if ! ip link show tailscale0 >/dev/null 2>&1; then
        log "WARNING: tailscale0 not available, skipping control plane setup"
        return
    fi
    
    local ts_ip
    ts_ip=$(get_interface_ip tailscale0)
    
    if [[ -z "$ts_ip" ]]; then
        log "WARNING: No IP on tailscale0"
        return
    fi
    
    log "Tailscale IP: $ts_ip"
    
    # Flush existing rules for this table
    ip route flush table "$CONTROL_TABLE" 2>/dev/null || true
    
    # Add default route via Tailscale
    ip route add default dev tailscale0 table "$CONTROL_TABLE" 2>/dev/null || true
    
    # Add rule: packets marked for control plane use control table
    ip rule del fwmark "$CONTROL_MARK" table "$CONTROL_TABLE" 2>/dev/null || true
    ip rule add fwmark "$CONTROL_MARK" table "$CONTROL_TABLE" priority 100
    
    log "Control plane routing configured"
}

# Setup data plane routing (WireGuard)
setup_data_plane() {
    log "Setting up data plane routing (WireGuard)..."
    
    if ! ip link show wg0 >/dev/null 2>&1; then
        log "WARNING: wg0 not available, skipping data plane setup"
        return
    fi
    
    local wg_ip
    wg_ip=$(get_interface_ip wg0)
    
    if [[ -z "$wg_ip" ]]; then
        log "WARNING: No IP on wg0"
        return
    fi
    
    log "WireGuard IP: $wg_ip"
    
    # Flush existing rules for this table
    ip route flush table "$DATA_TABLE" 2>/dev/null || true
    
    # Add default route via WireGuard
    ip route add default dev wg0 table "$DATA_TABLE" 2>/dev/null || true
    
    # Add rule: packets marked for data plane use data table
    ip rule del fwmark "$DATA_MARK" table "$DATA_TABLE" 2>/dev/null || true
    ip rule add fwmark "$DATA_MARK" table "$DATA_TABLE" priority 200
    
    log "Data plane routing configured"
}

# Setup nftables marking rules
setup_marking_rules() {
    log "Setting up packet marking rules..."
    
    # Create nftables rules for marking packets
    nft -f - << EOF
#!/usr/sbin/nft -f

# Flush existing infrasim table
table inet infrasim_routing
delete table inet infrasim_routing

# Create new table
table inet infrasim_routing {
    # Chain for marking outgoing packets
    chain output {
        type route hook output priority mangle; policy accept;
        
        # Mark control plane traffic
        # SSH, telemetry, management
        tcp dport { 22, 8080, 443 } meta mark set $CONTROL_MARK
        
        # Tailscale DERP traffic
        udp dport 3478 meta mark set $CONTROL_MARK
        
        # Mark data plane traffic
        # InfraSim mesh traffic (10.50.0.0/16)
        ip daddr 10.50.0.0/16 meta mark set $DATA_MARK
        
        # VM migration and storage replication
        tcp dport { 16509, 49152-49215 } meta mark set $DATA_MARK
    }
    
    # Chain for prerouting (incoming)
    chain prerouting {
        type filter hook prerouting priority mangle; policy accept;
        
        # Mark incoming based on interface
        iif "tailscale0" meta mark set $CONTROL_MARK
        iif "wg0" meta mark set $DATA_MARK
    }
}
EOF
    
    log "Packet marking rules configured"
}

# Setup isolation rules
setup_isolation() {
    log "Setting up traffic isolation..."
    
    # Prevent traffic from crossing between VPN interfaces
    nft -f - << EOF
#!/usr/sbin/nft -f

table inet infrasim_isolation {
    chain forward {
        type filter hook forward priority 0; policy accept;
        
        # Block WireGuard -> Tailscale
        iif "wg0" oif "tailscale0" drop
        
        # Block Tailscale -> WireGuard
        iif "tailscale0" oif "wg0" drop
        
        # Log dropped cross-interface traffic
        iif "wg0" oif "tailscale0" log prefix "ISOLATION-DROP-WG-TS: "
        iif "tailscale0" oif "wg0" log prefix "ISOLATION-DROP-TS-WG: "
    }
}
EOF
    
    log "Traffic isolation configured"
}

# Verify setup
verify_setup() {
    log "Verifying policy routing setup..."
    
    log "Routing tables:"
    ip rule list | tee -a "$LOG_FILE"
    
    log "Control plane table ($CONTROL_TABLE):"
    ip route show table "$CONTROL_TABLE" 2>/dev/null | tee -a "$LOG_FILE" || echo "  (empty)"
    
    log "Data plane table ($DATA_TABLE):"
    ip route show table "$DATA_TABLE" 2>/dev/null | tee -a "$LOG_FILE" || echo "  (empty)"
    
    log "nftables rules:"
    nft list table inet infrasim_routing 2>/dev/null | head -20 | tee -a "$LOG_FILE" || echo "  (no rules)"
}

# Main
main() {
    log "=== Policy-Based Routing Setup ==="
    log "Control plane mark: $CONTROL_MARK, table: $CONTROL_TABLE"
    log "Data plane mark: $DATA_MARK, table: $DATA_TABLE"
    
    wait_for_interfaces
    setup_routing_tables
    setup_control_plane
    setup_data_plane
    setup_marking_rules
    setup_isolation
    verify_setup
    
    log "=== Policy Routing Complete ==="
}

# Run if not sourced
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
