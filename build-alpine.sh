#!/bin/bash
# ============================================================================
# InfraSim Alpine Build Script
# ============================================================================
#
# This script provides multiple build modes for Alpine Linux images:
#
# 1. LOCAL MODE (default):
#    Builds a qcow2 image locally using images/alpine/build-alpine-qcow2.sh
#
# 2. KUBERNETES MODE (--k8s):
#    Deploys infrastructure via Terraform and captures a memory snapshot
#
# Usage:
#   ./build-alpine.sh              # Local build
#   ./build-alpine.sh --k8s        # Kubernetes/Terraform build
#   ./build-alpine.sh --bundle     # Local build + create .tar.gz bundle
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODE="local"
CREATE_BUNDLE=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --k8s|--kubernetes)
            MODE="kubernetes"
            shift
            ;;
        --bundle)
            CREATE_BUNDLE=true
            shift
            ;;
        --help)
            echo "Usage: $0 [--k8s] [--bundle]"
            echo ""
            echo "Options:"
            echo "  --k8s, --kubernetes   Use Terraform/K8s mode"
            echo "  --bundle              Create .tar.gz bundle after build"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# =============================================================================
# Local Build Mode
# =============================================================================
build_local() {
    echo "🏗️  Building Alpine qcow2 image locally..."
    
    cd "$SCRIPT_DIR/images/alpine"
    
    # Make scripts executable
    chmod +x build-alpine-qcow2.sh boot-test.sh
    
    # Build the image
    ./build-alpine-qcow2.sh \
        --version "${ALPINE_VERSION:-3.20}" \
        --arch "${ALPINE_ARCH:-aarch64}" \
        --size "${IMAGE_SIZE:-2G}" \
        --output output/base.qcow2
    
    echo "✅ Build complete: images/alpine/output/base.qcow2"
    
    # Optionally run boot test
    if [[ "${RUN_BOOT_TEST:-false}" == "true" ]]; then
        echo "🧪 Running boot test..."
        ./boot-test.sh output/base.qcow2
    fi
    
    # Create bundle if requested
    if [[ "$CREATE_BUNDLE" == "true" ]]; then
        echo "📦 Creating bundle..."
        cd "$SCRIPT_DIR"
        chmod +x ci/bundle_qcow2.sh
        
        GIT_SHA=$(git rev-parse --short HEAD 2>/dev/null || echo "local")
        mkdir -p dist
        
        ./ci/bundle_qcow2.sh \
            --input images/alpine/output \
            --output "dist/infrasim-alpine-${GIT_SHA}.tar.gz" \
            --version "${ALPINE_VERSION:-3.20}" \
            --git-sha "$GIT_SHA"
        
        echo "✅ Bundle created: dist/infrasim-alpine-${GIT_SHA}.tar.gz"
    fi
}

# =============================================================================
# Kubernetes/Terraform Build Mode (original behavior)
# =============================================================================
build_kubernetes() {
    echo "🚀 Starting Alpine build pipeline (Kubernetes mode)..."
    
    # Ensure DAEMON_ADDR is set
    DAEMON_ADDR="${DAEMON_ADDR:-http://127.0.0.1:8080}"
    
    # 1. Run Terraform plan
    echo "📋 Running Terraform plan..."
    terraform plan -out=plan.tfplan
    
    # 2. Apply infrastructure
    echo "🔧 Applying Terraform configuration..."
    terraform apply plan.tfplan
    
    # 3. Wait for VM to be ready
    echo "⏳ Waiting for VM..."
    sleep 30
    
    # 4. Get VM ID from Terraform output
    VM_ID=$(terraform output -raw vm_id)
    
    # 5. Start VM
    echo "▶️  Starting VM $VM_ID..."
    curl -X POST "$DAEMON_ADDR/api/vms/$VM_ID/start"
    
    # 6. Wait for boot
    echo "⏳ Waiting for VM to boot..."
    sleep 60
    
    # 7. Create memory snapshot
    echo "📸 Creating memory snapshot..."
    SNAPSHOT_ID=$(curl -X POST "$DAEMON_ADDR/api/vms/$VM_ID/snapshot" \
      -H "Content-Type: application/json" \
      -d '{"name": "alpine-built", "include_memory": true}' | jq -r .snapshot_id)
    
    echo "✅ Build complete! Snapshot ID: $SNAPSHOT_ID"
    
    # 8. Export snapshot as artifact
    echo "📦 Exporting snapshot..."
    mkdir -p dist
    curl -X GET "$DAEMON_ADDR/api/snapshots/$SNAPSHOT_ID" > dist/alpine-snapshot.qcow2
    
    echo "✅ Alpine memory snapshot ready: dist/alpine-snapshot.qcow2"
}

# =============================================================================
# Main
# =============================================================================
case "$MODE" in
    local)
        build_local
        ;;
    kubernetes)
        build_kubernetes
        ;;
    *)
        echo "Unknown mode: $MODE"
        exit 1
        ;;
esac