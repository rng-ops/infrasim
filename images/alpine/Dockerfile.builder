# ============================================================================
# Dockerfile.alpine-builder - Build Alpine images with full tooling
# ============================================================================
#
# This container provides all tools needed to build Alpine qcow2 images:
# - QEMU with aarch64 support
# - libguestfs for image customization
# - Cloud-init tools
# - Signing tools (Ed25519)
#
# Build:
#   docker build -f Dockerfile.alpine-builder -t infrasim/alpine-builder .
#
# Run:
#   docker run --rm -v $(pwd):/workspace -w /workspace/images/alpine \
#     --privileged infrasim/alpine-builder \
#     ./build-profile.sh wg-mesh-ipv6 --output output/wg-mesh-ipv6.qcow2
#
FROM alpine:3.20

LABEL org.opencontainers.image.title="InfraSim Alpine Builder"
LABEL org.opencontainers.image.description="Build environment for Alpine qcow2 images"
LABEL org.opencontainers.image.source="https://github.com/rng-ops/infrasim"

# Install build dependencies
RUN apk add --no-cache \
    # Core tools
    bash \
    coreutils \
    curl \
    git \
    jq \
    yq \
    # QEMU
    qemu-img \
    qemu-system-aarch64 \
    qemu-system-x86_64 \
    # Filesystem tools
    e2fsprogs \
    dosfstools \
    mtools \
    parted \
    # Guestfish/libguestfs (Alpine's guestfs-tools)
    libguestfs \
    # Cloud-init
    cloud-init \
    # Cryptography
    openssl \
    libsodium \
    # Networking
    wireguard-tools \
    # Python for selftests
    python3 \
    py3-pip \
    py3-yaml \
    py3-cryptography

# Set up libguestfs to work without KVM
ENV LIBGUESTFS_BACKEND=direct
ENV LIBGUESTFS_DEBUG=0
ENV LIBGUESTFS_TRACE=0

# Create workspace
WORKDIR /workspace

# Default command
CMD ["bash"]
