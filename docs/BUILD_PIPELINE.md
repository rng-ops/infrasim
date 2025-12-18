# InfraSim Build Pipeline - Quick Reference

## Build Artifacts Generated

After running `./build.sh`, the following artifacts are created in `dist/`:

### 1. Stripped Binaries
- `infrasim` (3.1M) - CLI tool
- `infrasimd` (4.9M) - Daemon server
- `terraform-provider-infrasim` (3.0M) - Terraform provider plugin

### 2. Distribution Tarballs
- `infrasim-{VERSION}-aarch64-apple-darwin.tar.gz` (1.4M)
- `infrasimd-{VERSION}-aarch64-apple-darwin.tar.gz` (2.3M)
- `terraform-provider-infrasim-{VERSION}-aarch64-apple-darwin.tar.gz` (1.3M)

### 3. Checksums
- `*.tar.gz.sha256` - SHA256 checksums for verification

### 4. Terraform Provider Bundle
```
dist/terraform-providers/
  └── registry.terraform.io/
      └── infrasim/
          └── infrasim/
              └── {VERSION}/
                  └── darwin_arm64/
                      ├── terraform-provider-infrasim_v{VERSION}
                      └── terraform-provider-infrasim_{VERSION}_manifest.json
```

### 5. Build Manifest
`dist/manifest.json` - Contains version, build date, artifact metadata, and checksums

## Build Methods

### Method 1: Build Script (Recommended)
```bash
./build.sh
```
**Output**: All artifacts in `dist/` directory

### Method 2: Makefile
```bash
# Quick build
make build

# Full release pipeline
make release

# Install to system
make install

# Development build
make dev

# Run tests
make test

# Clean everything
make clean
```

### Method 3: Direct Cargo
```bash
# Build all
cargo build --release --all

# Build specific binary
cargo build --release --bin infrasim
cargo build --release --bin infrasimd
cargo build --release --bin terraform-provider-infrasim
```

## Installation Examples

### Local Installation
```bash
# After running ./build.sh
sudo cp dist/infrasim /usr/local/bin/
sudo cp dist/infrasimd /usr/local/bin/

# Or use Makefile
make install PREFIX=/usr/local
```

### Terraform Provider Installation
```bash
# Manual
VERSION=$(cat dist/manifest.json | grep version | cut -d'"' -f4)
mkdir -p ~/.terraform.d/plugins/registry.terraform.io/infrasim/infrasim/$VERSION/darwin_arm64
cp dist/terraform-provider-infrasim \
   ~/.terraform.d/plugins/registry.terraform.io/infrasim/infrasim/$VERSION/darwin_arm64/

# Or use the pre-created bundle
cp -r dist/terraform-providers/registry.terraform.io ~/.terraform.d/plugins/
```

### From Tarball
```bash
# Download
curl -LO https://github.com/you/infrasim/releases/download/v0.1.0/infrasim-v0.1.0-aarch64-apple-darwin.tar.gz

# Verify
shasum -a 256 -c infrasim-v0.1.0-aarch64-apple-darwin.tar.gz.sha256

# Extract and install
tar -xzf infrasim-v0.1.0-aarch64-apple-darwin.tar.gz
sudo mv infrasim /usr/local/bin/
```

## CI/CD Pipeline (GitHub Actions)

The `.github/workflows/build.yml` workflow runs on:
- Push to `main` or `develop` branches
- Pull requests
- Git tags (v*)

### Workflow Steps:
1. Checkout code
2. Install Rust toolchain (aarch64-apple-darwin)
3. Install protobuf via Homebrew
4. Cache cargo dependencies
5. Run `./build.sh`
6. Run tests
7. Upload artifacts
8. Create GitHub Release (on tags)

### Artifacts Uploaded:
- All `.tar.gz` files
- All `.sha256` checksum files
- `manifest.json`

Artifacts retained for 30 days.

## Docker Build

```bash
# Build image
docker build -t infrasim/daemon:latest .

# Or with Makefile
make docker

# Run containerized daemon
docker run -it --rm \
  -v /var/run/qemu:/var/run/qemu \
  -p 50051:50051 \
  infrasim/daemon:latest
```

## Build Verification

### Smoke Test
```bash
make smoke
```

### Full Test Suite
```bash
cargo test --all --verbose
```

### Binary Size Analysis
```bash
make size
```

### Check Dependencies
```bash
make deps      # Check for outdated deps
make audit     # Security audit
```

## Versioning

Version is automatically determined from:
1. Git tag (if available): `git describe --tags --always --dirty`
2. Environment variable: `VERSION=v1.0.0 ./build.sh`
3. Default: `dev`

Example:
```bash
# Tag release
git tag v0.1.0

# Build with version
./build.sh
# Creates: infrasim-v0.1.0-aarch64-apple-darwin.tar.gz
```

## Build Times

On Apple M2 (16GB):
- Clean build: ~67 seconds
- Incremental: ~5 seconds
- Full release pipeline: ~75 seconds

## Artifact Sizes

| Binary | Unstripped | Stripped | Compressed |
|--------|-----------|----------|------------|
| infrasim | 5.4M | 3.1M | 1.4M |
| infrasimd | 8.2M | 4.9M | 2.3M |
| terraform-provider-infrasim | 5.1M | 3.0M | 1.3M |

**Total**: ~11M binaries, ~5M compressed

## Troubleshooting

### Protobuf not found
```bash
brew install protobuf
```

### Rust toolchain issues
```bash
rustup update stable
rustup target add aarch64-apple-darwin
```

### Cache issues
```bash
cargo clean
rm -rf target
```

### Linker errors
```bash
xcode-select --install
```

## Advanced Usage

### Cross-compilation (experimental)
```bash
# For Linux ARM64
cargo build --target aarch64-unknown-linux-gnu --release

# For x86_64 macOS
cargo build --target x86_64-apple-darwin --release
```

### Custom build flags
```bash
CARGO_FLAGS="--features experimental" ./build.sh
```

### Profile selection
```bash
# Dev profile (faster build, larger binaries)
make dev

# Release profile (optimized)
make release
```

### Documentation generation
```bash
make docs
# Opens browser with generated docs
```

## Distribution

### GitHub Release
Releases are automatically created for git tags via GitHub Actions.

### Homebrew (future)
```ruby
class Infrasim < Formula
  desc "Terraform-compatible QEMU platform for macOS ARM64"
  homepage "https://github.com/you/infrasim"
  url "https://github.com/you/infrasim/releases/download/v0.1.0/infrasim-v0.1.0-aarch64-apple-darwin.tar.gz"
  sha256 "..."
  
  def install
    bin.install "infrasim"
    bin.install "infrasimd"
  end
end
```

### Terraform Registry (future)
Publish to registry.terraform.io for automatic provider installation.
