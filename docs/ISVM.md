# ISVM - InfraSim Version Manager

ISVM provides NVM-style version management for InfraSim binaries. It allows developers to install, switch between, and manage multiple versions of InfraSim tools, similar to how `nvm` manages Node.js versions or `rustup` manages Rust toolchains.

## Architecture

```
~/.isvm/
├── isvm.sh                     # Main script (sourced in shell)
├── bin/                        # Symlinks added to PATH
│   ├── infrasim              → current/bin/infrasim
│   ├── infrasimd             → current/bin/infrasimd
│   ├── infrasim-web          → current/bin/infrasim-web
│   └── terraform-provider-infrasim → current/bin/terraform-provider-infrasim
├── current → versions/v0.1.0/  # Symlink to active version
└── versions/
    ├── v0.1.0/
    │   ├── bin/               # Actual binaries (copied)
    │   │   ├── infrasim
    │   │   ├── infrasimd
    │   │   ├── infrasim-web
    │   │   └── terraform-provider-infrasim
    │   └── version.json       # Installation metadata
    ├── v0.2.0-beta/
    │   ├── bin/
    │   └── version.json
    └── feature-xyz-abc1234/   # Feature branch builds
        ├── bin/
        └── version.json
```

## Installation

### Quick Install

```bash
# From the InfraSim repository
./scripts/install-isvm.sh

# Or via Make
make isvm-setup
```

### Manual Installation

```bash
# Create directory
mkdir -p ~/.isvm

# Copy script
cp scripts/isvm.sh ~/.isvm/

# Add to shell profile (~/.zshrc or ~/.bashrc)
cat >> ~/.zshrc << 'EOF'
export ISVM_DIR="$HOME/.isvm"
[ -s "$ISVM_DIR/isvm.sh" ] && source "$ISVM_DIR/isvm.sh"
EOF

# Reload shell
source ~/.zshrc
```

## Commands

### `isvm install [version]`

Install the current build or specify a version name.

```bash
# Install current build (version auto-detected from git)
cd ~/projects/infrasim
isvm install

# Install with explicit version name
isvm install v0.2.0

# Install from a different profile
ISVM_PROFILE=debug isvm install v0.2.0-debug
```

**What happens:**
1. Detects project root (finds `Cargo.toml`)
2. Auto-detects version from `git describe --tags --always`
3. Copies binaries from `target/release/` to `~/.isvm/versions/<version>/bin/`
4. Creates `version.json` with metadata (git commit, branch, timestamp)
5. Automatically switches to the new version

### `isvm use <version>`

Switch to an installed version.

```bash
# Switch to specific version
isvm use v0.1.0

# Switch to latest installed
isvm use latest
```

**What happens:**
1. Updates `~/.isvm/current` symlink to point to the version directory
2. Updates all symlinks in `~/.isvm/bin/` to point to the version's binaries

### `isvm list`

List all installed versions.

```bash
$ isvm list

Installed InfraSim versions:

  → v0.1.0-6-gf6dbd33 (current)
      branch: tag-builds  installed: 2025-12-19T04:00:27Z
    v0.1.0
      branch: main  installed: 2025-12-18T12:30:00Z
```

### `isvm link`

Development mode - create symlinks directly to project's `target/release/` directory.

```bash
cd ~/projects/infrasim
isvm link
```

**What happens:**
1. Creates symlinks in `~/.isvm/bin/` pointing directly to `target/release/`
2. Changes take effect immediately after `cargo build --release`
3. No copying - always runs the latest built binary

**Use case:** Rapid iteration during development without reinstalling.

### `isvm current`

Print the current active version.

```bash
$ isvm current
v0.1.0-6-gf6dbd33
```

### `isvm uninstall <version>`

Remove an installed version.

```bash
isvm uninstall v0.1.0
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ISVM_DIR` | `~/.isvm` | Installation directory |
| `ISVM_PROFILE` | `release` | Cargo profile to use (`release` or `dev`) |

## Version Metadata

Each installed version includes a `version.json` file:

```json
{
    "version": "v0.1.0-6-gf6dbd33",
    "installed_at": "2025-12-19T04:00:27Z",
    "source": "/Users/a/projects/infrasim",
    "profile": "release",
    "git_commit": "f6dbd33a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q",
    "git_branch": "tag-builds"
}
```

## Makefile Integration

The Makefile provides convenient targets:

```bash
# Install ISVM itself
make isvm-setup

# Build and install current version
make isvm-install

# Link project binaries (dev mode)
make isvm-link

# List installed versions
make isvm-list

# Switch to a version
make isvm-use V=v0.1.0
```

## CI/CD Integration

### GitHub Actions

```yaml
- name: Setup ISVM
  run: |
    ./scripts/install-isvm.sh <<< "n"  # Skip shell profile modification
    echo "$HOME/.isvm/bin" >> $GITHUB_PATH

- name: Build and Install
  run: |
    source ~/.isvm/isvm.sh
    cargo build --release
    isvm install "${{ github.ref_name }}"
```

### Artifact Caching

```yaml
- name: Cache ISVM versions
  uses: actions/cache@v4
  with:
    path: ~/.isvm/versions
    key: isvm-${{ runner.os }}-${{ hashFiles('Cargo.lock') }}
```

## Comparison with Other Tools

| Feature | ISVM | nvm | rustup |
|---------|------|-----|--------|
| Version switching | ✅ | ✅ | ✅ |
| Build from source | ✅ | ❌ | ✅ |
| Dev symlink mode | ✅ | ❌ | ❌ |
| Multiple binaries | ✅ | ❌ | ✅ |
| Shell function | ✅ | ✅ | ❌ |
| Metadata tracking | ✅ | ❌ | ❌ |

## Troubleshooting

### "isvm: command not found"

Ensure ISVM is sourced in your shell:

```bash
source ~/.isvm/isvm.sh
```

Or add to your shell profile and restart.

### "Not in an InfraSim project directory"

Run `isvm install` from a directory containing the InfraSim Cargo workspace.

### Binaries not found after switching

Check that `~/.isvm/bin` is in your PATH:

```bash
echo $PATH | grep -o '[^:]*isvm[^:]*'
# Should show: /Users/you/.isvm/bin
```

### Building with debug profile

```bash
ISVM_PROFILE=dev isvm install v0.1.0-debug
```
