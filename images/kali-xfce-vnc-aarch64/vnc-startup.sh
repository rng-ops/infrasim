#!/bin/bash
#
# VNC Startup Script for Kali Linux
#
# Starts TigerVNC server with XFCE desktop

set -e

VNC_DISPLAY="${VNC_DISPLAY:-:1}"
VNC_PORT="${VNC_PORT:-5901}"
VNC_GEOMETRY="${VNC_GEOMETRY:-1920x1080}"
VNC_DEPTH="${VNC_DEPTH:-24}"

# Kill any existing VNC sessions
vncserver -kill "${VNC_DISPLAY}" 2>/dev/null || true

# Clean up stale lock files
rm -f /tmp/.X*-lock /tmp/.X11-unix/X* 2>/dev/null || true

# Start VNC server
echo "Starting VNC server on ${VNC_DISPLAY} (port ${VNC_PORT})..."

vncserver "${VNC_DISPLAY}" \
    -geometry "${VNC_GEOMETRY}" \
    -depth "${VNC_DEPTH}" \
    -localhost no \
    -SecurityTypes VncAuth

echo "VNC server started successfully"
echo "Connect to: vnc://localhost:${VNC_PORT}"

# Keep the script running
tail -f /dev/null
