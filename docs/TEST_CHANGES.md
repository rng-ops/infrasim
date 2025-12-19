# Test Changes

This document describes the integration tests added to validate security and reproducibility claims already described in existing InfraSim documentation.

## Scope Note

These tests are intentionally minimal and are not a full certification of security, determinism, or infrastructure correctness. In particular:

- They do not prove *all* builds are reproducible across different machines, toolchains, or timestamps.
- They do not prove the full end-to-end VM lifecycle (QEMU boot, provisioning, snapshots) is deterministic.
- They do not prove Terraform configurations are safe or production-grade; they only check a narrow idempotency property for the included example.
- They do not prove network QoS affects real traffic on the host OS; the QoS test validates an observable delay effect in InfraSim's userspace shaper.

## Tests Added

### 1) Deterministic Build Test

- **File:** `crates/e2e/tests/determinism_build.rs`
- **Purpose:** Builds the same minimal artifact twice (in isolated `CARGO_TARGET_DIR`s) and asserts the resulting artifact SHA-256 hashes match.
- **Why Added / Claim Validated:** Validates the project's documented emphasis on checksummed artifacts and reproducible build intent (see `docs/BUILD_PIPELINE.md`, `docs/BUILD_SUMMARY.md`, and reproducibility discussion in `docs/RFC-TESTS.md`).
- **Risk / Gap Addressed:** Detects accidental non-determinism introduced by build scripts, embedded timestamps, unstable file ordering, or environment-dependent behavior.
- **Execution Notes:** Marked `#[ignore]` because it can be slow and is sensitive to local toolchain/environment. Run explicitly with `cargo test -p infrasim-e2e --test determinism_build -- --ignored`.

### 2) Attestation Tamper Detection Test

- **File:** `crates/e2e/tests/attestation_tamper.rs`
- **Purpose:** Generates an attestation report, mutates a signed field after signing, and asserts verification fails.
- **Why Added / Claim Validated:** Validates the documented claim that InfraSim produces cryptographically verifiable attestation/provenance (see `docs/api-reference.md` and attestation sections in `docs/architecture.md` and `docs/RFC-TESTS.md`).
- **Risk / Gap Addressed:** Ensures consumers are not accepting modified reports as valid, preventing accidental trust in tampered provenance.

### 3) Terraform Idempotency Smoke Test

- **File:** `crates/e2e/tests/terraform_idempotency.rs`
- **Purpose:** Runs `terraform apply` on the existing example configuration, then runs `terraform plan -detailed-exitcode` and asserts exit code `0` (no diff).
- **Why Added / Claim Validated:** Supports the documented claim that Terraform workflows are a first-class integration path and that the example is usable as a workflow baseline (see `examples/terraform/README.md`).
- **Risk / Gap Addressed:** Detects drift-causing provider behavior or non-idempotent defaults that will repeatedly propose changes.
- **Execution Notes:** Marked `#[ignore]` because it requires a working Terraform installation and may create real resources (even if local). Run explicitly with `cargo test -p infrasim-e2e --test terraform_idempotency -- --ignored`.

### 4) Network QoS Effect Test

- **File:** `crates/e2e/tests/network_qos.rs`
- **Purpose:** Measures a baseline delay, then applies a QoS latency profile via `infrasim_common::traffic_shaper::TrafficShaper` and asserts the observed delay increases by a meaningful amount.
- **Why Added / Claim Validated:** Validates the documented QoS simulation functionality (latency injection) described in `docs/RFC-TESTS.md` and in the API surface (`docs/api-reference.md`).
- **Risk / Gap Addressed:** Detects regressions where QoS settings are accepted but have no measurable effect.
- **What It Does Not Guarantee:** This does not validate OS-level network shaping of real traffic (e.g., `ping`), nor does it validate end-to-end behavior through QEMU networking.

## Running the Tests

```bash
# Run non-ignored tests (attestation_tamper, network_qos)
cargo test -p infrasim-e2e --test attestation_tamper
cargo test -p infrasim-e2e --test network_qos

# Run ignored tests (slow/requires external tools)
cargo test -p infrasim-e2e --test determinism_build -- --ignored
cargo test -p infrasim-e2e --test terraform_idempotency -- --ignored
```

