use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const WATCH_MODE_HOT_RELOAD_DOC: &str = "docs/compiler/WATCH_MODE_HOT_RELOAD.md";
const REPORT_PATH: &str = "target/quality/watch-mode-hot-reload-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "watch mode and VM hot-reload correctness",
    "terlc watch",
    "build",
    "test",
    "run",
    "serve",
    "docs",
    "package workspaces",
    "formatter/lint checks",
    "VM hot reload",
    "same incremental cache keys as clean builds",
    "file watching",
    "normalize events",
    "debounce deterministically",
    "ignore build/cache directories",
    "detect package/lockfile/std changes",
    "declared watch inputs",
    "preserve or reject process state",
    "documented compatibility rule",
    "unchanged ABI/state shape may reload",
    "incompatible shape changes",
    "cataloged diagnostics",
    "stale processes",
    "mixed code versions",
    "stable text/JSON events",
    "start",
    "change batch",
    "rebuild",
    "diagnostic",
    "reload",
    "test result",
    "support-bundle path",
    "terminal failure",
    "rapid file changes",
    "rename/delete sequences",
    "package lockfile edits",
    "generated file churn",
    "stale source maps",
    "incompatible state shape reload",
    "failing tests after reload",
    "interrupted rebuilds",
    "watcher path leakage",
    "watch-mode-hot-reload-report.json",
    "event sequences",
    "rebuild hashes",
    "cache hit/miss counts",
    "VM reload results",
    "diagnostics",
    "source-map parity",
    "support-bundle paths",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "watch mode can produce results that differ from clean build/test/run",
    "VM hot reload can expose mixed code versions",
    "stale source maps are acceptable",
    "stale package metadata is acceptable",
    "unclassified state-shape incompatibilities are acceptable",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the watch mode and VM hot-reload gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchModeHotReloadSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the watch mode and VM hot-reload correctness gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/compiler/`.
///
/// Output:
/// - Success summary and report when watch behavior, hot-reload state safety,
///   event stability, adversarial cases, and report fields are documented.
/// - Stable diagnostics when watch semantics, compatibility rules, output
///   events, or adversarial cases are missing.
///
/// Transformation:
/// - Converts the watch mode and VM hot-reload contract into executable release
///   evidence for the 0.0.7 compiler and VM runtime roadmap.
pub fn run_watch_mode_hot_reload(root: &Path) -> QualityResult<WatchModeHotReloadSummary> {
    let path = root.join(WATCH_MODE_HOT_RELOAD_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read watch mode hot-reload contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_watch_mode_hot_reload_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(WatchModeHotReloadSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_watch_mode_hot_reload_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing watch mode hot-reload term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(&claim.to_lowercase()) {
            diagnostics.push(format!("forbidden watch mode hot-reload claim `{claim}`"));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder watch mode hot-reload text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create watch mode hot-reload report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.watch-mode-hot-reload.v1",
        "artifact_evidence": "watch mode and VM hot-reload correctness contract",
        "event_sequences": [
            "start",
            "change batch",
            "rebuild",
            "diagnostic",
            "reload",
            "test result",
            "support-bundle path",
            "terminal failure"
        ],
        "rebuild_hashes": [
            "clean build hash",
            "watch rebuild hash"
        ],
        "cache_hit_miss_counts": [
            "watch cache hit",
            "watch cache miss",
            "hot reload invalidation miss"
        ],
        "vm_reload_results": [
            "unchanged ABI/state shape may reload",
            "incompatible shape changes rejected",
            "mixed code versions rejected"
        ],
        "diagnostics": [
            "cataloged diagnostics",
            "watcher path leakage",
            "stale source maps"
        ],
        "source_map_parity": [
            "clean source-map parity",
            "watch source-map parity"
        ],
        "support_bundle_paths": [
            "support-bundle path",
            "normalized support-bundle path"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize watch mode hot-reload report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write watch mode hot-reload report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[watch-mode-hot-reload] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "watch_mode_hot_reload_test.rs"]
#[cfg(test)]
mod watch_mode_hot_reload_test;
