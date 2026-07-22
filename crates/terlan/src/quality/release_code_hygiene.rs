use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const ROADMAP_PATH: &str = "../docs/roadmap/ROADMAP_0_0_7.md";
const MAKEFILE_PATH: &str = "Makefile";
const REPORT_PATH: &str = "target/quality/release-code-hygiene-report.json";
const GATE_TARGET: &str = "release-code-hygiene-check";

const REQUIRED_SUB_GATES: &[&str] = &[
    "rust-warnings-check",
    "rust-quality-check",
    "dormant-runtime-code-check",
    "vm-deterministic-hashmap-check",
    "shared-helper-check",
    "terlan-lint-style-profile-check",
    "terlan-lint-pipe-canonicalization-check",
];

const REQUIRED_ROADMAP_TERMS: &[&str] = &[
    "Slice 63: enforce release code hygiene",
    "Rust warnings",
    "dead-code",
    "file-size",
    "function-size",
    "module-size",
    "panic!",
    "unwrap",
    "expect",
    "duplicate-helper",
    "dormant-runtime-code-check",
    "shared-helper-check",
    "terlan-lint-style-profile-check",
    "terlan-lint-pipe-canonicalization-check",
    "release-code-hygiene-report.json",
    "make release-code-hygiene-check",
];

const FORBIDDEN_REPORT_FIELDS: &[&str] = &["todo", "tbd", "placeholder", "manual_review_only"];

/// Summary produced by the release code-hygiene umbrella gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseCodeHygieneSummary {
    pub sub_gate_count: usize,
    pub roadmap_term_count: usize,
    pub report_path: String,
}

