use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::roadmap_gate_integrity::parse_make_list_variable_values;
use super::support::{make_target_body, write_json_report};
use crate::terlan_quality::{render_failure, QualityResult};

const REPORT_PATH: &str = "target/quality/dev-fast-feedback-profile-report.json";

const PROFILE_SPECS: &[ProfileSpec] = &[
    ProfileSpec {
        name: "dev-check",
        gates: &[
            "rust-warnings-check",
            "std-test-honesty-check",
            "terlan-lint-style-profile-check",
            "cli-exact-selector-check",
        ],
        samples: &[
            "Rust warnings as errors",
            "stdlib test honesty",
            "lint style profile",
            "CLI exact selector inventory",
        ],
        omitted: &[
            "release packaging",
            "full stdlib release tests",
            "benchmarks",
            "networked integration checks",
        ],
    },
    ProfileSpec {
        name: "dev-vm-check",
        gates: &[
            "terlan-vm-run-command-check",
            "vm-diagnostics-quality-check",
            "vm-runtime-concept-inventory-check",
        ],
        samples: &[
            "VM run command",
            "VM diagnostic quality",
            "VM runtime concept inventory",
        ],
        omitted: &[
            "full VM coverage sweep",
            "distributed Docker scenarios",
            "release artifact validation",
        ],
    },
    ProfileSpec {
        name: "dev-web-check",
        gates: &[
            "tree-sitter-cli-check",
            "editor-debugger-surface-check",
            "angular-ts-namespace-generation-check",
        ],
        samples: &[
            "editor grammar",
            "debugger editor surface",
            "Angular.ts namespace generation",
        ],
        omitted: &[
            "browser package release preflight",
            "HTTP benchmark lanes",
            "external package publish checks",
        ],
    },
];

const FORBIDDEN_BODY_MARKERS: &[&str] = &[
    "$(MAKE) check",
    "$(MAKE) test-release",
    "$(MAKE) publish",
    "$(MAKE) publish-preflight",
    "cargo build --release",
    "benchmark",
    "vm-http-vs-axum-check",
    "docker",
    "release-artifact",
    "stdlib-release",
];

/// Summary produced by the dev fast-feedback profile gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevFastFeedbackProfileSummary {
    pub profile_count: usize,
    pub mapping_count: usize,
    pub report_path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct ProfileSpec {
    name: &'static str,
    gates: &'static [&'static str],
    samples: &'static [&'static str],
    omitted: &'static [&'static str],
}

#[derive(Debug, Serialize)]
struct DevFastFeedbackReport {
    profiles: Vec<ProfileReport>,
}

#[derive(Debug, Serialize)]
struct ProfileReport {
    name: &'static str,
    command: String,
    release_gate_mappings: &'static [&'static str],
    sampled_behavior: &'static [&'static str],
    omitted_coverage: &'static [&'static str],
    runtime_budget: &'static str,
    cache_mode: &'static str,
    escalation_command: &'static str,
}

/// Runs the dev fast-feedback profile gate.
///
/// Inputs:
/// - `root`: golden repository root containing `Makefile`.
///
/// Output:
/// - Success when the fast-feedback Make profiles exist, map to release gates,
///   avoid release-scale work, and emit a machine-readable report.
/// - Stable diagnostics when a profile is missing, unmapped, or misleading.
///
/// Transformation:
/// - Treats local development profiles as sampled release coverage rather than
///   release readiness, preserving a machine-readable escalation path.
pub fn run_dev_fast_feedback_profile(root: &Path) -> QualityResult<DevFastFeedbackProfileSummary> {
    let makefile = read_text(root, "Makefile")?;
    let make_graph = read_make_graph(root, &makefile)?;
    let mut diagnostics = Vec::new();

    for spec in PROFILE_SPECS {
        diagnostics.extend(validate_profile(&make_graph, spec));
    }
    diagnostics.extend(validate_gate_target(&makefile));

    if !diagnostics.is_empty() {
        return Err(render_failure("dev-fast-feedback-profile", &diagnostics));
    }

    let report = DevFastFeedbackReport {
        profiles: PROFILE_SPECS
            .iter()
            .map(|spec| ProfileReport {
                name: spec.name,
                command: format!("make {}", spec.name),
                release_gate_mappings: spec.gates,
                sampled_behavior: spec.samples,
                omitted_coverage: spec.omitted,
                runtime_budget: "fast local feedback; not a release-readiness budget",
                cache_mode: "uses the developer workspace cache only",
                escalation_command: "make check",
            })
            .collect(),
    };
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path, &report)?;

    Ok(DevFastFeedbackProfileSummary {
        profile_count: PROFILE_SPECS.len(),
        mapping_count: PROFILE_SPECS.iter().map(|spec| spec.gates.len()).sum(),
        report_path,
    })
}

fn read_text(root: &Path, relative: &str) -> QualityResult<String> {
    let path = root.join(relative);
    fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read file: {err}", path.display()))
}

fn read_make_graph(root: &Path, makefile: &str) -> QualityResult<String> {
    let mut graph = makefile.to_string();
    for include in makefile.lines().filter_map(include_path) {
        graph.push('\n');
        graph.push_str(&read_text(root, include)?);
    }
    Ok(graph)
}

fn include_path(line: &str) -> Option<&str> {
    line.trim_start().strip_prefix("include ")
}

fn validate_profile(makefile: &str, spec: &ProfileSpec) -> Vec<String> {
    let Some(body) = make_target_body(makefile, spec.name) else {
        return vec![format!("Makefile: missing target `{}`", spec.name)];
    };

    let mut diagnostics = Vec::new();
    for gate in spec.gates {
        let invocation = format!("$(MAKE) {gate}");
        if !body.contains(&invocation) {
            diagnostics.push(format!(
                "Makefile: `{}` must map to release gate `{gate}`",
                spec.name
            ));
        }
        if make_target_body(makefile, gate).is_none() {
            diagnostics.push(format!(
                "Makefile: `{}` maps to unknown release gate `{gate}`",
                spec.name
            ));
        }
    }

    for marker in FORBIDDEN_BODY_MARKERS {
        if body.contains(marker) {
            diagnostics.push(format!(
                "Makefile: `{}` must not run release-scale or benchmark marker `{marker}`",
                spec.name
            ));
        }
    }

    for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if !line.starts_with("$(MAKE) ") {
            diagnostics.push(format!(
                "Makefile: `{}` must use explicit `$(MAKE)` gate invocations; found `{line}`",
                spec.name
            ));
        }
    }

    diagnostics
}

fn validate_gate_target(makefile: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if make_target_body(makefile, "dev-fast-feedback-profile-check").is_none() {
        diagnostics.push("Makefile: missing target `dev-fast-feedback-profile-check`".to_string());
    }
    let check_gates = parse_make_list_variable_values(makefile, "CHECK_GATES");
    if !check_gates
        .iter()
        .any(|gate| gate == "dev-fast-feedback-profile-check")
    {
        diagnostics.push(
            "Makefile: `CHECK_GATES` must include `dev-fast-feedback-profile-check`".to_string(),
        );
    }
    diagnostics
}

fn write_report(path: &Path, report: &DevFastFeedbackReport) -> QualityResult<()> {
    write_json_report(path, report)
}

#[cfg(test)]
#[path = "dev_fast_feedback_profile_test.rs"]
#[cfg(test)]
mod dev_fast_feedback_profile_test;
