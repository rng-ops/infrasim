#!/usr/bin/env python3
"""
Base selftest module for infrasim Alpine images.
Validates core system functionality required by all profiles.
"""

import json
import os
import subprocess
import sys
import hashlib
from pathlib import Path
from typing import List, Dict, Any, Tuple, Optional


class TestResult:
    """Individual test result."""
    def __init__(self, name: str, passed: bool, message: str = "", details: Dict[str, Any] = None):
        self.name = name
        self.passed = passed
        self.message = message
        self.details = details or {}
    
    def to_dict(self) -> Dict[str, Any]:
        return {
            "name": self.name,
            "passed": self.passed,
            "message": self.message,
            "details": self.details
        }


class SelfTestRunner:
    """Base selftest runner for infrasim images."""
    
    def __init__(self):
        self.results: List[TestResult] = []
        self.node_descriptor_path = Path("/etc/infrasim/node-descriptor.json")
        self.manifest_path = Path("/etc/infrasim/manifest.json")
    
    def run_command(self, cmd: List[str], timeout: int = 30) -> Tuple[int, str, str]:
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
    
    def test_openrc_services(self) -> TestResult:
        """Verify critical OpenRC services are running."""
        required_services = ["sshd", "dhcpcd", "cloud-init"]
        running = []
        stopped = []
        
        for service in required_services:
            code, stdout, _ = self.run_command(["rc-service", service, "status"])
            if code == 0 and "started" in stdout.lower():
                running.append(service)
            else:
                stopped.append(service)
        
        passed = len(stopped) == 0
        return TestResult(
            name="openrc_services",
            passed=passed,
            message=f"Running: {running}, Stopped: {stopped}",
            details={"running": running, "stopped": stopped}
        )
    
    def test_network_interfaces(self) -> TestResult:
        """Verify network interfaces are up and configured."""
        code, stdout, _ = self.run_command(["ip", "-j", "addr", "show"])
        
        if code != 0:
            return TestResult(
                name="network_interfaces",
                passed=False,
                message="Failed to get network interfaces"
            )
        
        try:
            interfaces = json.loads(stdout)
            up_interfaces = [
                iface["ifname"] 
                for iface in interfaces 
                if "UP" in iface.get("flags", []) and iface["ifname"] != "lo"
            ]
            
            # At least one non-loopback interface should be up
            passed = len(up_interfaces) > 0
            return TestResult(
                name="network_interfaces",
                passed=passed,
                message=f"Up interfaces: {up_interfaces}",
                details={"up_interfaces": up_interfaces}
            )
        except json.JSONDecodeError:
            return TestResult(
                name="network_interfaces",
                passed=False,
                message="Failed to parse interface JSON"
            )
    
    def test_ipv6_enabled(self) -> TestResult:
        """Verify IPv6 is enabled (required for rendezvous)."""
        code, stdout, _ = self.run_command(["cat", "/proc/sys/net/ipv6/conf/all/disable_ipv6"])
        
        if code != 0:
            return TestResult(
                name="ipv6_enabled",
                passed=False,
                message="Failed to check IPv6 status"
            )
        
        disabled = stdout.strip() == "1"
        return TestResult(
            name="ipv6_enabled",
            passed=not disabled,
            message="IPv6 enabled" if not disabled else "IPv6 disabled",
            details={"disabled": disabled}
        )
    
    def test_firewall_loaded(self) -> TestResult:
        """Verify nftables/iptables firewall is loaded."""
        # Try nftables first
        code, stdout, _ = self.run_command(["nft", "list", "tables"])
        
        if code == 0:
            tables = [line for line in stdout.split("\n") if line.strip()]
            return TestResult(
                name="firewall_loaded",
                passed=True,
                message=f"nftables loaded with {len(tables)} tables",
                details={"backend": "nftables", "tables": tables}
            )
        
        # Fallback to iptables
        code, stdout, _ = self.run_command(["iptables", "-L", "-n"])
        if code == 0:
            return TestResult(
                name="firewall_loaded",
                passed=True,
                message="iptables loaded",
                details={"backend": "iptables"}
            )
        
        return TestResult(
            name="firewall_loaded",
            passed=False,
            message="No firewall backend available"
        )
    
    def test_node_descriptor_exists(self) -> TestResult:
        """Verify node descriptor exists and is valid JSON."""
        if not self.node_descriptor_path.exists():
            return TestResult(
                name="node_descriptor_exists",
                passed=False,
                message=f"Node descriptor not found at {self.node_descriptor_path}"
            )
        
        try:
            with open(self.node_descriptor_path) as f:
                descriptor = json.load(f)
            
            required_fields = ["node_id", "identity", "endpoints", "attestation"]
            missing = [f for f in required_fields if f not in descriptor]
            
            if missing:
                return TestResult(
                    name="node_descriptor_exists",
                    passed=False,
                    message=f"Missing required fields: {missing}",
                    details={"missing_fields": missing}
                )
            
            return TestResult(
                name="node_descriptor_exists",
                passed=True,
                message=f"Node ID: {descriptor['node_id']}",
                details={"node_id": descriptor["node_id"]}
            )
        except json.JSONDecodeError as e:
            return TestResult(
                name="node_descriptor_exists",
                passed=False,
                message=f"Invalid JSON: {e}"
            )
    
    def test_node_descriptor_signature(self) -> TestResult:
        """Verify node descriptor signature is valid."""
        if not self.node_descriptor_path.exists():
            return TestResult(
                name="node_descriptor_signature",
                passed=False,
                message="Node descriptor not found"
            )
        
        sig_path = self.node_descriptor_path.with_suffix(".json.sig")
        if not sig_path.exists():
            return TestResult(
                name="node_descriptor_signature",
                passed=False,
                message=f"Signature file not found at {sig_path}"
            )
        
        # Use the verify-signature.sh script
        code, stdout, stderr = self.run_command([
            "/usr/local/bin/verify-signature.sh",
            "node-descriptor",
            str(self.node_descriptor_path)
        ])
        
        return TestResult(
            name="node_descriptor_signature",
            passed=code == 0,
            message=stdout.strip() or stderr.strip(),
            details={"verified": code == 0}
        )
    
    def test_ssh_host_keys(self) -> TestResult:
        """Verify SSH host keys exist and have correct permissions."""
        key_types = ["ed25519", "ecdsa", "rsa"]
        found_keys = []
        issues = []
        
        for key_type in key_types:
            key_path = Path(f"/etc/ssh/ssh_host_{key_type}_key")
            if key_path.exists():
                stat = key_path.stat()
                if stat.st_mode & 0o777 != 0o600:
                    issues.append(f"{key_type}: bad permissions {oct(stat.st_mode & 0o777)}")
                else:
                    found_keys.append(key_type)
        
        passed = len(found_keys) > 0 and len(issues) == 0
        return TestResult(
            name="ssh_host_keys",
            passed=passed,
            message=f"Found: {found_keys}, Issues: {issues}" if issues else f"Found: {found_keys}",
            details={"found_keys": found_keys, "issues": issues}
        )
    
    def test_cloud_init_completed(self) -> TestResult:
        """Verify cloud-init has completed successfully."""
        status_path = Path("/var/lib/cloud/data/result.json")
        
        if not status_path.exists():
            return TestResult(
                name="cloud_init_completed",
                passed=False,
                message="cloud-init result file not found"
            )
        
        try:
            with open(status_path) as f:
                result = json.load(f)
            
            # Check for errors
            errors = result.get("v1", {}).get("errors", [])
            passed = len(errors) == 0
            
            return TestResult(
                name="cloud_init_completed",
                passed=passed,
                message="cloud-init completed" if passed else f"Errors: {errors}",
                details={"errors": errors}
            )
        except json.JSONDecodeError:
            return TestResult(
                name="cloud_init_completed",
                passed=False,
                message="Failed to parse cloud-init result"
            )
    
    def test_python3_available(self) -> TestResult:
        """Verify Python 3 is available for selftest framework."""
        code, stdout, _ = self.run_command(["python3", "--version"])
        
        if code != 0:
            return TestResult(
                name="python3_available",
                passed=False,
                message="Python 3 not available"
            )
        
        version = stdout.strip()
        return TestResult(
            name="python3_available",
            passed=True,
            message=version,
            details={"version": version}
        )
    
    def test_required_binaries(self) -> TestResult:
        """Verify required binaries are present."""
        required = [
            "ip", "nft", "iptables", "curl", "jq",
            "ssh", "openssl", "base64"
        ]
        found = []
        missing = []
        
        for binary in required:
            code, _, _ = self.run_command(["which", binary])
            if code == 0:
                found.append(binary)
            else:
                missing.append(binary)
        
        passed = len(missing) == 0
        return TestResult(
            name="required_binaries",
            passed=passed,
            message=f"Found: {len(found)}, Missing: {missing}",
            details={"found": found, "missing": missing}
        )
    
    def test_dns_resolution(self) -> TestResult:
        """Verify DNS resolution works."""
        # Try to resolve a well-known domain
        code, stdout, _ = self.run_command(["nslookup", "cloudflare.com"])
        
        if code != 0:
            # Fallback to getent
            code, stdout, _ = self.run_command(["getent", "hosts", "cloudflare.com"])
        
        return TestResult(
            name="dns_resolution",
            passed=code == 0,
            message="DNS resolution working" if code == 0 else "DNS resolution failed",
            details={"resolved": code == 0}
        )
    
    def run_all_tests(self) -> Dict[str, Any]:
        """Run all base tests and return results."""
        test_methods = [
            self.test_openrc_services,
            self.test_network_interfaces,
            self.test_ipv6_enabled,
            self.test_firewall_loaded,
            self.test_node_descriptor_exists,
            self.test_node_descriptor_signature,
            self.test_ssh_host_keys,
            self.test_cloud_init_completed,
            self.test_python3_available,
            self.test_required_binaries,
            self.test_dns_resolution,
        ]
        
        for test_method in test_methods:
            try:
                result = test_method()
                self.results.append(result)
            except Exception as e:
                self.results.append(TestResult(
                    name=test_method.__name__.replace("test_", ""),
                    passed=False,
                    message=f"Exception: {e}"
                ))
        
        passed_count = sum(1 for r in self.results if r.passed)
        total_count = len(self.results)
        
        return {
            "feature": "base-minimal",
            "version": "1.0.0",
            "passed": passed_count == total_count,
            "summary": f"{passed_count}/{total_count} tests passed",
            "tests": [r.to_dict() for r in self.results]
        }


def main():
    """Run selftests and output results."""
    runner = SelfTestRunner()
    results = runner.run_all_tests()
    
    # Output JSON to stdout
    print(json.dumps(results, indent=2))
    
    # Exit with appropriate code
    sys.exit(0 if results["passed"] else 1)


if __name__ == "__main__":
    main()
