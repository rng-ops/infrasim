#!/usr/bin/env python3
"""
rendezvous-client - Command-line client for IPv6 rendezvous

Manually trigger rendezvous discovery or broadcast descriptor.

Usage: rendezvous-client broadcast|discover|status
"""

import argparse
import hashlib
import hmac
import ipaddress
import json
import os
import socket
import struct
import subprocess
import sys
import time
from pathlib import Path
from typing import Dict, List, Optional

RENDEZVOUS_PORT = 51821


def load_config(config_path: str) -> Dict:
    """Load configuration from file."""
    config = {
        "mesh_secret": "",
        "epoch_seconds": 60,
        "slots_per_epoch": 4,
        "interface": "eth0",
        "descriptor_path": "/etc/infrasim/node-descriptor.json",
    }
    
    if os.path.exists(config_path):
        with open(config_path) as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                if "=" in line:
                    key, value = line.split("=", 1)
                    key = key.strip()
                    value = value.strip().strip('"\'')
                    if key in config:
                        if isinstance(config[key], int):
                            value = int(value)
                        config[key] = value
    
    return config


def derive_rendezvous_address(mesh_secret: bytes, epoch: int, slot: int) -> str:
    """Derive rendezvous IPv6 address."""
    message = struct.pack(">QI", epoch, slot)
    mac = hmac.new(mesh_secret, message, hashlib.sha256).digest()
    iid = mac[:8]
    iid = bytes([iid[0] & 0xfd]) + iid[1:]
    addr_bytes = bytes([0xfe, 0x80, 0, 0, 0, 0, 0, 0]) + iid
    return str(ipaddress.IPv6Address(addr_bytes))


