#!/bin/bash
# ============================================================================
# telemetry-agent.sh - InfraSim Telemetry Agent Stub
# ============================================================================
#
# This is a SIMULATION/LOGGING ONLY placeholder for future LoRaWAN gateway
# integration. It does NOT perform any real radio transmission.
#
# Purpose:
# - Listen on a UDP socket (simulating LoRaWAN packet forwarder port)
# - Log received frames for testing/debugging
# - Provide a hook for future real LoRaWAN integration
#
# IMPORTANT: This script is strictly for simulation and telemetry logging.
# It does not include any SDR drivers or radio transmission code.
#
set -euo pipefail

CONFIG_FILE="${TELEMETRY_CONFIG:-/etc/infrasim/telemetry/config.json}"
LOG_FILE="${TELEMETRY_LOG:-/var/log/infrasim-telemetry.log}"
PID_FILE="/var/run/infrasim-telemetry.pid"

# Default values (overridden by config file)
GATEWAY_PORT=1700
SIMULATION_MODE=true

# =============================================================================
# Logging
# =============================================================================
log() {
    local level="$1"
    shift
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] [$level] $*" >> "$LOG_FILE"
}

log_info()  { log "INFO" "$@"; }
log_warn()  { log "WARN" "$@"; }
log_error() { log "ERROR" "$@"; }

# =============================================================================
# Load configuration
# =============================================================================
load_config() {
    if [[ -f "$CONFIG_FILE" ]]; then
        GATEWAY_PORT=$(jq -r '.gateway_port // 1700' "$CONFIG_FILE")
        SIMULATION_MODE=$(jq -r '.simulation_mode // true' "$CONFIG_FILE")
        log_info "Loaded config from $CONFIG_FILE"
    else
        log_warn "Config file not found: $CONFIG_FILE (using defaults)"
    fi
}

# =============================================================================
# Frame handler (simulation only)
# =============================================================================
handle_frame() {
    local frame="$1"
    local timestamp=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    
    # Log the frame
    log_info "RX frame: $frame"
    
    # In simulation mode, we just log
    # In future real mode, this would forward to a LoRaWAN network server
    if [[ "$SIMULATION_MODE" == "true" ]]; then
        # Parse Semtech UDP protocol if applicable
        # For now, just log raw hex
        echo "$timestamp|RX|$(echo -n "$frame" | xxd -p | tr -d '\n')" >> "${LOG_FILE%.log}.frames.log"
    fi
}

# =============================================================================
# UDP listener (simulation)
# =============================================================================
start_listener() {
    log_info "Starting telemetry agent on UDP port $GATEWAY_PORT"
    log_info "Mode: $([ "$SIMULATION_MODE" == "true" ] && echo "SIMULATION" || echo "FORWARDING")"
    
    # Write PID file
    echo $$ > "$PID_FILE"
    
    # Use socat or netcat to listen on UDP
    if command -v socat &>/dev/null; then
        socat -u UDP-LISTEN:$GATEWAY_PORT,reuseaddr,fork SYSTEM:"while read line; do echo \"\$line\"; done" 2>/dev/null | \
        while IFS= read -r frame; do
            handle_frame "$frame"
        done
    elif command -v nc &>/dev/null; then
        # netcat-based listener (less robust but more portable)
        while true; do
            nc -lu -p "$GATEWAY_PORT" -w 1 2>/dev/null | while IFS= read -r frame; do
                handle_frame "$frame"
            done
            sleep 1
        done
    else
        log_error "No UDP listener available (install socat or netcat)"
        exit 1
    fi
}

# =============================================================================
# Signal handlers
# =============================================================================
cleanup() {
    log_info "Shutting down telemetry agent"
    rm -f "$PID_FILE"
    exit 0
}

trap cleanup SIGTERM SIGINT

# =============================================================================
# Main
# =============================================================================
main() {
    mkdir -p "$(dirname "$LOG_FILE")"
    
    log_info "InfraSim Telemetry Agent starting..."
    log_info "NOTE: This is a SIMULATION ONLY. No real radio transmission occurs."
    
    load_config
    start_listener
}

case "${1:-start}" in
    start)
        main
        ;;
    stop)
        if [[ -f "$PID_FILE" ]]; then
            kill "$(cat "$PID_FILE")" 2>/dev/null || true
            rm -f "$PID_FILE"
        fi
        ;;
    status)
        if [[ -f "$PID_FILE" ]] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
            echo "Running (PID: $(cat "$PID_FILE"))"
        else
            echo "Stopped"
        fi
        ;;
    *)
        echo "Usage: $0 {start|stop|status}"
        exit 1
        ;;
esac
