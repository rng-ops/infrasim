#!/usr/bin/env bash
# Example: Deploying InfraSim from build artifacts

set -e

echo "📦 InfraSim Deployment Example"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Configuration
DIST_DIR="${DIST_DIR:-./dist}"
VERSION=$(cat "$DIST_DIR/manifest.json" | grep version | cut -d'"' -f4)
PREFIX="${PREFIX:-/usr/local}"

echo "Version: $VERSION"
echo "Prefix:  $PREFIX"
echo

# Step 1: Verify artifacts
echo "1️⃣  Verifying artifacts..."
for tarball in "$DIST_DIR"/*.tar.gz; do
    if [ -f "$tarball.sha256" ]; then
        echo "  Checking $(basename $tarball)..."
        (cd "$DIST_DIR" && shasum -a 256 -c "$(basename $tarball).sha256" --quiet)
        echo "  ✅ Verified"
    fi
done

# Step 2: Install CLI
echo
echo "2️⃣  Installing CLI to $PREFIX/bin..."
if [ -f "$DIST_DIR/infrasim" ]; then
    sudo install -m 755 "$DIST_DIR/infrasim" "$PREFIX/bin/"
    echo "  ✅ infrasim installed"
    $PREFIX/bin/infrasim --version
fi

# Step 3: Install Daemon
echo
echo "3️⃣  Installing daemon to $PREFIX/bin..."
if [ -f "$DIST_DIR/infrasimd" ]; then
    sudo install -m 755 "$DIST_DIR/infrasimd" "$PREFIX/bin/"
    echo "  ✅ infrasimd installed"
    $PREFIX/bin/infrasimd --help | head -5
fi

# Step 4: Install Terraform Provider
echo
echo "4️⃣  Installing Terraform provider..."
PROVIDER_DIR="$HOME/.terraform.d/plugins/registry.terraform.io/infrasim/infrasim/$VERSION/darwin_arm64"
mkdir -p "$PROVIDER_DIR"

if [ -f "$DIST_DIR/terraform-provider-infrasim" ]; then
    cp "$DIST_DIR/terraform-provider-infrasim" \
       "$PROVIDER_DIR/terraform-provider-infrasim_v${VERSION}"
    chmod +x "$PROVIDER_DIR/terraform-provider-infrasim_v${VERSION}"
    echo "  ✅ Terraform provider installed to:"
    echo "     $PROVIDER_DIR"
fi

# Step 5: Create daemon config
echo
echo "5️⃣  Creating daemon configuration..."
CONFIG_DIR="$HOME/.config/infrasim"
mkdir -p "$CONFIG_DIR"

cat > "$CONFIG_DIR/config.toml" <<'EOF'
[daemon]
grpc_listen = "127.0.0.1:50051"
data_dir = "$HOME/.local/share/infrasim"
qemu_path = "/opt/homebrew/bin/qemu-system-aarch64"

[storage]
images_dir = "images"
volumes_dir = "volumes"
snapshots_dir = "snapshots"

[network]
default_mode = "user"
vmnet_interface = "bridge100"

[qos]
enabled = true
default_cpu_weight = 1024
default_io_weight = 500

[logging]
level = "info"
file = "$HOME/.local/share/infrasim/daemon.log"
EOF

echo "  ✅ Configuration created at:"
echo "     $CONFIG_DIR/config.toml"

# Step 6: Create data directories
echo
echo "6️⃣  Creating data directories..."
DATA_DIR="$HOME/.local/share/infrasim"
mkdir -p "$DATA_DIR"/{images,volumes,snapshots,state}
echo "  ✅ Data directory: $DATA_DIR"

# Step 7: Create systemd/launchd service (optional)
echo
echo "7️⃣  Creating launch daemon..."

PLIST="$HOME/Library/LaunchAgents/com.infrasim.daemon.plist"

cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.infrasim.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>$PREFIX/bin/infrasimd</string>
        <string>--config</string>
        <string>$CONFIG_DIR/config.toml</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardErrorPath</key>
    <string>$DATA_DIR/daemon.err.log</string>
    <key>StandardOutPath</key>
    <string>$DATA_DIR/daemon.out.log</string>
</dict>
</plist>
EOF

echo "  ✅ Launch daemon: $PLIST"
echo
echo "  To start daemon:"
echo "    launchctl load $PLIST"
echo "  To stop daemon:"
echo "    launchctl unload $PLIST"

# Step 8: Create example Terraform configuration
echo
echo "8️⃣  Creating example Terraform configuration..."

EXAMPLES_DIR="$HOME/infrasim-examples"
mkdir -p "$EXAMPLES_DIR"

cat > "$EXAMPLES_DIR/main.tf" <<'EOF'
terraform {
  required_providers {
    infrasim = {
      source  = "registry.terraform.io/infrasim/infrasim"
      version = "~> 0.1"
    }
  }
}

provider "infrasim" {
  daemon_address = "http://127.0.0.1:50051"
}

resource "infrasim_network" "private" {
  name         = "private-network"
  mode         = "user"
  cidr         = "192.168.100.0/24"
  gateway      = "192.168.100.1"
  dns          = "8.8.8.8"
  dhcp_enabled = true
  mtu          = 1500
}

resource "infrasim_volume" "debian_disk" {
  name       = "debian-root"
  kind       = "disk"
  format     = "qcow2"
  size_bytes = 10737418240  # 10GB
  source     = ""
}

resource "infrasim_vm" "example" {
  name       = "debian-vm"
  arch       = "aarch64"
  machine    = "virt"
  cpu_cores  = 2
  memory_mb  = 2048
  enable_tpm = false
  
  boot_disk_id = infrasim_volume.debian_disk.id
  network_ids  = [infrasim_network.private.id]
}

output "vm_id" {
  value = infrasim_vm.example.id
}
EOF

echo "  ✅ Example Terraform config: $EXAMPLES_DIR/main.tf"

# Step 9: Verification
echo
echo "9️⃣  Verification..."
echo "  Checking installations:"

command -v infrasim >/dev/null && echo "  ✅ infrasim: $(which infrasim)" || echo "  ❌ infrasim not found"
command -v infrasimd >/dev/null && echo "  ✅ infrasimd: $(which infrasimd)" || echo "  ❌ infrasimd not found"
[ -f "$PROVIDER_DIR/terraform-provider-infrasim_v${VERSION}" ] && echo "  ✅ terraform provider installed" || echo "  ❌ terraform provider not found"

# Summary
echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "✅ Deployment Complete!"
echo
echo "Next steps:"
echo
echo "  1. Start the daemon:"
echo "     infrasimd --config $CONFIG_DIR/config.toml"
echo
echo "  2. Test the CLI:"
echo "     infrasim status"
echo
echo "  3. Try Terraform:"
echo "     cd $EXAMPLES_DIR"
echo "     terraform init"
echo "     terraform plan"
echo
echo "  4. Or start as a service:"
echo "     launchctl load $PLIST"
echo
echo "Documentation:"
echo "  • Configuration: $CONFIG_DIR/config.toml"
echo "  • Data directory: $DATA_DIR"
echo "  • Examples: $EXAMPLES_DIR"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
