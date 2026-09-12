//! Real parallel Make graph with fixture-owned report and distribution producers.
//! Coverage and Cargo remain real; this fixture does not download or attest releases.

use super::*;
use std::os::unix::fs::PermissionsExt;

/// Keeps production dependencies while substituting external report services.
pub(super) fn prepare(root: &Path, publication: &str) -> String {
    write(root, "target/publication-stage", STAGE);
    fs::set_permissions(
        root.join("target/publication-stage"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let downloader =
        "bash scripts/download_validated_release_artifacts.sh \"$$(git rev-parse HEAD)\" --restore";
    assert_eq!(publication.matches(downloader).count(), 1);
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let expanded = source.replace("\\\n", " ");
    let lease = expanded
        .lines()
        .find(|line| line.starts_with("TERLAN_PREPARATION_OWNER ="))
        .unwrap();
    let mut proof_rules = String::new();
    for target in [
        "lean-proof-native-boundary-check: native-boundary-security-check",
        "lean-proof-smoke-check: proof-repro-check",
        "lean-proof-lanes-check: lean-proof-smoke-check",
    ] {
        proof_rules.push_str(make_block(&source, target, "\n\n"));
        proof_rules.push('\n');
    }
    format!(
        "{}\n{lease}\n{proof_rules}\n{}",
        publication.replace(downloader, "$(TERLAN_FIXTURE_STAGE) restore"),
        DECLARATIONS
    )
}

const DECLARATIONS: &str = r#"
TERLAN_FIXTURE_STAGE := $(CURDIR)/target/publication-stage
TERLAN_RELEASE_PROMOTION := $(TERLAN_FIXTURE_STAGE)
TERLAN_TVM_PLATFORM_MATRIX := $(TERLAN_FIXTURE_STAGE)
TERLAN_OWNED_PROOF_TRACK = $(TERLAN_FIXTURE_STAGE) prepare-proof-kernels
TERLAN_PROOF_TRACK_RUN = $(TERLAN_FIXTURE_STAGE) unowned-proof
TERLAN_OWNED_PROOF_POLICIES = $(TERLAN_FIXTURE_STAGE) prepare-proof-policies
TERLAN_OWNED_PROOF_NATIVE_BOUNDARY = $(TERLAN_FIXTURE_STAGE) prepare-proof-native
TERLAN_OWNED_PROOF_SMOKE = $(TERLAN_FIXTURE_STAGE) prepare-proof-smoke
TERLAN_OWNED_PROOF_LANES = $(TERLAN_FIXTURE_STAGE) prepare-proof-lanes
TERLAN_PROOF_RUNTIME_RUN = $(TERLAN_FIXTURE_STAGE) unowned-runtime
TERLAN_PROOF_PR_RUN = $(TERLAN_FIXTURE_STAGE) unowned-pr
TERLAN_PROOF_REGRESSION_RUN = $(TERLAN_FIXTURE_STAGE) unowned-regression
.PHONY: proof-repro-check lean-proof-lanes-check publish-preparation-lock-directory terlan-release-closeout-bootstrap release-generated-artifacts-check
terlan-semantic-kernel-bootstrap:
publish-preparation-lock-directory:
	mkdir -p target/quality
native-boundary-security-check:
proof-repro-check: lean-proof-track-runtime-check
	$(TERLAN_PROOF_TRACK_RUN)
lean-proof-track-runtime-check:
	$(TERLAN_PROOF_RUNTIME_RUN)
lean-proof-track-pr-gate:
	$(TERLAN_PROOF_PR_RUN)
lean-proof-track-regression-check:
	$(TERLAN_PROOF_REGRESSION_RUN)
lean-proof-semantic-kernels-check:
	$(TERLAN_SEMANTIC_KERNEL_RUN)
terlan-release-closeout-bootstrap:
release-generated-artifacts-check:
	@if test "$(TERLAN_RUST_COVERAGE_SCOPE)" = hosted-source; then $(TERLAN_FIXTURE_STAGE) generated; fi
"#;

/// Isolated side effects prove order; a failure must not admit later consumers.
const STAGE: &str = r#"#!/bin/sh
set -eu
stage=$1
events=target/publication-events
failure=
if test -f target/publication-failure; then read -r failure < target/publication-failure; fi
if test "$failure" = "$stage"; then
    printf 'failed:%s\n' "$stage" >> "$events"
    exit 17
fi
case "$stage" in
    prepare-proof-kernels)
        "$0" prepare-proof-track
        "$0" prepare-proof-semantic
        exit 0 ;;
    prepare-proof-policies)
        printf 'batch\n' >> target/publication-policy-batches
        for policy in lean-proof-runtime lean-proof-pr lean-proof-regression; do "$0" "$policy"; done
        exit 0 ;;
    lean-proof-runtime) ;;
    lean-proof-pr) test "$(grep -cx lean-proof-runtime "$events")" -eq 1 ;;
    lean-proof-regression) test "$(grep -cx lean-proof-pr "$events")" -eq 1 ;;
    prepare-proof-track) test "$(grep -cx lean-proof-regression "$events")" -eq 1 ;;
    prepare-proof-semantic) test "$(grep -cx prepare-proof-track "$events")" -eq 1 ;;
    prepare-proof-native) test "$(grep -cx prepare-proof-semantic "$events")" -eq 1 ;;
    prepare-proof-smoke) test "$(grep -cx prepare-proof-native "$events")" -eq 1 ;;
    prepare-proof-lanes) test "$(grep -cx prepare-proof-smoke "$events")" -eq 1 ;;
    prepare-release-reports) test "$(grep -cx prepare-proof-lanes "$events")" -eq 1 ;;
    restore) test "$(grep -cx prepare-release-reports "$events")" -eq 1 ;;
    release-artifact-matrix) test "$(grep -cx restore "$events")" -eq 1 ;;
    consume|generated) test "$(grep -cx release-artifact-matrix "$events")" -eq 1 ;;
    *) exit 18 ;;
