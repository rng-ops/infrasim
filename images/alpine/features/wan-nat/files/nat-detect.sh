#!/bin/sh
# nat-detect.sh - Detect NAT type and public endpoint
#
# Usage: nat-detect.sh [--json]

set -eu

CONFIG_FILE="${CONFIG_FILE:-/etc/infrasim/nat.conf}"
OUTPUT_JSON=false

if [ "${1:-}" = "--json" ]; then
    OUTPUT_JSON=true
fi

# Load config
STUN_SERVERS="stun.cloudflare.com:3478 stun.l.google.com:19302"
if [ -f "$CONFIG_FILE" ]; then
    STUN_SERVERS=$(grep "^stun_servers=" "$CONFIG_FILE" | cut -d= -f2- | tr ',' ' ' || echo "$STUN_SERVERS")
fi

log() {
    if [ "$OUTPUT_JSON" = "false" ]; then
        echo "$*"
    fi
}

# Probe STUN server
probe_stun() {
    local server="$1"
    local host="${server%:*}"
    local port="${server#*:}"
    
    # Use stun-client if available
    if command -v stun > /dev/null 2>&1; then
        local result
        result=$(stun "$host" "$port" 2>&1) || true
        
        # Parse result
        local mapped_ip=""
        local mapped_port=""
        local nat_type=""
        
        mapped_ip=$(echo "$result" | grep -o 'MappedAddress.*' | head -1 | awk '{print $2}' | cut -d: -f1)
        mapped_port=$(echo "$result" | grep -o 'MappedAddress.*' | head -1 | awk '{print $2}' | cut -d: -f2)
        nat_type=$(echo "$result" | grep -o 'NAT Type:.*' | cut -d: -f2 | tr -d ' ')
        
        if [ -n "$mapped_ip" ]; then
            echo "$mapped_ip $mapped_port $nat_type $server"
            return 0
        fi
    fi
    
    return 1
}

# Detect NAT type by comparing mapped addresses from multiple STUN servers
detect_nat_type() {
    local results=""
    local first_ip=""
    local first_port=""
    local successful=0
    
    for server in $STUN_SERVERS; do
        local result
        if result=$(probe_stun "$server" 2>/dev/null); then
            local ip=$(echo "$result" | awk '{print $1}')
            local port=$(echo "$result" | awk '{print $2}')
            local nat_type=$(echo "$result" | awk '{print $3}')
            
            if [ -z "$first_ip" ]; then
                first_ip="$ip"
                first_port="$port"
            fi
            
            results="${results}${result}\n"
            successful=$((successful + 1))
        fi
    done
    
    if [ "$successful" -eq 0 ]; then
        if [ "$OUTPUT_JSON" = "true" ]; then
            echo '{"success":false,"error":"No STUN servers reachable"}'
        else
            echo "ERROR: No STUN servers reachable"
        fi
        return 1
    fi
    
    # Analyze results
    local unique_ips
    unique_ips=$(echo -e "$results" | awk '{print $1}' | sort -u | wc -l)
    
    local unique_ports
    unique_ports=$(echo -e "$results" | awk '{print $2}' | sort -u | wc -l)
    
    local nat_classification="unknown"
    
    if [ "$unique_ips" -eq 1 ] && [ "$unique_ports" -eq 1 ]; then
        nat_classification="endpoint-independent"
    elif [ "$unique_ips" -eq 1 ] && [ "$unique_ports" -gt 1 ]; then
        nat_classification="address-dependent"
    elif [ "$unique_ips" -gt 1 ]; then
        nat_classification="address-and-port-dependent"
    fi
    
    if [ "$OUTPUT_JSON" = "true" ]; then
        cat <<EOF
{
  "success": true,
  "public_ip": "$first_ip",
  "public_port": $first_port,
  "nat_type": "$nat_classification",
  "stun_servers_probed": $successful,
  "unique_ips": $unique_ips,
  "unique_ports": $unique_ports
}
EOF
    else
        echo "Public IP: $first_ip"
        echo "Public Port: $first_port"
        echo "NAT Type: $nat_classification"
        echo "STUN Servers Probed: $successful"
    fi
}

detect_nat_type
