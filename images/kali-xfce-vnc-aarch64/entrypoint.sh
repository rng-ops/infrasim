#!/bin/bash
#
# Entrypoint script for Kali Linux container/VM
#
# Starts SSH and VNC services

set -e

echo "==================================="
echo " Kali Linux XFCE - InfraSim Image"
echo "==================================="

# Start SSH server
if [ -x /usr/sbin/sshd ]; then
    echo "Starting SSH server..."
    /usr/sbin/sshd
fi

# Start VNC server as kali user
echo "Starting VNC server..."
su - kali -c "/usr/local/bin/vnc-startup.sh" &

# Optional: Start noVNC web interface
if [ -x /usr/share/novnc/utils/novnc_proxy ]; then
    echo "Starting noVNC web interface on port 6080..."
    /usr/share/novnc/utils/novnc_proxy \
        --vnc localhost:5901 \
        --listen 6080 \
        --web /usr/share/novnc &
fi

echo ""
echo "Services started:"
echo "  - SSH:   port 22"
echo "  - VNC:   port 5901 (display :1)"
echo "  - noVNC: port 6080 (web interface)"
echo ""
echo "Default credentials: kali / kali"
echo ""

# Keep container running
exec tail -f /dev/null
