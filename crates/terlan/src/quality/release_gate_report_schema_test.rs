use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the release gate report-schema gate writes the roadmap-required
/// report.
#[test]
fn release_gate_report_schema_writes_report() {
    let repo = TempRepo::new("release_gate_report_schema_writes_report");
    repo.write(
        "docs/release/RELEASE_GATE_REPORT_SCHEMA.md",
        &complete_contract(),
    );

    let summary =
        run_release_gate_report_schema(repo.path()).expect("release gate report-schema gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        ADVERSARIAL_REPORT_FIXTURES.len(),
        summary.adversarial_case_count
    );
    assert_eq!(
        "target/quality/release-gate-report-schema-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/release-gate-report-schema-report.json"),
    )
    .expect("read release gate report-schema report");
    assert!(report.contains("terlan.release-gate-report-schema.v1"));
    assert!(report.contains("release gate report-schema contract"));
    assert!(report.contains("schema_inventory"));
    assert!(report.contains("adversarial_validation_fixtures"));
}

/// Verifies absolute-path report claims are rejected.
#[test]
fn release_gate_report_schema_rejects_absolute_path_claims() {
    let text = format!(
        "{}\nabsolute paths are acceptable in release reports",
        complete_contract()
    );

    let diagnostics = validate_release_gate_report_schema_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("absolute paths")),
        "diagnostics should reject absolute-path report claims: {diagnostics:?}"
    );
}

/// Verifies input digest evidence is required.
#[test]
fn release_gate_report_schema_rejects_missing_input_digests() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "input digests")
        .filter(|term| *term != "missing input digests")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_release_gate_report_schema_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("input digests")),
        "diagnostics should reject missing input digest evidence: {diagnostics:?}"
    );
}

/// Verifies placeholder report-schema text is rejected.
#[test]
fn release_gate_report_schema_rejects_placeholder_text() {
    let text = format!("{}\nTODO: define report schema later", complete_contract());

    let diagnostics = validate_release_gate_report_schema_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder release gate report-schema text")),
        "diagnostics should reject placeholder text: {diagnostics:?}"
    );
}

/// Verifies malformed or partial report JSON is rejected before release.
#[test]
fn release_gate_report_schema_rejects_malformed_report_json() {
    let diagnostics = validate_release_gate_report_candidate("{");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("malformed report JSON")),
        "diagnostics should reject malformed report JSON: {diagnostics:?}"
    );
}

/// Verifies schema compatibility is enforced for report payloads.
#[test]
fn release_gate_report_schema_rejects_unknown_report_schema() {
    let diagnostics = validate_release_gate_report_candidate(
        r#"{
            "schema": "terlan.unknown-report.v1",
            "gate_id": "unknown-schema",
            "input_digests": {"roadmap": "sha256:abc"},
            "decision": "pass"
        }"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("unknown schema version")),
        "diagnostics should reject unknown schema versions: {diagnostics:?}"
    );
}

#[test]
fn release_gate_report_schema_accepts_vm_db_migration_schema() {
    let diagnostics = validate_release_gate_report_candidate(
        r#"{
            "schema": "terlan.vm-db-migration-command.v1",
            "gate_id": "vm-db-migration-command-check",
            "input_digests": {"migration": "sha256:abc"},
            "decision": "pass",
            "artifact_evidence": {"migration_count": 3}
        }"#,
    );

    assert!(
        diagnostics.is_empty(),
        "registered migration report schema should validate: {diagnostics:?}"
    );
}

#[test]
fn release_gate_report_schema_accepts_vm_dev_dependency_schema() {
    let diagnostics = validate_release_gate_report_candidate(
        r#"{
            "schema": "terlan.vm-dev-dependency-orchestration.v1",
            "gate_id": "vm-dev-dependency-orchestration-check",
            "input_digests": {"compose": "sha256:abc"},
            "decision": "pass",
            "artifact_evidence": {"command_count": 6}
        }"#,
    );

    assert!(
        diagnostics.is_empty(),
        "registered dependency report schema should validate: {diagnostics:?}"
    );
}

/// Verifies release reports cannot omit input digest evidence.
#[test]
fn release_gate_report_schema_rejects_report_without_input_digests() {
    let diagnostics = validate_release_gate_report_candidate(
        r#"{
            "schema": "terlan.release-gate-report-schema.v1",
            "gate_id": "missing-digest",
            "decision": "pass"
        }"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("missing input digests")),
        "diagnostics should reject missing input digests: {diagnostics:?}"
    );
}

/// Verifies local paths and user names cannot leak into report strings.
#[test]
fn release_gate_report_schema_rejects_report_path_leakage() {
    let diagnostics = validate_release_gate_report_candidate(
        r#"{
            "schema": "terlan.release-gate-report-schema.v1",
            "gate_id": "path-leak",
            "input_digests": {"roadmap": "sha256:abc"},
            "decision": "pass",
            "diagnostics": ["/home/anatoly/Applications/terlan"]
        }"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("path leakage")),
        "diagnostics should reject path leakage: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("unredacted local user data")),
        "diagnostics should reject unredacted local user data: {diagnostics:?}"
    );
}

/// Verifies report sets cannot reuse gate identities.
#[test]
fn release_gate_report_schema_rejects_duplicated_gate_ids() {
    let diagnostics = validate_release_gate_report_set(&[
        r#"{
            "schema": "terlan.release-gate-report-schema.v1",
            "gate_id": "duplicate",
            "input_digests": {"roadmap": "sha256:abc"},
            "decision": "pass"
        }"#,
        r#"{
            "schema": "terlan.release-gate-report-schema.v1",
            "gate_id": "duplicate",
            "input_digests": {"roadmap": "sha256:def"},
            "decision": "pass"
        }"#,
    ]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("duplicated gate ID")),
        "diagnostics should reject duplicated gate IDs: {diagnostics:?}"
    );
}

/// Verifies undocumented top-level report fields are rejected.
#[test]
fn release_gate_report_schema_rejects_undocumented_report_fields() {
    let diagnostics = validate_release_gate_report_candidate(
        r#"{
            "schema": "terlan.release-gate-report-schema.v1",
            "gate_id": "extra-field",
            "input_digests": {"roadmap": "sha256:abc"},
            "decision": "pass",
            "surprise": true
        }"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("undocumented ad hoc field")),
        "diagnostics should reject undocumented fields: {diagnostics:?}"
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
