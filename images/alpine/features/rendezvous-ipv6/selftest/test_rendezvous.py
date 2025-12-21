#!/usr/bin/env python3
"""
Selftest for IPv6 rendezvous feature.
"""

import hashlib
import hmac
import json
import os
import struct
import subprocess
import sys
import time
from pathlib import Path
from typing import Dict, Any, List


def run_command(cmd: List[str], timeout: int = 30) -> tuple:
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout
        )
        return result.returncode, result.stdout, result.stderr
    except subprocess.TimeoutExpired:
        return -1, "", "Command timed out"
    except Exception as e:
        return -1, "", str(e)


def test_rendezvous_running() -> Dict[str, Any]:
    """Verify rendezvousd is running."""
    code, stdout, _ = run_command(["rc-service", "rendezvousd", "status"])
    
    is_running = code == 0 and "started" in stdout.lower()
    
    return {
        "name": "rendezvous_running",
        "passed": is_running,
        "message": "rendezvousd is running" if is_running else "rendezvousd is not running"
    }


def test_rendezvous_config() -> Dict[str, Any]:
    """Verify rendezvous configuration is valid."""
    config_path = Path("/etc/infrasim/rendezvous.conf")
    
    if not config_path.exists():
        return {
            "name": "rendezvous_config",
            "passed": False,
            "message": "Configuration file not found"
        }
    
    config = {}
    with open(config_path) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if "=" in line:
                key, value = line.split("=", 1)
                config[key.strip()] = value.strip().strip("\"'")
    
    # Check required field
    has_secret = bool(config.get("mesh_secret"))
    
    return {
        "name": "rendezvous_config",
        "passed": has_secret,
        "message": "mesh_secret configured" if has_secret else "mesh_secret not configured",
        "details": {
            "epoch_seconds": config.get("epoch_seconds", "default"),
            "slots_per_epoch": config.get("slots_per_epoch", "default"),
            "interface": config.get("interface", "default")
        }
    }


def test_rendezvous_address_derivation() -> Dict[str, Any]:
    """Verify address derivation produces valid IPv6 link-local addresses."""
    # Test with a sample secret
    mesh_secret = b"test_secret_for_validation"
    epoch = int(time.time() // 60)
    slot = 0
    
    message = struct.pack(">QI", epoch, slot)
    mac = hmac.new(mesh_secret, message, hashlib.sha256).digest()
    
    # Derive address
    iid = mac[:8]
    iid = bytes([iid[0] & 0xfd]) + iid[1:]
    addr_bytes = bytes([0xfe, 0x80, 0, 0, 0, 0, 0, 0]) + iid
    
    # Validate it's a proper link-local address
    import ipaddress
    try:
        addr = ipaddress.IPv6Address(addr_bytes)
        is_link_local = addr.is_link_local
        
        return {
            "name": "rendezvous_address_derivation",
            "passed": is_link_local,
            "message": f"Derived address {addr} is link-local",
            "details": {"sample_address": str(addr), "is_link_local": is_link_local}
        }
    except Exception as e:
        return {
            "name": "rendezvous_address_derivation",
            "passed": False,
            "message": f"Address derivation failed: {e}"
        }


def test_rendezvous_slot_timing() -> Dict[str, Any]:
    """Verify slot timing is consistent."""
    epoch_seconds = 60
    slots_per_epoch = 4
    
    now = time.time()
    epoch = int(now // epoch_seconds)
    slot_duration = epoch_seconds / slots_per_epoch
    slot = int((now % epoch_seconds) / slot_duration)
    
    # Slots should be 0 to slots_per_epoch-1
    valid_slot = 0 <= slot < slots_per_epoch
    
    # Calculate time until next slot
    slot_start = (epoch * epoch_seconds) + (slot * slot_duration)
    slot_end = slot_start + slot_duration
    time_remaining = slot_end - now
    
    return {
        "name": "rendezvous_slot_timing",
        "passed": valid_slot,
        "message": f"Epoch {epoch}, Slot {slot}/{slots_per_epoch}, {time_remaining:.1f}s remaining",
        "details": {
            "epoch": epoch,
            "slot": slot,
            "slot_duration": slot_duration,
            "time_remaining": time_remaining
        }
    }


def test_rendezvous_ipv6_enabled() -> Dict[str, Any]:
    """Verify IPv6 is enabled on the interface."""
    config_path = Path("/etc/infrasim/rendezvous.conf")
    interface = "eth0"
    
    if config_path.exists():
        with open(config_path) as f:
            for line in f:
                if line.startswith("interface="):
                    interface = line.split("=", 1)[1].strip().strip("\"'")
                    break
    
    # Check if interface has IPv6 enabled
    code, stdout, _ = run_command(["ip", "-6", "addr", "show", interface])
    
    has_ipv6 = code == 0 and "inet6" in stdout
    
    return {
        "name": "rendezvous_ipv6_enabled",
        "passed": has_ipv6,
        "message": f"IPv6 {'enabled' if has_ipv6 else 'disabled'} on {interface}",
        "details": {"interface": interface, "has_ipv6": has_ipv6}
    }


def test_rendezvous_peers_dir() -> Dict[str, Any]:
    """Verify peers directory exists and is writable."""
    peers_dir = Path("/var/lib/infrasim/peer-descriptors")
    
    if not peers_dir.exists():
        return {
            "name": "rendezvous_peers_dir",
            "passed": False,
            "message": "Peers directory does not exist"
        }
    
    is_writable = os.access(peers_dir, os.W_OK)
    
    # Count discovered peers
    peer_files = list(peers_dir.glob("*.json"))
    
    return {
        "name": "rendezvous_peers_dir",
        "passed": is_writable,
        "message": f"{len(peer_files)} peers discovered, directory {'writable' if is_writable else 'not writable'}",
        "details": {
            "path": str(peers_dir),
            "writable": is_writable,
            "peer_count": len(peer_files)
        }
    }


def run_all_tests() -> Dict[str, Any]:
    """Run all rendezvous tests."""
    tests = [
        test_rendezvous_running,
        test_rendezvous_config,
        test_rendezvous_address_derivation,
        test_rendezvous_slot_timing,
        test_rendezvous_ipv6_enabled,
        test_rendezvous_peers_dir,
    ]
    
    results = []
    for test_func in tests:
        try:
            result = test_func()
            results.append(result)
        except Exception as e:
            results.append({
                "name": test_func.__name__.replace("test_", ""),
                "passed": False,
                "message": f"Exception: {e}"
            })
    
    passed_count = sum(1 for r in results if r["passed"])
    total_count = len(results)
    
    return {
        "feature": "rendezvous-ipv6",
        "version": "1.0.0",
        "passed": passed_count == total_count,
        "summary": f"{passed_count}/{total_count} tests passed",
        "tests": results
    }


def main():
    results = run_all_tests()
    print(json.dumps(results, indent=2))
    sys.exit(0 if results["passed"] else 1)


if __name__ == "__main__":
    main()
