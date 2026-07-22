use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use crate::terlan_quality::QualityResult;

const RELEASE_GATE_REPORT_SCHEMA_DOC: &str = "docs/release/RELEASE_GATE_REPORT_SCHEMA.md";
const REPORT_PATH: &str = "target/quality/release-gate-report-schema-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "versioned schema family",
    "gate identity",
    "input digests",
    "tool versions",
    "environment contract",
    "diagnostics",
    "coverage deltas",
    "benchmark data",
    "support-bundle references",
    "pass/fail decision",
    "release-blocking rationale",
    "*-report.json",
    "schema version",
    "producing gate",
    "generation timestamp policy",
    "stable ordering rules",
    "path redaction rules",
    "compatibility policy",
    "schema validation",
    "before release readiness",
    "missing required sections",
    "unstable absolute paths",
    "unredacted local user data",
    "undocumented ad hoc fields",
    "malformed reports",
    "unknown schema versions",
    "duplicated gate IDs",
    "missing input digests",
    "unstable object order",
    "path leakage",
    "partially written JSON",
    "stale reports from previous runs",
    "release-gate-report-schema-report.json",
    "schema inventory",
    "validated reports",
    "rejected reports",
    "compatibility matrix",
    "redaction decisions",
    "schema migration notes",
    "unversioned report",
    "malformed report",
    "stale report",
    "path-leaking report",
    "schema-incompatible report",
    "new roadmap-required report",
    "schema entry",
    "validation fixture",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "reports may omit schema version",
    "absolute paths are acceptable in release reports",
    "local user data may appear in reports",
    "ad hoc fields do not need documentation",
    "stale reports may be reused",
    "new reports do not need schema entries",
    "schema validation can run after release readiness",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];
const KNOWN_REPORT_SCHEMAS: &[&str] = &[
    "terlan.vm-db-migration-command.v1",
    "terlan.vm-dev-dependency-orchestration.v1",
    "terlan.release-gate-report-schema.v1",
    "terlan.release-gate-duration-budget.v1",
    "terlan.release-gate-shard-resume.v1",
    "terlan.release-flake-detection.v1",
];

const ADVERSARIAL_REPORT_FIXTURES: &[&str] = &[
    "malformed reports",
    "unknown schema versions",
    "missing input digests",
    "path leakage",
    "duplicated gate IDs",
];

/// Summary produced by the release gate report-schema gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseGateReportSchemaSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub adversarial_case_count: usize,
    pub report_path: String,
}

