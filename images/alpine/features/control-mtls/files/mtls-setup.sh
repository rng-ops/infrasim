#!/bin/sh
# mtls-setup.sh - Set up mTLS certificates for control plane
#
# Usage: mtls-setup.sh [ca|server|client] [options]

set -eu

MTLS_DIR="${MTLS_DIR:-/etc/infrasim/mtls}"
CA_DAYS="${CA_DAYS:-3650}"
CERT_DAYS="${CERT_DAYS:-365}"

log() {
    logger -t "mtls-setup" -p daemon.info "$*"
    echo "[$(date -Iseconds)] $*"
}

error() {
    logger -t "mtls-setup" -p daemon.err "$*"
    echo "[$(date -Iseconds)] ERROR: $*" >&2
}

# Create CA certificate
create_ca() {
    local ca_name="${1:-infrasim-ca}"
    local ca_dir="${MTLS_DIR}/ca"
    
    mkdir -p "$ca_dir"
    chmod 700 "$ca_dir"
    
    log "Creating CA certificate: $ca_name"
    
    # Generate CA private key
    openssl genpkey -algorithm RSA -out "${ca_dir}/ca.key" -pkeyopt rsa_keygen_bits:4096
    chmod 600 "${ca_dir}/ca.key"
    
    # Generate CA certificate
    openssl req -new -x509 \
        -key "${ca_dir}/ca.key" \
        -out "${ca_dir}/ca.crt" \
        -days "$CA_DAYS" \
        -subj "/CN=${ca_name}/O=infrasim/OU=control-plane"
    
    chmod 644 "${ca_dir}/ca.crt"
    
    # Create serial file
    echo "1000" > "${ca_dir}/serial"
    
    # Create index file
    touch "${ca_dir}/index.txt"
    
    log "CA certificate created at ${ca_dir}/ca.crt"
    echo "${ca_dir}/ca.crt"
}

# Create server certificate
create_server_cert() {
    local server_name="${1:-$(hostname)}"
    local ca_dir="${MTLS_DIR}/ca"
    local server_dir="${MTLS_DIR}/server"
    
    if [ ! -f "${ca_dir}/ca.key" ]; then
        error "CA not found. Run: $0 ca first"
        exit 1
    fi
    
    mkdir -p "$server_dir"
    chmod 700 "$server_dir"
    
    log "Creating server certificate: $server_name"
    
    # Generate server private key
    openssl genpkey -algorithm RSA -out "${server_dir}/server.key" -pkeyopt rsa_keygen_bits:2048
    chmod 600 "${server_dir}/server.key"
    
    # Create CSR config with SANs
    cat > "${server_dir}/server.conf" <<EOF
[req]
distinguished_name = req_dn
req_extensions = req_ext
prompt = no

[req_dn]
CN = ${server_name}
O = infrasim
OU = control-plane

[req_ext]
subjectAltName = @alt_names
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth

[alt_names]
DNS.1 = ${server_name}
DNS.2 = localhost
IP.1 = 127.0.0.1
IP.2 = ::1
EOF
    
    # Generate CSR
    openssl req -new \
        -key "${server_dir}/server.key" \
        -out "${server_dir}/server.csr" \
        -config "${server_dir}/server.conf"
    
    # Sign with CA
    openssl x509 -req \
        -in "${server_dir}/server.csr" \
        -CA "${ca_dir}/ca.crt" \
        -CAkey "${ca_dir}/ca.key" \
        -CAserial "${ca_dir}/serial" \
        -out "${server_dir}/server.crt" \
        -days "$CERT_DAYS" \
        -extfile "${server_dir}/server.conf" \
        -extensions req_ext
    
    chmod 644 "${server_dir}/server.crt"
    
    # Clean up
    rm -f "${server_dir}/server.csr" "${server_dir}/server.conf"
    
    log "Server certificate created at ${server_dir}/server.crt"
    echo "${server_dir}/server.crt"
}

