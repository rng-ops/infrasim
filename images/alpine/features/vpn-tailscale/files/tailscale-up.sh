#!/bin/sh
# tailscale-up.sh - Initialize and authenticate Tailscale
#
# Usage: tailscale-up.sh [--auth-key KEY]

set -eu

CONFIG_FILE="${CONFIG_FILE:-/etc/infrasim/tailscale.conf}"

log() {
    logger -t "tailscale-up" -p daemon.info "$*"
    echo "[$(date -Iseconds)] $*"
}

error() {
    logger -t "tailscale-up" -p daemon.err "$*"
    echo "[$(date -Iseconds)] ERROR: $*" >&2
}

# Load configuration
AUTH_KEY=""
CONTROL_URL=""
HOSTNAME=""
ADVERTISE_ROUTES=""
ACCEPT_ROUTES="true"
EXIT_NODE="false"
SSH="false"
SHIELDS_UP="false"

if [ -f "$CONFIG_FILE" ]; then
    while IFS='=' read -r key value; do
        case "$key" in
            auth_key) AUTH_KEY="$value" ;;
            control_url) CONTROL_URL="$value" ;;
            hostname) HOSTNAME="$value" ;;
            advertise_routes) ADVERTISE_ROUTES="$value" ;;
            accept_routes) ACCEPT_ROUTES="$value" ;;
            exit_node) EXIT_NODE="$value" ;;
            ssh) SSH="$value" ;;
            shields_up) SHIELDS_UP="$value" ;;
        esac
    done < "$CONFIG_FILE"
fi

# Override with command line
while [ $# -gt 0 ]; do
    case "$1" in
        --auth-key)
            AUTH_KEY="$2"
            shift 2
            ;;
        *)
            shift
            ;;
    esac
done

# Build tailscale up command
build_up_args() {
    local args=""
    
    if [ -n "$AUTH_KEY" ]; then
        args="$args --authkey=$AUTH_KEY"
    fi
    
    if [ -n "$CONTROL_URL" ]; then
        args="$args --login-server=$CONTROL_URL"
    fi
    
    if [ -n "$HOSTNAME" ]; then
        args="$args --hostname=$HOSTNAME"
    fi
    
    if [ -n "$ADVERTISE_ROUTES" ]; then
        args="$args --advertise-routes=$ADVERTISE_ROUTES"
    fi
    
    if [ "$ACCEPT_ROUTES" = "true" ]; then
        args="$args --accept-routes"
    fi
    
    if [ "$EXIT_NODE" = "true" ]; then
        args="$args --advertise-exit-node"
    fi
    
    if [ "$SSH" = "true" ]; then
        args="$args --ssh"
    fi
    
    if [ "$SHIELDS_UP" = "true" ]; then
        args="$args --shields-up"
    fi
    
    echo "$args"
}

# Ensure tailscaled is running
ensure_daemon() {
    if ! rc-service tailscaled status > /dev/null 2>&1; then
        log "Starting tailscaled..."
        rc-service tailscaled start
        sleep 2
    fi
}

# Main
main() {
    ensure_daemon
    
    # Check current status
    local status
    status=$(tailscale status --json 2>/dev/null || echo '{"BackendState":"Unknown"}')
    local state
    state=$(echo "$status" | jq -r '.BackendState // "Unknown"')
    
    log "Current state: $state"
    
    case "$state" in
        Running)
            log "Tailscale already connected"
            tailscale status
            ;;
        
        NeedsLogin|Stopped)
            if [ -z "$AUTH_KEY" ]; then
                log "No auth key provided, generating login URL..."
                tailscale up $(build_up_args)
            else
                log "Authenticating with auth key..."
                tailscale up $(build_up_args)
            fi
            ;;
        
        *)
            log "Starting Tailscale..."
            tailscale up $(build_up_args)
            ;;
    esac
    
    # Wait for connection
    local retries=30
    while [ $retries -gt 0 ]; do
        if tailscale status > /dev/null 2>&1; then
            state=$(tailscale status --json | jq -r '.BackendState')
            if [ "$state" = "Running" ]; then
                log "Tailscale connected"
                tailscale status
                
                # Update node descriptor with Tailscale info
                update_node_descriptor
                
                return 0
            fi
        fi
        sleep 1
        retries=$((retries - 1))
    done
    
    error "Tailscale failed to connect"
    return 1
}

# Update node descriptor with Tailscale node key
update_node_descriptor() {
    local descriptor="/etc/infrasim/node-descriptor.json"
    
    if [ ! -f "$descriptor" ]; then
        return
    fi
    
    # Get Tailscale node key
    local node_key
    node_key=$(tailscale status --json | jq -r '.Self.PublicKey // empty')
    
    if [ -n "$node_key" ]; then
        local tmp
        tmp=$(mktemp)
        jq --arg key "$node_key" '.identity.tailscale_node_key = $key' "$descriptor" > "$tmp"
        mv "$tmp" "$descriptor"
        log "Updated node descriptor with Tailscale node key"
        
        # Remove old signature
        rm -f "${descriptor}.sig"
    fi
}

main
