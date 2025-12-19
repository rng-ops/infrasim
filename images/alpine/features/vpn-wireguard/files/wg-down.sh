#!/bin/sh
# wg-down.sh - WireGuard post-down script
# Called by wg-quick after interface is down

set -eu

INTERFACE="${1:-wg0}"

log() {
    logger -t "wg-down" -p daemon.info "$*"
    echo "[$(date -Iseconds)] $*"
}

log "WireGuard interface $INTERFACE is going down"

# Notify control plane if available
if [ -S /run/infrasim/control.sock ]; then
    echo '{"event":"wg_down","interface":"'"$INTERFACE"'"}' | \
        nc -U /run/infrasim/control.sock 2>/dev/null || true
fi

# Clean up any interface-specific firewall marks
nft delete table inet wg_${INTERFACE} 2>/dev/null || true

log "WireGuard $INTERFACE post-down complete"
