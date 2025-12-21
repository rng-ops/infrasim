#!/bin/sh
# wg-up.sh - WireGuard post-up script
# Called by wg-quick after interface is up

set -eu

INTERFACE="${1:-wg0}"
PEERS_DIR="${PEERS_DIR:-/var/lib/infrasim/peer-descriptors}"

log() {
    logger -t "wg-up" -p daemon.info "$*"
    echo "[$(date -Iseconds)] $*"
}

log "WireGuard interface $INTERFACE is up"

# Apply any additional firewall rules
if [ -f "/etc/nftables.d/wireguard.nft" ]; then
    nft -f /etc/nftables.d/wireguard.nft
    log "Applied WireGuard nftables rules"
fi

# Sync peers from descriptors directory
if [ -d "$PEERS_DIR" ]; then
    /usr/local/bin/apply-peers.sh sync "$PEERS_DIR"
fi

# Start peer discovery if rendezvous is enabled
if [ -x /usr/local/bin/rendezvousd ]; then
    log "Starting rendezvous peer discovery"
    # rendezvousd will discover peers and call apply-peers.sh
fi

# Notify control plane if available
if [ -S /run/infrasim/control.sock ]; then
    echo '{"event":"wg_up","interface":"'"$INTERFACE"'"}' | \
        nc -U /run/infrasim/control.sock 2>/dev/null || true
fi

log "WireGuard $INTERFACE post-up complete"
