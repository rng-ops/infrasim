#!/bin/bash
# ============================================================================
# setup-runner.sh - Set up a self-hosted GitHub Actions runner on macOS
# ============================================================================
#
# This script sets up a GitHub Actions runner on your local macOS machine.
# The runner will be used for building Alpine images with proper QEMU/HVF
# acceleration and Docker support.
#
# Prerequisites:
#   - macOS with Apple Silicon (M1/M2/M3)
#   - Docker Desktop or Colima installed
#   - GitHub personal access token with repo scope
#
# Usage:
#   ./setup-runner.sh
#
# Environment variables:
#   GITHUB_TOKEN    - Personal access token (required)
#   RUNNER_NAME     - Runner name (default: hostname)
#   RUNNER_LABELS   - Comma-separated labels (default: self-hosted,macOS,ARM64,docker)
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Configuration
GITHUB_ORG="${GITHUB_ORG:-rng-ops}"
GITHUB_REPO="${GITHUB_REPO:-infrasim}"
RUNNER_DIR="${RUNNER_DIR:-$HOME/actions-runner}"
RUNNER_NAME="${RUNNER_NAME:-$(hostname -s)}"
RUNNER_LABELS="${RUNNER_LABELS:-self-hosted,macOS,ARM64,docker,qemu}"
RUNNER_VERSION="${RUNNER_VERSION:-2.311.0}"

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
# Preflight checks
# =============================================================================
preflight_checks() {
    log_step "Running preflight checks..."
    
    # Check macOS
    if [[ "$(uname -s)" != "Darwin" ]]; then
        log_error "This script is for macOS only"
        exit 1
    fi
    
    # Check architecture
    if [[ "$(uname -m)" != "arm64" ]]; then
        log_warn "Not running on ARM64 - HVF acceleration won't be available"
    fi
    
    # Check Docker
    if ! command -v docker &>/dev/null; then
        log_error "Docker not found. Install Docker Desktop or Colima first."
        log_info "  brew install --cask docker"
        log_info "  # or"
        log_info "  brew install colima && colima start"
        exit 1
    fi
    
    if ! docker info &>/dev/null; then
        log_error "Docker is not running. Start Docker Desktop or Colima."
        exit 1
    fi
    log_info "Docker: $(docker --version)"
    
    # Check QEMU
    if ! command -v qemu-system-aarch64 &>/dev/null; then
        log_warn "QEMU not found. Installing..."
        brew install qemu
    fi
    log_info "QEMU: $(qemu-system-aarch64 --version | head -1)"
    
    # Check GitHub token
    if [[ -z "${GITHUB_TOKEN:-}" ]]; then
        log_error "GITHUB_TOKEN environment variable not set"
        log_info ""
        log_info "Create a Personal Access Token at:"
        log_info "  https://github.com/settings/tokens/new"
        log_info ""
        log_info "Required scopes: repo"
        log_info ""
        log_info "Then run:"
        log_info "  export GITHUB_TOKEN=ghp_xxxx"
        log_info "  $0"
        exit 1
    fi
    
    log_info "All preflight checks passed"
}

# =============================================================================
# Get runner registration token
# =============================================================================
get_registration_token() {
    log_step "Getting runner registration token..."
    
    local response
    response=$(curl -s -X POST \
        -H "Accept: application/vnd.github+json" \
        -H "Authorization: Bearer ${GITHUB_TOKEN}" \
        "https://api.github.com/repos/${GITHUB_ORG}/${GITHUB_REPO}/actions/runners/registration-token")
    
    REGISTRATION_TOKEN=$(echo "$response" | jq -r '.token')
    
    if [[ -z "$REGISTRATION_TOKEN" ]] || [[ "$REGISTRATION_TOKEN" == "null" ]]; then
        log_error "Failed to get registration token"
        log_error "Response: $response"
        exit 1
    fi
    
    log_info "Registration token obtained"
}

# =============================================================================
# Download and install runner
# =============================================================================
install_runner() {
    log_step "Installing GitHub Actions runner..."
    
    # Create runner directory
    mkdir -p "$RUNNER_DIR"
    cd "$RUNNER_DIR"
    
    # Download runner if not already present
    local runner_tar="actions-runner-osx-arm64-${RUNNER_VERSION}.tar.gz"
    if [[ ! -f "$runner_tar" ]]; then
        log_info "Downloading runner v${RUNNER_VERSION}..."
        curl -sL -o "$runner_tar" \
            "https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/${runner_tar}"
    fi
    
    # Extract
    log_info "Extracting runner..."
    tar xzf "$runner_tar"
    
    log_info "Runner installed to: $RUNNER_DIR"
}

# =============================================================================
# Configure runner
# =============================================================================
configure_runner() {
    log_step "Configuring runner..."
    
    cd "$RUNNER_DIR"
    
    # Remove existing configuration if present
    if [[ -f ".runner" ]]; then
        log_info "Removing existing configuration..."
        ./config.sh remove --token "$REGISTRATION_TOKEN" 2>/dev/null || true
    fi
    
    # Configure
    ./config.sh \
        --url "https://github.com/${GITHUB_ORG}/${GITHUB_REPO}" \
        --token "$REGISTRATION_TOKEN" \
        --name "$RUNNER_NAME" \
        --labels "$RUNNER_LABELS" \
        --work "_work" \
        --replace
    
    log_info "Runner configured as: $RUNNER_NAME"
    log_info "Labels: $RUNNER_LABELS"
}

# =============================================================================
# Create launchd service (optional)
# =============================================================================
create_launchd_service() {
    log_step "Creating launchd service..."
    
    local plist_path="$HOME/Library/LaunchAgents/com.github.actions.runner.plist"
    
    cat > "$plist_path" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.github.actions.runner</string>
    <key>ProgramArguments</key>
    <array>
        <string>${RUNNER_DIR}/run.sh</string>
    </array>
    <key>WorkingDirectory</key>
    <string>${RUNNER_DIR}</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>${RUNNER_DIR}/runner.log</string>
    <key>StandardErrorPath</key>
    <string>${RUNNER_DIR}/runner.err</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
        <key>DOCKER_HOST</key>
        <string>unix:///var/run/docker.sock</string>
    </dict>
</dict>
</plist>
EOF
    
    log_info "Launchd service created: $plist_path"
    log_info ""
    log_info "To start the runner as a service:"
    log_info "  launchctl load $plist_path"
    log_info ""
    log_info "To stop the service:"
    log_info "  launchctl unload $plist_path"
}

# =============================================================================
# Start runner
# =============================================================================
start_runner() {
    log_step "Starting runner..."
    
    cd "$RUNNER_DIR"
    
    log_info ""
    log_info "=========================================="
    log_info "Runner is ready! Starting interactively..."
    log_info "Press Ctrl+C to stop"
    log_info "=========================================="
    log_info ""
    
    ./run.sh
}

# =============================================================================
# Main
# =============================================================================
main() {
    echo ""
    echo "================================================"
    echo "  GitHub Actions Self-Hosted Runner Setup"
    echo "  Repository: ${GITHUB_ORG}/${GITHUB_REPO}"
    echo "================================================"
    echo ""
    
    preflight_checks
    get_registration_token
    install_runner
    configure_runner
    create_launchd_service
    
    echo ""
    log_info "Setup complete!"
    echo ""
    
    read -p "Start runner now? [Y/n] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]] || [[ -z $REPLY ]]; then
        start_runner
    else
        log_info "To start the runner manually:"
        log_info "  cd $RUNNER_DIR && ./run.sh"
    fi
}

main "$@"
