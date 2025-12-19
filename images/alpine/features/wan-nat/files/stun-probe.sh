#!/bin/sh
# stun-probe.sh - Simple STUN probe for endpoint discovery
#
# Usage: stun-probe.sh [server:port]

set -eu

SERVER="${1:-stun.cloudflare.com:3478}"
HOST="${SERVER%:*}"
PORT="${SERVER#*:}"

if command -v stun > /dev/null 2>&1; then
    stun "$HOST" "$PORT" 2>&1 | grep -E 'MappedAddress|NAT Type' || echo "STUN probe failed"
else
    echo "stun-client not installed"
    exit 1
fi
