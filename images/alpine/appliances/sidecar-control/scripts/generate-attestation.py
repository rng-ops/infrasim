#!/usr/bin/env python3
"""
generate-attestation.py - Generate signed attestation for test results

Creates an in-toto attestation with Ed25519 signature for test results.

Usage: generate-attestation.py --results FILE --signing-key KEY [--output FILE]
"""

import argparse
import base64
import hashlib
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, Any


def load_results(results_file: str) -> Dict[str, Any]:
    """Load test results from file."""
    with open(results_file) as f:
        return json.load(f)


def calculate_digest(data: bytes) -> str:
    """Calculate SHA256 digest."""
    return hashlib.sha256(data).hexdigest()


def sign_payload(payload: bytes, signing_key: str) -> bytes:
    """Sign payload with Ed25519 key."""
    try:
        result = subprocess.run(
            [
                "openssl", "pkeyutl", "-sign",
                "-inkey", signing_key,
                "-rawin"
            ],
            input=payload,
            capture_output=True
        )
        
        if result.returncode != 0:
            raise Exception(f"Signing failed: {result.stderr.decode()}")
        
        return result.stdout
    except FileNotFoundError:
        raise Exception("openssl not found")


def get_public_key(signing_key: str) -> str:
    """Extract public key from private key."""
    try:
        result = subprocess.run(
            ["openssl", "pkey", "-in", signing_key, "-pubout"],
            capture_output=True,
            text=True
        )
        
        if result.returncode != 0:
            return ""
        
        return result.stdout
    except Exception:
        return ""


def create_intoto_attestation(results: Dict[str, Any], signing_key: str) -> Dict[str, Any]:
    """Create in-toto attestation envelope."""
    
    # Create statement
    statement = {
        "_type": "https://in-toto.io/Statement/v1",
        "subject": [
            {
                "name": results.get("target", "unknown"),
                "digest": {
                    "sha256": calculate_digest(json.dumps(results).encode())
                }
            }
        ],
        "predicateType": "https://infrasim.io/test-results/v1",
        "predicate": {
            "testRunner": {
                "uri": "https://github.com/infrasim/sidecar-control",
                "version": "1.0.0"
            },
            "runDetails": {
                "target": results.get("target"),
                "startedAt": results.get("started_at"),
                "finishedAt": results.get("finished_at"),
                "durationSeconds": results.get("duration_seconds")
            },
            "results": {
                "passed": results.get("passed"),
                "summary": results.get("summary"),
                "tests": results.get("tests", [])
            },
            "metadata": {
                "buildInvocationId": os.environ.get("BUILD_ID", "local"),
                "completeness": {
                    "parameters": True,
                    "environment": True,
                    "materials": True
                }
            }
        }
    }
    
    # Encode statement
    statement_bytes = json.dumps(statement, separators=(",", ":")).encode()
    statement_b64 = base64.b64encode(statement_bytes).decode()
    
    # Sign
    signature = sign_payload(statement_bytes, signing_key)
    signature_b64 = base64.b64encode(signature).decode()
    
    # Get public key ID (hash of public key)
    public_key = get_public_key(signing_key)
    key_id = calculate_digest(public_key.encode())[:16] if public_key else "unknown"
    
    # Create DSSE envelope
    envelope = {
        "payloadType": "application/vnd.in-toto+json",
        "payload": statement_b64,
        "signatures": [
            {
                "keyid": key_id,
                "sig": signature_b64
            }
        ]
    }
    
    return envelope


def create_simple_attestation(results: Dict[str, Any], signing_key: str) -> Dict[str, Any]:
    """Create simple attestation format."""
    
    attestation = {
        "_type": "https://infrasim.io/attestation/v1",
        "version": "1.0.0",
        "target": results.get("target"),
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "results": {
            "passed": results.get("passed"),
            "summary": results.get("summary"),
            "test_count": len(results.get("tests", [])),
            "duration_seconds": results.get("duration_seconds")
        },
        "digest": calculate_digest(json.dumps(results).encode()),
        "signer": {
            "key_id": calculate_digest(get_public_key(signing_key).encode())[:16]
        }
    }
    
    # Sign the attestation
    attestation_bytes = json.dumps(attestation, separators=(",", ":")).encode()
    signature = sign_payload(attestation_bytes, signing_key)
    
    return {
        "attestation": attestation,
        "signature": base64.b64encode(signature).decode()
    }


def main():
    parser = argparse.ArgumentParser(description="Generate signed attestation")
    parser.add_argument("--results", required=True, help="Test results file")
    parser.add_argument("--signing-key", required=True, help="Ed25519 signing key")
    parser.add_argument("--output", help="Output file")
    parser.add_argument("--format", choices=["intoto", "simple"], default="intoto",
                        help="Attestation format")
    
    args = parser.parse_args()
    
    if not os.path.exists(args.results):
        print(f"ERROR: Results file not found: {args.results}", file=sys.stderr)
        sys.exit(1)
    
    if not os.path.exists(args.signing_key):
        print(f"ERROR: Signing key not found: {args.signing_key}", file=sys.stderr)
        sys.exit(1)
    
    results = load_results(args.results)
    
    if args.format == "intoto":
        attestation = create_intoto_attestation(results, args.signing_key)
    else:
        attestation = create_simple_attestation(results, args.signing_key)
    
    output = json.dumps(attestation, indent=2)
    
    if args.output:
        with open(args.output, "w") as f:
            f.write(output)
        print(f"Attestation written to: {args.output}")
    else:
        print(output)


if __name__ == "__main__":
    main()