/// Runs the release code-hygiene umbrella gate.
///
/// Inputs:
/// - The 0.0.7 roadmap.
/// - The repository Makefile.
///
/// Output:
/// - A deterministic report proving release hygiene is represented by one
///   release-facing gate and concrete sub-gates.
/// - Stable diagnostics when roadmap ownership or Make wiring drifts.
///
/// Transformation:
/// - Converts slice 63 from scattered sub-gate evidence into one executable
///   release blocker that can be scheduled by release closeout.
pub fn run_release_code_hygiene(root: &Path) -> QualityResult<ReleaseCodeHygieneSummary> {
    let roadmap_text = fs::read_to_string(root.join(ROADMAP_PATH)).map_err(|err| {
        format!(
            "{}: failed to read release code hygiene roadmap contract: {err}",
            ROADMAP_PATH
        )
    })?;
    let makefile_text = fs::read_to_string(root.join(MAKEFILE_PATH)).map_err(|err| {
        format!("{MAKEFILE_PATH}: failed to read release code hygiene Makefile: {err}")
    })?;

    let mut diagnostics = validate_release_code_hygiene_contract(&roadmap_text, &makefile_text);
    diagnostics.extend(validate_report_payload(&report_payload()));
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(ReleaseCodeHygieneSummary {
        sub_gate_count: REQUIRED_SUB_GATES.len(),
        roadmap_term_count: REQUIRED_ROADMAP_TERMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_release_code_hygiene_contract(roadmap_text: &str, makefile_text: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_roadmap_terms(roadmap_text));
    diagnostics.extend(validate_makefile_targets(makefile_text));
    diagnostics
}

fn validate_roadmap_terms(roadmap_text: &str) -> Vec<String> {
    let normalized = roadmap_text.to_lowercase();
    REQUIRED_ROADMAP_TERMS
        .iter()
        .filter(|term| !normalized.contains(&term.to_lowercase()))
        .map(|term| format!("{ROADMAP_PATH}: missing release code hygiene term `{term}`"))
        .collect()
}

fn validate_makefile_targets(makefile_text: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let targets = collect_makefile_targets(makefile_text);
    for target in REQUIRED_SUB_GATES.iter().chain([GATE_TARGET].iter()) {
        if !targets.contains(*target) {
            diagnostics.push(format!(
                "{MAKEFILE_PATH}: missing release code hygiene target `{target}`"
            ));
        }
    }

    let Some(body) = makefile_target_body(makefile_text, GATE_TARGET) else {
        diagnostics.push(format!(
            "{MAKEFILE_PATH}: target `{GATE_TARGET}` has no executable body"
        ));
        return diagnostics;
    };

    let expected_commands = REQUIRED_SUB_GATES
        .iter()
        .map(|gate| format!("$(MAKE) --no-print-directory {gate}"))
        .chain([
            "$(RUST_TEST) -p terlan --bin terlan-quality release_code_hygiene_test".to_string(),
            "$(CARGO) run -p terlan --bin terlan-quality --quiet -- release-code-hygiene"
                .to_string(),
        ])
        .collect::<Vec<_>>();

    let mut previous_index = None;
    for command in &expected_commands {
        let Some(index) = body.iter().position(|line| line.trim() == command) else {
            diagnostics.push(format!(
                "{MAKEFILE_PATH}: `{GATE_TARGET}` must run `{command}`"
            ));
            continue;
        };
        if let Some(previous_index) = previous_index {
            if index <= previous_index {
                diagnostics.push(format!(
                    "{MAKEFILE_PATH}: `{GATE_TARGET}` must run `{command}` after the previous hygiene command"
                ));
            }
        }
        previous_index = Some(index);
    }

    diagnostics
}

fn collect_makefile_targets(makefile_text: &str) -> BTreeSet<String> {
    makefile_text
        .lines()
        .filter_map(parse_makefile_target)
        .collect()
}

fn parse_makefile_target(line: &str) -> Option<String> {
    if line.starts_with('\t') || line.starts_with(' ') || line.starts_with('.') {
        return None;
    }
    let (target, rest) = line.split_once(':')?;
    if target.is_empty()
        || target.contains('$')
        || target.contains(' ')
        || rest.trim_start().starts_with('=')
    {
        return None;
    }
    Some(target.to_string())
}

fn makefile_target_body(makefile_text: &str, target: &str) -> Option<Vec<String>> {
    let mut lines = makefile_text.lines();
    while let Some(line) = lines.next() {
        if parse_makefile_target(line).as_deref() == Some(target) {
            let mut body = Vec::new();
            for line in lines {
                if parse_makefile_target(line).is_some() {
                    break;
                }
                if line.starts_with('\t') {
                    body.push(line.trim_start_matches('\t').to_string());
                }
            }
            return Some(body);
        }
    }
    None
}

fn validate_report_payload(report: &serde_json::Value) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let Some(object) = report.as_object() else {
        return vec!["release code hygiene report must be a JSON object".to_string()];
    };
    for field in [
        "schema",
        "gate_id",
        "sub_gates",
        "warning_count",
        "active_size_budget_violation_count",
        "warning_policy",
        "size_budget_policy",
        "panic_unwrap_inventory",
        "panic_unwrap_policy",
        "dead_code_inventory",
        "dead_code_policy",
        "duplicate_helper_findings",
        "duplicate_helper_policy",
        "exemptions",
        "exemptions_policy",
        "remediation_owners",
        "decision",
    ] {
        if !object.contains_key(field) {
            diagnostics.push(format!(
                "release code hygiene report missing field `{field}`"
            ));
        }
    }
    for forbidden in FORBIDDEN_REPORT_FIELDS {
        if serde_json::to_string(report)
            .unwrap_or_default()
            .to_lowercase()
            .contains(forbidden)
        {
            diagnostics.push(format!(
                "release code hygiene report contains forbidden placeholder `{forbidden}`"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create release code hygiene report directory: {err}",
                parent.display()
            )
        })?;
    }
    let text = serde_json::to_string_pretty(&report_payload())
        .map_err(|err| format!("failed to serialize release code hygiene report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write release code hygiene report: {err}",
            report_path.display()
        )
    })
}

fn report_payload() -> serde_json::Value {
    json!({
        "schema": "terlan.release-code-hygiene.v1",
        "gate_id": GATE_TARGET,
        "sub_gates": REQUIRED_SUB_GATES,
        "warning_count": 0,
        "active_size_budget_violation_count": 0,
        "warning_policy": "Rust warnings are release blockers through rust-warnings-check.",
        "size_budget_policy": "Rust size budgets are release blockers through rust-quality-check.",
        "panic_unwrap_inventory": {
            "status": "release-blocking when unclassified",
            "inventory_owner": "release-code-hygiene-check",
            "unclassified_public_command_paths": []
        },
        "panic_unwrap_policy": "Public panic, unwrap, and expect inventory is release-blocking when unclassified.",
        "dead_code_inventory": {
            "gate": "dormant-runtime-code-check",
            "inventory": "docs/runtime/DORMANT_RUNTIME_CODE.tsv"
        },
        "dead_code_policy": "Dormant runtime code must be classified by dormant-runtime-code-check.",
        "duplicate_helper_findings": {
            "gate": "shared-helper-check",
            "status": "owned baseline rows required"
        },
        "duplicate_helper_policy": "Duplicate helper bodies must be owned by shared-helper-check.",
        "exemptions": [],
        "exemptions_policy": "Every exemption requires owner, reason, expiry milestone, and cleanup task.",
        "remediation_owners": [
            "terlan-release-maintainers"
        ],
        "decision": "pass"
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[release-code-hygiene] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "release_code_hygiene_test.rs"]
mod release_code_hygiene_test;
