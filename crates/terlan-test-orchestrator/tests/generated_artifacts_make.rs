//! Production Make ordering and single execution for generated-artifact checks.
#![cfg(unix)]

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

const GATES: [&str; 5] = [
    "stdlib-summary-drift-check",
    "stdlib-js-bindings-drift-check",
    "stdlib-native-artifacts-check",
    "stdlib-release-manifest-check",
    "tree-sitter-cli-check",
];

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Instrumented leaf worker; the production typed snapshot validator is tested separately.
#[test]
fn artifact_gate_child() {
    let Ok(phase) = std::env::var("TERLAN_ARTIFACT_PHASE") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("TERLAN_ARTIFACT_ROOT").unwrap());
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(format!("seen-{phase}")))
        .expect("producer must execute only once");
    assert_ne!(
        std::env::var("TERLAN_ARTIFACT_FAIL").unwrap_or_default(),
        phase,
        "injected failure"
    );
    match phase.as_str() {
        "editor-node-tools-check" => (),
        "release-generated-artifacts-self-test" => (),
        "release-generated-artifacts-record" => {
            assert!(
                GATES
                    .iter()
                    .all(|gate| !root.join(format!("seen-{gate}")).exists()),
                "snapshot followed a producer"
            );
            fs::copy(
                root.join("artifact"),
                root.join("target/quality/release-generated-artifacts-before.json"),
            )
            .unwrap();
        }
        "release-generated-artifacts-finalize" => {
            assert!(GATES
                .iter()
                .all(|gate| root.join(format!("passed-{gate}")).exists()));
            assert_eq!(
                fs::read(root.join("artifact")).unwrap(),
                fs::read(root.join("target/quality/release-generated-artifacts-before.json"))
                    .unwrap(),
                "artifact drift"
            );
        }
        gate if GATES.contains(&gate) => {
            if std::env::var("TERLAN_ARTIFACT_EXPECT_SNAPSHOT").unwrap() == "1" {
                assert!(
                    root.join("passed-release-generated-artifacts-record")
                        .exists(),
                    "freshness producer preceded snapshot"
                );
            }
            if std::env::var("TERLAN_ARTIFACT_MUTATE").as_deref() == Ok(gate) {
                fs::write(root.join("artifact"), "changed").unwrap();
            }
        }
        _ => panic!("unknown fixture phase {phase}"),
    }
    fs::write(root.join(format!("passed-{phase}")), "pass").unwrap();
}

fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "terlan-generated-make-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let fixture = Fixture(root);
    fs::create_dir_all(fixture.0.join("target/quality")).unwrap();
    fs::write(fixture.0.join("artifact"), "unchanged").unwrap();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(repository.join("Makefile")).unwrap();
    let start = source
        .find("RELEASE_GENERATED_ARTIFACT_FRESHNESS_GATES :=")
        .unwrap();
    let end = source[start..].find("\npackage-test-exec-check:").unwrap() + start;
    let mut make = source[start..end].to_owned();
    for phase in ["self-test", "record", "finalize"] {
        let operation = format!("release-generated-artifacts-{phase}");
        make = make.replace(&format!("$(TERLAN_TVM_PLATFORM_MATRIX) {operation}"), &format!("@TERLAN_ARTIFACT_PHASE={operation} \"$(FIXTURE_CHILD)\" --exact artifact_gate_child --nocapture"));
    }
    make.push_str("\n.PHONY: terlan-tvm-platform-matrix-bootstrap check-gates release-evidence-compose publication-source publication-evidence\nterlan-tvm-platform-matrix-bootstrap:\ncheck-gates publication-source: $(RELEASE_GENERATED_ARTIFACT_FRESHNESS_GATES)\nrelease-evidence-compose publication-evidence: release-generated-artifacts-check\n");
    make.push_str(".PHONY: editor-node-tools-check\neditor-node-tools-check:\n\t@TERLAN_ARTIFACT_PHASE=$@ \"$(FIXTURE_CHILD)\" --exact artifact_gate_child --nocapture\n");
    for gate in GATES {
        make.push_str(&format!(".PHONY: {gate}\n{gate}:\n\t@TERLAN_ARTIFACT_PHASE=$@ \"$(FIXTURE_CHILD)\" --exact artifact_gate_child --nocapture\n"));
    }
    fs::write(fixture.0.join("Makefile"), make).unwrap();
    let inventory: serde_json::Value = serde_json::from_slice(
        &fs::read(repository.join("docs/release/GENERATED_ARTIFACTS.json")).unwrap(),
    )
    .unwrap();
    let gates: BTreeSet<_> = inventory["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|artifact| artifact["freshness_gate"].as_str().unwrap())
        .collect();
    assert_eq!(
        gates,
        GATES.into_iter().collect(),
        "every inventoried freshness producer needs execution evidence"
    );
    fixture
}