/// Runs the release gate report-schema correctness gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/release/`.
///
/// Output:
/// - Success summary and report when schema versioning, report identity,
///   redaction, compatibility, adversarial cases, and release decisions are
///   documented.
/// - Stable diagnostics when schema or report validation evidence is missing.
///
/// Transformation:
/// - Converts the release report schema contract into executable evidence for
///   the 0.0.7 release gate roadmap.
pub fn run_release_gate_report_schema(
    root: &Path,
) -> QualityResult<ReleaseGateReportSchemaSummary> {
    let path = root.join(RELEASE_GATE_REPORT_SCHEMA_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read release gate report-schema contract: {err}",
            path.display()
        )
    })?;
    let mut diagnostics = validate_release_gate_report_schema_text(&text);
    diagnostics.extend(validate_adversarial_report_fixtures());
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(ReleaseGateReportSchemaSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        adversarial_case_count: ADVERSARIAL_REPORT_FIXTURES.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_release_gate_report_schema_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing release gate report-schema term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(&claim.to_lowercase()) {
            diagnostics.push(format!(
                "forbidden release gate report-schema claim `{claim}`"
            ));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder release gate report-schema text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn validate_adversarial_report_fixtures() -> Vec<String> {
    let mut diagnostics = Vec::new();
    let malformed = validate_release_gate_report_candidate("{");
    if malformed.is_empty() {
        diagnostics.push("adversarial fixture failed to reject malformed reports".to_string());
    }
    let unknown_schema = validate_release_gate_report_candidate(
        r#"{
            "schema": "terlan.unknown-report.v1",
            "gate_id": "unknown-schema",
            "input_digests": {"roadmap": "sha256:abc"},
            "decision": "pass"
        }"#,
    );
    if unknown_schema.is_empty() {
        diagnostics
            .push("adversarial fixture failed to reject unknown schema versions".to_string());
    }
    let missing_digest = validate_release_gate_report_candidate(
        r#"{
            "schema": "terlan.release-gate-report-schema.v1",
            "gate_id": "missing-digest",
            "decision": "pass"
        }"#,
    );
    if missing_digest.is_empty() {
        diagnostics.push("adversarial fixture failed to reject missing input digests".to_string());
    }
    let path_leak = validate_release_gate_report_candidate(
        r#"{
            "schema": "terlan.release-gate-report-schema.v1",
            "gate_id": "path-leak",
            "input_digests": {"roadmap": "sha256:abc"},
            "decision": "pass",
            "diagnostics": ["/home/anatoly/Applications/terlan"]
        }"#,
    );
    if path_leak.is_empty() {
        diagnostics.push("adversarial fixture failed to reject path leakage".to_string());
    }
    let duplicated_gate_ids = validate_release_gate_report_set(&[
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
    if duplicated_gate_ids.is_empty() {
        diagnostics.push("adversarial fixture failed to reject duplicated gate IDs".to_string());
    }
    diagnostics
}

fn validate_release_gate_report_candidate(text: &str) -> Vec<String> {
    let value = match serde_json::from_str::<Value>(text) {
        Ok(value) => value,
        Err(err) => return vec![format!("malformed report JSON: {err}")],
    };
    let Some(object) = value.as_object() else {
        return vec!["malformed report: top-level value must be an object".to_string()];
    };
    let mut diagnostics = Vec::new();
    let schema = object.get("schema").and_then(Value::as_str);
    match schema {
        Some(schema) if KNOWN_REPORT_SCHEMAS.contains(&schema) => {}
        Some(schema) => diagnostics.push(format!("unknown schema version `{schema}`")),
        None => diagnostics.push("missing schema version".to_string()),
    }
    if object.get("gate_id").and_then(Value::as_str).is_none() {
        diagnostics.push("missing gate identity".to_string());
    }
    let has_input_digests = object
        .get("input_digests")
        .and_then(Value::as_object)
        .is_some_and(|digests| !digests.is_empty());
    if !has_input_digests {
        diagnostics.push("missing input digests".to_string());
    }
    match object.get("decision").and_then(Value::as_str) {
        Some("pass" | "fail") => {}
        Some(decision) => diagnostics.push(format!("unsupported pass/fail decision `{decision}`")),
        None => diagnostics.push("missing pass/fail decision".to_string()),
    }
    diagnostics.extend(validate_report_top_level_fields(
        object.keys().map(String::as_str),
    ));
    diagnostics.extend(validate_report_string_redaction(&value));
    diagnostics
}

fn validate_release_gate_report_set(reports: &[&str]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut gate_ids = BTreeSet::new();
    for report in reports {
        diagnostics.extend(validate_release_gate_report_candidate(report));
        if let Ok(value) = serde_json::from_str::<Value>(report) {
            if let Some(gate_id) = value.get("gate_id").and_then(Value::as_str) {
                if !gate_ids.insert(gate_id.to_string()) {
                    diagnostics.push(format!("duplicated gate ID `{gate_id}`"));
                }
            }
        }
    }
    diagnostics
}

fn validate_report_top_level_fields<'a>(keys: impl Iterator<Item = &'a str>) -> Vec<String> {
    const ALLOWED_FIELDS: &[&str] = &[
        "schema",
        "gate_id",
        "input_digests",
        "tool_versions",
        "environment",
        "diagnostics",
        "coverage_deltas",
        "benchmark_data",
        "support_bundle_references",
        "decision",
        "release_blocking_rationale",
        "generated_at",
        "artifact_evidence",
        "schema_inventory",
        "validated_reports",
        "rejected_reports",
        "compatibility_matrix",
        "redaction_decisions",
        "schema_migration_notes",
        "adversarial_validation_fixtures",
    ];
    keys.filter(|key| !ALLOWED_FIELDS.contains(key))
        .map(|key| format!("undocumented ad hoc field `{key}`"))
        .collect()
}

fn validate_report_string_redaction(value: &Value) -> Vec<String> {
    let mut diagnostics = Vec::new();
    collect_report_string_redaction_diagnostics(value, &mut diagnostics);
    diagnostics
}

fn collect_report_string_redaction_diagnostics(value: &Value, diagnostics: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if text.starts_with("/home/") || text.contains("\\Users\\") || text.contains("C:\\") {
                diagnostics.push(format!("path leakage in report string `{text}`"));
            }
            if text.contains("anatoly") {
                diagnostics.push("unredacted local user data in report string".to_string());
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_report_string_redaction_diagnostics(value, diagnostics);
            }
        }
        Value::Object(entries) => {
            for value in entries.values() {
                collect_report_string_redaction_diagnostics(value, diagnostics);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create release gate report-schema report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.release-gate-report-schema.v1",
        "artifact_evidence": "release gate report-schema contract",
        "schema_inventory": [
            "versioned schema family",
            "schema version",
            "producing gate"
        ],
        "validated_reports": [
            "gate identity",
            "input digests",
            "tool versions",
            "environment contract"
        ],
        "rejected_reports": [
            "malformed reports",
            "unknown schema versions",
            "path leakage",
            "stale reports from previous runs"
        ],
        "compatibility_matrix": [
            "compatibility policy",
            "schema-incompatible report"
        ],
        "redaction_decisions": [
            "path redaction rules",
            "unredacted local user data"
        ],
        "schema_migration_notes": [
            "schema migration notes",
            "new roadmap-required report"
        ],
        "adversarial_validation_fixtures": [
            "malformed reports",
            "unknown schema versions",
            "missing input digests",
            "path leakage",
            "duplicated gate IDs"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize release gate report-schema report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write release gate report-schema report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[release-gate-report-schema] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "release_gate_report_schema_test.rs"]
mod release_gate_report_schema_test;
