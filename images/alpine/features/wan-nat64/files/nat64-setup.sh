#!/bin/sh
# nat64-setup.sh - Set up NAT64 translation
#
# Usage: nat64-setup.sh [start|stop|status]

set -eu

PREFIX="${NAT64_PREFIX:-64:ff9b::/96}"
IPV4_POOL="${NAT64_IPV4_POOL:-192.168.255.0/24}"
TAYGA_ADDR="${NAT64_TAYGA_ADDR:-192.168.255.1}"

log() {
    logger -t "nat64" -p daemon.info "$*"
    echo "[$(date -Iseconds)] $*"
}

start_nat64() {
    log "Starting NAT64 with prefix $PREFIX"
    
    # Create data directory
    mkdir -p /var/lib/tayga
    
    # Create TUN device
    tayga --mktun
    
    # Configure interface
    ip link set nat64 up
    ip addr add "$TAYGA_ADDR" dev nat64
    ip route add "$IPV4_POOL" dev nat64
    ip -6 route add "$PREFIX" dev nat64
    
    # Start TAYGA
    tayga -d
    
    # Set up masquerading
    nft add table inet nat 2>/dev/null || true
    nft add chain inet nat postrouting '{ type nat hook postrouting priority 100; }' 2>/dev/null || true
    nft add rule inet nat postrouting oifname "eth0" ip saddr "$IPV4_POOL" masquerade 2>/dev/null || true
    
    log "NAT64 started"
}

stop_nat64() {
    log "Stopping NAT64"
    
    # Stop TAYGA
    pkill tayga 2>/dev/null || true
    
    # Remove interface
    ip link del nat64 2>/dev/null || true
    
    log "NAT64 stopped"
}

status_nat64() {
    if ip link show nat64 > /dev/null 2>&1; then
        echo "NAT64 interface: UP"
        ip addr show nat64
        
        if pgrep tayga > /dev/null 2>&1; then
            echo "TAYGA: running"
        else
            echo "TAYGA: not running"
        fi
    else
        echo "NAT64: not configured"
    fi
}

case "${1:-status}" in
    start)
        start_nat64
        ;;
    stop)
        stop_nat64
        ;;
    restart)
        stop_nat64
        sleep 1
        start_nat64
        ;;
    status)
        status_nat64
        ;;
    *)
        echo "Usage: $0 {start|stop|restart|status}"
        exit 1
        ;;
esac