def get_epoch_slot(epoch_seconds: int, slots_per_epoch: int):
    """Get current epoch and slot."""
    now = time.time()
    epoch = int(now // epoch_seconds)
    slot_duration = epoch_seconds / slots_per_epoch
    slot = int((now % epoch_seconds) / slot_duration)
    return epoch, slot


def add_address(interface: str, addr: str) -> bool:
    """Add IPv6 address to interface."""
    result = subprocess.run(
        ["ip", "-6", "addr", "add", f"{addr}/128", "dev", interface],
        capture_output=True
    )
    return result.returncode == 0 or "exists" in result.stderr.decode()


def remove_address(interface: str, addr: str):
    """Remove IPv6 address from interface."""
    subprocess.run(
        ["ip", "-6", "addr", "del", f"{addr}/128", "dev", interface],
        capture_output=True
    )


def broadcast_descriptor(config: Dict):
    """Broadcast our descriptor to current rendezvous point."""
    mesh_secret = config["mesh_secret"].encode()
    epoch, slot = get_epoch_slot(config["epoch_seconds"], config["slots_per_epoch"])
    addr = derive_rendezvous_address(mesh_secret, epoch, slot)
    port = RENDEZVOUS_PORT + slot
    
    print(f"Epoch {epoch}, Slot {slot}")
    print(f"Rendezvous: {addr}:{port}")
    
    # Add address to interface
    if not add_address(config["interface"], addr):
        print("Failed to add rendezvous address")
        return
    
    try:
        # Load descriptor
        descriptor_path = Path(config["descriptor_path"])
        if not descriptor_path.exists():
            print(f"Descriptor not found: {descriptor_path}")
            return
        
        with open(descriptor_path) as f:
            descriptor = json.load(f)
        
        # Load signature if available
        sig_path = descriptor_path.with_suffix(".json.sig")
        signature = b""
        if sig_path.exists():
            with open(sig_path, "rb") as f:
                signature = f.read()
        
        # Create message
        descriptor_json = json.dumps(descriptor).encode()
        message = struct.pack(">I", len(signature)) + signature + descriptor_json
        
        # Send
        sock = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        
        if_index = socket.if_nametoindex(config["interface"])
        dest = (addr, port, 0, if_index)
        
        sock.sendto(message, dest)
        sock.close()
        
        print(f"Broadcast descriptor ({len(message)} bytes)")
        print(f"Node ID: {descriptor.get('node_id', 'unknown')}")
        
    finally:
        remove_address(config["interface"], addr)


def discover_peers(config: Dict, timeout: int = 5) -> List[Dict]:
    """Listen for peer descriptors on current rendezvous point."""
    mesh_secret = config["mesh_secret"].encode()
    epoch, slot = get_epoch_slot(config["epoch_seconds"], config["slots_per_epoch"])
    addr = derive_rendezvous_address(mesh_secret, epoch, slot)
    port = RENDEZVOUS_PORT + slot
    
    print(f"Epoch {epoch}, Slot {slot}")
    print(f"Rendezvous: {addr}:{port}")
    print(f"Listening for {timeout} seconds...")
    
    # Add address to interface
    if not add_address(config["interface"], addr):
        print("Failed to add rendezvous address")
        return []
    
    peers = []
    
    try:
        sock = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        sock.settimeout(1.0)
        
        if_index = socket.if_nametoindex(config["interface"])
        sock.bind((addr, port, 0, if_index))
        
        end_time = time.time() + timeout
        
        while time.time() < end_time:
            try:
                data, sender = sock.recvfrom(65535)
                
                # Parse message
                if len(data) < 4:
                    continue
                
                sig_len = struct.unpack(">I", data[:4])[0]
                if len(data) < 4 + sig_len:
                    continue
                
                descriptor_json = data[4 + sig_len:]
                descriptor = json.loads(descriptor_json.decode())
                
                node_id = descriptor.get("node_id", "unknown")
                print(f"  Found: {node_id} from {sender[0]}")
                peers.append(descriptor)
                
            except socket.timeout:
                continue
            except json.JSONDecodeError:
                continue
        
        sock.close()
        
    finally:
        remove_address(config["interface"], addr)
    
    return peers


def show_status(config: Dict):
    """Show current rendezvous status."""
    mesh_secret = config["mesh_secret"].encode() if config["mesh_secret"] else None
    
    if not mesh_secret:
        print("mesh_secret not configured")
        return
    
    epoch_seconds = config["epoch_seconds"]
    slots_per_epoch = config["slots_per_epoch"]
    
    now = time.time()
    epoch, slot = get_epoch_slot(epoch_seconds, slots_per_epoch)
    addr = derive_rendezvous_address(mesh_secret, epoch, slot)
    port = RENDEZVOUS_PORT + slot
    
    slot_duration = epoch_seconds / slots_per_epoch
    slot_start = (epoch * epoch_seconds) + (slot * slot_duration)
    slot_end = slot_start + slot_duration
    time_remaining = slot_end - now
    
    print(f"Current Time:     {time.strftime('%Y-%m-%d %H:%M:%S')}")
    print(f"Epoch:            {epoch}")
    print(f"Slot:             {slot}/{slots_per_epoch}")
    print(f"Time in slot:     {time_remaining:.1f}s remaining")
    print(f"Rendezvous Addr:  {addr}")
    print(f"Rendezvous Port:  {port}")
    print(f"Interface:        {config['interface']}")
    
    # Show next few slots
    print("\nUpcoming slots:")
    for i in range(1, 4):
        future_slot = (slot + i) % slots_per_epoch
        future_epoch = epoch if (slot + i) < slots_per_epoch else epoch + 1
        future_addr = derive_rendezvous_address(mesh_secret, future_epoch, future_slot)
        future_port = RENDEZVOUS_PORT + future_slot
        print(f"  +{i}: Epoch {future_epoch}, Slot {future_slot} -> {future_addr}:{future_port}")


def main():
    parser = argparse.ArgumentParser(description="IPv6 Rendezvous Client")
    parser.add_argument(
        "command",
        choices=["broadcast", "discover", "status"],
        help="Command to execute"
    )
    parser.add_argument(
        "-c", "--config",
        default="/etc/infrasim/rendezvous.conf",
        help="Configuration file"
    )
    parser.add_argument(
        "-t", "--timeout",
        type=int,
        default=5,
        help="Discovery timeout in seconds"
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output JSON"
    )
    
    args = parser.parse_args()
    config = load_config(args.config)
    
    if not config["mesh_secret"]:
        print("Error: mesh_secret not configured", file=sys.stderr)
        sys.exit(1)
    
    if args.command == "broadcast":
        broadcast_descriptor(config)
    elif args.command == "discover":
        peers = discover_peers(config, args.timeout)
        if args.json:
            print(json.dumps(peers, indent=2))
        else:
            print(f"\nDiscovered {len(peers)} peer(s)")
    elif args.command == "status":
        show_status(config)


if __name__ == "__main__":
    main()
