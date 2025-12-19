#!/bin/bash
# ISVM - InfraSim Version Manager
# NVM-style version management for InfraSim binaries
#
# Installation:
#   Add to your shell profile (~/.zshrc or ~/.bashrc):
#     export ISVM_DIR="$HOME/.isvm"
#     [ -s "$ISVM_DIR/isvm.sh" ] && source "$ISVM_DIR/isvm.sh"
#
# Usage:
#   isvm install           # Install current build
#   isvm install v0.1.0    # Install specific version
#   isvm use v0.1.0        # Switch to version
#   isvm use latest        # Switch to latest
#   isvm list              # List installed versions
#   isvm current           # Show current version
#   isvm uninstall v0.1.0  # Remove a version
#   isvm link              # Link project binaries to PATH

set -e

# Configuration
ISVM_DIR="${ISVM_DIR:-$HOME/.isvm}"
ISVM_VERSIONS_DIR="$ISVM_DIR/versions"
ISVM_CURRENT_LINK="$ISVM_DIR/current"
ISVM_BIN_DIR="$ISVM_DIR/bin"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color
BOLD='\033[1m'

# Binaries we manage
BINARIES=("infrasim" "infrasimd" "infrasim-web" "terraform-provider-infrasim")

isvm_echo() {
    echo -e "${CYAN}isvm:${NC} $*"
}

isvm_error() {
    echo -e "${RED}error:${NC} $*" >&2
}

isvm_success() {
    echo -e "${GREEN}✔${NC} $*"
}

isvm_warn() {
    echo -e "${YELLOW}⚠${NC} $*"
}

# Initialize ISVM directories
isvm_init() {
    mkdir -p "$ISVM_VERSIONS_DIR"
    mkdir -p "$ISVM_BIN_DIR"
    
    # Ensure bin dir is in PATH
    if [[ ":$PATH:" != *":$ISVM_BIN_DIR:"* ]]; then
        export PATH="$ISVM_BIN_DIR:$PATH"
    fi
}

# Get project root (where Cargo.toml is)
isvm_find_project_root() {
    local dir="$PWD"
    while [[ "$dir" != "/" ]]; do
        if [[ -f "$dir/Cargo.toml" ]] && grep -q 'name = "infrasim-cli"' "$dir/crates/cli/Cargo.toml" 2>/dev/null; then
            echo "$dir"
            return 0
        fi
        dir="$(dirname "$dir")"
    done
    return 1
}

# Get version from git or Cargo.toml
isvm_detect_version() {
    local project_root="$1"
    
    if [[ -d "$project_root/.git" ]]; then
        # Try git describe for version
        local version
        version=$(cd "$project_root" && git describe --tags --always 2>/dev/null || echo "")
        
        if [[ -n "$version" ]]; then
            echo "$version"
            return 0
        fi
    fi
    
    # Fall back to Cargo.toml version
    if [[ -f "$project_root/Cargo.toml" ]]; then
        grep '^version' "$project_root/Cargo.toml" | head -1 | cut -d'"' -f2
        return 0
    fi
    
    echo "dev"
}

# Install binaries from current project or a specific path
isvm_install() {
    isvm_init
    
    local version="${1:-}"
    local project_root
    local source_dir
    local profile="${ISVM_PROFILE:-release}"
    
    # Find project root
    project_root=$(isvm_find_project_root) || {
        isvm_error "Not in an InfraSim project directory"
        return 1
    }
    
    source_dir="$project_root/target/$profile"
    
    # Detect version if not specified
    if [[ -z "$version" ]]; then
        version=$(isvm_detect_version "$project_root")
    fi
    
    # Sanitize version name (replace / with -)
    version="${version//\//-}"
    
    local version_dir="$ISVM_VERSIONS_DIR/$version"
    
    isvm_echo "Installing InfraSim ${BOLD}$version${NC}..."
    
    # Check if binaries exist
    if [[ ! -f "$source_dir/infrasim" ]]; then
        isvm_warn "Binaries not found in $source_dir"
        isvm_echo "Building $profile binaries..."
        (cd "$project_root" && cargo build --profile "$profile" --all) || {
            isvm_error "Build failed"
            return 1
        }
    fi
    
    # Create version directory
    mkdir -p "$version_dir/bin"
    
    # Copy binaries
    local installed=0
    for binary in "${BINARIES[@]}"; do
        if [[ -f "$source_dir/$binary" ]]; then
            cp "$source_dir/$binary" "$version_dir/bin/"
            chmod +x "$version_dir/bin/$binary"
            isvm_success "Installed $binary"
            installed=$((installed + 1))
        fi
    done
    
    if [[ $installed -eq 0 ]]; then
        isvm_error "No binaries found to install"
        rm -rf "$version_dir"
        return 1
    fi
    
    # Create version metadata
    cat > "$version_dir/version.json" << EOF
{
    "version": "$version",
    "installed_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
    "source": "$project_root",
    "profile": "$profile",
    "git_commit": "$(cd "$project_root" && git rev-parse HEAD 2>/dev/null || echo "unknown")",
    "git_branch": "$(cd "$project_root" && git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")"
}
EOF
    
    isvm_success "Installed InfraSim $version"
    
    # Auto-switch to new version
    isvm_use "$version"
}

