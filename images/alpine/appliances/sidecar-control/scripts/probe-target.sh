#!/bin/sh
# probe-target.sh - Network probe target VM
#
# Usage: probe-target.sh <target-ip> [--port PORT] [--detailed]

set -eu

TARGET="${1:-}"
PORT="${PORT:-22}"
DETAILED=false

if [ -z "$TARGET" ]; then
    echo "Usage: $0 <target-ip> [--port PORT] [--detailed]"
    exit 1
fi

shift
while [ $# -gt 0 ]; do
    case "$1" in
        --port)
            PORT="$2"
            shift 2
            ;;
        --detailed)
            DETAILED=true
            shift
            ;;
        *)
            shift
            ;;
    esac
done

log() {
    echo "[$(date -Iseconds)] $*"
}

# Basic connectivity
check_ping() {
    log "Checking ICMP connectivity..."
    if ping -c 3 -W 5 "$TARGET" > /dev/null 2>&1; then
        log "✓ ICMP: reachable"
        return 0
    else
        log "✗ ICMP: unreachable"
        return 1
    fi
}

# SSH port
check_ssh() {
    log "Checking SSH port $PORT..."
    if nc -z -w 5 "$TARGET" "$PORT" 2>/dev/null; then
        log "✓ SSH: port $PORT open"
        return 0
    else
        log "✗ SSH: port $PORT closed"
        return 1
    fi
}

# WireGuard port
check_wireguard() {
    log "Checking WireGuard port 51820..."
    # UDP check is less reliable, use nmap if available
    if command -v nmap > /dev/null 2>&1; then
        if nmap -sU -p 51820 --open "$TARGET" 2>/dev/null | grep -q "51820/udp"; then
            log "✓ WireGuard: port 51820 open"
            return 0
        fi
    fi
    log "? WireGuard: cannot verify UDP port"
    return 0
}

# Tailscale port
check_tailscale() {
    log "Checking Tailscale port 41641..."
    if command -v nmap > /dev/null 2>&1; then
        if nmap -sU -p 41641 --open "$TARGET" 2>/dev/null | grep -q "41641/udp"; then
            log "✓ Tailscale: port 41641 open"
            return 0
        fi
    fi
    log "? Tailscale: cannot verify UDP port"
    return 0
}

# mTLS port
check_mtls() {
    log "Checking mTLS port 8443..."
    if nc -z -w 5 "$TARGET" 8443 2>/dev/null; then
        log "✓ mTLS: port 8443 open"
        return 0
    else
        log "- mTLS: port 8443 not open (optional)"
        return 0
    fi
}

# Detailed scan
detailed_scan() {
    if ! command -v nmap > /dev/null 2>&1; then
        log "nmap not available for detailed scan"
        return
    fi
    
    log "Running detailed port scan..."
    nmap -sT -sU -p 22,51820,41641,8443,5353 "$TARGET" 2>/dev/null
}

# Main
main() {
    log "Probing target: $TARGET"
    log ""
    
    local errors=0
    
    check_ping || errors=$((errors + 1))
    check_ssh || errors=$((errors + 1))
    check_wireguard
    check_tailscale
    check_mtls
    
    if [ "$DETAILED" = "true" ]; then
        log ""
        detailed_scan
    fi
    
    log ""
    if [ "$errors" -eq 0 ]; then
        log "Probe complete: target reachable"
        exit 0
    else
        log "Probe complete: $errors issue(s) found"
        exit 1
    fi
}

main
