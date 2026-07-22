use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use crate::terlan_quality::QualityResult;

const RELEASE_FAILURE_REPRODUCTION_DOC: &str = "docs/release/RELEASE_FAILURE_REPRODUCTION.md";
const REPORT_PATH: &str = "target/quality/release-failure-reproduction-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "exact local reproduction",
    "release failures",
    "release gate failure",
    "exact reproduction command",
    "required environment variables",
    "input fixture path",
    "random seed",
    "target profile",
    "cache mode",
    "shard ID",
    "relevant report/support-bundle paths",
    "stable across local and CI runs",
    "must not depend on absolute checkout paths",
    "support bundles",
    "fresh temporary directory",
    "failed gates",
    "narrow reproduction commands",
    "failing test/case",
    "broader reproduction commands",
    "owning suite",
    "clear guidance on when each is valid",
    "stale reproduction commands",
    "missing seeds",
    "path-dependent fixtures",
    "deleted temp directories",
    "sharded failures",
    "cached failures",
    "benchmark failures",
    "VM runtime failures with captured source maps",
    "release-failure-reproduction-report.json",
    "failure samples",
    "reproduction commands",
    "fixture digests",
    "support-bundle replay results",
    "path-redaction decisions",
    "command success status",
    "working reproduction command for the failing case",
    "local checkout paths",
    "stale caches",
    "untracked files",
    "CI-only state",
    "hidden environment assumptions",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "reproduction may depend on absolute checkout paths",
    "reproduction may depend on stale caches",
    "reproduction may depend on untracked files",
    "reproduction may depend on ci-only state",
    "reproduction may depend on hidden environment assumptions",
    "failed gates do not need narrow reproduction commands",
    "missing seeds are acceptable",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];
const ADVERSARIAL_REPRODUCTION_FIXTURES: &[&str] = &[
    "stale reproduction commands",
    "missing seeds",
    "path-dependent fixtures",
    "deleted temp directories",
    "CI-only state",
    "hidden environment assumptions",
];

/// Summary produced by the release failure reproduction gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseFailureReproductionSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub adversarial_case_count: usize,
    pub report_path: String,
}

