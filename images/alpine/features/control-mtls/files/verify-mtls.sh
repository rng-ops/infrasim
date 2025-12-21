#!/bin/sh
# verify-mtls.sh - Verify mTLS configuration and connectivity
#
# Usage: verify-mtls.sh [endpoint]

set -eu

MTLS_DIR="${MTLS_DIR:-/etc/infrasim/mtls}"
CLIENT_CERT="${CLIENT_CERT:-${MTLS_DIR}/clients/default/client.crt}"
CLIENT_KEY="${CLIENT_KEY:-${MTLS_DIR}/clients/default/client.key}"
CA_CERT="${CA_CERT:-${MTLS_DIR}/ca/ca.crt}"

log() {
    echo "[$(date -Iseconds)] $*"
}

error() {
    echo "[$(date -Iseconds)] ERROR: $*" >&2
}

# Check if required files exist
check_files() {
    local missing=0
    
    for file in "$CA_CERT" "$CLIENT_CERT" "$CLIENT_KEY"; do
        if [ ! -f "$file" ]; then
            error "Missing: $file"
            missing=1
        fi
    done
    
    return $missing
}

# Verify certificate chain
verify_chain() {
    log "Verifying certificate chain..."
    
    if openssl verify -CAfile "$CA_CERT" "$CLIENT_CERT" > /dev/null 2>&1; then
        log "✓ Client certificate verified against CA"
        return 0
    else
        error "✗ Client certificate verification failed"
        return 1
    fi
}

# Check certificate expiry
check_expiry() {
    log "Checking certificate expiry..."
    
    local now
    now=$(date +%s)
    
    local cert_end
    cert_end=$(openssl x509 -in "$CLIENT_CERT" -noout -enddate | cut -d= -f2)
    local cert_end_ts
    cert_end_ts=$(date -d "$cert_end" +%s 2>/dev/null || date -j -f "%b %d %T %Y %Z" "$cert_end" +%s 2>/dev/null)
    
    local days_left
    days_left=$(( (cert_end_ts - now) / 86400 ))
    
    if [ "$days_left" -lt 0 ]; then
        error "✗ Certificate expired $((days_left * -1)) days ago"
        return 1
    elif [ "$days_left" -lt 30 ]; then
        log "⚠ Certificate expires in $days_left days"
        return 0
    else
        log "✓ Certificate valid for $days_left days"
        return 0
    fi
}

# Test mTLS connection
test_connection() {
    local endpoint="$1"
    
    log "Testing mTLS connection to $endpoint..."
    
    # Use curl with client certificate
    local response
    if response=$(curl -s -o /dev/null -w "%{http_code}" \
        --cacert "$CA_CERT" \
        --cert "$CLIENT_CERT" \
        --key "$CLIENT_KEY" \
        --connect-timeout 10 \
        "$endpoint" 2>&1); then
        
        if [ "$response" = "200" ] || [ "$response" = "401" ] || [ "$response" = "403" ]; then
            log "✓ mTLS handshake successful (HTTP $response)"
            return 0
        else
            error "✗ Unexpected response: HTTP $response"
            return 1
        fi
    else
        error "✗ Connection failed: $response"
        return 1
    fi
}

# Main
main() {
    local endpoint="${1:-}"
    local errors=0
    
    log "mTLS Verification"
    log "================"
    
    if ! check_files; then
        error "Required files missing"
        exit 1
    fi
    
    verify_chain || errors=$((errors + 1))
    check_expiry || errors=$((errors + 1))
    
    if [ -n "$endpoint" ]; then
        test_connection "$endpoint" || errors=$((errors + 1))
    fi
    
    echo ""
    if [ "$errors" -eq 0 ]; then
        log "All checks passed"
        exit 0
    else
        error "$errors check(s) failed"
        exit 1
    fi
}

main "$@"
