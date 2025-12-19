#!/bin/sh
# tailscale-check.sh - Check Tailscale status and connectivity
#
# Usage: tailscale-check.sh [--json]

set -eu

OUTPUT_JSON=false
if [ "${1:-}" = "--json" ]; then
    OUTPUT_JSON=true
fi

check_daemon() {
    if rc-service tailscaled status > /dev/null 2>&1; then
        return 0
    fi
    return 1
}

check_status() {
    local status
    status=$(tailscale status --json 2>/dev/null) || return 1
    
    local state
    state=$(echo "$status" | jq -r '.BackendState // "Unknown"')
    
    if [ "$state" = "Running" ]; then
        return 0
    fi
    return 1
}

check_connectivity() {
    # Try to ping a known Tailscale IP (100.100.100.100 is login server)
    if tailscale ping --timeout=5s 100.100.100.100 > /dev/null 2>&1; then
        return 0
    fi
    return 1
}

get_ip() {
    tailscale ip -4 2>/dev/null || echo ""
}

get_peer_count() {
    tailscale status --json 2>/dev/null | jq '.Peer | length' 2>/dev/null || echo "0"
}

main() {
    local daemon_ok=false
    local status_ok=false
    local connectivity_ok=false
    local ip=""
    local peer_count=0
    
    if check_daemon; then
        daemon_ok=true
    fi
    
    if check_status; then
        status_ok=true
        ip=$(get_ip)
        peer_count=$(get_peer_count)
    fi
    
    if [ "$status_ok" = "true" ] && check_connectivity; then
        connectivity_ok=true
    fi
    
    if [ "$OUTPUT_JSON" = "true" ]; then
        cat <<EOF
{
  "daemon_running": $daemon_ok,
  "status_ok": $status_ok,
  "connectivity_ok": $connectivity_ok,
  "tailscale_ip": "$ip",
  "peer_count": $peer_count
}
EOF
    else
        echo "Tailscale Status"
        echo "================"
        echo "Daemon:        $([ "$daemon_ok" = "true" ] && echo "✓ running" || echo "✗ not running")"
        echo "State:         $([ "$status_ok" = "true" ] && echo "✓ connected" || echo "✗ not connected")"
        echo "Connectivity:  $([ "$connectivity_ok" = "true" ] && echo "✓ reachable" || echo "✗ unreachable")"
        if [ -n "$ip" ]; then
            echo "Tailscale IP:  $ip"
        fi
        echo "Peers:         $peer_count"
    fi
    
    if [ "$daemon_ok" = "true" ] && [ "$status_ok" = "true" ]; then
        return 0
    fi
    return 1
}

main
