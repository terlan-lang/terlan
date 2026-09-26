//! Exercise the production proof-binding DAG with instrumented tool boundaries.
#![cfg(unix)]

use std::fs;
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

fn executable(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::write(&path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn fixture(owned: bool) -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "terlan-proof-binding-make-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let fixture = Fixture(root);
    fs::create_dir_all(fixture.0.join("target/debug")).unwrap();
    executable(
        &fixture.0,
        "record",
        "#!/bin/sh\nset -eu\necho \"$1\" >> events\ntest \"$1\" != \"$FAIL_STAGE\"\ntouch \"passed-$1\"\n",
    );
    executable(
        &fixture.0,
        "promotion",
        "#!/bin/sh\nset -eu\ntest \"$*\" = 'prepare-proof-binding target/debug/terlc'\ntest -f passed-lanes\ntest -f passed-bootstrap\ntest -f passed-lock\nexec ./record owner\n",
    );
    executable(
        &fixture.0,
        "target/debug/terlc",
        "#!/bin/sh\nset -eu\ntest \"$1 $2\" = 'test --incremental'\ncase \"$3\" in\nscripts/self_validation/LeanProofFeatureBindingTest.terl)\n test $# = 3\n test -f passed-lanes\n exec ./record matrix;;\nscripts/self_validation/LeanProofSnapshotTest.terl)\n test \"$TERLAN_LEAN_PROOF_ROOT\" = \"$PWD\"\n case \"$TERLAN_LEAN_SNAPSHOT_TASK\" in\n diff) test $# = 3; test -f passed-matrix;;\n impact) test -f passed-diff;;\n review) test -f passed-impact;;\n snapshot) test -f passed-review;;\n *) exit 9;;\n esac\n if test \"$TERLAN_LEAN_SNAPSHOT_TASK\" != diff; then\n test $# = 5; test \"$4 $5\" = '--name selected_snapshot_task_holds'; fi\n exec ./record \"$TERLAN_LEAN_SNAPSHOT_TASK\";;\n*) exit 9;;\nesac\n",
    );
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let begin = source.find("\nTERLAN_PROOF_BINDING_RUN =").unwrap();
    let end = begin
        + source[begin..]
            .find("\nlean-proof-counterexample-check:")
            .unwrap();
    let mut make = format!(
        "TERLAN_RELEASE_PROMOTION := ./promotion\nTERLAN_PREPARATION_OWNER :=\n{}\n",
        &source[begin..end]
    );
    if owned {
        for prefix in [
            "lean-proof-feature-binding-contract-check: TERLAN_PROOF_BINDING_RUN =",
            "lean-proof-feature-binding-contract-check: |",
            "lean-proof-feature-binding-check: TERLAN_PROOF_SNAPSHOT_RUN =",
            "lean-proof-change-impact-report lean-proof-feature-binding-review lean-proof-snapshot-consistency-check: TERLAN_PROOF_SNAPSHOT_SELECTED_RUN =",
        ] {
            make.push_str(source.lines().find(|line| line.starts_with(prefix)).unwrap());
            make.push('\n');
        }
    }
    for (target, stage) in [
        ("lean-proof-lanes-check", "lanes"),
        ("terlan-release-promotion-bootstrap", "bootstrap"),
        ("publish-preparation-lock-directory", "lock"),
    ] {
        make.push_str(&format!(
            ".PHONY: {target}\n{target}:\n\t@./record {stage}\n"
        ));
    }
    make.push_str("accepted: lean-proof-snapshot-consistency-check\n\t@touch accepted\n");
    fs::write(fixture.0.join("Makefile"), make).unwrap();
    fixture
}

#[test]
fn proof_binding_preserves_order_and_owns_the_chain_once() {
    for owned in [false, true] {
        for failure in if owned {
            vec!["", "lanes", "bootstrap", "lock", "owner"]
        } else {
            vec![
                "", "lanes", "matrix", "diff", "impact", "review", "snapshot",
            ]
        } {
            let fixture = fixture(owned);
            let mut command = Command::new("make");
            command
                .current_dir(&fixture.0)
                .args([
                    "--no-print-directory",
                    "-j8",
                    "-k",
                    "accepted",
                    "lean-proof-feature-binding-check",
                    "lean-proof-feature-binding-review",
                ])
                .env_remove("MAKEFLAGS")
                .env_remove("MFLAGS")
                .env_remove("MAKEOVERRIDES")
                .env("FAIL_STAGE", failure);
            assert_eq!(
                ProcessControl::new(Duration::from_secs(30))
                    .run(&mut command, |_| Ok(()))
                    .is_ok(),
                failure.is_empty(),
                "owned={owned}, failure={failure}"
            );
            let events = fs::read_to_string(fixture.0.join("events")).unwrap();
            let producers = events
                .lines()
                .filter(|stage| !matches!(*stage, "lanes" | "bootstrap" | "lock"))
                .collect::<Vec<_>>();
            let all = if owned {
                vec!["owner"]
            } else {
                vec!["matrix", "diff", "impact", "review", "snapshot"]
            };
            let expected = if failure.is_empty() {
                all.clone()
            } else if let Some(index) = all.iter().position(|stage| *stage == failure) {
                all[..=index].to_vec()
            } else {
                vec![]
            };
            assert_eq!(producers, expected, "owned={owned}, failure={failure}");
            assert_eq!(fixture.0.join("accepted").is_file(), failure.is_empty());
        }
    }
}
