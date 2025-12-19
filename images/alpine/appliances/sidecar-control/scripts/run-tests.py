#!/usr/bin/env python3
"""
run-tests.py - Sidecar test runner for infrasim VMs

This script connects to a target VM via SSH and runs the selftest
framework, collecting results and generating attestations.

Usage: run-tests.py --target HOST [--port PORT] [--key KEY_FILE]
"""

import argparse
import json
import os
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Any, Optional


class SSHConnection:
    """SSH connection to target VM."""
    
    def __init__(self, host: str, port: int = 22, user: str = "root", 
                 key_file: Optional[str] = None):
        self.host = host
        self.port = port
        self.user = user
        self.key_file = key_file
    
    def _ssh_args(self) -> List[str]:
        args = [
            "ssh",
            "-o", "StrictHostKeyChecking=no",
            "-o", "UserKnownHostsFile=/dev/null",
            "-o", "ConnectTimeout=10",
            "-p", str(self.port),
        ]
        if self.key_file:
            args.extend(["-i", self.key_file])
        args.append(f"{self.user}@{self.host}")
        return args
    
    def run(self, command: str, timeout: int = 60) -> tuple:
        """Run command on target and return (exit_code, stdout, stderr)."""
        try:
            result = subprocess.run(
                self._ssh_args() + [command],
                capture_output=True,
                text=True,
                timeout=timeout
            )
            return result.returncode, result.stdout, result.stderr
        except subprocess.TimeoutExpired:
            return -1, "", "Command timed out"
        except Exception as e:
            return -1, "", str(e)
    
    def copy_from(self, remote_path: str, local_path: str) -> bool:
        """Copy file from target to local."""
        try:
            scp_args = [
                "scp",
                "-o", "StrictHostKeyChecking=no",
                "-o", "UserKnownHostsFile=/dev/null",
                "-P", str(self.port),
            ]
            if self.key_file:
                scp_args.extend(["-i", self.key_file])
            scp_args.extend([
                f"{self.user}@{self.host}:{remote_path}",
                local_path
            ])
            
            result = subprocess.run(scp_args, capture_output=True)
            return result.returncode == 0
        except Exception:
            return False
    
    def wait_for_ssh(self, timeout: int = 300) -> bool:
        """Wait for SSH to become available."""
        start = time.time()
        while time.time() - start < timeout:
            code, stdout, _ = self.run("echo ok", timeout=5)
            if code == 0 and "ok" in stdout:
                return True
            time.sleep(5)
        return False


