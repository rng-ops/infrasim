#!/bin/sh
# generate-client-cert.sh - Generate client certificate for mTLS
#
# Usage: generate-client-cert.sh <client-name> [--node-id NODE_ID] [--output-dir DIR]

set -eu

CLIENT_NAME="${1:-}"
NODE_ID=""
OUTPUT_DIR=""

if [ -z "$CLIENT_NAME" ]; then
    echo "Usage: $0 <client-name> [--node-id NODE_ID] [--output-dir DIR]"
    exit 1
fi

shift
while [ $# -gt 0 ]; do
    case "$1" in
        --node-id)
            NODE_ID="$2"
            shift 2
            ;;
        --output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        *)
            shift
            ;;
    esac
done

# Use mtls-setup.sh for actual generation
exec /usr/local/bin/mtls-setup.sh client "$CLIENT_NAME" "$NODE_ID"
