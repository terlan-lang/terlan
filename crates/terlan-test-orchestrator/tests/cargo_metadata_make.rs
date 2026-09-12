//! Exercise the actual Make recipe through Cargo's declared metadata-owner binary.
#![cfg(unix)]

use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> Fixture {
    let path = std::env::temp_dir().join(format!(
        "terlan-metadata-make-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    let fixture = Fixture(path);
    control()
        .run(
            Command::new("git")
                .args(["init", "--quiet"])
                .arg(&fixture.0),
            |_| Ok(()),
        )
        .unwrap();
    fs::write(fixture.0.join(".gitignore"), "/target\n").unwrap();
    let recipe = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../mk/code-quality.mk")
        .canonicalize()
        .unwrap();
    let root_make =
        fs::read_to_string(recipe.parent().unwrap().parent().unwrap().join("Makefile")).unwrap();
    let supply_chain = root_make
        .lines()
        .find(|line| line.starts_with("release-supply-chain-provenance-check:"))
        .unwrap();
    let rust_suite = root_make
        .lines()
        .rev()
        .find(|line| line.starts_with("rust-test-suite:"))
        .unwrap();
    // Exercise the production prerequisite declaration with a bounded stand-in
    // consumer. Actual typed SBOM admission is covered by its AOT rehearsal.
    fs::write(fixture.0.join("Makefile"), format!("SHELL := /bin/bash\ninclude {}\n.PHONY: first second terlan-compiler-bootstrap terlan-release-closeout-bootstrap\nfirst: rust-cargo-metadata-report\nsecond: rust-cargo-metadata-report\n{supply_chain}\n\tcp target/quality/rust-cargo-metadata.json target/sbom-input.json\n\techo consumed >> target/sbom-calls\n{rust_suite}\n\tcp target/quality/rust-cargo-metadata.json target/suite-input.json\n\techo consumed >> target/suite-calls\n", recipe.display())).unwrap();
    fs::create_dir(fixture.0.join("target")).unwrap();
    fs::create_dir(fixture.0.join("target/cargo-home")).unwrap();
    fixture
}

fn control() -> ProcessControl<'static> {
    ProcessControl::new(Duration::from_secs(30))
}

fn command(root: &Path, cargo: &Path) -> Command {
    let mut command = Command::new("make");
    command
        .current_dir(root)
        .env("CARGO_HOME", root.join("target/cargo-home"))
        .env_remove("MAKEFLAGS")
        .env_remove("MFLAGS")
        .args([
            "--no-print-directory",
            "-j4",
            "first",
            "second",
            "release-supply-chain-provenance-check",
            "rust-test-suite",
        ])
        .arg(format!("CARGO={}", cargo.display()))
        .arg(format!(
            "TERLAN_CARGO_METADATA_OWNER={}",
            env!("CARGO_BIN_EXE_terlan-test-orchestrator")
        ));
    command
}

fn report(root: &Path) -> PathBuf {
    root.join("target/quality/rust-cargo-metadata.json")
}

#[test]
fn metadata_make_runs_once_and_failed_refresh_preserves_prior_observation() {
    let fixture = fixture();
    let root = &fixture.0;
    let source = root.join("cargo.rs");
    fs::write(&source, r#"
fn main() -> std::process::ExitCode {
    if std::env::args().skip(1).collect::<Vec<_>>() == ["fetch", "--locked"] { return std::process::ExitCode::SUCCESS; }
    assert_eq!(std::env::args().skip(1).collect::<Vec<_>>(), ["metadata", "--locked", "--all-features", "--format-version", "1"]);
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open("target/calls").unwrap();
    std::io::Write::write_all(&mut file, b"metadata\n").unwrap();
    match std::env::var("METADATA_MODE").unwrap().as_str() {
        "success" => println!("{{\"version\":1,\"workspace_root\":{:?},\"packages\":[{{\"id\":\"member\"}}],\"workspace_members\":[\"member\"]}}", std::env::current_dir().unwrap().to_str().unwrap()),
        "failure" => { println!("{{partial"); return std::process::ExitCode::from(7); },
        "empty" => (),
        _ => panic!("unexpected mode"),
    }
    std::process::ExitCode::SUCCESS
}
"#).unwrap();
    let cargo = root.join("target/cargo-fixture");
    control()
        .run(
            Command::new("rustc")
                .arg("--edition=2021")
                .arg(source)
                .arg("-o")
                .arg(&cargo),
            |_| Ok(()),
        )
        .unwrap();
    let mut prior = Vec::new();
    for (index, mode) in ["success", "failure", "empty"].into_iter().enumerate() {
        let result = control().capture_stdout(
            command(root, &cargo).env("METADATA_MODE", mode),
            64 * 1024,
            |_| Ok(()),
        );
        assert_eq!(result.is_ok(), mode == "success");
        assert_eq!(
            fs::read_to_string(root.join("target/calls")).unwrap(),
            "metadata\n".repeat(index + 1)
        );
        if mode == "success" {
            prior = fs::read(report(root)).unwrap();
        }
        assert_eq!(fs::read(report(root)).unwrap(), prior);
        let value: serde_json::Value = serde_json::from_slice(&prior).unwrap();
        assert_eq!(value["terlan_preparation"]["source"]["verified"], true);
        assert_eq!(
            value["terlan_preparation"]["resolver_cache"]["verified"],
            true
        );
        assert_eq!(value["terlan_preparation"]["reusable"], false);
        assert_eq!(
            fs::read(root.join("target/suite-input.json")).unwrap(),
            prior
        );
        assert_eq!(
            fs::read_to_string(root.join("target/suite-calls")).unwrap(),
            "consumed\n"
        );
        assert_eq!(
            fs::read(root.join("target/sbom-input.json")).unwrap(),
            prior
        );
        assert_eq!(
            fs::read_to_string(root.join("target/sbom-calls")).unwrap(),
            "consumed\n",
            "a failed producer must not run the SBOM consumer on retained metadata"
        );
        let attempt: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("target/quality/rust-cargo-metadata.json.attempt.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(attempt["reusable"], false);
        if mode == "success" {
            assert_eq!(
                attempt["metadata_sha256"],
                Sha256::digest(&prior)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            );
        } else {
            assert!(attempt["metadata_sha256"].is_null());
        }
        assert_eq!(
            attempt["state"],
            if mode == "success" {
                "succeeded"
            } else {
                "failed"
            }
        );
        assert_eq!(
            attempt["run_id"] == value["terlan_preparation"]["run_id"],
            mode == "success",
            "failed refresh must not impersonate the previous successful observation"
        );
        let launches = attempt["launches"].as_array().unwrap();
        assert_eq!(launches.len(), if mode == "success" { 4 } else { 3 });
        assert_eq!(launches[1]["role"], "cargo-fetch");
        assert_eq!(launches[2]["role"], "cargo-metadata");
        assert!(launches.iter().all(|row| row["pid"].as_u64().unwrap() > 0));
        assert_eq!(
            fs::read_dir(root.join("target/quality")).unwrap().count(),
            4,
            "only the successful document, latest attempt, and their writer leases remain"
        );
    }
}

#[test]
fn metadata_make_rejects_lockfile_changes_without_clobbering_the_report() {
    let fixture = fixture();
    let root = &fixture.0;
    let manifest = |version: &str| {
        format!("[package]\nname=\"metadata_fixture\"\nversion=\"{version}\"\nedition=\"2021\"\n[lib]\npath=\"lib.rs\"\n")
    };
    fs::write(root.join("Cargo.toml"), manifest("0.0.0")).unwrap();
    fs::write(root.join("lib.rs"), "pub fn value() {}\n").unwrap();
    let lock = "version=4\n[[package]]\nname=\"metadata_fixture\"\nversion=\"0.0.0\"\n";
    fs::write(root.join("Cargo.lock"), lock).unwrap();
    control()
        .capture_stdout(
            &mut command(root, Path::new("cargo")),
            64 * 1024,
            |_| Ok(()),
        )
        .unwrap();
    let before = fs::read(report(root)).unwrap();
    fs::write(root.join("Cargo.toml"), manifest("0.0.1")).unwrap();
    assert!(control()
        .capture_stdout(
            &mut command(root, Path::new("cargo")),
            64 * 1024,
            |_| Ok(())
        )
        .is_err());
    assert_eq!(fs::read(report(root)).unwrap(), before);
    assert_eq!(fs::read_to_string(root.join("Cargo.lock")).unwrap(), lock);
}