class TestRunner:
    """Run tests on target VM."""
    
    def __init__(self, ssh: SSHConnection, results_dir: str):
        self.ssh = ssh
        self.results_dir = Path(results_dir)
        self.results_dir.mkdir(parents=True, exist_ok=True)
        self.results: List[Dict[str, Any]] = []
    
    def test_boot(self) -> Dict[str, Any]:
        """Test that VM has booted successfully."""
        code, stdout, stderr = self.ssh.run("uptime")
        
        return {
            "name": "boot",
            "passed": code == 0,
            "message": stdout.strip() if code == 0 else stderr.strip(),
            "category": "boot"
        }
    
    def test_cloud_init(self) -> Dict[str, Any]:
        """Test that cloud-init completed."""
        code, stdout, _ = self.ssh.run("cat /var/lib/cloud/data/result.json")
        
        if code != 0:
            return {
                "name": "cloud_init",
                "passed": False,
                "message": "cloud-init result not found",
                "category": "boot"
            }
        
        try:
            result = json.loads(stdout)
            errors = result.get("v1", {}).get("errors", [])
            passed = len(errors) == 0
            return {
                "name": "cloud_init",
                "passed": passed,
                "message": "cloud-init completed" if passed else f"Errors: {errors}",
                "category": "boot"
            }
        except json.JSONDecodeError:
            return {
                "name": "cloud_init",
                "passed": False,
                "message": "Invalid cloud-init result JSON",
                "category": "boot"
            }
    
    def test_network_connectivity(self) -> Dict[str, Any]:
        """Test network connectivity."""
        code, stdout, _ = self.ssh.run("ip -j addr show")
        
        if code != 0:
            return {
                "name": "network_connectivity",
                "passed": False,
                "message": "Failed to get network info",
                "category": "network"
            }
        
        try:
            interfaces = json.loads(stdout)
            up_interfaces = [
                iface["ifname"]
                for iface in interfaces
                if "UP" in iface.get("flags", []) and iface["ifname"] != "lo"
            ]
            
            passed = len(up_interfaces) > 0
            return {
                "name": "network_connectivity",
                "passed": passed,
                "message": f"Up interfaces: {up_interfaces}",
                "category": "network"
            }
        except json.JSONDecodeError:
            return {
                "name": "network_connectivity",
                "passed": False,
                "message": "Failed to parse network info",
                "category": "network"
            }
    
    def test_dns_resolution(self) -> Dict[str, Any]:
        """Test DNS resolution."""
        code, _, _ = self.ssh.run("getent hosts cloudflare.com")
        
        return {
            "name": "dns_resolution",
            "passed": code == 0,
            "message": "DNS working" if code == 0 else "DNS resolution failed",
            "category": "network"
        }
    
    def test_node_descriptor(self) -> Dict[str, Any]:
        """Test node descriptor exists and is valid."""
        code, stdout, _ = self.ssh.run("cat /etc/infrasim/node-descriptor.json")
        
        if code != 0:
            return {
                "name": "node_descriptor",
                "passed": False,
                "message": "Node descriptor not found",
                "category": "security"
            }
        
        try:
            descriptor = json.loads(stdout)
            required = ["node_id", "identity", "attestation"]
            missing = [f for f in required if f not in descriptor]
            
            passed = len(missing) == 0
            return {
                "name": "node_descriptor",
                "passed": passed,
                "message": f"Node ID: {descriptor.get('node_id', 'unknown')}" if passed else f"Missing: {missing}",
                "category": "security",
                "details": {"node_id": descriptor.get("node_id")}
            }
        except json.JSONDecodeError:
            return {
                "name": "node_descriptor",
                "passed": False,
                "message": "Invalid node descriptor JSON",
                "category": "security"
            }
    
    def test_signature_verification(self) -> Dict[str, Any]:
        """Test signature verification script."""
        code, stdout, stderr = self.ssh.run(
            "/usr/local/bin/verify-signature.sh node-descriptor /etc/infrasim/node-descriptor.json"
        )
        
        return {
            "name": "signature_verification",
            "passed": code == 0,
            "message": stdout.strip() if code == 0 else stderr.strip(),
            "category": "security"
        }
    
    def run_builtin_selftests(self) -> List[Dict[str, Any]]:
        """Run the target's built-in selftest framework."""
        results = []
        
        # Find and run selftest modules
        code, stdout, _ = self.ssh.run("ls /usr/share/infrasim/selftest/*.py 2>/dev/null")
        
        if code != 0:
            return [{
                "name": "builtin_selftests",
                "passed": True,
                "message": "No selftest modules found",
                "category": "selftest"
            }]
        
        for module in stdout.strip().split("\n"):
            if not module:
                continue
            
            module_name = os.path.basename(module).replace(".py", "")
            code, stdout, stderr = self.ssh.run(f"python3 {module}", timeout=120)
            
            if code == 0:
                try:
                    module_results = json.loads(stdout)
                    # Add each test from the module
                    for test in module_results.get("tests", []):
                        test["category"] = "selftest"
                        test["module"] = module_name
                        results.append(test)
                except json.JSONDecodeError:
                    results.append({
                        "name": f"{module_name}_parse_error",
                        "passed": False,
                        "message": "Failed to parse selftest output",
                        "category": "selftest"
                    })
            else:
                results.append({
                    "name": f"{module_name}_execution_error",
                    "passed": False,
                    "message": stderr.strip() or "Execution failed",
                    "category": "selftest"
                })
        
        return results
    
    def run_all(self) -> Dict[str, Any]:
        """Run all tests and return results."""
        start_time = datetime.now(timezone.utc)
        
        # Core tests
        self.results.append(self.test_boot())
        self.results.append(self.test_cloud_init())
        self.results.append(self.test_network_connectivity())
        self.results.append(self.test_dns_resolution())
        self.results.append(self.test_node_descriptor())
        self.results.append(self.test_signature_verification())
        
        # Built-in selftests
        self.results.extend(self.run_builtin_selftests())
        
        end_time = datetime.now(timezone.utc)
        
        passed_count = sum(1 for r in self.results if r.get("passed"))
        total_count = len(self.results)
        
        summary = {
            "target": self.ssh.host,
            "started_at": start_time.isoformat(),
            "finished_at": end_time.isoformat(),
            "duration_seconds": (end_time - start_time).total_seconds(),
            "passed": passed_count == total_count,
            "summary": f"{passed_count}/{total_count} tests passed",
            "tests": self.results
        }
        
        # Save results
        results_file = self.results_dir / "test-results.json"
        with open(results_file, "w") as f:
            json.dump(summary, f, indent=2)
        
        return summary


def main():
    parser = argparse.ArgumentParser(description="Run tests on target VM")
    parser.add_argument("--target", required=True, help="Target host")
    parser.add_argument("--port", type=int, default=22, help="SSH port")
    parser.add_argument("--user", default="root", help="SSH user")
    parser.add_argument("--key", help="SSH key file")
    parser.add_argument("--results-dir", default="/var/lib/sidecar/results", help="Results directory")
    parser.add_argument("--wait", type=int, default=300, help="Wait for SSH timeout")
    parser.add_argument("--json", action="store_true", help="Output JSON only")
    
    args = parser.parse_args()
    
    ssh = SSHConnection(args.target, args.port, args.user, args.key)
    
    if not args.json:
        print(f"Connecting to {args.target}:{args.port}...")
    
    if not ssh.wait_for_ssh(args.wait):
        if args.json:
            print(json.dumps({"error": "SSH connection failed", "passed": False}))
        else:
            print("ERROR: SSH connection failed")
        sys.exit(1)
    
    if not args.json:
        print("Connected. Running tests...")
    
    runner = TestRunner(ssh, args.results_dir)
    results = runner.run_all()
    
    if args.json:
        print(json.dumps(results, indent=2))
    else:
        print(f"\n{results['summary']}")
        print(f"Duration: {results['duration_seconds']:.1f}s")
        
        # Print failures
        failures = [t for t in results["tests"] if not t.get("passed")]
        if failures:
            print("\nFailures:")
            for f in failures:
                print(f"  - {f['name']}: {f.get('message', 'Unknown error')}")
    
    sys.exit(0 if results["passed"] else 1)


if __name__ == "__main__":
    main()
