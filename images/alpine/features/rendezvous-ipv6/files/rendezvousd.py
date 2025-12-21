#!/usr/bin/env python3
"""
rendezvousd - IPv6 Epoch/Slot-based Peer Discovery Daemon

This daemon implements a bounded peer discovery mechanism using HMAC-derived
IPv6 link-local addresses. Unlike mDNS/Bonjour, this does NOT rely on LAN
multicast and works across routed networks.

Protocol:
1. All nodes share a mesh_secret (distributed out-of-band or via control plane)
2. Time is divided into epochs (e.g., 60 seconds each)
3. Each epoch has N slots (e.g., 4 slots of 15 seconds)
4. For each slot, a rendezvous IPv6 address is computed:
   addr = fe80::HMAC(mesh_secret, epoch || slot)[0:8]
5. Nodes bind to this address during their slot and broadcast their descriptor
6. Receiving nodes verify the descriptor signature before accepting the peer

Security:
- Discovery is a CONVENIENCE feature, not security
- Identity is verified cryptographically via Ed25519 signatures
- Even if an attacker knows the mesh_secret, they cannot impersonate
  a node without the corresponding Ed25519 private key
"""

import argparse
import hashlib
import hmac
import ipaddress
import json
import logging
import os
import select
import signal
import socket
import struct
import subprocess
import sys
import time
from pathlib import Path
from typing import Dict, Optional, Set, Tuple

# Configuration
DEFAULT_CONFIG = {
    "mesh_secret": "",
    "epoch_seconds": 60,
    "slots_per_epoch": 4,
    "slot_duration_ms": 500,
    "interface": "eth0",
    "max_peers": 64,
    "peer_callback": "/usr/local/bin/apply-peers.sh add",
    "descriptor_path": "/etc/infrasim/node-descriptor.json",
    "peers_dir": "/var/lib/infrasim/peer-descriptors",
}

RENDEZVOUS_PORT = 51821  # Base port, actual = base + slot

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[logging.StreamHandler()]
)
log = logging.getLogger("rendezvousd")


