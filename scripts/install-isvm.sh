#!/bin/bash
# ISVM Installer
# Installs InfraSim Version Manager to ~/.isvm

set -e

ISVM_DIR="${ISVM_DIR:-$HOME/.isvm}"
REPO_URL="https://raw.githubusercontent.com/rng-ops/infrasim/main/scripts/isvm.sh"

echo ""
echo "╔══════════════════════════════════════════════════════════╗"
echo "║          ISVM - InfraSim Version Manager                 ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""

# Create directory
mkdir -p "$ISVM_DIR"

# Check if running from repo or downloading
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -f "$SCRIPT_DIR/isvm.sh" ]]; then
    echo "→ Installing from local repository..."
    cp "$SCRIPT_DIR/isvm.sh" "$ISVM_DIR/isvm.sh"
else
    echo "→ Downloading latest isvm.sh..."
    curl -fsSL "$REPO_URL" -o "$ISVM_DIR/isvm.sh" || {
        echo "Failed to download. Using embedded version..."
        # Embedded fallback would go here
        exit 1
    }
fi

chmod +x "$ISVM_DIR/isvm.sh"

# Create bin directory and convenience wrapper
mkdir -p "$ISVM_DIR/bin"
cat > "$ISVM_DIR/bin/isvm" << 'WRAPPER'
#!/bin/bash
source "${ISVM_DIR:-$HOME/.isvm}/isvm.sh"
isvm "$@"
WRAPPER
chmod +x "$ISVM_DIR/bin/isvm"

echo "✔ Installed ISVM to $ISVM_DIR"
echo ""

# Detect shell
SHELL_NAME=$(basename "$SHELL")
PROFILE=""

case "$SHELL_NAME" in
    zsh)
        PROFILE="$HOME/.zshrc"
        ;;
    bash)
        if [[ -f "$HOME/.bash_profile" ]]; then
            PROFILE="$HOME/.bash_profile"
        else
            PROFILE="$HOME/.bashrc"
        fi
        ;;
    *)
        echo "Unknown shell: $SHELL_NAME"
        echo "Please manually add the following to your shell profile:"
        echo ""
        echo '  export ISVM_DIR="$HOME/.isvm"'
        echo '  [ -s "$ISVM_DIR/isvm.sh" ] && source "$ISVM_DIR/isvm.sh"'
        echo ""
        exit 0
        ;;
esac

# Check if already configured
SETUP_LINES='export ISVM_DIR="$HOME/.isvm"
[ -s "$ISVM_DIR/isvm.sh" ] && source "$ISVM_DIR/isvm.sh"'

if grep -q 'ISVM_DIR' "$PROFILE" 2>/dev/null; then
    echo "✔ Shell already configured in $PROFILE"
else
    echo ""
    echo "Add the following to $PROFILE:"
    echo ""
    echo '  export ISVM_DIR="$HOME/.isvm"'
    echo '  [ -s "$ISVM_DIR/isvm.sh" ] && source "$ISVM_DIR/isvm.sh"'
    echo ""
    
    read -p "Add automatically? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "" >> "$PROFILE"
        echo "# InfraSim Version Manager" >> "$PROFILE"
        echo 'export ISVM_DIR="$HOME/.isvm"' >> "$PROFILE"
        echo '[ -s "$ISVM_DIR/isvm.sh" ] && source "$ISVM_DIR/isvm.sh"' >> "$PROFILE"
        echo "✔ Added to $PROFILE"
    fi
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "To get started, restart your shell or run:"
echo ""
echo "  source $PROFILE"
echo ""
echo "Then from your InfraSim project directory:"
echo ""
echo "  isvm install     # Install current build"
echo "  isvm list        # Show installed versions"
echo "  isvm use v0.1.0  # Switch versions"
echo "  isvm link        # Dev mode - link to project binaries"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
