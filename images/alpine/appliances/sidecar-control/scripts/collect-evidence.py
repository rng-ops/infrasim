#!/usr/bin/env python3
"""
collect-evidence.py - Collect evidence from target VM for provenance

Gathers configuration, logs, and state from target VM to include
in attestation materials.

Usage: collect-evidence.py --target HOST [--output-dir DIR]
"""

import argparse
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, Any, List, Optional


class EvidenceCollector:
    """Collect evidence from target VM."""
    
    EVIDENCE_ITEMS = [
        {
            "name": "node_descriptor",
            "command": "cat /etc/infrasim/node-descriptor.json",
            "format": "json"
        },
        {
            "name": "manifest",
            "command": "cat /etc/infrasim/manifest.json 2>/dev/null || echo '{}'",
            "format": "json"
        },
        {
            "name": "network_config",
            "command": "ip -j addr show",
            "format": "json"
        },
        {
            "name": "routes",
            "command": "ip -j route show",
            "format": "json"
        },
        {
            "name": "firewall_rules",
            "command": "nft -j list ruleset 2>/dev/null || echo '[]'",
            "format": "json"
        },
        {
            "name": "services",
            "command": "rc-status -a 2>/dev/null | grep -E 'started|stopped' || echo 'N/A'",
            "format": "text"
        },
        {
            "name": "wireguard_status",
            "command": "wg show all 2>/dev/null || echo 'not configured'",
            "format": "text"
        },
        {
            "name": "tailscale_status",
            "command": "tailscale status --json 2>/dev/null || echo '{}'",
            "format": "json"
        },
        {
            "name": "cloud_init_result",
            "command": "cat /var/lib/cloud/data/result.json 2>/dev/null || echo '{}'",
            "format": "json"
        },
        {
            "name": "packages",
            "command": "apk info -v 2>/dev/null | sort || echo 'N/A'",
            "format": "text"
        },
        {
            "name": "kernel_version",
            "command": "uname -a",
            "format": "text"
        },
        {
            "name": "boot_log",
            "command": "dmesg | tail -100",
            "format": "text"
        }
    ]
    
    def __init__(self, host: str, port: int = 22, user: str = "root",
                 key_file: Optional[str] = None, output_dir: str = "/tmp/evidence"):
        self.host = host
        self.port = port
        self.user = user
        self.key_file = key_file
        self.output_dir = Path(output_dir)
        self.output_dir.mkdir(parents=True, exist_ok=True)
    
    def _ssh_run(self, command: str, timeout: int = 30) -> tuple:
        """Run command on target."""
        ssh_args = [
            "ssh",
            "-o", "StrictHostKeyChecking=no",
            "-o", "UserKnownHostsFile=/dev/null",
            "-o", "ConnectTimeout=10",
            "-p", str(self.port),
        ]
        if self.key_file:
            ssh_args.extend(["-i", self.key_file])
        ssh_args.append(f"{self.user}@{self.host}")
        ssh_args.append(command)
        
        try:
            result = subprocess.run(
                ssh_args,
                capture_output=True,
                text=True,
                timeout=timeout
            )
            return result.returncode, result.stdout, result.stderr
        except subprocess.TimeoutExpired:
            return -1, "", "timeout"
        except Exception as e:
            return -1, "", str(e)
    
    def collect_item(self, item: Dict[str, str]) -> Dict[str, Any]:
        """Collect single evidence item."""
        name = item["name"]
        command = item["command"]
        fmt = item.get("format", "text")
        
        code, stdout, stderr = self._ssh_run(command)
        
        result = {
            "name": name,
            "collected": code == 0,
            "timestamp": datetime.now(timezone.utc).isoformat()
        }
        
        if code == 0:
            if fmt == "json":
                try:
                    result["data"] = json.loads(stdout)
                except json.JSONDecodeError:
                    result["data"] = stdout
                    result["format"] = "text"
            else:
                result["data"] = stdout.strip()
        else:
            result["error"] = stderr.strip() or "Command failed"
        
        return result
    
    def collect_all(self) -> Dict[str, Any]:
        """Collect all evidence items."""
        evidence = {
            "target": self.host,
            "collected_at": datetime.now(timezone.utc).isoformat(),
            "items": {}
        }
        
        for item in self.EVIDENCE_ITEMS:
            print(f"Collecting: {item['name']}...", file=sys.stderr)
            result = self.collect_item(item)
            evidence["items"][item["name"]] = result
        
        return evidence
    
    def save(self, evidence: Dict[str, Any], filename: str = "evidence.json"):
        """Save evidence to file."""
        output_file = self.output_dir / filename
        with open(output_file, "w") as f:
            json.dump(evidence, f, indent=2, default=str)
        return output_file


def main():
    parser = argparse.ArgumentParser(description="Collect evidence from target VM")
    parser.add_argument("--target", required=True, help="Target host")
    parser.add_argument("--port", type=int, default=22, help="SSH port")
    parser.add_argument("--user", default="root", help="SSH user")
    parser.add_argument("--key", help="SSH key file")
    parser.add_argument("--output-dir", default="/tmp/evidence", help="Output directory")
    parser.add_argument("--json", action="store_true", help="Output JSON only")
    
    args = parser.parse_args()
    
    collector = EvidenceCollector(
        args.target, args.port, args.user, args.key, args.output_dir
    )
    
    if not args.json:
        print(f"Collecting evidence from {args.target}...", file=sys.stderr)
    
    evidence = collector.collect_all()
    
    if args.json:
        print(json.dumps(evidence, indent=2, default=str))
    else:
        output_file = collector.save(evidence)
        print(f"\nEvidence saved to: {output_file}")
        
        # Summary
        items = evidence.get("items", {})
        collected = sum(1 for v in items.values() if v.get("collected"))
        print(f"Collected: {collected}/{len(items)} items")


if __name__ == "__main__":
    main()