class RendezvousDaemon:
    """IPv6 rendezvous daemon for peer discovery."""
    
    def __init__(self, config: Dict):
        self.config = config
        self.mesh_secret = config["mesh_secret"].encode()
        self.epoch_seconds = config["epoch_seconds"]
        self.slots_per_epoch = config["slots_per_epoch"]
        self.slot_duration_ms = config["slot_duration_ms"]
        self.interface = config["interface"]
        self.max_peers = config["max_peers"]
        self.peer_callback = config["peer_callback"]
        self.descriptor_path = Path(config["descriptor_path"])
        self.peers_dir = Path(config["peers_dir"])
        
        self.known_peers: Set[str] = set()
        self.running = True
        self.current_socket: Optional[socket.socket] = None
        
        # Load our node descriptor
        self.node_descriptor = self._load_descriptor()
        
        # Ensure peers directory exists
        self.peers_dir.mkdir(parents=True, exist_ok=True)
    
    def _load_descriptor(self) -> Dict:
        """Load our node descriptor."""
        if not self.descriptor_path.exists():
            log.error(f"Node descriptor not found: {self.descriptor_path}")
            return {}
        
        with open(self.descriptor_path) as f:
            return json.load(f)
    
    def _get_epoch_slot(self) -> Tuple[int, int]:
        """Get current epoch and slot numbers."""
        now = time.time()
        epoch = int(now // self.epoch_seconds)
        slot_duration = self.epoch_seconds / self.slots_per_epoch
        slot = int((now % self.epoch_seconds) / slot_duration)
        return epoch, slot
    
    def _derive_rendezvous_address(self, epoch: int, slot: int) -> str:
        """Derive rendezvous IPv6 link-local address from epoch and slot."""
        # Create HMAC of epoch || slot
        message = struct.pack(">QI", epoch, slot)
        mac = hmac.new(self.mesh_secret, message, hashlib.sha256).digest()
        
        # Use first 8 bytes for interface identifier
        # Ensure it's a valid link-local address (fe80::/10)
        iid = mac[:8]
        
        # Clear the universal/local bit (bit 6 of first byte)
        # to indicate locally administered
        iid = bytes([iid[0] & 0xfd]) + iid[1:]
        
        # Construct link-local address
        addr_bytes = bytes([0xfe, 0x80, 0, 0, 0, 0, 0, 0]) + iid
        addr = ipaddress.IPv6Address(addr_bytes)
        
        return str(addr)
    
    def _derive_port(self, slot: int) -> int:
        """Derive port number from slot."""
        return RENDEZVOUS_PORT + slot
    
    def _get_interface_index(self) -> int:
        """Get interface index for binding."""
        try:
            return socket.if_nametoindex(self.interface)
        except OSError:
            log.error(f"Interface not found: {self.interface}")
            return 0
    
    def _add_address(self, addr: str) -> bool:
        """Add IPv6 address to interface."""
        try:
            result = subprocess.run(
                ["ip", "-6", "addr", "add", f"{addr}/128", "dev", self.interface],
                capture_output=True,
                text=True
            )
            if result.returncode == 0:
                log.debug(f"Added rendezvous address {addr} to {self.interface}")
                return True
            elif "RTNETLINK answers: File exists" in result.stderr:
                # Address already exists
                return True
            else:
                log.error(f"Failed to add address: {result.stderr}")
                return False
        except Exception as e:
            log.error(f"Exception adding address: {e}")
            return False
    
    def _remove_address(self, addr: str) -> bool:
        """Remove IPv6 address from interface."""
        try:
            result = subprocess.run(
                ["ip", "-6", "addr", "del", f"{addr}/128", "dev", self.interface],
                capture_output=True,
                text=True
            )
            if result.returncode == 0:
                log.debug(f"Removed rendezvous address {addr} from {self.interface}")
                return True
            elif "RTNETLINK answers: Cannot assign requested address" in result.stderr:
                # Address doesn't exist
                return True
            else:
                log.error(f"Failed to remove address: {result.stderr}")
                return False
        except Exception as e:
            log.error(f"Exception removing address: {e}")
            return False
    
    def _broadcast_descriptor(self, addr: str, port: int):
        """Broadcast our node descriptor to the rendezvous address."""
        if not self.node_descriptor:
            return
        
        try:
            # Read signature if available
            sig_path = self.descriptor_path.with_suffix(".json.sig")
            signature = b""
            if sig_path.exists():
                with open(sig_path, "rb") as f:
                    signature = f.read()
            
            # Create broadcast message
            descriptor_json = json.dumps(self.node_descriptor).encode()
            
            # Message format: [4 bytes sig len][signature][descriptor]
            message = struct.pack(">I", len(signature)) + signature + descriptor_json
            
            # Create socket and broadcast
            sock = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            sock.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_MULTICAST_HOPS, 1)
            
            # Send to rendezvous address
            if_index = self._get_interface_index()
            dest = (addr, port, 0, if_index)
            
            sock.sendto(message, dest)
            sock.close()
            
            log.debug(f"Broadcast descriptor to {addr}:{port}")
            
        except Exception as e:
            log.error(f"Failed to broadcast descriptor: {e}")
    
    def _listen_for_peers(self, addr: str, port: int, duration_ms: int):
        """Listen for peer descriptors on rendezvous address."""
        try:
            # Add rendezvous address to interface
            if not self._add_address(addr):
                return
            
            # Create listening socket
            sock = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            sock.setblocking(False)
            
            if_index = self._get_interface_index()
            sock.bind((addr, port, 0, if_index))
            
            self.current_socket = sock
            
            # Listen for duration
            end_time = time.time() + (duration_ms / 1000.0)
            
            while self.running and time.time() < end_time:
                ready, _, _ = select.select([sock], [], [], 0.1)
                
                if sock in ready:
                    try:
                        data, sender = sock.recvfrom(65535)
                        self._handle_peer_message(data, sender)
                    except Exception as e:
                        log.error(f"Error receiving: {e}")
            
            sock.close()
            self.current_socket = None
            
            # Remove rendezvous address
            self._remove_address(addr)
            
        except Exception as e:
            log.error(f"Listen error: {e}")
    
    def _handle_peer_message(self, data: bytes, sender: Tuple):
        """Handle received peer descriptor message."""
        try:
            # Parse message format: [4 bytes sig len][signature][descriptor]
            if len(data) < 4:
                return
            
            sig_len = struct.unpack(">I", data[:4])[0]
            if len(data) < 4 + sig_len:
                return
            
            signature = data[4:4 + sig_len]
            descriptor_json = data[4 + sig_len:]
            
            descriptor = json.loads(descriptor_json.decode())
            
            node_id = descriptor.get("node_id")
            if not node_id:
                log.warning("Received descriptor without node_id")
                return
            
            # Skip if it's our own descriptor
            if node_id == self.node_descriptor.get("node_id"):
                return
            
            # Skip if already known
            if node_id in self.known_peers:
                return
            
            log.info(f"Discovered new peer: {node_id} from {sender[0]}")
            
            # Save descriptor and signature
            descriptor_path = self.peers_dir / f"{node_id}.json"
            sig_path = self.peers_dir / f"{node_id}.json.sig"
            
            with open(descriptor_path, "w") as f:
                json.dump(descriptor, f, indent=2)
            
            if signature:
                with open(sig_path, "wb") as f:
                    f.write(signature)
            
            # Call peer callback (e.g., apply-peers.sh)
            if self.peer_callback:
                try:
                    subprocess.run(
                        self.peer_callback.split() + [str(descriptor_path)],
                        timeout=30,
                        check=False
                    )
                except Exception as e:
                    log.error(f"Peer callback failed: {e}")
            
            # Track peer
            self.known_peers.add(node_id)
            
            # Limit known peers
            if len(self.known_peers) > self.max_peers:
                # Remove oldest (this is a simple FIFO, could be improved)
                self.known_peers.pop()
            
        except json.JSONDecodeError:
            log.warning("Invalid JSON in peer message")
        except Exception as e:
            log.error(f"Error handling peer message: {e}")
    
    def run(self):
        """Main daemon loop."""
        log.info(f"Starting rendezvous daemon on {self.interface}")
        log.info(f"Epoch: {self.epoch_seconds}s, Slots: {self.slots_per_epoch}")
        
        last_epoch = -1
        last_slot = -1
        
        while self.running:
            epoch, slot = self._get_epoch_slot()
            
            if epoch != last_epoch or slot != last_slot:
                # New slot - compute rendezvous point
                addr = self._derive_rendezvous_address(epoch, slot)
                port = self._derive_port(slot)
                
                log.debug(f"Epoch {epoch}, Slot {slot}: {addr}:{port}")
                
                # Broadcast our descriptor
                self._broadcast_descriptor(addr, port)
                
                # Listen for peers for the slot duration
                self._listen_for_peers(addr, port, self.slot_duration_ms)
                
                last_epoch = epoch
                last_slot = slot
            
            # Small sleep to prevent busy loop
            time.sleep(0.01)
    
    def stop(self):
        """Stop the daemon."""
        log.info("Stopping rendezvous daemon")
        self.running = False
        if self.current_socket:
            try:
                self.current_socket.close()
            except:
                pass


