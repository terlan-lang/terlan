use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// Verifies complete dev profiles pass and write the report artifact.
///
/// Inputs:
/// - Temporary Makefile containing the three dev profiles, mapped release gates,
///   and the owning quality gate.
///
/// Output:
/// - Summary counts and a generated JSON report.
///
/// Transformation:
/// - Keeps fast local checks executable while making clear that `make check`
///   remains the release-authoritative escalation path.
#[test]
fn dev_fast_feedback_profile_accepts_complete_makefile() {
    let root = temp_repo("dev_fast_feedback_accepts");
    write_makefile(&root, &complete_makefile());

    let summary =
        run_dev_fast_feedback_profile(&root).expect("complete fast-feedback profiles should pass");

    assert_eq!(summary.profile_count, 3);
    assert_eq!(summary.mapping_count, 10);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("\"name\": \"dev-check\""));
    assert!(report.contains("\"escalation_command\": \"make check\""));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies missing dev profile targets are rejected.
///
/// Inputs:
/// - Temporary Makefile without `dev-web-check`.
///
/// Output:
/// - Diagnostic naming the missing target.
///
/// Transformation:
/// - Prevents roadmap-visible profile names from drifting into prose-only
///   requirements.
#[test]
fn dev_fast_feedback_profile_rejects_missing_profile_target() {
    let root = temp_repo("dev_fast_feedback_missing_profile");
    write_makefile(
        &root,
        &complete_makefile().replace("dev-web-check:", "web-dev-check:"),
    );

    let error = run_dev_fast_feedback_profile(&root).expect_err("missing profile should fail");

    assert!(error.contains("Makefile: missing target `dev-web-check`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies benchmark and release-scale work is forbidden in fast profiles.
///
/// Inputs:
/// - Temporary Makefile where `dev-check` invokes a benchmark lane.
///
/// Output:
/// - Diagnostic naming the forbidden marker.
///
/// Transformation:
/// - Keeps fast-feedback profiles honest and distinct from performance or
///   release-readiness gates.
#[test]
fn dev_fast_feedback_profile_rejects_benchmark_marker() {
    let root = temp_repo("dev_fast_feedback_benchmark_marker");
    write_makefile(
        &root,
        &complete_makefile().replace(
            "\t$(MAKE) cli-exact-selector-check",
            "\t$(MAKE) vm-http-vs-axum-check",
        ),
    );

    let error = run_dev_fast_feedback_profile(&root).expect_err("benchmark marker should fail");

    assert!(error.contains("must map to release gate `cli-exact-selector-check`"));
    assert!(error.contains("benchmark marker `vm-http-vs-axum-check`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies each profile keeps its declared release-gate mapping.
///
/// Inputs:
/// - Temporary Makefile where `dev-vm-check` drops one mapped gate.
///
/// Output:
/// - Diagnostic naming the missing gate mapping.
///
/// Transformation:
/// - Prevents fast profiles from silently losing representative release
///   coverage.
#[test]
fn dev_fast_feedback_profile_rejects_missing_gate_mapping() {
    let root = temp_repo("dev_fast_feedback_missing_mapping");
    write_makefile(
        &root,
        &complete_makefile().replace("\t$(MAKE) vm-runtime-concept-inventory-check\n", ""),
    );

    let error = run_dev_fast_feedback_profile(&root).expect_err("missing mapping should fail");

    assert!(error.contains(
        "Makefile: `dev-vm-check` must map to release gate `vm-runtime-concept-inventory-check`"
    ));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies defining the quality target does not bypass canonical ownership.
#[test]
fn dev_fast_feedback_profile_rejects_gate_outside_check_gates() {
    let root = temp_repo("dev_fast_feedback_unowned_gate");
    write_makefile(
        &root,
        &complete_makefile().replace(
            "CHECK_GATES := dev-fast-feedback-profile-check",
            "CHECK_GATES := rust-warnings-check",
        ),
    );

    let error = run_dev_fast_feedback_profile(&root).expect_err("unowned gate should fail");

    assert!(
        error.contains("Makefile: `CHECK_GATES` must include `dev-fast-feedback-profile-check`")
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

fn temp_repo(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("{name}_{}_{}", std::process::id(), nanos));
    fs::create_dir_all(&root).expect("create fixture root");
    root
}

fn write_makefile(root: &Path, text: &str) {
    fs::write(root.join("Makefile"), text).expect("write Makefile");
}

fn complete_makefile() -> String {
    [
        "CHECK_GATES := dev-fast-feedback-profile-check",
        "",
        "check: check-gates",
        "",
        "check-gates: $(CHECK_GATES)",
        "",
        "dev-check:",
        "\t$(MAKE) rust-warnings-check",
        "\t$(MAKE) std-test-honesty-check",
        "\t$(MAKE) terlan-lint-style-profile-check",
        "\t$(MAKE) cli-exact-selector-check",
        "",
        "dev-vm-check:",
        "\t$(MAKE) terlan-vm-run-command-check",
        "\t$(MAKE) vm-diagnostics-quality-check",
        "\t$(MAKE) vm-runtime-concept-inventory-check",
        "",
        "dev-web-check:",
        "\t$(MAKE) tree-sitter-cli-check",
        "\t$(MAKE) editor-debugger-surface-check",
        "\t$(MAKE) angular-ts-namespace-generation-check",
        "",
        "dev-fast-feedback-profile-check:",
        "\tcargo test -p terlan --bin terlan-quality dev_fast_feedback_profile_test",
        "",
        "rust-warnings-check:",
        "\tcargo check",
        "",
        "std-test-honesty-check:",
        "\tcargo test",
        "",
        "terlan-lint-style-profile-check:",
        "\tcargo test",
        "",
        "cli-exact-selector-check:",
        "\tcargo test",
        "",
        "terlan-vm-run-command-check:",
        "\tcargo test",
        "",
        "vm-diagnostics-quality-check:",
        "\tcargo test",
        "",
        "vm-runtime-concept-inventory-check:",
        "\tcargo test",
        "",
        "tree-sitter-cli-check:",
        "\tcargo test",
        "",
        "editor-debugger-surface-check:",
        "\tcargo test",
        "",
        "angular-ts-namespace-generation-check:",
        "\tcargo test",
        "",
    ]
    .join("\n")
}
