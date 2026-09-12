//! Focused proof validation shares its image owner without unrelated bootstraps.
#![cfg(unix)]

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn bootstrap_child() {
    let Ok(owner) = std::env::var("TERLAN_PROOF_BOOTSTRAP_OWNER") else {
        return;
    };
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(format!("seen-{owner}"))
        .expect("a bootstrap owner must execute once");
    assert_ne!(
        std::env::var("TERLAN_PROOF_BOOTSTRAP_FAIL").unwrap_or_default(),
        owner
    );
    fs::write(format!("passed-{owner}"), "pass").unwrap();
}

fn instrument(source: &str) -> String {
    let mut result = String::new();
    let mut recipe = false;
    for line in source.lines() {
        if line.starts_with('\t') {
            if !recipe {
                result.push_str("\t@TERLAN_PROOF_BOOTSTRAP_OWNER=$@ \"$(FIXTURE_CHILD)\" --exact bootstrap_child --nocapture\n");
            }
            recipe = true;
        } else {
            recipe = false;
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

#[test]
fn proof_bootstrap_is_focused_shared_and_fail_closed() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source.find("\nterlan-self-validation-bootstrap:").unwrap();
    let end = source[start..]
        .find("\n$(TERLAN_COMPILER_CONSUMER_GATES):")
        .unwrap()
        + start;
    let mut make = instrument(&source[start..end]);
    let aggregate = source[start..].lines().nth(1).unwrap();
    let dependencies: BTreeSet<_> = aggregate
        .split_once(':')
        .unwrap()
        .1
        .split_whitespace()
        .chain([
            "terlan-typed-validator-fingerprint",
            "lean-proof-lanes-check",
        ])
        .filter(|owner| *owner != "terlan-proof-release-bootstrap")
        .collect();
    for owner in &dependencies {
        make.push_str(&instrument(&format!(
            ".PHONY: {owner}\n{owner}:\n\tproducer\n"
        )));
    }
    for target in [
        "release-proof-baseline-check",
        "release-artifacts-closeout-check",
    ] {
        let start = source.find(&format!("\n{target}:")).unwrap() + 1;
        let end = source[start..].find("\n\n").unwrap() + start;
        make.push_str(&instrument(&source[start..end]));
    }
    for (goals, failure, broad) in [
        (vec!["release-proof-baseline-check"], "", false),
        (
            vec![
                "release-artifacts-closeout-check",
                "release-proof-baseline-check",
            ],
            "",
            false,
        ),
        (
            vec![
                "terlan-self-validation-bootstrap",
                "release-proof-baseline-check",
            ],
            "",
            true,
        ),
        (
            vec![
                "release-proof-baseline-check",
                "terlan-self-validation-bootstrap",
            ],
            "",
            true,
        ),
        (
            vec!["release-proof-baseline-check"],
            "terlan-proof-release-bootstrap",
            false,
        ),
        (
            vec!["release-proof-baseline-check"],
            "terlan-typed-validator-fingerprint",
            false,
        ),
    ] {
        let root = std::env::temp_dir().join(format!(
            "terlan-proof-bootstrap-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let fixture = Fixture(root);
        fs::write(fixture.0.join("Makefile"), &make).unwrap();
        let mut command = Command::new("make");
        command
            .current_dir(&fixture.0)
            .args(["--no-print-directory", "-j8", "-k"])
            .args(goals)
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS")
            .env_remove("TERLAN_VALIDATION_BOOTSTRAPPED")
            .env("FIXTURE_CHILD", std::env::current_exe().unwrap())
            .env("TERLAN_PROOF_BOOTSTRAP_FAIL", failure);
        assert_eq!(
            ProcessControl::new(Duration::from_secs(30))
                .run(&mut command, |_| Ok(()))
                .is_ok(),
            failure.is_empty()
        );
        assert_eq!(
            fixture
                .0
                .join("passed-release-proof-baseline-check")
                .exists(),
            failure.is_empty()
        );
        for owner in dependencies
            .iter()
            .filter(|owner| owner.ends_with("-bootstrap") && **owner != "terlan-compiler-bootstrap")
        {
            assert_eq!(
                fixture.0.join(format!("seen-{owner}")).exists(),
                broad,
                "unexpected bootstrap reachability: {owner}"
            );
        }
    }
}
