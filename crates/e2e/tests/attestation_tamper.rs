use infrasim_common::attestation::AttestationProvider;
use infrasim_common::crypto::KeyPair;
use infrasim_common::types::{ResourceMeta, Vm, VmSpec, VmStatus};

/// Attestation Tamper Detection Test
///
/// Generates an attestation report, then mutates a signed field and asserts verification fails.
#[test]
fn attestation_report_tamper_is_detected() {
    let key_pair = KeyPair::generate();
    let provider = AttestationProvider::new(key_pair);

    let vm = Vm {
        meta: ResourceMeta::new("test-vm".to_string()),
        spec: VmSpec::default(),
        status: VmStatus::default(),
    };

    let mut report = provider
        .generate_report(&vm, &[], &[])
        .expect("generate report");

    assert!(
        provider.verify_report(&report).expect("verify report"),
        "freshly generated report must verify"
    );

    // Tamper with a signed field.
    report.host_provenance.hostname.push_str("-tampered");

    let ok = provider.verify_report(&report).expect("verify tampered report");
    assert!(!ok, "tampering must invalidate verification");
}