# Switch to a specific version
isvm_use() {
    isvm_init
    
    local version="$1"
    
    if [[ -z "$version" ]]; then
        isvm_error "Version required. Usage: isvm use <version>"
        return 1
    fi
    
    # Handle 'latest' alias
    if [[ "$version" == "latest" ]]; then
        version=$(isvm_latest)
        if [[ -z "$version" ]]; then
            isvm_error "No versions installed"
            return 1
        fi
    fi
    
    local version_dir="$ISVM_VERSIONS_DIR/$version"
    
    if [[ ! -d "$version_dir" ]]; then
        isvm_error "Version $version not installed"
        isvm_echo "Available versions:"
        isvm_list
        return 1
    fi
    
    # Update current symlink
    rm -f "$ISVM_CURRENT_LINK"
    ln -s "$version_dir" "$ISVM_CURRENT_LINK"
    
    # Update bin symlinks
    for binary in "${BINARIES[@]}"; do
        rm -f "$ISVM_BIN_DIR/$binary"
        if [[ -f "$version_dir/bin/$binary" ]]; then
            ln -s "$version_dir/bin/$binary" "$ISVM_BIN_DIR/$binary"
        fi
    done
    
    isvm_success "Now using InfraSim ${BOLD}$version${NC}"
    
    # Show version
    if [[ -f "$ISVM_BIN_DIR/infrasim" ]]; then
        "$ISVM_BIN_DIR/infrasim" --version 2>/dev/null || true
    fi
}

