use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn cargo_build_release(workspace_root: &Path, cargo_target_dir: &Path) -> std::io::Result<()> {
    // Build a single, minimal binary twice and compare.
    let status = Command::new("cargo")
        .arg("build")
        .arg("-p")
        .arg("infrasim-web")
        .arg("--release")
        .env("CARGO_TARGET_DIR", cargo_target_dir)
        .current_dir(workspace_root)
        .status()?;

    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("cargo build failed with status: {status}"),
        ));
    }

    Ok(())
}

fn artifact_path(target_dir: &Path) -> PathBuf {
    target_dir.join("release").join("infrasim-web")
}

/// Deterministic Build Test
///
/// Builds the same minimal artifact twice with isolated target dirs and compares SHA-256.
///
/// Marked ignored because it can be expensive and is sensitive to toolchain/environment.
#[test]
#[ignore]
fn deterministic_build_infrasim_web_sha256_matches() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/e2e has a parent")
        .to_path_buf();

    if Command::new("cargo").arg("--version").output().is_err() {
        eprintln!("Skipping: cargo not available in PATH");
        return;
    }

    let tmp = TempDir::new().expect("create temp dir");
    let t1 = tmp.path().join("target1");
    let t2 = tmp.path().join("target2");

    cargo_build_release(&workspace_root, &t1).expect("first build should succeed");
    cargo_build_release(&workspace_root, &t2).expect("second build should succeed");

    let a1 = artifact_path(&t1);
    let a2 = artifact_path(&t2);

    assert!(a1.exists(), "expected artifact to exist: {}", a1.display());
    assert!(a2.exists(), "expected artifact to exist: {}", a2.display());

    let h1 = sha256_file(&a1).expect("hash first artifact");
    let h2 = sha256_file(&a2).expect("hash second artifact");

    assert_eq!(
        h1, h2,
        "non-determinism detected: artifact hashes differ ({} vs {})",
        h1, h2
    );
}
