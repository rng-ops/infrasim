#!/usr/bin/env bash
set -euo pipefail

# InfraSim Build Pipeline
# Generates all release artifacts for macOS ARM64

VERSION="${VERSION:-$(git describe --tags --always --dirty 2>/dev/null || echo "dev")}"
BUILD_DATE="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
TARGET="aarch64-apple-darwin"
PROFILE="${PROFILE:-release}"

echo "🚀 InfraSim Build Pipeline"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Version:     $VERSION"
echo "Build Date:  $BUILD_DATE"
echo "Target:      $TARGET"
echo "Profile:     $PROFILE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo

# Step 1: Clean previous builds
echo "📦 Step 1/8: Cleaning previous builds..."
cargo clean
mkdir -p dist

# Step 2: Check dependencies
echo "🔍 Step 2/8: Checking dependencies..."
if ! command -v protoc &> /dev/null; then
    echo "❌ protoc not found. Install with: brew install protobuf"
    exit 1
fi
echo "✅ protoc $(protoc --version)"
echo "✅ rustc $(rustc --version)"
echo "✅ cargo $(cargo --version)"

# Step 3: Generate proto files
echo "🔧 Step 3/8: Generating protobuf files..."
# This happens automatically via build.rs, but we verify
cargo check --quiet

# Step 4: Build all binaries
echo "🏗️  Step 4/8: Building all binaries..."
cargo build --profile "$PROFILE" --all

# Step 5: Run tests
echo "🧪 Step 5/8: Running tests..."
cargo test --profile "$PROFILE" --all 2>&1 | grep -E "(test result|running)" || true

# Step 6: Generate artifacts
echo "📦 Step 6/8: Generating artifacts..."

BINARIES=(
    "infrasim"
    "infrasimd"
    "terraform-provider-infrasim"
)

for binary in "${BINARIES[@]}"; do
    src="target/$PROFILE/$binary"
    if [ -f "$src" ]; then
        # Copy to dist
        cp "$src" "dist/$binary"
        
        # Strip debug symbols
        strip "dist/$binary"
        
        # Create tarball
        tar -czf "dist/${binary}-${VERSION}-${TARGET}.tar.gz" -C dist "$binary"
        
        # Generate checksum
        shasum -a 256 "dist/${binary}-${VERSION}-${TARGET}.tar.gz" > "dist/${binary}-${VERSION}-${TARGET}.tar.gz.sha256"
        
        echo "  ✅ $binary → dist/${binary}-${VERSION}-${TARGET}.tar.gz"
    else
        echo "  ⚠️  $binary not found"
    fi
done

# Step 7: Create Terraform provider bundle
echo "🔧 Step 7/8: Creating Terraform provider bundle..."
PROVIDER_DIR="dist/terraform-providers/registry.terraform.io/infrasim/infrasim/$VERSION/$TARGET"
mkdir -p "$PROVIDER_DIR"

if [ -f "dist/terraform-provider-infrasim" ]; then
    cp "dist/terraform-provider-infrasim" "$PROVIDER_DIR/terraform-provider-infrasim_v${VERSION}"
    
    # Create provider manifest
    cat > "$PROVIDER_DIR/terraform-provider-infrasim_${VERSION}_manifest.json" <<EOF
{
  "version": 1,
  "metadata": {
    "protocol_versions": ["6.0"]
  }
}
EOF
    
    echo "  ✅ Terraform provider bundle created"
fi

# Step 8: Generate build manifest
echo "📋 Step 8/8: Generating build manifest..."
cat > dist/manifest.json <<EOF
{
  "version": "$VERSION",
  "build_date": "$BUILD_DATE",
  "target": "$TARGET",
  "artifacts": [
$(for binary in "${BINARIES[@]}"; do
    if [ -f "dist/${binary}-${VERSION}-${TARGET}.tar.gz" ]; then
        size=$(stat -f%z "dist/${binary}-${VERSION}-${TARGET}.tar.gz")
        checksum=$(awk '{print $1}' "dist/${binary}-${VERSION}-${TARGET}.tar.gz.sha256")
        echo "    {"
        echo "      \"name\": \"$binary\","
        echo "      \"file\": \"${binary}-${VERSION}-${TARGET}.tar.gz\","
        echo "      \"size\": $size,"
        echo "      \"sha256\": \"$checksum\""
        echo "    },"
    fi
done | sed '$ s/,$//')
  ]
}
EOF

echo
echo "✅ Build Complete!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Artifacts:"
ls -lh dist/*.tar.gz 2>/dev/null | awk '{print "  "$9" ("$5")"}'
echo
echo "Total artifacts: $(ls -1 dist/*.tar.gz 2>/dev/null | wc -l)"
echo "Location: $(pwd)/dist/"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
