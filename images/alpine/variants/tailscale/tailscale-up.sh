#!/bin/bash
# ============================================================================
# Tailscale Up Script
# ============================================================================
#
# This script brings up the Tailscale connection with the appropriate
# configuration for InfraSim nodes. It handles authentication and
# node registration with the Tailscale control plane.
#
# Environment Variables:
#   TAILSCALE_AUTH_KEY   - Pre-authorized key for headless login
#   TAILSCALE_HOSTNAME   - Override hostname (default: use system hostname)
#   TAILSCALE_TAGS       - Additional ACL tags
#   TAILSCALE_CONTROL    - Custom control server (for Headscale)
#
set -euo pipefail

LOG_FILE="/var/log/tailscale-up.log"
CONFIG_FILE="/etc/infrasim/tailscale/config.json"
STATE_DIR="/var/lib/tailscale"

# Logging
log() {
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*" | tee -a "$LOG_FILE"
}

log_error() {
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] ERROR: $*" | tee -a "$LOG_FILE" >&2
}

# Read configuration
read_config() {
    if [[ -f "$CONFIG_FILE" ]]; then
        jq -r "$1 // empty" "$CONFIG_FILE" 2>/dev/null || echo ""
    else
        echo ""
    fi
}

# Wait for tailscaled to be ready
wait_for_daemon() {
    log "Waiting for tailscaled..."
    local retries=0
    while ! tailscale status >/dev/null 2>&1; do
        sleep 2
        ((retries++))
        if [[ $retries -gt 30 ]]; then
            log_error "tailscaled not responding after 60s"
            return 1
        fi
    done
    log "tailscaled is ready"
}

# Build tailscale up command
build_up_command() {
    local cmd="tailscale up"
    
    # Authentication
    if [[ -n "${TAILSCALE_AUTH_KEY:-}" ]]; then
        cmd="$cmd --authkey=$TAILSCALE_AUTH_KEY"
    fi
    
    # Hostname
    local hostname="${TAILSCALE_HOSTNAME:-$(hostname)}"
    cmd="$cmd --hostname=$hostname"
    
    # Control URL (for Headscale)
    if [[ -n "${TAILSCALE_CONTROL:-}" ]]; then
        cmd="$cmd --login-server=$TAILSCALE_CONTROL"
    fi
    
    # Read config options
    local advertise_exit
    advertise_exit=$(read_config '.advertise_exit_node')
    if [[ "$advertise_exit" == "true" ]]; then
        cmd="$cmd --advertise-exit-node"
    fi
    
    local accept_dns
    accept_dns=$(read_config '.accept_dns')
    if [[ "$accept_dns" == "true" ]]; then
        cmd="$cmd --accept-dns"
    else
        cmd="$cmd --accept-dns=false"
    fi
    
    local accept_routes
    accept_routes=$(read_config '.accept_routes')
    if [[ "$accept_routes" == "true" ]]; then
        cmd="$cmd --accept-routes"
    fi
    
    local ssh_enabled
    ssh_enabled=$(read_config '.ssh')
    if [[ "$ssh_enabled" == "true" ]]; then
        cmd="$cmd --ssh"
    fi
    
    # Tags
    local tags
    tags=$(read_config '.tags | join(",")')
    if [[ -n "$tags" ]]; then
        cmd="$cmd --advertise-tags=$tags"
    fi
    if [[ -n "${TAILSCALE_TAGS:-}" ]]; then
        cmd="$cmd --advertise-tags=$TAILSCALE_TAGS"
    fi
    
    # Don't run interactively
    cmd="$cmd --reset"
    
    echo "$cmd"
}

# Bring up Tailscale
bring_up() {
    log "Bringing up Tailscale connection..."
    
    local cmd
    cmd=$(build_up_command)
    
    # Log command (hide auth key)
    log "Running: ${cmd/--authkey=*\ /--authkey=*** }"
    
    # Execute
    if eval "$cmd"; then
        log "Tailscale is up"
        
        # Log connection info
        local ip
        ip=$(tailscale ip -4 2>/dev/null || echo "unknown")
        log "Tailscale IP: $ip"
        
        local status
        status=$(tailscale status --json 2>/dev/null | jq -r '.BackendState // "unknown"')
        log "Backend state: $status"
        
        return 0
    else
        log_error "Failed to bring up Tailscale"
        return 1
    fi
}

# Report to InfraSim telemetry
report_status() {
    local telemetry_log="/var/log/infrasim-telemetry.log"
    
    # Get Tailscale status
    local status_json
    status_json=$(tailscale status --json 2>/dev/null || echo '{}')
    
    local ts_ip
    ts_ip=$(tailscale ip -4 2>/dev/null || echo "")
    
    local peer_count
    peer_count=$(echo "$status_json" | jq '.Peer | length // 0')
    
    # Log to telemetry
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] TAILSCALE_UP: ip=$ts_ip peers=$peer_count" >> "$telemetry_log"
}

# Health check
health_check() {
    if tailscale status >/dev/null 2>&1; then
        local state
        state=$(tailscale status --json 2>/dev/null | jq -r '.BackendState // "unknown"')
        if [[ "$state" == "Running" ]]; then
            return 0
        fi
    fi
    return 1
}

# Main
main() {
    log "=== Tailscale Up Script ==="
    log "Hostname: $(hostname)"
    log "Config file: $CONFIG_FILE"
    
    # Check for auth key
    if [[ -z "${TAILSCALE_AUTH_KEY:-}" ]]; then
        log "WARNING: TAILSCALE_AUTH_KEY not set. Interactive login may be required."
    fi
    
    # Wait for daemon
    wait_for_daemon || exit 1
    
    # Check if already connected
    if health_check; then
        log "Tailscale already connected"
        report_status
        exit 0
    fi
    
    # Bring up connection
    if bring_up; then
        report_status
        log "=== Tailscale Up Complete ==="
        exit 0
    else
        log_error "Failed to connect to Tailscale"
        exit 1
    fi
}

# Run if not sourced
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
