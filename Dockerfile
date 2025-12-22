# Multi-stage Dockerfile for InfraSim Daemon
# Target: ARM64 Linux (for Docker Desktop on macOS)

FROM rustlang/rust:nightly-slim AS builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    libprotobuf-dev \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy source
COPY . .

# Build release binary (using nightly for edition2024 support)
RUN cargo build --release --package infrasim-daemon --bin infrasimd

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    qemu-system-aarch64 \
    qemu-utils \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -u 1000 infrasim

# Create directories
RUN mkdir -p /var/lib/infrasim/{images,volumes,snapshots} && \
    chown -R infrasim:infrasim /var/lib/infrasim

# Copy binary
COPY --from=builder /build/target/release/infrasimd /usr/local/bin/

# Copy default config
COPY --chown=infrasim:infrasim <<EOF /etc/infrasim/config.toml
[daemon]
grpc_listen = "0.0.0.0:50051"
data_dir = "/var/lib/infrasim"
qemu_path = "/usr/bin/qemu-system-aarch64"

[storage]
images_dir = "images"
volumes_dir = "volumes"
snapshots_dir = "snapshots"

[network]
default_mode = "user"
EOF

USER infrasim
WORKDIR /home/infrasim

EXPOSE 50051

ENTRYPOINT ["/usr/local/bin/infrasimd"]
CMD ["--config", "/etc/infrasim/config.toml"]
