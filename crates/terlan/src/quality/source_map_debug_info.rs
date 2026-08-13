use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const SOURCE_MAP_DEBUG_INFO_DOC: &str = "docs/compiler/SOURCE_MAP_DEBUG_INFO.md";
const REPORT_PATH: &str = "target/quality/source-map-debug-info-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "source-map/debug-info contract",
    "Terlan source spans",
    "parser",
    "typechecker",
    "CoreIR",
    "VM artifacts",
    "generated docs",
    "package artifacts",
    "diagnostics",
    "support bundles",
    "debugger commands",
    "editor/LSP output",
    "VM runtime errors",
    "test failures",
    "panic-like internal failures",
    "package resolution failures",
    "template failures",
    "HTTP handler failures",
    "NativeBoundary failures",
    "Terlan module/function/source spans",
    "package builds",
    "workspace builds",
    "incremental rebuilds",
    "installed release artifacts",
    "generated bindings",
    "support-bundle redaction",
    "host-local absolute paths",
    "editor navigation",
    "hover diagnostics",
    "debugger breakpoints",
    "stack traces",
    "file path normalization",
    "module identity",
    "function identity",
    "line/column offsets",
    "stale source maps",
    "generated file span drift",
    "package artifact relocation",
    "redacted support bundles",
    "missing package sources",
    "invalid UTF-8/source offsets",
    "line-ending differences",
    "runtime errors without source-linked diagnostics",
    "source-map-debug-info-report.json",
    "fixture artifacts",
    "span roundtrips",
    "stack trace mappings",
    "package relocation cases",
    "editor/LSP parity snapshots",
    "support-bundle redaction checks",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "source maps may depend on source checkout paths",
    "failures may collapse into generated/internal file locations",
    "target-specific offset drift is acceptable",
    "runtime errors can omit source-linked diagnostics",
    "support bundles may leak host-local absolute paths",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the source-map/debug-info gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMapDebugInfoSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the source-map/debug-info gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/compiler/`.
///
/// Output:
/// - Success summary and report when source identity requirements are
///   documented.
/// - Stable diagnostics when source spans, artifact boundaries, redaction, or
///   adversarial cases are missing.
///
/// Transformation:
/// - Converts the source-map/debug-info contract into executable release
///   evidence for the 0.0.7 compiler, VM, package, debugger, and editor
///   roadmap.
pub fn run_source_map_debug_info(root: &Path) -> QualityResult<SourceMapDebugInfoSummary> {
    let path = root.join(SOURCE_MAP_DEBUG_INFO_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read source-map/debug-info contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_source_map_debug_info_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(SourceMapDebugInfoSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_source_map_debug_info_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing source-map/debug-info term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!("forbidden source-map/debug-info claim `{claim}`"));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder source-map/debug-info text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create source-map/debug-info report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.source-map-debug-info.v1",
        "artifact_evidence": "source-map/debug-info contract",
        "fixture_artifacts": [
            "compiler diagnostic fixture",
            "VM runtime error fixture",
            "package relocation fixture",
            "editor navigation fixture",
            "support bundle fixture"
        ],
        "span_roundtrips": [
            "parser source span",
            "typechecker diagnostic span",
            "CoreIR source span",
            "VM artifact source span"
        ],
        "stack_trace_mappings": [
            "Terlan module/function/source spans",
            "normalized source path",
            "line/column offsets"
        ],
        "package_relocation_cases": [
            "package builds",
            "workspace builds",
            "incremental rebuilds",
            "installed release artifacts"
        ],
        "editor_lsp_parity_snapshots": [
            "editor navigation",
            "hover diagnostics",
            "debugger breakpoints"
        ],
        "support_bundle_redaction_checks": [
            "support-bundle redaction",
            "host-local absolute paths",
            "redacted support bundles"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize source-map/debug-info report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write source-map/debug-info report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[source-map-debug-info] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "source_map_debug_info_test.rs"]
#[cfg(test)]
mod source_map_debug_info_test;
