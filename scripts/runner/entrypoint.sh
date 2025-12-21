#!/bin/bash
# ============================================================================
# entrypoint.sh - GitHub Actions Runner Entrypoint
# ============================================================================
#
# This script:
# 1. Fetches secrets from Vault
# 2. Registers the runner with GitHub
# 3. Starts the runner
#
set -euo pipefail

log() {
    echo "[$(date -Iseconds)] $*"
}

error() {
    echo "[$(date -Iseconds)] ERROR: $*" >&2
    exit 1
}

# =============================================================================
# Fetch secrets from Vault
# =============================================================================
fetch_secrets() {
    log "Fetching secrets from Vault..."
    
    # Wait for Vault to be ready
    local retries=30
    while ! vault status &>/dev/null; do
        retries=$((retries - 1))
        if [[ $retries -le 0 ]]; then
            error "Vault not available after 30 seconds"
        fi
        log "Waiting for Vault... ($retries attempts left)"
        sleep 1
    done
    
    # Fetch GitHub token from Vault
    GITHUB_TOKEN=$(vault kv get -field=token secret/github 2>/dev/null || echo "")
    
    if [[ -z "$GITHUB_TOKEN" ]]; then
        # Check if token is in environment (for initial setup)
        if [[ -n "${INIT_GITHUB_TOKEN:-}" ]]; then
            log "Using INIT_GITHUB_TOKEN for initial setup"
            GITHUB_TOKEN="$INIT_GITHUB_TOKEN"
            
            # Store in Vault for future use
            vault kv put secret/github token="$GITHUB_TOKEN"
            log "Stored GitHub token in Vault"
        else
            error "No GitHub token found. Set INIT_GITHUB_TOKEN or store in Vault at secret/github"
        fi
    fi
    
    export GITHUB_TOKEN
    log "Secrets loaded from Vault"
}

# =============================================================================
# Get runner registration token
# =============================================================================
get_registration_token() {
    log "Getting runner registration token..."
    
    local response
    response=$(curl -s -X POST \
        -H "Accept: application/vnd.github+json" \
        -H "Authorization: Bearer ${GITHUB_TOKEN}" \
        "https://api.github.com/repos/${GITHUB_ORG}/${GITHUB_REPO}/actions/runners/registration-token")
    
    REGISTRATION_TOKEN=$(echo "$response" | jq -r '.token')
    
    if [[ -z "$REGISTRATION_TOKEN" ]] || [[ "$REGISTRATION_TOKEN" == "null" ]]; then
        error "Failed to get registration token: $response"
    fi
    
    log "Registration token obtained"
}

# =============================================================================
# Configure runner
# =============================================================================
configure_runner() {
    log "Configuring runner..."
    
    # Check if already configured
    if [[ -f ".runner" ]]; then
        log "Runner already configured, checking registration..."
        
        # Try to remove old registration
        ./config.sh remove --token "$REGISTRATION_TOKEN" 2>/dev/null || true
    fi
    
    # Configure
    ./config.sh \
        --url "https://github.com/${GITHUB_ORG}/${GITHUB_REPO}" \
        --token "$REGISTRATION_TOKEN" \
        --name "${RUNNER_NAME:-infrasim-docker-runner}" \
        --labels "${RUNNER_LABELS:-self-hosted,Linux,ARM64,docker}" \
        --work "${RUNNER_WORKDIR:-_work}" \
        --replace \
        --unattended
    
    log "Runner configured: ${RUNNER_NAME}"
}

# =============================================================================
# Cleanup on exit
# =============================================================================
cleanup() {
    log "Cleaning up..."
    
    if [[ -n "${REGISTRATION_TOKEN:-}" ]]; then
        ./config.sh remove --token "$REGISTRATION_TOKEN" 2>/dev/null || true
    fi
}

trap cleanup EXIT

# =============================================================================
# Main
# =============================================================================
main() {
    log "Starting InfraSim GitHub Actions Runner"
    log "  Org: ${GITHUB_ORG}"
    log "  Repo: ${GITHUB_REPO}"
    log "  Runner: ${RUNNER_NAME:-infrasim-docker-runner}"
    
    fetch_secrets
    get_registration_token
    configure_runner
    
    log "Starting runner..."
    exec ./run.sh
}

main "$@"
