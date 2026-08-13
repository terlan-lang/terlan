use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

use super::roadmap_gate_integrity::parse_make_list_variable_values;

const RELEASE_GATE_SHARD_RESUME_DOC: &str = "docs/release/RELEASE_GATE_SHARD_RESUME.md";
const MAKEFILE: &str = "Makefile";
const REPORT_PATH: &str = "target/quality/release-gate-shard-resume-report.json";

const RELEASE_GATE_CHAIN: &[(&str, &str)] = &[
    (
        "package-registry-publish-check",
        "package-resolver-reproducibility-check",
    ),
    (
        "package-capability-contract-check",
        "package-registry-publish-check",
    ),
    (
        "package-release-test-matrix-check",
        "package-capability-contract-check",
    ),
    (
        "package-api-compatibility-check",
        "package-release-test-matrix-check",
    ),
    (
        "package-cli-workflow-check",
        "package-api-compatibility-check",
    ),
    (
        "package-editor-integration-check",
        "package-cli-workflow-check",
    ),
    (
        "package-cache-integrity-check",
        "package-editor-integration-check",
    ),
    (
        "package-workspace-graph-check",
        "package-cache-integrity-check",
    ),
    (
        "package-build-artifact-isolation-check",
        "package-workspace-graph-check",
    ),
    (
        "source-map-debug-info-check",
        "package-build-artifact-isolation-check",
    ),
    (
        "compiler-incremental-cache-check",
        "source-map-debug-info-check",
    ),
    (
        "watch-mode-hot-reload-check",
        "compiler-incremental-cache-check",
    ),
    (
        "release-flake-detection-check",
        "watch-mode-hot-reload-check",
    ),
    (
        "release-gate-shard-resume-check",
        "release-flake-detection-check",
    ),
    (
        "release-gate-duration-budget-check",
        "release-gate-shard-resume-check",
    ),
    (
        "release-gate-report-schema-check",
        "release-gate-duration-budget-check",
    ),
    (
        "release-failure-reproduction-check",
        "release-gate-report-schema-check",
    ),
];