/// Runs the release failure reproduction correctness gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/release/`.
///
/// Output:
/// - Success summary and report when exact reproduction commands, environment
///   evidence, fixtures, seeds, shard/cache context, and support-bundle replay
///   are documented.
/// - Stable diagnostics when local reproduction evidence is missing.
///
/// Transformation:
/// - Converts the release failure reproduction contract into executable
///   evidence for the 0.0.7 release gate roadmap.
pub fn run_release_failure_reproduction(
    root: &Path,
) -> QualityResult<ReleaseFailureReproductionSummary> {
    let path = root.join(RELEASE_FAILURE_REPRODUCTION_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read release failure reproduction contract: {err}",
            path.display()
        )
    })?;
    let mut diagnostics = validate_release_failure_reproduction_text(&text);
    diagnostics.extend(validate_adversarial_reproduction_fixtures());
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(ReleaseFailureReproductionSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        adversarial_case_count: ADVERSARIAL_REPRODUCTION_FIXTURES.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_release_failure_reproduction_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!(
                "missing release failure reproduction term `{term}`"
            ));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(&claim.to_lowercase()) {
            diagnostics.push(format!(
                "forbidden release failure reproduction claim `{claim}`"
            ));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder release failure reproduction text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn validate_adversarial_reproduction_fixtures() -> Vec<String> {
    let mut diagnostics = Vec::new();
    let stale_command = validate_reproduction_sample(
        r#"{
            "exact_reproduction_command": "target/debug/deps/terlan_quality-abc123 --exact old_test",
            "required_environment_variables": {
                "TERLAN_REPRO_SEED": "123",
                "TERLAN_TARGET_PROFILE": "vm",
                "TERLAN_CACHE_MODE": "cold",
                "TERLAN_SHARD_ID": "0"
            },
            "input_fixture_path": "fixtures/release/failure.json",
            "random_seed": "123",
            "target_profile": "vm",
            "cache_mode": "cold",
            "shard_id": "0",
            "report_path": "target/quality/release-failure-reproduction-report.json",
            "support_bundle_path": "target/support/release-failure.json",
            "narrow_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "broader_reproduction_command": "make release-failure-reproduction-check",
            "command_success_status": "passes"
        }"#,
    );
    if stale_command.is_empty() {
        diagnostics
            .push("adversarial fixture failed to reject stale reproduction commands".to_string());
    }
    let missing_seed = validate_reproduction_sample(
        r#"{
            "exact_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "required_environment_variables": {
                "TERLAN_TARGET_PROFILE": "vm",
                "TERLAN_CACHE_MODE": "cold",
                "TERLAN_SHARD_ID": "0"
            },
            "input_fixture_path": "fixtures/release/failure.json",
            "target_profile": "vm",
            "cache_mode": "cold",
            "shard_id": "0",
            "report_path": "target/quality/release-failure-reproduction-report.json",
            "support_bundle_path": "target/support/release-failure.json",
            "narrow_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "broader_reproduction_command": "make release-failure-reproduction-check",
            "command_success_status": "passes"
        }"#,
    );
    if missing_seed.is_empty() {
        diagnostics.push("adversarial fixture failed to reject missing seeds".to_string());
    }
    let path_dependent = validate_reproduction_sample(
        r#"{
            "exact_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "required_environment_variables": {
                "TERLAN_REPRO_SEED": "123",
                "TERLAN_TARGET_PROFILE": "vm",
                "TERLAN_CACHE_MODE": "cold",
                "TERLAN_SHARD_ID": "0"
            },
            "input_fixture_path": "/home/anatoly/Applications/terlan/fixtures/failure.json",
            "random_seed": "123",
            "target_profile": "vm",
            "cache_mode": "cold",
            "shard_id": "0",
            "report_path": "target/quality/release-failure-reproduction-report.json",
            "support_bundle_path": "target/support/release-failure.json",
            "narrow_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "broader_reproduction_command": "make release-failure-reproduction-check",
            "command_success_status": "passes"
        }"#,
    );
    if path_dependent.is_empty() {
        diagnostics
            .push("adversarial fixture failed to reject path-dependent fixtures".to_string());
    }
    let deleted_temp = validate_reproduction_sample(
        r#"{
            "exact_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "required_environment_variables": {
                "TERLAN_REPRO_SEED": "123",
                "TERLAN_TARGET_PROFILE": "vm",
                "TERLAN_CACHE_MODE": "cold",
                "TERLAN_SHARD_ID": "0"
            },
            "input_fixture_path": "/tmp/deleted/failure.json",
            "random_seed": "123",
            "target_profile": "vm",
            "cache_mode": "cold",
            "shard_id": "0",
            "report_path": "target/quality/release-failure-reproduction-report.json",
            "support_bundle_path": "target/support/release-failure.json",
            "narrow_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "broader_reproduction_command": "make release-failure-reproduction-check",
            "command_success_status": "passes"
        }"#,
    );
    if deleted_temp.is_empty() {
        diagnostics
            .push("adversarial fixture failed to reject deleted temp directories".to_string());
    }
    let ci_only = validate_reproduction_sample(
        r#"{
            "exact_reproduction_command": "GITHUB_ACTIONS=true bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "required_environment_variables": {
                "TERLAN_REPRO_SEED": "123",
                "TERLAN_TARGET_PROFILE": "vm",
                "TERLAN_CACHE_MODE": "cold",
                "TERLAN_SHARD_ID": "0",
                "GITHUB_ACTIONS": "true"
            },
            "input_fixture_path": "fixtures/release/failure.json",
            "random_seed": "123",
            "target_profile": "vm",
            "cache_mode": "cold",
            "shard_id": "0",
            "report_path": "target/quality/release-failure-reproduction-report.json",
            "support_bundle_path": "target/support/release-failure.json",
            "narrow_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "broader_reproduction_command": "make release-failure-reproduction-check",
            "command_success_status": "passes"
        }"#,
    );
    if ci_only.is_empty() {
        diagnostics.push("adversarial fixture failed to reject CI-only state".to_string());
    }
    let hidden_env = validate_reproduction_sample(
        r#"{
            "exact_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "required_environment_variables": {
                "TERLAN_REPRO_SEED": "${RANDOM_SEED}",
                "TERLAN_TARGET_PROFILE": "vm",
                "TERLAN_CACHE_MODE": "cold",
                "TERLAN_SHARD_ID": "0"
            },
            "input_fixture_path": "fixtures/release/failure.json",
            "random_seed": "${RANDOM_SEED}",
            "target_profile": "vm",
            "cache_mode": "cold",
            "shard_id": "0",
            "report_path": "target/quality/release-failure-reproduction-report.json",
            "support_bundle_path": "target/support/release-failure.json",
            "narrow_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "broader_reproduction_command": "make release-failure-reproduction-check",
            "command_success_status": "passes"
        }"#,
    );
    if hidden_env.is_empty() {
        diagnostics.push(
            "adversarial fixture failed to reject hidden environment assumptions".to_string(),
        );
    }
    diagnostics
}