esac
printf '%s\n' "$stage" >> "$events"
"#;

/// Uses the parent suite once; all later Make requests consume exact coverage.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(super) fn exercise(root: &Path) {
    let bodies = fs::read(root.join("target/bodies.txt")).unwrap();
    for (failure, goals) in [
        ("", vec!["publish-evidence-covered-gates"]),
        (
            "",
            vec![
                "release-evidence-compose",
                "publish-evidence-covered-gates",
                "lean-proof-semantic-kernels-check",
            ],
        ),
        (
            "source-prerequisites",
            vec![
                "release-evidence-compose",
                "release-generated-artifacts-check",
                "publish-evidence-covered-gates",
            ],
        ),
        ("lean-proof-runtime", vec!["publish-evidence-covered-gates"]),
        ("lean-proof-pr", vec!["publish-evidence-covered-gates"]),
        (
            "lean-proof-regression",
            vec!["publish-evidence-covered-gates"],
        ),
        (
            "prepare-proof-track",
            vec!["publish-evidence-covered-gates"],
        ),
        (
            "prepare-proof-semantic",
            vec!["publish-evidence-covered-gates"],
        ),
        (
            "prepare-proof-native",
            vec!["publish-evidence-covered-gates"],
        ),
        (
            "prepare-proof-smoke",
            vec!["publish-evidence-covered-gates"],
        ),
        (
            "prepare-proof-lanes",
            vec!["publish-evidence-covered-gates"],
        ),
        (
            "prepare-release-reports",
            vec!["publish-evidence-covered-gates"],
        ),
        ("restore", vec!["publish-evidence-covered-gates"]),
        (
            "release-artifact-matrix",
            vec!["publish-evidence-covered-gates"],
        ),
    ] {
        write(root, "target/publication-events", b"");
        write(root, "target/publication-policy-batches", b"");
        write(root, "target/publication-failure", format!("{failure}\n"));
        write(
            root,
            "target/publication-source-failure",
            if failure == "source-prerequisites" {
                "fail"
            } else {
                ""
            },
        );
        let entries = fs::read_to_string(root.join("target/gate-entries")).unwrap();
        let mut make = command(root, root.join("target/driver"));
        make.env("MAKEFLAGS", "-j8 -k");
        make.args([
            "--with-hosted-cargo-coverage",
            "--",
            "make",
            "--no-print-directory",
        ])
        .args(goals);
        let captured = ProcessControl::new(Duration::from_secs(120))
            .capture_stdout_result(&mut make, 128 * 1024, |_| Ok(()))
            .unwrap();
        let output = String::from_utf8(captured.stdout).unwrap();
        let passed = failure.is_empty();
        assert_eq!(captured.outcome.is_ok(), passed, "{failure}: {output}");
        assert_eq!(
            output.contains(
                "[release-evidence-compose] version fixture candidate-bound evidence composed"
            ),
            passed,
            "{output}"
        );
        let events = fs::read_to_string(root.join("target/publication-events")).unwrap();
        let events = events.lines().collect::<Vec<_>>();
        let stages = [
            "lean-proof-runtime",
            "lean-proof-pr",
            "lean-proof-regression",
            "prepare-proof-track",
            "prepare-proof-semantic",
            "prepare-proof-native",
            "prepare-proof-smoke",
            "prepare-proof-lanes",
            "prepare-release-reports",
            "restore",
            "release-artifact-matrix",
        ];
        if passed {
            assert_eq!(&events[..stages.len()], &stages);
            let mut consumers = events[stages.len()..].to_vec();
            consumers.sort();
            assert_eq!(consumers, ["consume", "generated"]);
        } else if failure == "source-prerequisites" {
            assert!(
                events.is_empty(),
                "source failure entered a producer: {events:?}"
            );
        } else {
            let failed = stages.iter().position(|stage| *stage == failure).unwrap();
            assert_eq!(&events[..failed], &stages[..failed]);
            assert_eq!(events[failed..], [format!("failed:{failure}")]);
        }
        assert_eq!(
            fs::read_to_string(root.join("target/publication-policy-batches")).unwrap(),
            if failure == "source-prerequisites" {
                ""
            } else {
                "batch\n"
            },
            "policy graph admitted more than once"
        );
        assert_eq!(
            fs::read_to_string(root.join("target/gate-entries")).unwrap(),
            format!("{entries}normal\n"),
            "shared source gate replayed"
        );
        assert_eq!(
            fs::read(root.join("target/bodies.txt")).unwrap(),
            bodies,
            "Rust test body replayed"
        );
        let report: Value = serde_json::from_slice(
            &fs::read(root.join("target/quality/hosted-source-coverage.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report["decision"] == "hosted-source-gates-covered", passed);
    }
    println!("[publication-graph] shared sources once; proof/report/restore/matrix order preserved; source and producer failures block consumers and composition under -j8 -k");
}