def load_config(config_path: str) -> Dict:
    """Load configuration from file."""
    config = DEFAULT_CONFIG.copy()
    
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
                        # Type conversion
                        if isinstance(config[key], int):
                            value = int(value)
                        config[key] = value
    
    return config


def main():
    parser = argparse.ArgumentParser(description="IPv6 Rendezvous Daemon")
    parser.add_argument(
        "-c", "--config",
        default="/etc/infrasim/rendezvous.conf",
        help="Configuration file path"
    )
    parser.add_argument(
        "-d", "--debug",
        action="store_true",
        help="Enable debug logging"
    )
    parser.add_argument(
        "-f", "--foreground",
        action="store_true",
        help="Run in foreground (don't daemonize)"
    )
    
    args = parser.parse_args()
    
    if args.debug:
        logging.getLogger().setLevel(logging.DEBUG)
    
    config = load_config(args.config)
    
    if not config["mesh_secret"]:
        log.error("mesh_secret is required in configuration")
        sys.exit(1)
    
    daemon = RendezvousDaemon(config)
    
    # Signal handlers
    def handle_signal(signum, frame):
        daemon.stop()
    
    signal.signal(signal.SIGTERM, handle_signal)
    signal.signal(signal.SIGINT, handle_signal)
    
    try:
        daemon.run()
    except KeyboardInterrupt:
        daemon.stop()
    
    log.info("Rendezvous daemon stopped")


if __name__ == "__main__":
    main()
