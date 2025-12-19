#!/usr/bin/env python3
"""
WireGuard selftest module.
Validates WireGuard interface and peer connectivity.
"""

import json
import subprocess
import sys
from pathlib import Path
from typing import Dict, Any, List


def run_command(cmd: List[str], timeout: int = 30) -> tuple:
    """Run a command and return exit code, stdout, stderr."""
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


def test_wg_interface_up() -> Dict[str, Any]:
    """Verify WireGuard interface wg0 is up."""
    code, stdout, _ = run_command(["ip", "link", "show", "wg0"])
    
    if code != 0:
        return {
            "name": "wg_interface_up",
            "passed": False,
            "message": "WireGuard interface wg0 not found"
        }
    
    is_up = "UP" in stdout or "state UP" in stdout.upper()
    
    return {
        "name": "wg_interface_up",
        "passed": is_up,
        "message": "wg0 is UP" if is_up else "wg0 is DOWN",
        "details": {"interface_state": stdout.strip()}
    }


def test_wg_has_address() -> Dict[str, Any]:
    """Verify WireGuard interface has assigned addresses."""
    code, stdout, _ = run_command(["ip", "-j", "addr", "show", "wg0"])
    
    if code != 0:
        return {
            "name": "wg_has_address",
            "passed": False,
            "message": "Failed to get wg0 addresses"
        }
    
    try:
        data = json.loads(stdout)
        if not data:
            return {
                "name": "wg_has_address",
                "passed": False,
                "message": "No address data returned"
            }
        
        addrs = data[0].get("addr_info", [])
        ipv4_addrs = [a["local"] for a in addrs if a.get("family") == "inet"]
        ipv6_addrs = [a["local"] for a in addrs if a.get("family") == "inet6" and not a["local"].startswith("fe80")]
        
        has_addr = len(ipv4_addrs) > 0 or len(ipv6_addrs) > 0
        
        return {
            "name": "wg_has_address",
            "passed": has_addr,
            "message": f"IPv4: {ipv4_addrs}, IPv6: {ipv6_addrs}",
            "details": {"ipv4": ipv4_addrs, "ipv6": ipv6_addrs}
        }
    except (json.JSONDecodeError, IndexError, KeyError) as e:
        return {
            "name": "wg_has_address",
            "passed": False,
            "message": f"Failed to parse address info: {e}"
        }


def test_wg_handshake_recent() -> Dict[str, Any]:
    """Verify at least one peer has a recent handshake (<5 minutes)."""
    code, stdout, _ = run_command(["wg", "show", "wg0", "latest-handshakes"])
    
    if code != 0:
        return {
            "name": "wg_handshake_recent",
            "passed": False,
            "message": "Failed to get WireGuard handshakes"
        }
    
    import time
    current_time = int(time.time())
    max_age = 300  # 5 minutes
    
    recent_peers = []
    stale_peers = []
    
    for line in stdout.strip().split("\n"):
        if not line:
            continue
        parts = line.split("\t")
        if len(parts) != 2:
            continue
        
        pubkey, timestamp = parts
        timestamp = int(timestamp) if timestamp.isdigit() else 0
        
        if timestamp == 0:
            stale_peers.append(pubkey[:16] + "...")
        elif current_time - timestamp < max_age:
            recent_peers.append(pubkey[:16] + "...")
        else:
            stale_peers.append(pubkey[:16] + "...")
    
    # Pass if we have at least one recent handshake, or no peers configured
    passed = len(recent_peers) > 0 or (len(recent_peers) == 0 and len(stale_peers) == 0)
    
    if len(recent_peers) == 0 and len(stale_peers) == 0:
        message = "No peers configured"
    else:
        message = f"Recent: {len(recent_peers)}, Stale: {len(stale_peers)}"
    
    return {
        "name": "wg_handshake_recent",
        "passed": passed,
        "message": message,
        "details": {"recent_peers": recent_peers, "stale_peers": stale_peers}
    }


def test_wg_peer_verified() -> Dict[str, Any]:
    """Verify peer signature verification is working."""
    peers_dir = Path("/etc/wireguard/peers")
    
    if not peers_dir.exists():
        return {
            "name": "wg_peer_verified",
            "passed": True,
            "message": "No peers directory - skipped"
        }
    
    verified = []
    unverified = []
    
    for peer_file in peers_dir.glob("*.json"):
        if peer_file.suffix == ".sig":
            continue
        
        node_id = peer_file.stem
        verified_marker = peers_dir / f"{node_id}.verified"
        
        if verified_marker.exists():
            verified.append(node_id)
        else:
            unverified.append(node_id)
    
    # Pass if all peers are verified or no peers present
    passed = len(unverified) == 0
    
    return {
        "name": "wg_peer_verified",
        "passed": passed,
        "message": f"Verified: {len(verified)}, Unverified: {len(unverified)}",
        "details": {"verified": verified, "unverified": unverified}
    }


def test_wg_firewall_rules() -> Dict[str, Any]:
    """Verify WireGuard firewall rules are in place."""
    code, stdout, _ = run_command(["nft", "list", "ruleset"])
    
    if code != 0:
        return {
            "name": "wg_firewall_rules",
            "passed": False,
            "message": "Failed to list nftables rules"
        }
    
    # Check for WireGuard port rule
    has_port_rule = "51820" in stdout or "wg0" in stdout.lower()
    
    return {
        "name": "wg_firewall_rules",
        "passed": has_port_rule,
        "message": "WireGuard firewall rules present" if has_port_rule else "No WireGuard firewall rules found"
    }


def test_wg_private_key_permissions() -> Dict[str, Any]:
    """Verify WireGuard private key has correct permissions."""
    key_path = Path("/etc/wireguard/privatekey")
    
    if not key_path.exists():
        return {
            "name": "wg_private_key_permissions",
            "passed": False,
            "message": "Private key not found"
        }
    
    stat = key_path.stat()
    mode = stat.st_mode & 0o777
    
    # Should be 0600 (owner read/write only)
    passed = mode == 0o600
    
    return {
        "name": "wg_private_key_permissions",
        "passed": passed,
        "message": f"Permissions: {oct(mode)}" + (" (correct)" if passed else " (should be 0600)"),
        "details": {"mode": oct(mode), "expected": "0o600"}
    }


def run_all_tests() -> Dict[str, Any]:
    """Run all WireGuard tests."""
    tests = [
        test_wg_interface_up,
        test_wg_has_address,
        test_wg_handshake_recent,
        test_wg_peer_verified,
        test_wg_firewall_rules,
        test_wg_private_key_permissions,
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
        "feature": "vpn-wireguard",
        "version": "1.0.0",
        "passed": passed_count == total_count,
        "summary": f"{passed_count}/{total_count} tests passed",
        "tests": results
    }


def main():
    """Run selftests and output results."""
    results = run_all_tests()
    print(json.dumps(results, indent=2))
    sys.exit(0 if results["passed"] else 1)


if __name__ == "__main__":
    main()
