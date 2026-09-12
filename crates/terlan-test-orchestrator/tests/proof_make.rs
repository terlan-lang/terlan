//! Exercise production proof dependencies with bounded, instrumented leaf producers.
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

fn dependencies(gate: &str) -> &'static [&'static str] {
    match gate {
        "lean-proof-semantic-kernels-check" => &["lean-proof-feature-cull-check"],
        "lean-proof-track-runtime-check" => &["lean-proof-feature-cull-check"],
        "proof-repro-check" => &["lean-proof-track-runtime-check"],
        "lean-proof-smoke-check" => &["proof-repro-check", "lean-proof-native-boundary-check"],
        "lean-proof-lanes-check" => &[
            "lean-proof-smoke-check",
            "lean-proof-track-pr-gate",
            "lean-proof-track-regression-check",
            "lean-proof-semantic-kernels-check",
        ],
        "lean-proof-feature-binding-contract-check" | "release-artifacts-closeout-check" => {
            &["lean-proof-lanes-check"]
        }
        "lean-proof-feature-binding-check" => &["lean-proof-feature-binding-contract-check"],
        "lean-proof-parser-shape-check" | "lalrpop-parser-parity-check" => {
            &["lalrpop-grammar-contract-check"]
        }
        _ => &[],
    }
}

#[test]
fn proof_gate_child() {
    let Some(root) = std::env::var_os("TERLAN_PROOF_GRAPH_FIXTURE") else {
        return;
    };
    let root = PathBuf::from(root);
    let gate = std::env::var("TERLAN_PROOF_GRAPH_GATE").unwrap();
    assert!(gate
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(format!("started-{gate}")))
        .expect("duplicate proof producer");
    for dependency in dependencies(&gate) {
        assert!(
            root.join(format!("passed-{dependency}")).is_file(),
            "{gate} ran before {dependency}"
        );
    }
    assert_ne!(
        std::env::var("TERLAN_PROOF_GRAPH_FAIL").ok().as_deref(),
        Some(gate.as_str()),
        "injected proof producer failure"
    );
    fs::write(
        root.join(format!("passed-{gate}")),
        std::process::id().to_string(),
    )
    .unwrap();
}

fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "terlan-proof-make-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let fixture = Fixture(root);
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let begin = source.find(".PHONY: lean-proof-lanes-check").unwrap();
    let end = source[begin..].find("\nRELEASE_EVIDENCE_GATES :=").unwrap() + begin;
    let mut make = String::from(".DEFAULT_GOAL := lean-proof-track-check\nLEAN_PROOF_CLOSEOUT_DEPS := lean-proof-track-check\n");
    let mut in_recipe = false;
    for line in source[begin..end].lines() {
        if line.starts_with('\t') {
            if !in_recipe {
                make.push_str("\t@TERLAN_PROOF_GRAPH_GATE=$@ \"$(FIXTURE_CHILD)\" --exact proof_gate_child --nocapture\n");
                in_recipe = true;
            }
        } else {
            in_recipe = false;
            make.push_str(line);
            make.push('\n');
        }
    }
    for line in source.lines().filter(|line| {
        line.contains(": | terlan-")
            && (line.starts_with("lean-proof-")
                || line.starts_with("lalrpop-grammar-contract-check"))
    }) {
        make.push_str(line);
        make.push('\n');
    }
    make.push_str(".PHONY: terlan-self-validation-bootstrap terlan-compiler-bootstrap terlan-quality-tools-bootstrap terlan-semantic-kernel-bootstrap tree-sitter-package-check tree-sitter-cli-check editor-check native-boundary-security-check rust-test-suite\nterlan-self-validation-bootstrap terlan-compiler-bootstrap terlan-quality-tools-bootstrap terlan-semantic-kernel-bootstrap tree-sitter-package-check tree-sitter-cli-check editor-check native-boundary-security-check rust-test-suite:\n");
    make.push_str(".PHONY: terlan-proof-release-bootstrap\nterlan-proof-release-bootstrap:\n");
    make.push_str(".PHONY: terlan-release-promotion-bootstrap publish-preparation-lock-directory\nterlan-release-promotion-bootstrap publish-preparation-lock-directory:\n");
    fs::write(fixture.0.join("Makefile"), make).unwrap();
    fixture
}

fn run(root: &Path, goals: &[&str], fail: Option<&str>) -> bool {
    let mut command = Command::new("make");
    command
        .current_dir(root)
        .args(["--no-print-directory", "-j8"])
        .args(goals)
        .env_remove("MAKEFLAGS")
        .env_remove("MFLAGS")
        .env_remove("MAKEOVERRIDES")
        .env("TERLAN_PROOF_GRAPH_FIXTURE", root)
        .env("FIXTURE_CHILD", std::env::current_exe().unwrap());
    if let Some(gate) = fail {
        command.env("TERLAN_PROOF_GRAPH_FAIL", gate);
    } else {
        command.env_remove("TERLAN_PROOF_GRAPH_FAIL");
    }
    ProcessControl::new(Duration::from_secs(30))
        .run(&mut command, |_| Ok(()))
        .is_ok()
}

fn completed(root: &Path) -> BTreeSet<String> {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter_map(|name| name.strip_prefix("passed-").map(String::from))
        .collect()
}

#[test]
fn parallel_proof_aggregates_share_producers_and_preserve_evidence_order() {
    let goals = [
        "lean-proof-track-check",
        "release-artifacts-closeout-check",
        "lean-proof-feature-binding-check",
        "lalrpop-parser-parity-check",
        "lean-proof-parser-shape-check",
    ];
    let first = fixture();
    assert!(run(&first.0, &goals, None));
    let passed = completed(&first.0);
    for gate in [
        "lean-proof-feature-cull-check",
        "lean-proof-semantic-kernels-check",
        "lean-proof-track-runtime-check",
        "proof-repro-check",
        "lean-proof-native-boundary-check",
        "lean-proof-smoke-check",
        "lean-proof-track-pr-gate",
        "lean-proof-track-regression-check",
        "lean-proof-lanes-check",
        "lean-proof-feature-binding-contract-check",
        "release-artifacts-closeout-check",
        "lean-proof-feature-binding-check",
        "lalrpop-grammar-contract-check",
        "lalrpop-parser-parity-check",
        "lean-proof-parser-shape-check",
    ] {
        assert!(passed.contains(gate), "missing required producer {gate}");
    }
    assert_eq!(passed.len(), 15);
    let second = fixture();
    let reversed = goals.into_iter().rev().collect::<Vec<_>>();
    assert!(run(&second.0, &reversed, None));
    assert_eq!(completed(&second.0), passed);
    let failed = fixture();
    assert!(!run(&failed.0, &goals, Some("proof-repro-check")));
    let passed = completed(&failed.0);
    for consumer in [
        "proof-repro-check",
        "lean-proof-smoke-check",
        "lean-proof-lanes-check",
        "lean-proof-feature-binding-contract-check",
        "release-artifacts-closeout-check",
    ] {
        assert!(
            !passed.contains(consumer),
            "sealed after failed prerequisite: {consumer}"
        );
    }
}
