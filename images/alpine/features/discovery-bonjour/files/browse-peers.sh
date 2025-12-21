#!/bin/sh
# browse-peers.sh - Browse for infrasim peers via mDNS
#
# Usage: browse-peers.sh [--json] [--timeout SECONDS]

set -eu

TIMEOUT="${TIMEOUT:-5}"
OUTPUT_JSON=false

while [ $# -gt 0 ]; do
    case "$1" in
        --json)
            OUTPUT_JSON=true
            shift
            ;;
        --timeout)
            TIMEOUT="$2"
            shift 2
            ;;
        *)
            shift
            ;;
    esac
done

# Browse for infrasim services
browse_peers() {
    local services
    services=$(timeout "$TIMEOUT" avahi-browse -tpr _infrasim._udp 2>/dev/null || true)
    
    if [ -z "$services" ]; then
        if [ "$OUTPUT_JSON" = "true" ]; then
            echo "[]"
        else
            echo "No peers found"
        fi
        return
    fi
    
    if [ "$OUTPUT_JSON" = "true" ]; then
        # Parse avahi-browse output into JSON
        echo "$services" | awk -F';' '
        BEGIN { print "["; first=1 }
        /^=/ {
            if (!first) print ","
            first=0
            printf "  {\"interface\": \"%s\", \"protocol\": \"%s\", \"name\": \"%s\", \"type\": \"%s\", \"domain\": \"%s\", \"hostname\": \"%s\", \"address\": \"%s\", \"port\": %s, \"txt\": \"%s\"}", $2, $3, $4, $5, $6, $7, $8, $9, $10
        }
        END { print "\n]" }
        '
    else
        echo "Discovered peers:"
        echo "$services" | grep "^=" | while IFS=';' read -r _ iface proto name type domain hostname addr port txt; do
            echo "  - $hostname ($addr:$port) on $iface"
            if [ -n "$txt" ]; then
                echo "    TXT: $txt"
            fi
        done
    fi
}

browse_peers