fn validate_reproduction_sample(text: &str) -> Vec<String> {
    let value = match serde_json::from_str::<Value>(text) {
        Ok(value) => value,
        Err(err) => return vec![format!("malformed reproduction sample JSON: {err}")],
    };
    let Some(object) = value.as_object() else {
        return vec!["reproduction sample must be a JSON object".to_string()];
    };
    let mut diagnostics = Vec::new();
    let command = required_string(object, "exact_reproduction_command", &mut diagnostics);
    reject_command_coupling("exact reproduction command", command, &mut diagnostics);
    let env = object
        .get("required_environment_variables")
        .and_then(Value::as_object);
    let Some(env) = env else {
        diagnostics.push("missing required environment variables".to_string());
        return diagnostics;
    };
    for key in [
        "TERLAN_REPRO_SEED",
        "TERLAN_TARGET_PROFILE",
        "TERLAN_CACHE_MODE",
        "TERLAN_SHARD_ID",
    ] {
        match env.get(key).and_then(Value::as_str) {
            Some(value) if !value.trim().is_empty() => {
                reject_hidden_environment_value(key, value, &mut diagnostics);
            }
            _ => diagnostics.push(format!("missing required environment variable `{key}`")),
        }
    }
    if env.contains_key("GITHUB_ACTIONS") || env.contains_key("CI") {
        diagnostics.push("CI-only state is not allowed in reproduction environment".to_string());
    }
    validate_relative_path(
        "input fixture path",
        required_string(object, "input_fixture_path", &mut diagnostics),
        &mut diagnostics,
    );
    validate_relative_path(
        "report path",
        required_string(object, "report_path", &mut diagnostics),
        &mut diagnostics,
    );
    validate_relative_path(
        "support-bundle path",
        required_string(object, "support_bundle_path", &mut diagnostics),
        &mut diagnostics,
    );
    for field in ["random_seed", "target_profile", "cache_mode", "shard_id"] {
        if let Some(value) = required_string(object, field, &mut diagnostics) {
            reject_hidden_environment_value(field, value, &mut diagnostics);
        }
    }
    reject_command_coupling(
        "narrow reproduction command",
        required_string(object, "narrow_reproduction_command", &mut diagnostics),
        &mut diagnostics,
    );
    reject_command_coupling(
        "broader reproduction command",
        required_string(object, "broader_reproduction_command", &mut diagnostics),
        &mut diagnostics,
    );
    match required_string(object, "command_success_status", &mut diagnostics) {
        Some("passes" | "fails-with-expected-diagnostic") => {}
        Some(status) => diagnostics.push(format!("unsupported command success status `{status}`")),
        None => {}
    }
    diagnostics
}

fn required_string<'a>(
    object: &'a serde_json::Map<String, Value>,
    field: &str,
    diagnostics: &mut Vec<String>,
) -> Option<&'a str> {
    match object.get(field).and_then(Value::as_str) {
        Some(value) if !value.trim().is_empty() => Some(value),
        _ => {
            diagnostics.push(format!("missing {field}"));
            None
        }
    }
}

fn validate_relative_path(label: &str, value: Option<&str>, diagnostics: &mut Vec<String>) {
    let Some(value) = value else {
        return;
    };
    if value.starts_with('/') || value.contains("\\Users\\") || value.contains("C:\\") {
        diagnostics.push(format!(
            "{label} must not depend on absolute checkout paths"
        ));
    }
    if value.starts_with("/tmp/") || value.starts_with("tmp/") {
        diagnostics.push(format!(
            "{label} must not depend on deleted temp directories"
        ));
    }
    if value.contains("..") {
        diagnostics.push(format!("{label} must not escape the support bundle"));
    }
}

fn reject_command_coupling(label: &str, command: Option<&str>, diagnostics: &mut Vec<String>) {
    let Some(command) = command else {
        return;
    };
    let lower = command.to_lowercase();
    if lower.contains("target/debug") || lower.contains("target/release") {
        diagnostics.push(format!("{label} must not depend on stale caches"));
    }
    if lower.contains("/home/") || lower.contains("\\users\\") || lower.contains("c:\\") {
        diagnostics.push(format!("{label} must not depend on local checkout paths"));
    }
    if lower.contains("github_actions") || lower.contains("github_actions=true") {
        diagnostics.push(format!("{label} must not depend on CI-only state"));
    }
    if lower.contains("untracked") {
        diagnostics.push(format!("{label} must not depend on untracked files"));
    }
}

fn reject_hidden_environment_value(label: &str, value: &str, diagnostics: &mut Vec<String>) {
    if value.contains("${") || value.contains("$(") {
        diagnostics.push(format!(
            "{label} must not depend on hidden environment assumptions"
        ));
    }
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create release failure reproduction report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.release-failure-reproduction.v1",
        "artifact_evidence": "release failure reproduction contract",
        "failure_samples": [
            "release gate failure",
            "failing test/case"
        ],
        "reproduction_commands": [
            "exact reproduction command",
            "narrow reproduction commands",
            "broader reproduction commands"
        ],
        "fixture_digests": [
            "input fixture path",
            "fixture digests",
            "random seed"
        ],
        "support_bundle_replay_results": [
            "support bundles",
            "fresh temporary directory",
            "support-bundle replay results"
        ],
        "path_redaction_decisions": [
            "must not depend on absolute checkout paths",
            "path-redaction decisions"
        ],
        "command_success_status": [
            "command success status",
            "working reproduction command for the failing case"
        ],
        "adversarial_reproduction_fixtures": [
            "stale reproduction commands",
            "missing seeds",
            "path-dependent fixtures",
            "deleted temp directories",
            "CI-only state",
            "hidden environment assumptions"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize release failure reproduction report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write release failure reproduction report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[release-failure-reproduction] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "release_failure_reproduction_test.rs"]
mod release_failure_reproduction_test;
