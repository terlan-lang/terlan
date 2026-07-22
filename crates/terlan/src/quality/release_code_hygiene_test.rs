use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_roadmap_contract() -> String {
    REQUIRED_ROADMAP_TERMS.join("\n")
}

fn complete_makefile_contract() -> String {
    let mut lines = REQUIRED_SUB_GATES
        .iter()
        .map(|target| format!("{target}:\n\t@true"))
        .collect::<Vec<_>>();
    lines.push(format!(
        "{GATE_TARGET}:\n\t$(MAKE) --no-print-directory rust-warnings-check\n\t$(MAKE) --no-print-directory rust-quality-check\n\t$(MAKE) --no-print-directory dormant-runtime-code-check\n\t$(MAKE) --no-print-directory vm-deterministic-hashmap-check\n\t$(MAKE) --no-print-directory shared-helper-check\n\t$(MAKE) --no-print-directory terlan-lint-style-profile-check\n\t$(MAKE) --no-print-directory terlan-lint-pipe-canonicalization-check\n\t$(RUST_TEST) -p terlan --bin terlan-quality release_code_hygiene_test\n\t$(CARGO) run -p terlan --bin terlan-quality --quiet -- release-code-hygiene"
    ));
    lines.join("\n\n")
}

/// Verifies the release code-hygiene gate writes the roadmap-required report.
#[test]
fn release_code_hygiene_writes_report() {
    let repo = TempRepo::new("release_code_hygiene_writes_report");
    repo.write(ROADMAP_PATH, &complete_roadmap_contract());
    repo.write(MAKEFILE_PATH, &complete_makefile_contract());

    let summary = run_release_code_hygiene(repo.path()).expect("release code hygiene gate");

    assert_eq!(REQUIRED_SUB_GATES.len(), summary.sub_gate_count);
    assert_eq!(REQUIRED_ROADMAP_TERMS.len(), summary.roadmap_term_count);
    assert_eq!(REPORT_PATH, summary.report_path);
    let report =
        fs::read_to_string(repo.path().join(REPORT_PATH)).expect("read release hygiene report");
    assert!(report.contains("terlan.release-code-hygiene.v1"));
    assert!(report.contains("rust-warnings-check"));
    assert!(report.contains("shared-helper-check"));
    assert!(report.contains("panic_unwrap_inventory"));
    assert!(report.contains("dead_code_inventory"));
    assert!(report.contains("duplicate_helper_findings"));
    assert!(report.contains("remediation_owners"));
    assert!(report.contains("owner, reason, expiry milestone"));
}

/// Verifies roadmap ownership is required before the umbrella gate can pass.
#[test]
fn release_code_hygiene_rejects_missing_roadmap_terms() {
    let diagnostics =
        validate_release_code_hygiene_contract("Slice 63: enforce release code hygiene", "");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("release-code-hygiene-report.json")),
        "diagnostics should reject missing roadmap report ownership: {diagnostics:?}"
    );
}

/// Verifies every hygiene sub-gate must exist as a Make target.
#[test]
fn release_code_hygiene_rejects_missing_sub_gate_target() {
    let makefile = complete_makefile_contract().replace("shared-helper-check:\n\t@true\n\n", "");

    let diagnostics = validate_makefile_targets(&makefile);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("shared-helper-check")),
        "diagnostics should reject missing sub-gate target: {diagnostics:?}"
    );
}

/// Verifies the umbrella target must call the sub-gates before reporting.
#[test]
fn release_code_hygiene_rejects_missing_umbrella_command() {
    let makefile = complete_makefile_contract().replace(
        "\n\t$(MAKE) --no-print-directory terlan-lint-pipe-canonicalization-check",
        "",
    );

    let diagnostics = validate_makefile_targets(&makefile);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("terlan-lint-pipe-canonicalization-check")),
        "diagnostics should reject missing umbrella command: {diagnostics:?}"
    );
}

/// Verifies the umbrella target preserves deterministic sub-gate order.
#[test]
fn release_code_hygiene_rejects_misordered_umbrella_commands() {
    let makefile = complete_makefile_contract().replace(
        "\t$(MAKE) --no-print-directory rust-warnings-check\n\t$(MAKE) --no-print-directory rust-quality-check",
        "\t$(MAKE) --no-print-directory rust-quality-check\n\t$(MAKE) --no-print-directory rust-warnings-check",
    );

    let diagnostics = validate_makefile_targets(&makefile);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("after the previous hygiene command")),
        "diagnostics should reject reordered umbrella commands: {diagnostics:?}"
    );
}

/// Verifies placeholder report payloads are rejected.
#[test]
fn release_code_hygiene_rejects_placeholder_report_payloads() {
    let mut report = report_payload();
    report["warning_policy"] = serde_json::json!("TODO");

    let diagnostics = validate_report_payload(&report);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("forbidden placeholder")),
        "diagnostics should reject placeholder report payloads: {diagnostics:?}"
    );
}

/// Verifies the report schema carries every roadmap-required evidence section.
#[test]
fn release_code_hygiene_rejects_missing_required_report_evidence_section() {
    let mut report = report_payload();
    report
        .as_object_mut()
        .expect("report object")
        .remove("panic_unwrap_inventory");

    let diagnostics = validate_report_payload(&report);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("panic_unwrap_inventory")),
        "diagnostics should reject missing panic/unwrap inventory: {diagnostics:?}"
    );
}

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(name: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("terlan_{name}_{stamp}"));
        fs::create_dir_all(&path).expect("create temp repo");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative_path: &str, text: &str) {
        let path = self.path.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, text).expect("write fixture");
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
