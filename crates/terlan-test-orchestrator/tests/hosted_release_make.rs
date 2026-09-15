//! Exercise the production hosted-release Make graph with instrumented leaf work.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

const GOALS: [&str; 8] = [
    "release-hosted-validation-check",
    "tvm-aot-platform-aggregate-check",
    "release-artifact-set-check",
    "vm-multicore-release-contract-check",
    "tvm-aot-release-closeout-contract-check",
    "tvm-aot-platform-matrix-contract-check",
    "tvm-aot-thread-sanitizer-contract-check",
    "terlan-compiler-bootstrap",
];

const LEAVES: [&str; 9] = [
    "compiler-bootstrap",
    "matrix-bootstrap",
    "repository-bootstrap",
    "self-test",
    "aggregate",
    "release-artifact-set",
    "multicore-release-self-test",
    "tsan-self-test",
    "release-self-test",
];

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn rule(source: &str, name: &str) -> String {
    let start = source.find(&format!("\n{name}:")).expect("production rule") + 1;
    let end = source[start..].find("\n\n").expect("rule end") + start;
    format!("{}\n\n", &source[start..end])
}

fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "terlan-hosted-release-make-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let fixture = Fixture(root);
    fs::create_dir(fixture.0.join("seen")).unwrap();
    fs::create_dir(fixture.0.join("passed")).unwrap();
    // Every leaf claims its own marker exclusively. A duplicate invocation
    // fails even under parallel Make rather than being hidden by a counter race.
    let worker = fixture.0.join("worker");
    fs::write(
        &worker,
        r#"#!/bin/sh
set -eu
role=$1
mkdir "seen/$role"
case "$role" in
  compiler-bootstrap) ;;
  matrix-bootstrap|repository-bootstrap) test -d passed/compiler-bootstrap ;;
  release-artifact-set)
    test -d passed/repository-bootstrap
    test "$2" = archives
    ;;
  aggregate)
    test -d passed/matrix-bootstrap
    test -d passed/self-test
    test "$2" = reports
    ;;
  release-self-test)
    test -d passed/matrix-bootstrap
    test -d passed/self-test
    test -d passed/tsan-self-test
    ;;
  self-test|tsan-self-test|multicore-release-self-test)
    test -d passed/matrix-bootstrap
    ;;
  *) exit 90 ;;
esac
test "${TERLAN_HOSTED_FAIL:-}" != "$role"
mkdir "passed/$role"
"#,
    )
    .unwrap();
    fs::set_permissions(&worker, fs::Permissions::from_mode(0o700)).unwrap();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(repository.join("Makefile")).unwrap();
    let mut make = String::from(
        "TERLAN_TVM_CONTRACT_BOOTSTRAP := terlan-tvm-platform-matrix-bootstrap\n\
         TERLAN_TVM_CONTRACT_CHECK := ./worker\n\
         TERLAN_TVM_PLATFORM_MATRIX := ./worker\n\
         TERLAN_REPOSITORY_VALIDATION := ./worker\n\
         TERLAN_TVM_PLATFORM_REPORT_ROOT := reports\n\
         RELEASE_ARTIFACT_SET_ROOT := archives\n\
         .PHONY: terlan-tvm-platform-matrix-bootstrap terlan-repository-validation-bootstrap\n\
         terlan-compiler-bootstrap:\n\t./worker compiler-bootstrap\n\
         terlan-tvm-platform-matrix-bootstrap: terlan-compiler-bootstrap\n\t./worker matrix-bootstrap\n\
         terlan-repository-validation-bootstrap: terlan-compiler-bootstrap\n\t./worker repository-bootstrap\n",
    );
    for goal in &GOALS[..GOALS.len() - 1] {
        make.push_str(&rule(&source, goal));
    }
    make.push_str(&format!(".PHONY: {}\n", GOALS.join(" ")));
    fs::write(fixture.0.join("Makefile"), make).unwrap();
    fixture
}

fn run(fixture: &Fixture, failure: &str, parallel: bool) -> bool {
    let mut command = Command::new("make");
    command
        .current_dir(&fixture.0)
        .args(["--no-print-directory", if parallel { "-j8" } else { "-j1" }])
        .arg("release-hosted-validation-check")
        .env_remove("MAKEFLAGS")
        .env_remove("MAKEOVERRIDES")
        .env_remove("MFLAGS")
        .env("TERLAN_HOSTED_FAIL", failure);
    ProcessControl::new(Duration::from_secs(30))
        .run(&mut command, |_| Ok(()))
        .is_ok()
}

#[test]
fn hosted_release_shares_bootstrap_and_executes_every_check_once() {
    for parallel in [false, true] {
        let fixture = fixture();
        assert!(run(&fixture, "", parallel));
        for leaf in LEAVES {
            assert!(
                fixture.0.join("passed").join(leaf).is_dir(),
                "missing {leaf}"
            );
        }
        assert_eq!(
            fs::read_dir(fixture.0.join("seen")).unwrap().count(),
            LEAVES.len()
        );
    }
}

#[test]
fn every_hosted_release_leaf_failure_fails_the_aggregate() {
    for leaf in LEAVES {
        let fixture = fixture();
        assert!(!run(&fixture, leaf, true), "lost failure of {leaf}");
        assert!(fixture.0.join("seen").join(leaf).is_dir());
        assert!(!fixture.0.join("passed").join(leaf).exists());
    }
}

#[test]
fn repository_report_consumer_requires_the_reviewed_cargo_budget() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(repository.join("Makefile")).unwrap();
    for maximum in [6, 8, 9] {
        let fixture = fixture();
        fs::create_dir_all(fixture.0.join("target/quality")).unwrap();
        let report = serde_json::json!({
            "decision":"pass", "duplicate_equivalent_build_count":0,
            "terlc_test_invocation_count":0, "terlc_test_invocation_maximum":32,
            "terlc_build_invocation_maximum":16, "incremental_terlc_build_invocation_count":0,
            "lifecycle_partial_check_count":2, "cargo_invocation_maximum":maximum,
            "typed_validator_request_maximum":17, "typed_validator_parallelism_maximum":2,
        });
        fs::write(
            fixture
                .0
                .join("target/quality/validation-build-plan-report.json"),
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
        fs::write(
            fixture.0.join("Makefile"),
            format!(
                "TERLAN_REPOSITORY_VALIDATION := true\nterlan-repository-validation-bootstrap:\n{}",
                rule(&source, "repository-build-release-contract-check"),
            ),
        )
        .unwrap();
        let mut command = Command::new("make");
        command
            .current_dir(&fixture.0)
            .args([
                "--no-print-directory",
                "repository-build-release-contract-check",
            ])
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS");
        let result = ProcessControl::new(Duration::from_secs(10)).run(&mut command, |_| Ok(()));
        assert_eq!(
            result.is_ok(),
            maximum == 8,
            "wrong report budget {maximum}"
        );
    }
}
