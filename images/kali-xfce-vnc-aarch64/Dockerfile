# Kali Linux XFCE aarch64 Image for InfraSim
#
# This builds a Kali Linux image optimized for ARM64 with:
# - XFCE desktop environment
# - VNC server for remote access
# - Common penetration testing tools
# - Cloud-init for automated configuration
#
# Based on the official Kali ARM64 cloud image

FROM arm64v8/debian:bookworm-slim AS base

# Set environment variables
ENV DEBIAN_FRONTEND=noninteractive
ENV LANG=C.UTF-8
ENV LC_ALL=C.UTF-8

# Add Kali repository
RUN apt-get update && apt-get install -y \
    curl \
    gnupg \
    ca-certificates \
    && curl -fsSL https://archive.kali.org/archive-key.asc | gpg --dearmor -o /etc/apt/trusted.gpg.d/kali.gpg \
    && echo "deb http://http.kali.org/kali kali-rolling main contrib non-free non-free-firmware" > /etc/apt/sources.list.d/kali.list \
    && apt-get update

# Install base system
RUN apt-get install -y \
    kali-linux-core \
    kali-desktop-xfce \
    kali-tools-top10 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Install VNC and remote access
RUN apt-get update && apt-get install -y \
    tigervnc-standalone-server \
    tigervnc-common \
    novnc \
    websockify \
    dbus-x11 \
    x11-xserver-utils \
    xfonts-base \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Install cloud-init for automated configuration
RUN apt-get update && apt-get install -y \
    cloud-init \
    cloud-guest-utils \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Install additional useful tools
RUN apt-get update && apt-get install -y \
    openssh-server \
    sudo \
    vim \
    tmux \
    htop \
    net-tools \
    iproute2 \
    iputils-ping \
    dnsutils \
    curl \
    wget \
    git \
    python3 \
    python3-pip \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Create kali user
RUN useradd -m -s /bin/bash -G sudo kali \
    && echo "kali:kali" | chpasswd \
    && echo "kali ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Configure VNC
RUN mkdir -p /home/kali/.vnc \
    && echo "kali" | vncpasswd -f > /home/kali/.vnc/passwd \
    && chmod 600 /home/kali/.vnc/passwd \
    && chown -R kali:kali /home/kali/.vnc

# VNC startup script
COPY vnc-startup.sh /usr/local/bin/vnc-startup.sh
RUN chmod +x /usr/local/bin/vnc-startup.sh

# Configure xstartup for XFCE
RUN echo '#!/bin/bash' > /home/kali/.vnc/xstartup \
    && echo 'unset SESSION_MANAGER' >> /home/kali/.vnc/xstartup \
    && echo 'unset DBUS_SESSION_BUS_ADDRESS' >> /home/kali/.vnc/xstartup \
    && echo 'exec startxfce4' >> /home/kali/.vnc/xstartup \
    && chmod +x /home/kali/.vnc/xstartup \
    && chown kali:kali /home/kali/.vnc/xstartup

# Configure cloud-init
COPY cloud.cfg /etc/cloud/cloud.cfg

# Configure SSH
RUN mkdir -p /var/run/sshd \
    && sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin no/' /etc/ssh/sshd_config \
    && sed -i 's/#PasswordAuthentication yes/PasswordAuthentication yes/' /etc/ssh/sshd_config

# Enable services
RUN systemctl enable ssh || true

# Set default display
ENV DISPLAY=:1

# Expose ports
EXPOSE 22 5901 6080

# Start script
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

CMD ["/entrypoint.sh"]