const REQUIRED_TERMS: &[&str] = &[
    "release gates shardable, resumable, and non-redundant",
    "release gate manifest",
    "every check",
    "inputs",
    "output artifacts",
    "dependency gates",
    "expected reports",
    "estimated cost",
    "shard assignment",
    "valid cache",
    "release runs",
    "stop at first failure",
    "collect-all mode",
    "exact resume command",
    "next unchecked gate",
    "without re-running completed gates",
    "candidate-bound composition",
    "evidence refresh and preflight are separate commands",
    "preflight never executes completed gates",
    "gate caching",
    "content-addressed",
    "source files",
    "lock files",
    "generated artifacts",
    "tool versions",
    "environment contracts",
    "declared external dependencies",
    "cache hits",
    "invalidated",
    "shard execution",
    "deterministic output ordering",
    "stable JSON summaries",
    "stable support-bundle layout",
    "identical final release decisions",
    "single-process serial run",
    "interrupted release runs",
    "stale cached reports",
    "reordered shards",
    "missing gate artifacts",
    "changed toolchain versions",
    "partial support bundles",
    "resume commands after failure",
    "release-gate-shard-resume-report.json",
    "gate DAG",
    "cache keys",
    "skipped gates",
    "executed gates",
    "shard timings",
    "resume command",
    "first-failure decision",
    "collect-all decision",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "repeated release invocations re-run completed gates without an input change",
    "skip gates without a valid cache proof",
    "sharded execution can change diagnostics",
    "sharded execution can change report contents",
    "sharded execution can change benchmark inclusion",
    "sharded execution can change support-bundle paths",
    "sharded execution can change final release pass/fail status",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the release gate shard/resume gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseGateShardResumeSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the release gate shard/resume correctness gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/release/`.
///
/// Output:
/// - Success summary and report when the gate manifest, resume semantics,
///   cache semantics, shard semantics, adversarial cases, and report fields are
///   documented.
/// - Stable diagnostics when sharding, resume, caching, or serial parity
///   evidence is missing.
///
/// Transformation:
/// - Converts the shard/resume release contract into executable release
///   evidence for the 0.0.7 release gate roadmap.
pub fn run_release_gate_shard_resume(root: &Path) -> QualityResult<ReleaseGateShardResumeSummary> {
    let path = root.join(RELEASE_GATE_SHARD_RESUME_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read release gate shard/resume contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_release_gate_shard_resume_text(&text);
    let makefile_path = root.join(MAKEFILE);
    let makefile = fs::read_to_string(&makefile_path)
        .map_err(|err| format!("{}: failed to read: {err}", makefile_path.display()))?;
    let mut diagnostics = diagnostics;
    diagnostics.extend(validate_release_makefile(&makefile));
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(ReleaseGateShardResumeSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

#[derive(Debug)]
struct MakeTarget<'a> {
    prerequisites: Vec<&'a str>,
    recipe: Vec<&'a str>,
}

fn make_target<'a>(makefile: &'a str, name: &str) -> Option<MakeTarget<'a>> {
    let mut lines = makefile.lines();
    while let Some(line) = lines.next() {
        let Some((targets, prerequisites)) = line.split_once(':') else {
            continue;
        };
        if !targets.split_whitespace().any(|target| target == name) {
            continue;
        }
        let mut recipe = Vec::new();
        for recipe_line in lines.by_ref() {
            if let Some(command) = recipe_line.strip_prefix('\t') {
                recipe.push(command.trim());
            } else if !recipe_line.trim().is_empty() {
                break;
            }
        }
        return Some(MakeTarget {
            prerequisites: prerequisites.split_whitespace().collect(),
            recipe,
        });
    }
    None
}

fn validate_release_makefile(makefile: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for &(target_name, prerequisite) in RELEASE_GATE_CHAIN {
        let Some(target) = make_target(makefile, target_name) else {
            diagnostics.push(format!("{MAKEFILE}: missing target `{target_name}`"));
            continue;
        };
        if !target.prerequisites.contains(&prerequisite) {
            diagnostics.push(format!(
                "{MAKEFILE}: `{target_name}` must declare `{prerequisite}` as a prerequisite"
            ));
        }
        if target.recipe.iter().any(|line| line.contains("$(MAKE)")) {
            diagnostics.push(format!(
                "{MAKEFILE}: `{target_name}` must not recursively invoke completed release gates"
            ));
        }
    }

    let Some(check) = make_target(makefile, "check") else {
        diagnostics.push(format!("{MAKEFILE}: missing target `check`"));
        return diagnostics;
    };
    for prerequisite in ["rust-test-suite", "terlan-self-validation-bootstrap"] {
        if !check.prerequisites.contains(&prerequisite) {
            diagnostics.push(format!(
                "{MAKEFILE}: `check` must build `{prerequisite}` exactly once before its validation cycle"
            ));
        }
    }
    for required_fragment in [
        "TERLAN_RUST_SUITE_ALREADY_RUN=1",
        "TERLAN_VALIDATION_BOOTSTRAPPED=1",
        "$(MAKE) --no-print-directory check-gates",
    ] {
        if !check
            .recipe
            .iter()
            .any(|line| line.contains(required_fragment))
        {
            diagnostics.push(format!(
                "{MAKEFILE}: `check` must propagate `{required_fragment}` to the owned validation cycle"
            ));
        }
    }
    let final_gate = "release-failure-reproduction-check";
    let refresh_name = "release-0-0-7-evidence-refresh";
    match make_target(makefile, refresh_name) {
        None => diagnostics.push(format!("{MAKEFILE}: missing target `{refresh_name}`")),
        Some(refresh) if !refresh.prerequisites.contains(&final_gate) => {
            diagnostics.push(format!(
                "{MAKEFILE}: `{refresh_name}` must own the release-only `{final_gate}` chain"
            ));
        }
        Some(_) => {}
    }
    let check_gates = parse_make_list_variable_values(makefile, "CHECK_GATES");
    if check_gates.iter().any(|gate| gate == final_gate) {
        diagnostics.push(format!(
            "{MAKEFILE}: ordinary `CHECK_GATES` must not own release-only `{final_gate}`"
        ));
    }
    for &(target_name, _) in RELEASE_GATE_CHAIN {
        if target_name != "release-failure-reproduction-check"
            && check
                .recipe
                .iter()
                .any(|line| *line == format!("$(MAKE) {target_name}"))
        {
            diagnostics.push(format!(
                "{MAKEFILE}: `check` redundantly invokes prerequisite `{target_name}`"
            ));
        }
    }

    let closeout_name = "lean-proof-track-release-closeout-check";
    match make_target(makefile, closeout_name) {
        None => diagnostics.push(format!("{MAKEFILE}: missing target `{closeout_name}`")),
        Some(closeout) => {
            if !closeout.prerequisites.contains(&"rust-test-suite") {
                diagnostics.push(format!(
                    "{MAKEFILE}: `{closeout_name}` must consume the canonical `rust-test-suite` owner"
                ));
            }
            if closeout.recipe.iter().any(|line| {
                line.contains("$(RUST_TEST) --locked -p terlan --lib --features quality-tools")
            }) {
                diagnostics.push(format!(
                    "{MAKEFILE}: `{closeout_name}` must not rerun the complete Rust library suite"
                ));
            }
        }
    }

    let preflight_name = "release-0-0-7-preflight";
    match make_target(makefile, preflight_name) {
        None => diagnostics.push(format!("{MAKEFILE}: missing target `{preflight_name}`")),
        Some(preflight) => {
            if !preflight.prerequisites.is_empty() {
                diagnostics.push(format!(
                    "{MAKEFILE}: `{preflight_name}` must compose existing evidence without prerequisite gates"
                ));
            }
            for forbidden in ["$(MAKE)", "$(CARGO)", "$(RUST_TEST)", "terlc build"] {
                if preflight.recipe.iter().any(|line| line.contains(forbidden)) {
                    diagnostics.push(format!(
                        "{MAKEFILE}: `{preflight_name}` must not execute `{forbidden}`"
                    ));
                }
            }
        }
    }

    diagnostics
}

fn validate_release_gate_shard_resume_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing release gate shard/resume term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(&claim.to_lowercase()) {
            diagnostics.push(format!(
                "forbidden release gate shard/resume claim `{claim}`"
            ));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder release gate shard/resume text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create release gate shard/resume report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.release-gate-shard-resume.v1",
        "artifact_evidence": "release gate shard/resume contract",
        "gate_dag": [
            "dependency gates",
            "expected reports"
        ],
        "cache_keys": [
            "source files",
            "lock files",
            "generated artifacts",
            "tool versions",
            "environment contracts",
            "declared external dependencies"
        ],
        "skipped_gates": [
            "valid cache"
        ],
        "executed_gates": [
            "every check"
        ],
        "shard_timings": [
            "estimated cost",
            "shard assignment"
        ],
        "resume_command": [
            "exact resume command",
            "next unchecked gate"
        ],
        "first_failure_decision": [
            "stop at first failure"
        ],
        "collect_all_decision": [
            "collect-all mode"
        ],
        "evidence_composition": "candidate-bound composition",
        "evidence_refresh_command": "make release-0-0-7-evidence-refresh",
        "preflight_command": "make release-0-0-7-preflight",
        "preflight_replays_completed_gates": false
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize release gate shard/resume report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write release gate shard/resume report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[release-gate-shard-resume] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "release_gate_shard_resume_test.rs"]
#[cfg(test)]
mod release_gate_shard_resume_test;
