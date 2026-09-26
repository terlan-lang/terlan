//! Run the production proof-composition recipes with an instrumented VM boundary.
#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
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

/// The VM boundary records actual invocations and enforces prerequisite order.
#[test]
fn proof_composition_child() {
    let Ok(stage) = std::env::var("TERLAN_PROOF_COMPOSITION_STAGE") else {
        return;
    };
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("events")
        .unwrap()
        .write_all(format!("{stage}\n").as_bytes())
        .unwrap();
    if stage == "closeout" {
        assert!(Path::new("passed-lanes").is_file());
        assert!(Path::new("passed-bootstrap").is_file());
        if std::env::var("TERLAN_PROOF_COMPOSITION_OWNED").unwrap() == "true" {
            assert!(Path::new("passed-reports").is_file());
        }
    }
    if stage == "readiness" {
        assert!(Path::new("passed-closeout").is_file());
    }
    assert_ne!(
        std::env::var("TERLAN_PROOF_COMPOSITION_FAIL").unwrap(),
        stage
    );
    fs::write(format!("passed-{stage}"), "pass").unwrap();
}

fn fixture(owned: bool) -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "terlan-proof-composition-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let fixture = Fixture(root);
    fs::create_dir_all(fixture.0.join("target/quality")).unwrap();
    fs::write(
        fixture.0.join("promotion"),
        "#!/bin/sh\nset -eu\ntest \"$*\" = 'prepare-proof-release ./vm proof.tvm'\nexport TERLAN_PROOF_COMPOSITION_STAGE=closeout\nexec \"$FIXTURE_CHILD\" --exact proof_composition_child --nocapture\n",
    )
    .unwrap();
    fs::set_permissions(
        fixture.0.join("promotion"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    fs::write(
        fixture.0.join("vm"),
        "#!/bin/sh\nset -eu\ntest \"$1 $2 $3 $4\" = 'run proof.tvm --script-eval --'\ntest $# = 5\nexport TERLAN_PROOF_COMPOSITION_STAGE=\"$5\"\nexec \"$FIXTURE_CHILD\" --exact proof_composition_child --nocapture\n",
    )
    .unwrap();
    fs::set_permissions(fixture.0.join("vm"), fs::Permissions::from_mode(0o700)).unwrap();
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let begin = source.find("\nTERLAN_PROOF_RELEASE_RUN =").unwrap();
    let end = source[begin..]
        .find("\nlean-proof-track-gap-hygiene-check:")
        .unwrap()
        + begin;
    let mut make = format!(
        "TERLAN_BOOTSTRAP_VM := ./vm\nTERLAN_RELEASE_PROMOTION := ./promotion\nTERLAN_PROOF_RELEASE_IMAGE := proof.tvm\n{}\n",
        &source[begin..end]
    );
    if owned {
        for prefix in [
            "release-artifacts-closeout-check: TERLAN_PROOF_RELEASE_RUN =",
            "release-artifacts-closeout-check: publish-evidence-staged-inputs",
            "release-readiness-attestation-check: release-artifacts-closeout-check",
        ] {
            make.push_str(
                source
                    .lines()
                    .find(|line| line.starts_with(prefix))
                    .unwrap(),
            );
            make.push('\n');
        }
        make.push_str("terlan-release-promotion-bootstrap:\npublish-evidence-staged-inputs:\n\t@TERLAN_PROOF_COMPOSITION_STAGE=reports \"$(FIXTURE_CHILD)\" --exact proof_composition_child --nocapture\nrelease-readiness-attestation-check:\n\t@TERLAN_PROOF_COMPOSITION_STAGE=readiness \"$(FIXTURE_CHILD)\" --exact proof_composition_child --nocapture\naccepted: release-readiness-attestation-check\n");
    }
    for (target, stage) in [
        ("lean-proof-lanes-check", "lanes"),
        ("terlan-proof-release-bootstrap", "bootstrap"),
    ] {
        make.push_str(&format!(
            ".PHONY: {target}\n{target}:\n\t@TERLAN_PROOF_COMPOSITION_STAGE={stage} \"$(FIXTURE_CHILD)\" --exact proof_composition_child --nocapture\n"
        ));
    }
    make.push_str("accepted: release-artifacts-closeout-check proof-readiness-release-mode-check\n\t@touch accepted\n");
    fs::write(fixture.0.join("Makefile"), make).unwrap();
    fixture
}

/// Aggregate and focused goals share one composer, including under keep-going.
#[test]
fn proof_composition_is_single_owned_and_fail_closed() {
    for (goals, failure, owned) in [
        (vec!["proof-readiness-release-mode-check"], "", false),
        (vec!["release-artifacts-closeout-check"], "", false),
        (
            vec!["accepted", "proof-readiness-release-mode-check"],
            "",
            false,
        ),
        (
            vec!["proof-readiness-release-mode-check", "accepted"],
            "",
            false,
        ),
        (vec!["accepted"], "lanes", false),
        (vec!["accepted"], "bootstrap", false),
        (vec!["accepted"], "closeout", false),
        (vec!["accepted"], "", true),
        (vec!["accepted"], "lanes", true),
        (vec!["accepted"], "bootstrap", true),
        (vec!["accepted"], "closeout", true),
        (vec!["accepted"], "reports", true),
    ] {
        let fixture = fixture(owned);
        let mut command = Command::new("make");
        command
            .current_dir(&fixture.0)
            .args(["--no-print-directory", "-j8", "-k"])
            .args(&goals)
            .env_remove("MAKEFLAGS")
            .env_remove("MFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env("FIXTURE_CHILD", std::env::current_exe().unwrap())
            .env("TERLAN_PROOF_COMPOSITION_OWNED", owned.to_string())
            .env("TERLAN_PROOF_COMPOSITION_FAIL", failure);
        assert_eq!(
            ProcessControl::new(Duration::from_secs(30))
                .run(&mut command, |_| Ok(()))
                .is_ok(),
            failure.is_empty()
        );
        let events = fs::read_to_string(fixture.0.join("events")).unwrap();
        let composers = events
            .lines()
            .filter(|stage| !matches!(*stage, "lanes" | "bootstrap" | "reports" | "readiness"))
            .collect::<Vec<_>>();
        assert_eq!(
            composers,
            if matches!(failure, "lanes" | "bootstrap" | "reports") {
                vec![]
            } else {
                vec!["closeout"]
            }
        );
        assert_eq!(
            fixture.0.join("accepted").is_file(),
            goals.contains(&"accepted") && failure.is_empty()
        );
        if owned {
            assert_eq!(
                fixture.0.join("passed-readiness").is_file(),
                failure.is_empty()
            );
        }
    }
}