# List installed versions
isvm_list() {
    isvm_init
    
    local current=""
    if [[ -L "$ISVM_CURRENT_LINK" ]]; then
        current=$(basename "$(readlink "$ISVM_CURRENT_LINK")")
    fi
    
    echo ""
    echo -e "${BOLD}Installed InfraSim versions:${NC}"
    echo ""
    
    local found=0
    for version_dir in "$ISVM_VERSIONS_DIR"/*; do
        if [[ -d "$version_dir" ]]; then
            local version=$(basename "$version_dir")
            found=1
            
            if [[ "$version" == "$current" ]]; then
                echo -e "  ${GREEN}→ $version${NC} ${CYAN}(current)${NC}"
            else
                echo "    $version"
            fi
            
            # Show metadata if available
            if [[ -f "$version_dir/version.json" ]]; then
                local installed_at=$(grep '"installed_at"' "$version_dir/version.json" | cut -d'"' -f4)
                local git_branch=$(grep '"git_branch"' "$version_dir/version.json" | cut -d'"' -f4)
                if [[ -n "$git_branch" && "$git_branch" != "unknown" ]]; then
                    echo -e "      ${YELLOW}branch:${NC} $git_branch  ${YELLOW}installed:${NC} $installed_at"
                fi
            fi
        fi
    done
    
    if [[ $found -eq 0 ]]; then
        echo -e "  ${YELLOW}No versions installed${NC}"
        echo ""
        echo "  Run 'isvm install' from an InfraSim project to install"
    fi
    
    echo ""
}

# Get the latest installed version (by installation time)
isvm_latest() {
    local latest=""
    local latest_time=0
    
    for version_dir in "$ISVM_VERSIONS_DIR"/*; do
        if [[ -d "$version_dir" ]]; then
            local mtime
            mtime=$(stat -f %m "$version_dir" 2>/dev/null || stat -c %Y "$version_dir" 2>/dev/null || echo 0)
            if [[ $mtime -gt $latest_time ]]; then
                latest_time=$mtime
                latest=$(basename "$version_dir")
            fi
        fi
    done
    
    echo "$latest"
}

# Show current version
isvm_current() {
    isvm_init
    
    if [[ -L "$ISVM_CURRENT_LINK" ]]; then
        local current=$(basename "$(readlink "$ISVM_CURRENT_LINK")")
        echo "$current"
    else
        echo "none"
    fi
}

# Uninstall a version
isvm_uninstall() {
    local version="$1"
    
    if [[ -z "$version" ]]; then
        isvm_error "Version required. Usage: isvm uninstall <version>"
        return 1
    fi
    
    local version_dir="$ISVM_VERSIONS_DIR/$version"
    
    if [[ ! -d "$version_dir" ]]; then
        isvm_error "Version $version not installed"
        return 1
    fi
    
    # Check if this is the current version
    local current=$(isvm_current)
    if [[ "$version" == "$current" ]]; then
        isvm_warn "Uninstalling current version"
        rm -f "$ISVM_CURRENT_LINK"
        for binary in "${BINARIES[@]}"; do
            rm -f "$ISVM_BIN_DIR/$binary"
        done
    fi
    
    rm -rf "$version_dir"
    isvm_success "Uninstalled InfraSim $version"
}

# Link project binaries directly (dev mode)
isvm_link() {
    isvm_init
    
    local project_root
    project_root=$(isvm_find_project_root) || {
        isvm_error "Not in an InfraSim project directory"
        return 1
    }
    
    local profile="${ISVM_PROFILE:-release}"
    local source_dir="$project_root/target/$profile"
    
    isvm_echo "Linking binaries from $source_dir..."
    
    # Remove current symlink
    rm -f "$ISVM_CURRENT_LINK"
    
    # Create direct symlinks to project binaries
    local linked=0
    for binary in "${BINARIES[@]}"; do
        rm -f "$ISVM_BIN_DIR/$binary"
        if [[ -f "$source_dir/$binary" ]]; then
            ln -s "$source_dir/$binary" "$ISVM_BIN_DIR/$binary"
            isvm_success "Linked $binary → $source_dir/$binary"
            linked=$((linked + 1))
        fi
    done
    
    if [[ $linked -eq 0 ]]; then
        isvm_error "No binaries found in $source_dir"
        isvm_echo "Run 'cargo build --release' first"
        return 1
    fi
    
    isvm_success "Development binaries linked to PATH"
    echo ""
    echo -e "  ${YELLOW}Note:${NC} Changes take effect after rebuilding"
}

# Show help
isvm_help() {
    cat << 'EOF'

ISVM - InfraSim Version Manager

Usage:
  isvm <command> [arguments]

Commands:
  install [version]    Install current build (or specify version tag)
  use <version>        Switch to an installed version
  use latest           Switch to most recently installed
  list                 List all installed versions
  current              Show current active version
  uninstall <version>  Remove an installed version
  link                 Link project binaries directly (dev mode)
  help                 Show this help message

Environment:
  ISVM_DIR             Installation directory (default: ~/.isvm)
  ISVM_PROFILE         Cargo profile to use (default: release)

Examples:
  isvm install                    # Install from current project
  isvm install v0.2.0             # Install with specific version name
  isvm use v0.1.0                 # Switch to v0.1.0
  isvm link                       # Dev mode - link to project binaries
  
Shell Setup:
  Add to ~/.zshrc or ~/.bashrc:
  
    export ISVM_DIR="$HOME/.isvm"
    [ -s "$ISVM_DIR/isvm.sh" ] && source "$ISVM_DIR/isvm.sh"

EOF
}

# Main entrypoint
isvm() {
    local cmd="${1:-help}"
    shift || true
    
    case "$cmd" in
        install)
            isvm_install "$@"
            ;;
        use)
            isvm_use "$@"
            ;;
        list|ls)
            isvm_list
            ;;
        current)
            isvm_current
            ;;
        uninstall|rm|remove)
            isvm_uninstall "$@"
            ;;
        link)
            isvm_link
            ;;
        help|--help|-h)
            isvm_help
            ;;
        *)
            isvm_error "Unknown command: $cmd"
            isvm_help
            return 1
            ;;
    esac
}

# Auto-initialize
isvm_init

# If sourced, make isvm function available
# If executed directly, run with arguments
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    isvm "$@"
fi
