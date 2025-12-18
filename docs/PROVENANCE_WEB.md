# Web Provenance & Evidence Bundles

Date: 2025-12-15

This document describes provenance/evidence outputs produced by `infrasim-web`.

## Endpoint: Evidence Bundle (MVP)

`POST /api/provenance/evidence`

Creates a signed evidence manifest binding a "purpose" to either:
- a project (`project_id`), and/or
- an appliance instance (`appliance_id`)

### Request

```json
{
  "appliance_id": "...",
  "project_id": "...",
  "purpose": "snapshot"
}
```

At least one of `appliance_id` or `project_id` is required.

### Response (MVP)

Returns:
- `digest`: `sha256:<hex>` digest of the manifest JSON bytes.
- `signature`: Ed25519 signature (hex) over the digest string bytes.
- `public_key`: Ed25519 public key (hex).
- `manifest`: the evidence JSON itself.

### Notes

- The current implementation uses an ephemeral signing key (generated per request).
  Next step is to sign evidence with the daemon’s long-lived key (or a vTPM-backed key).
- Next step is to store the manifest bytes into the daemon CAS and return the CAS object digest.
- Canonical JSON serialization and deterministic digests are a requirement for auditor-grade bundles.
  This MVP uses `serde_json::to_vec()`; we will replace it with a strict canonicalization step.