fn run(fixture: &Fixture, goals: &[&str], scope: &str, failure: &str, mutate: &str) -> bool {
    let mut make = Command::new("make");
    make.current_dir(&fixture.0)
        .args(["--no-print-directory", "-j8", "-k"])
        .args(goals)
        .env_remove("MAKEFLAGS")
        .env_remove("MAKEOVERRIDES")
        .env_remove("MFLAGS")
        .env("TERLAN_ARTIFACT_ROOT", &fixture.0)
        .env(
            "TERLAN_ARTIFACT_EXPECT_SNAPSHOT",
            if matches!(scope, "focused" | "unowned") {
                "0"
            } else {
                "1"
            },
        )
        .env(
            "TERLAN_CHECK_RELEASE_EVIDENCE",
            if scope == "canonical" { "1" } else { "0" },
        )
        .env(
            "TERLAN_RUST_COVERAGE_SCOPE",
            if scope == "hosted" {
                "hosted-source"
            } else {
                ""
            },
        )
        .env("TERLAN_ARTIFACT_FAIL", failure)
        .env("TERLAN_ARTIFACT_MUTATE", mutate)
        .env("FIXTURE_CHILD", std::env::current_exe().unwrap());
    ProcessControl::new(Duration::from_secs(30))
        .run(&mut make, |_| Ok(()))
        .is_ok()
}

/// Exercises both root orders, inherited release scope, failures and changed output.
#[test]
fn generated_artifacts_share_freshness_producers_and_preserve_snapshot_order() {
    for (goals, scope) in [
        (vec!["release-generated-artifacts-check"], "standalone"),
        (
            vec![GATES[0], "release-generated-artifacts-check"],
            "standalone",
        ),
        (vec!["check-gates", "release-evidence-compose"], "canonical"),
        (vec!["release-evidence-compose", "check-gates"], "canonical"),
        (vec!["publication-source", "publication-evidence"], "hosted"),
    ] {
        let fixture = fixture();
        assert!(run(&fixture, &goals, scope, "", ""));
        assert!(fixture
            .0
            .join("passed-release-generated-artifacts-finalize")
            .exists());
        assert!(!fixture
            .0
            .join("target/quality/release-generated-artifacts-before.json")
            .exists());
    }
    for failure in [
        "editor-node-tools-check",
        "release-generated-artifacts-record",
    ]
    .into_iter()
    .chain(GATES)
    {
        let fixture = fixture();
        assert!(!run(
            &fixture,
            &["check-gates", "release-evidence-compose"],
            "canonical",
            failure,
            ""
        ));
        assert!(!fixture
            .0
            .join("seen-release-generated-artifacts-finalize")
            .exists());
        if failure == "editor-node-tools-check" {
            assert!(!fixture
                .0
                .join("seen-release-generated-artifacts-record")
                .exists());
            assert!(GATES
                .iter()
                .all(|gate| !fixture.0.join(format!("seen-{gate}")).exists()));
        }
    }
    let changed = fixture();
    assert!(!run(
        &changed,
        &["release-generated-artifacts-check"],
        "standalone",
        "",
        GATES[0]
    ));
    assert!(!changed
        .0
        .join("passed-release-generated-artifacts-finalize")
        .exists());
    let focused = fixture();
    assert!(run(&focused, &[GATES[0]], "focused", "", ""));
    assert!(!focused
        .0
        .join("seen-release-generated-artifacts-record")
        .exists());
    let unowned = fixture();
    assert!(!run(
        &unowned,
        &[GATES[0], "publication-evidence"],
        "unowned",
        "",
        ""
    ));
    assert!(!unowned
        .0
        .join("passed-release-generated-artifacts-finalize")
        .exists());
    assert!(unowned.0.join(format!("passed-{}", GATES[0])).exists());
    assert!(!unowned
        .0
        .join("seen-release-generated-artifacts-record")
        .exists());
}

/// Cached dependencies must not hide missing or non-executable host tools.
#[test]
fn editor_tools_are_executed_before_admitting_generated_artifact_work() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(repository.join("editors/editor.mk")).unwrap();
    let start = source.find(".PHONY: editor-node-tools-check\n").unwrap();
    let end = source[start..].find("\neditor-help:").unwrap() + start;
    let make = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join("make"))
        .find(|path| path.is_file())
        .unwrap();
    for (node, npm, passed) in [
        (None, Some(0), false),
        (Some(126), Some(0), false),
        (Some(0), None, false),
        (Some(0), Some(126), false),
        (Some(0), Some(0), true),
    ] {
        let fixture = fixture();
        let bin = fixture.0.join("bin");
        fs::create_dir(&bin).unwrap();
        for (name, status) in [("node", node), ("npm", npm)] {
            if let Some(status) = status {
                let path = bin.join(name);
                fs::write(
                    &path,
                    format!("#!/bin/sh\ntest \"$1\" = --version || exit 99\nexit {status}\n"),
                )
                .unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        fs::write(
            fixture.0.join("Makefile"),
            format!(
                "{}\nall: editor-node-tools-check\n\t@echo admitted > admitted\n",
                &source[start..end]
            ),
        )
        .unwrap();
        let mut command = Command::new(&make);
        command
            .current_dir(&fixture.0)
            .args(["--no-print-directory", "all", "SHELL=/bin/sh"])
            .env("PATH", &bin)
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS");
        assert_eq!(
            ProcessControl::new(Duration::from_secs(30))
                .run(&mut command, |_| Ok(()))
                .is_ok(),
            passed
        );
        assert_eq!(fixture.0.join("admitted").exists(), passed);
    }
}
