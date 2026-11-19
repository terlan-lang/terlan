//! Focused proof validation shares its image owner without unrelated bootstraps.
#![cfg(unix)]

mod support;

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
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

fn recipe(source: &str, target: &str) -> String {
    let prefix = format!("\n{target}:");
    source
        .match_indices(&prefix)
        .find_map(|(offset, _)| {
            let start = offset + 1;
            let end = source[start..]
                .find("\n\n")
                .map_or(source.len(), |length| start + length);
            let block = &source[start..end];
            block
                .lines()
                .any(|line| line.starts_with('\t'))
                .then(|| block.to_owned())
        })
        .unwrap_or_else(|| panic!("production recipe `{target}`"))
}

#[test]
fn release_control_waits_for_the_shared_fingerprint() {
    let source = crate::support::read_make_sources();
    let release_control = recipe(&source, "terlan-release-control-fingerprint");
    assert!(release_control
        .starts_with("terlan-release-control-fingerprint: terlan-typed-validator-fingerprint\n"));

    let root = std::env::temp_dir().join(format!(
        "terlan-release-control-fingerprint-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let fixture = Fixture(root);
    fs::write(fixture.0.join("terlc"), "#!/bin/sh\nexit 0\n").unwrap();
    fs::write(fixture.0.join("terlan-vm"), "#!/bin/sh\nexit 0\n").unwrap();
    for program in ["terlc", "terlan-vm"] {
        let mut permissions = fs::metadata(fixture.0.join(program)).unwrap().permissions();
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o700);
        fs::set_permissions(fixture.0.join(program), permissions).unwrap();
    }
    fs::write(
        fixture.0.join("Makefile"),
        format!(
            ".PHONY: terlan-typed-validator-fingerprint terlan-release-control-fingerprint\n\
             TERLAN_BOOTSTRAP_COMPILER := terlc\n\
             TERLAN_BOOTSTRAP_VM := terlan-vm\n\
             TERLAN_TYPED_VALIDATOR_COMMON_FINGERPRINT := common.sha256\n\
             terlan-typed-validator-fingerprint:\n\
             \t@sleep 0.2\n\
             \t@printf 'ready\\n' > common.sha256\n\
             {release_control}\n"
        ),
    )
    .unwrap();

    let status = Command::new("make")
        .current_dir(&fixture.0)
        .args([
            "--no-print-directory",
            "--jobs=8",
            "TERLAN_BUILD_ARTIFACTS_PREBUILT=1",
            "terlan-release-control-fingerprint",
        ])
        .env_remove("MAKEFLAGS")
        .env_remove("MAKEOVERRIDES")
        .env_remove("MFLAGS")
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn proof_bootstrap_is_focused_shared_and_fail_closed() {
    let source = crate::support::read_make_sources();
    let aggregate = source
        .lines()
        .find(|line| line.starts_with("terlan-self-validation-bootstrap:"))
        .unwrap();
    let mut make = format!("{aggregate}\n");
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
    make.push_str(&instrument(&recipe(
        &source,
        "terlan-proof-release-bootstrap",
    )));
    for target in [
        "release-proof-baseline-check",
        "release-artifacts-closeout-check",
    ] {
        make.push_str(&instrument(&recipe(&source, target)));
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