# Create client certificate
create_client_cert() {
    local client_name="${1:-client}"
    local node_id="${2:-}"
    local ca_dir="${MTLS_DIR}/ca"
    local client_dir="${MTLS_DIR}/clients/${client_name}"
    
    if [ ! -f "${ca_dir}/ca.key" ]; then
        error "CA not found. Run: $0 ca first"
        exit 1
    fi
    
    mkdir -p "$client_dir"
    chmod 700 "$client_dir"
    
    log "Creating client certificate: $client_name"
    
    # Generate client private key
    openssl genpkey -algorithm RSA -out "${client_dir}/client.key" -pkeyopt rsa_keygen_bits:2048
    chmod 600 "${client_dir}/client.key"
    
    # Create CSR config
    # Include node_id in OU if provided (for binding cert to node)
    local ou="control-plane-client"
    if [ -n "$node_id" ]; then
        ou="node:${node_id}"
    fi
    
    cat > "${client_dir}/client.conf" <<EOF
[req]
distinguished_name = req_dn
req_extensions = req_ext
prompt = no

[req_dn]
CN = ${client_name}
O = infrasim
OU = ${ou}

[req_ext]
keyUsage = digitalSignature
extendedKeyUsage = clientAuth
EOF
    
    # Generate CSR
    openssl req -new \
        -key "${client_dir}/client.key" \
        -out "${client_dir}/client.csr" \
        -config "${client_dir}/client.conf"
    
    # Sign with CA
    openssl x509 -req \
        -in "${client_dir}/client.csr" \
        -CA "${ca_dir}/ca.crt" \
        -CAkey "${ca_dir}/ca.key" \
        -CAserial "${ca_dir}/serial" \
        -out "${client_dir}/client.crt" \
        -days "$CERT_DAYS" \
        -extfile "${client_dir}/client.conf" \
        -extensions req_ext
    
    chmod 644 "${client_dir}/client.crt"
    
    # Create combined PEM for convenience
    cat "${client_dir}/client.crt" "${client_dir}/client.key" > "${client_dir}/client.pem"
    chmod 600 "${client_dir}/client.pem"
    
    # Copy CA cert for client use
    cp "${ca_dir}/ca.crt" "${client_dir}/ca.crt"
    
    # Clean up
    rm -f "${client_dir}/client.csr" "${client_dir}/client.conf"
    
    log "Client certificate created at ${client_dir}/client.crt"
    echo "${client_dir}/client.crt"
}

# Verify certificate chain
verify_cert() {
    local cert_path="$1"
    local ca_path="${2:-${MTLS_DIR}/ca/ca.crt}"
    
    if [ ! -f "$cert_path" ]; then
        error "Certificate not found: $cert_path"
        return 1
    fi
    
    if [ ! -f "$ca_path" ]; then
        error "CA certificate not found: $ca_path"
        return 1
    fi
    
    if openssl verify -CAfile "$ca_path" "$cert_path" > /dev/null 2>&1; then
        log "Certificate verified: $cert_path"
        
        # Print certificate info
        openssl x509 -in "$cert_path" -noout -subject -issuer -dates
        return 0
    else
        error "Certificate verification failed: $cert_path"
        return 1
    fi
}

# Main
case "${1:-help}" in
    ca)
        shift
        create_ca "${1:-infrasim-ca}"
        ;;
    
    server)
        shift
        create_server_cert "${1:-$(hostname)}"
        ;;
    
    client)
        shift
        create_client_cert "${1:-client}" "${2:-}"
        ;;
    
    verify)
        shift
        verify_cert "${1:-}" "${2:-}"
        ;;
    
    *)
        echo "Usage: $0 <command> [options]"
        echo ""
        echo "Commands:"
        echo "  ca [name]                    Create CA certificate"
        echo "  server [hostname]            Create server certificate"
        echo "  client <name> [node_id]      Create client certificate"
        echo "  verify <cert> [ca]           Verify certificate against CA"
        echo ""
        echo "Environment:"
        echo "  MTLS_DIR    Base directory (default: /etc/infrasim/mtls)"
        echo "  CA_DAYS     CA certificate validity (default: 3650)"
        echo "  CERT_DAYS   Certificate validity (default: 365)"
        exit 1
        ;;
esac
