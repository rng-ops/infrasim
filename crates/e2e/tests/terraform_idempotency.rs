use std::path::PathBuf;
use std::process::Command;

fn in_path(bin: &str) -> bool {
    Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {bin} >/dev/null 2>&1"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Terraform Idempotency Smoke Test
///
/// Runs `terraform apply` on an example configuration.
/// Immediately runs `terraform plan -detailed-exitcode`.
/// Asserts exit code == 0 (no diff).
///
/// Marked ignored because it requires Terraform and may perform real work.
#[test]
#[ignore]
fn terraform_example_is_idempotent_after_apply() {
    if !in_path("terraform") {
        eprintln!("Skipping: terraform not available in PATH");
        return;
    }

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/e2e has a parent")
        .to_path_buf();
    let example_dir = workspace_root.join("examples").join("terraform");

    if !example_dir.exists() {
        panic!("expected terraform example dir to exist: {}", example_dir.display());
    }

    let status = Command::new("terraform")
        .arg("init")
        .arg("-input=false")
        .current_dir(&example_dir)
        .status()
        .expect("run terraform init");
    assert!(status.success(), "terraform init failed: {status}");

    let status = Command::new("terraform")
        .arg("apply")
        .arg("-auto-approve")
        .arg("-input=false")
        .current_dir(&example_dir)
        .status()
        .expect("run terraform apply");
    assert!(status.success(), "terraform apply failed: {status}");

    let status = Command::new("terraform")
        .arg("plan")
        .arg("-detailed-exitcode")
        .arg("-input=false")
        .current_dir(&example_dir)
        .status()
        .expect("run terraform plan -detailed-exitcode");

    let code = status.code().unwrap_or(1);
    assert_eq!(
        code, 0,
        "expected no diff after apply; terraform plan exit code was {code}"
    );
}
