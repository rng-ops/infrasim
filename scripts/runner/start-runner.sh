#!/bin/bash
# ============================================================================
# start-runner.sh - Start the containerized GitHub Actions runner
# ============================================================================
#
# This script starts the secure containerized runner with Vault.
#
# Usage:
#   ./start-runner.sh              # Start with existing Vault secrets
#   ./start-runner.sh --init       # Initialize Vault with GitHub token
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }
log_step()  { echo -e "${BLUE}[STEP]${NC} $*"; }

# =============================================================================
# Check prerequisites
# =============================================================================
check_prerequisites() {
    log_step "Checking prerequisites..."
    
    if ! command -v docker &>/dev/null; then
        log_error "Docker not found. Install Docker Desktop or Colima."
        exit 1
    fi
    
    if ! docker info &>/dev/null; then
        log_error "Docker is not running."
        exit 1
    fi
    
    if ! command -v docker compose &>/dev/null && ! docker compose version &>/dev/null; then
        log_error "Docker Compose not found."
        exit 1
    fi
    
    log_info "Prerequisites OK"
}

# =============================================================================
# Initialize Vault with secrets
# =============================================================================
init_vault() {
    log_step "Initializing Vault with secrets..."
    
    # Check for GitHub token
    if [[ -z "${GITHUB_TOKEN:-}" ]]; then
        log_error "GITHUB_TOKEN environment variable not set"
        log_info ""
        log_info "Set your GitHub token:"
        log_info "  export GITHUB_TOKEN=ghp_xxxx"
        exit 1
    fi
    
    # Start Vault first
    docker compose up -d vault
    
    # Wait for Vault to be ready
    log_info "Waiting for Vault to be ready..."
    local retries=60
    while ! docker compose exec -T vault vault status &>/dev/null; do
        retries=$((retries - 1))
        if [[ $retries -le 0 ]]; then
            log_error "Vault did not become ready"
            log_info "Check logs: docker compose logs vault"
            exit 1
        fi
        echo -n "."
        sleep 2
    done
    echo ""
    
    # Store GitHub token in Vault
    log_info "Storing GitHub token in Vault..."
    docker compose exec -T vault vault kv put secret/github token="$GITHUB_TOKEN"
    
    log_info "Vault initialized with secrets"
}

# =============================================================================
# Start runner
# =============================================================================
start_runner() {
    log_step "Starting containerized runner..."
    
    # Build and start
    docker compose up -d --build
    
    log_info ""
    log_info "Runner started!"
    log_info ""
    log_info "View logs:"
    log_info "  docker compose logs -f runner"
    log_info ""
    log_info "View Vault UI:"
    log_info "  http://localhost:8200 (token: infrasim-dev-token)"
    log_info ""
    log_info "Stop runner:"
    log_info "  docker compose down"
}

# =============================================================================
# Main
# =============================================================================
main() {
    local init_mode=false
    
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --init|-i)
                init_mode=true
                shift
                ;;
            --help|-h)
                echo "Usage: $0 [--init]"
                echo ""
                echo "Options:"
                echo "  --init    Initialize Vault with GITHUB_TOKEN"
                echo ""
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done
    
    echo ""
    echo "================================================"
    echo "  InfraSim Containerized GitHub Actions Runner"
    echo "================================================"
    echo ""
    
    check_prerequisites
    
    if [[ "$init_mode" == "true" ]]; then
        init_vault
    fi
    
    start_runner
}

main "$@"
