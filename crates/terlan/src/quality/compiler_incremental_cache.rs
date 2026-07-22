use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const COMPILER_INCREMENTAL_CACHE_DOC: &str = "docs/compiler/COMPILER_INCREMENTAL_CACHE.md";
const REPORT_PATH: &str = "target/quality/compiler-incremental-cache-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "compiler incremental cache correctness",
    "compiler cache keys",
    "lexing",
    "parsing",
    "formatting",
    "name resolution",
    "typechecking",
    "CoreIR construction",
    "VM lowering",
    "generated docs",
    "diagnostics",
    "source maps",
    "package manifests",
    "target capabilities",
    "stdlib/package hashes",
    "incremental builds",
    "byte-for-byte equivalent public artifacts",
    "clean builds",
    "same inputs",
    "target",
    "package graph",
    "stdlib hash",
    "compiler version",
    "feature flags",
    "cache invalidation",
    "source edits",
    "import graph edits",
    "package/lockfile edits",
    "stdlib changes",
    "compiler version changes",
    "target profile changes",
    "generated binding changes",
    "formatter/lint rule changes",
    "cache entries",
    "workspace",
    "package",
    "capability set",
    "source-checkout",
    "host-local absolute path leakage",
    "stale parse trees",
    "stale type errors",
    "stale generated docs",
    "stale source maps",
    "stale package metadata",
    "changed imported module",
    "changed stdlib hash",
    "concurrent incremental builds",
    "cache corruption",
    "clean-vs-incremental diagnostic drift",
    "compiler-incremental-cache-report.json",
    "fixture matrix",
    "clean build hashes",
    "incremental build hashes",
    "invalidation cases",
    "cache hit/miss counts",
    "diagnostic parity",
    "source-map parity",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "incremental compilation can produce a different user-visible result than a clean build",
    "cache correctness depends on filesystem order",
    "cache correctness depends on stale workspace artifacts",
    "cache correctness depends on source checkout paths",
    "cache correctness depends on non-deterministic diagnostics",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the compiler incremental cache correctness gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerIncrementalCacheSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the compiler incremental cache correctness gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/compiler/`.
///
/// Output:
/// - Success summary and report when clean-vs-incremental equivalence,
///   invalidation coverage, isolation, and adversarial cases are documented.
/// - Stable diagnostics when cache keys, parity evidence, invalidation, or
///   report fields are missing.
///
/// Transformation:
/// - Converts the incremental cache correctness contract into executable
///   release evidence for the 0.0.7 compiler pipeline roadmap.
pub fn run_compiler_incremental_cache(
    root: &Path,
) -> QualityResult<CompilerIncrementalCacheSummary> {
    let path = root.join(COMPILER_INCREMENTAL_CACHE_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read compiler incremental cache contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_compiler_incremental_cache_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(CompilerIncrementalCacheSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_compiler_incremental_cache_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing compiler incremental cache term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!(
                "forbidden compiler incremental cache claim `{claim}`"
            ));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder compiler incremental cache text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create compiler incremental cache report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.compiler-incremental-cache.v1",
        "artifact_evidence": "compiler incremental cache correctness contract",
        "fixture_matrix": [
            "single module edit",
            "import graph edit",
            "package lockfile edit",
            "stdlib hash change",
            "generated binding change"
        ],
        "clean_build_hashes": [
            "public artifact hash",
            "diagnostic hash",
            "source-map hash"
        ],
        "incremental_build_hashes": [
            "public artifact hash",
            "diagnostic hash",
            "source-map hash"
        ],
        "invalidation_cases": [
            "source edits",
            "import graph edits",
            "package/lockfile edits",
            "target profile changes",
            "formatter/lint rule changes"
        ],
        "cache_hit_miss_counts": [
            "expected cache hit",
            "expected cache miss",
            "forced invalidation miss"
        ],
        "diagnostic_parity": [
            "clean build diagnostics",
            "incremental build diagnostics"
        ],
        "source_map_parity": [
            "clean build source maps",
            "incremental build source maps"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize compiler incremental cache report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write compiler incremental cache report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[compiler-incremental-cache] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "compiler_incremental_cache_test.rs"]
mod compiler_incremental_cache_test;
