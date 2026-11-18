//! The artifact owner must follow Cargo's configured target directory.
#![cfg(unix)]

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "terlan-artifact-root-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&path).unwrap();
    let quality = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../mk/code-quality.mk")
        .canonicalize()
        .unwrap();
    fs::write(
        path.join("Makefile"),
        format!(
            "include {}\n.PHONY: print-artifact-root\nprint-artifact-root:\n\t@printf '%s\\n' \"$(TERLAN_CARGO_ARTIFACT_DEBUG_ROOT)\"\n",
            quality.display()
        ),
    )
    .unwrap();
    path
}

fn artifact_root(root: &std::path::Path, cargo_target: Option<&str>) -> String {
    let mut command = Command::new("make");
    command
        .current_dir(root)
        .args(["--no-print-directory", "print-artifact-root"])
        .env_remove("CARGO_TARGET_DIR");
    if let Some(target) = cargo_target {
        command.env("CARGO_TARGET_DIR", target);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn artifact_retention_uses_the_real_cargo_debug_root() {
    let root = fixture();
    assert_eq!(
        artifact_root(&root, None),
        root.join("target/debug").display().to_string()
    );
    assert_eq!(
        artifact_root(&root, Some("shared/build")),
        root.join("shared/build/debug").display().to_string()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn complete_check_retains_artifacts_only_after_every_consumer() {
    let makefile =
        fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile"))
            .unwrap();
    let suite = makefile
        .split_once("rust-test-suite: rust-cargo-metadata-report\n")
        .unwrap()
        .1
        .split_once("\nendif")
        .unwrap()
        .0;
    assert!(suite.contains("$(TERLAN_RUST_ORCHESTRATOR)"));
    assert!(suite.contains("test -s target/quality/rust-test-suite-report.json"));
    assert!(!suite.contains("rust-artifact-retention-check"));

    let check = makefile
        .split_once("check: rust-test-suite\n")
        .unwrap()
        .1
        .split_once("\n\n# Aggregates every target listed in CHECK_GATES.")
        .unwrap()
        .0;
    let validation = check.find("--with-cargo-coverage").unwrap();
    let retention = check
        .find("$(MAKE) --no-print-directory rust-artifact-retention-check")
        .unwrap();

    assert!(validation < retention);
}
