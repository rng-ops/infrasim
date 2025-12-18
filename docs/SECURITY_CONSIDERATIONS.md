# Security & Attack Surface Considerations

This document summarizes the main security properties and risks of the InfraSim Web + Console.

## Threat model (high-level)

The web service is a control-plane for a virtualization daemon (QEMU). Compromise of the web UI or its auth tokens can lead to:

- creation of VMs
- snapshot/export of VM disk + memory (highly sensitive)
- access to consoles (VNC)

Assume:
- an attacker may control a browser tab (XSS risk)
- an attacker may observe network traffic if deployed without TLS
- an attacker may attempt CSRF-like actions from another origin

## Authentication & tokens

- The UI uses bearer tokens.
- The console store keeps tokens **in `sessionStorage`** by default (reduces persistence after browser restart).

Guidance:
- Prefer TLS in any non-local deployment.
- Prefer JWT mode (issuer allowlist + audience enforcement) when possible.
- Do not log tokens in UI, server logs, or telemetry.

## Admin control endpoints

Admin endpoints (restart/stop controls) are gated by `INFRASIM_WEB_CONTROL_ENABLED=1` and an optional `x-infrasim-admin-token`.

Recommendations:
- Do not enable these endpoints on public interfaces.
- Bind the web server to localhost for development if possible.
- If enabled on a LAN, require an admin token and firewall restrict access.

## Static UI serving

The web server can optionally serve a Vite-built SPA from disk via `INFRASIM_WEB_STATIC_DIR`.

Controls:
- The implementation canonicalizes requested paths and rejects traversal outside the configured directory.

Recommendations:
- Only point `INFRASIM_WEB_STATIC_DIR` at trusted build outputs.
- Avoid serving arbitrary user-controlled directories.

## XSS / UI injection

- React escapes text by default.
- Avoid `dangerouslySetInnerHTML`.
- Treat all strings from API as untrusted.

## Renderer / WebGPU “partial state push”

The console includes a "render channel" patch mechanism designed for high-frequency renderer updates.

Security model:
- patches are JSON objects stored in the store
- renderer code must validate/normalize patch contents before applying to GPU resources

Recommendations:
- never accept renderer patches from untrusted origins (no cross-window postMessage without strict origin checks)
- cap patch sizes and frequency if a remote source is introduced

## Data sensitivity

Snapshots with memory (`include_memory=true`) can contain:
- credentials
- keys
- decrypted secrets

Operational guidance:
- store snapshot artifacts on encrypted disks
- restrict filesystem permissions
- treat snapshot exports as secrets
