#!/usr/bin/env python3
"""
Selftest for mTLS control plane feature.
"""

import json
import os
import subprocess
import sys
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


def test_mtls_ca_exists() -> Dict[str, Any]:
    """Verify CA certificate exists."""
    ca_path = Path("/etc/infrasim/mtls/ca/ca.crt")
    
    if not ca_path.exists():
        return {
            "name": "mtls_ca_exists",
            "passed": False,
            "message": "CA certificate not found"
        }
    
    # Verify it's a valid certificate
    code, stdout, _ = run_command(["openssl", "x509", "-in", str(ca_path), "-noout", "-subject"])
    
    if code != 0:
        return {
            "name": "mtls_ca_exists",
            "passed": False,
            "message": "Invalid CA certificate"
        }
    
    return {
        "name": "mtls_ca_exists",
        "passed": True,
        "message": stdout.strip(),
        "details": {"path": str(ca_path)}
    }


def test_mtls_server_cert() -> Dict[str, Any]:
    """Verify server certificate exists and is valid."""
    cert_path = Path("/etc/infrasim/mtls/server/server.crt")
    key_path = Path("/etc/infrasim/mtls/server/server.key")
    
    if not cert_path.exists():
        return {
            "name": "mtls_server_cert",
            "passed": True,
            "message": "Server certificate not configured (optional)"
        }
    
    if not key_path.exists():
        return {
            "name": "mtls_server_cert",
            "passed": False,
            "message": "Server key missing"
        }
    
    # Verify certificate and key match
    code1, cert_mod, _ = run_command([
        "openssl", "x509", "-in", str(cert_path), "-noout", "-modulus"
    ])
    code2, key_mod, _ = run_command([
        "openssl", "rsa", "-in", str(key_path), "-noout", "-modulus"
    ])
    
    if code1 != 0 or code2 != 0:
        return {
            "name": "mtls_server_cert",
            "passed": False,
            "message": "Failed to read certificate or key"
        }
    
    if cert_mod.strip() != key_mod.strip():
        return {
            "name": "mtls_server_cert",
            "passed": False,
            "message": "Certificate and key do not match"
        }
    
    return {
        "name": "mtls_server_cert",
        "passed": True,
        "message": "Server certificate and key valid"
    }


def test_mtls_client_cert() -> Dict[str, Any]:
    """Verify client certificate exists and is valid."""
    default_dir = Path("/etc/infrasim/mtls/clients/default")
    
    if not default_dir.exists():
        return {
            "name": "mtls_client_cert",
            "passed": True,
            "message": "No default client certificate (optional)"
        }
    
    cert_path = default_dir / "client.crt"
    key_path = default_dir / "client.key"
    
    if not cert_path.exists() or not key_path.exists():
        return {
            "name": "mtls_client_cert",
            "passed": False,
            "message": "Client certificate or key missing"
        }
    
    # Verify against CA
    ca_path = Path("/etc/infrasim/mtls/ca/ca.crt")
    if ca_path.exists():
        code, _, stderr = run_command([
            "openssl", "verify", "-CAfile", str(ca_path), str(cert_path)
        ])
        
        if code != 0:
            return {
                "name": "mtls_client_cert",
                "passed": False,
                "message": f"Certificate verification failed: {stderr}"
            }
    
    return {
        "name": "mtls_client_cert",
        "passed": True,
        "message": "Client certificate valid"
    }


def test_mtls_cert_expiry() -> Dict[str, Any]:
    """Check certificate expiry dates."""
    certs = [
        ("/etc/infrasim/mtls/ca/ca.crt", "CA"),
        ("/etc/infrasim/mtls/server/server.crt", "Server"),
        ("/etc/infrasim/mtls/clients/default/client.crt", "Client"),
    ]
    
    import time
    now = time.time()
    warnings = []
    
    for cert_path, cert_name in certs:
        if not os.path.exists(cert_path):
            continue
        
        code, stdout, _ = run_command([
            "openssl", "x509", "-in", cert_path, "-noout", "-enddate"
        ])
        
        if code != 0:
            continue
        
        # Parse date (format: notAfter=Jan  1 00:00:00 2030 GMT)
        try:
            date_str = stdout.split("=")[1].strip()
            # Check if expiring within 30 days
            code2, _, _ = run_command([
                "openssl", "x509", "-in", cert_path, "-noout", 
                "-checkend", str(30 * 24 * 3600)
            ])
            if code2 != 0:
                warnings.append(f"{cert_name}: expiring soon ({date_str})")
        except (IndexError, ValueError):
            pass
    
    passed = len(warnings) == 0
    
    return {
        "name": "mtls_cert_expiry",
        "passed": passed,
        "message": "All certificates valid" if passed else f"Warnings: {warnings}",
        "details": {"warnings": warnings}
    }


def test_mtls_key_permissions() -> Dict[str, Any]:
    """Verify private key permissions are secure."""
    key_files = [
        "/etc/infrasim/mtls/ca/ca.key",
        "/etc/infrasim/mtls/server/server.key",
        "/etc/infrasim/mtls/clients/default/client.key",
    ]
    
    issues = []
    
    for key_file in key_files:
        if not os.path.exists(key_file):
            continue
        
        stat = os.stat(key_file)
        mode = stat.st_mode & 0o777
        
        if mode != 0o600:
            issues.append(f"{key_file}: mode {oct(mode)} (should be 0600)")
    
    passed = len(issues) == 0
    
    return {
        "name": "mtls_key_permissions",
        "passed": passed,
        "message": "Key permissions correct" if passed else f"Issues: {issues}",
        "details": {"issues": issues}
    }


def run_all_tests() -> Dict[str, Any]:
    """Run all mTLS tests."""
    tests = [
        test_mtls_ca_exists,
        test_mtls_server_cert,
        test_mtls_client_cert,
        test_mtls_cert_expiry,
        test_mtls_key_permissions,
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
        "feature": "control-mtls",
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
