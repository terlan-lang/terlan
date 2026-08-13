use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::QualityResult;

/// One classified Erlang/BEAM path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ErlangBackendClassification {
    path: &'static str,
    reason: &'static str,
}

/// Summary produced by the Erlang/BEAM backend classification gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErlangBackendClassificationSummary {
    pub classified_count: usize,
    pub remove_count: usize,
    pub reference_only_count: usize,
    pub temporary_bridge_count: usize,
    pub historical_artifact_count: usize,
}

const CLASSIFICATIONS: &[ErlangBackendClassification] = &[];

const TEST_ONLY_REFERENCE_MODULE_MARKERS: &[(&str, &str, &str)] = &[];

/// Runs the Erlang/BEAM backend classification gate.
///
/// Inputs:
/// - `root`: repository root.
///
/// Output:
/// - Category counts when every Erlang/BEAM path is classified.
/// - Stable diagnostics when a classified path is missing or a new path is not
///   classified.
///
/// Transformation:
/// - Scans compiler source paths for Erlang/BEAM/OTP ownership markers and
///   verifies each one is covered by the migration classification table.
pub fn run_erlang_backend_classification(
    root: &Path,
) -> QualityResult<ErlangBackendClassificationSummary> {
    let mut diagnostics = Vec::new();
    for classification in CLASSIFICATIONS {
        if !root.join(classification.path).exists() {
            diagnostics.push(format!(
                "classified path `{}` is missing ({})",
                classification.path, classification.reason
            ));
        }
    }

    for path in discovered_erlang_backend_paths(root)? {
        if classification_for_path(&path).is_none() {
            diagnostics.push(format!(
                "unclassified Erlang/BEAM path `{}`",
                path.display()
            ));
        }
    }
    diagnostics.extend(test_only_reference_module_diagnostics(root)?);

    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    Ok(summary())
}

/// Discovers source paths likely owned by the Erlang/BEAM migration.
fn discovered_erlang_backend_paths(root: &Path) -> QualityResult<Vec<PathBuf>> {
    let source_root = root.join("crates/terlan/src");
    let mut paths = Vec::new();
    collect_candidate_paths(root, &source_root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

/// Returns diagnostics for legacy reference modules that must stay test-only.
fn test_only_reference_module_diagnostics(root: &Path) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for (path, marker, reason) in TEST_ONLY_REFERENCE_MODULE_MARKERS {
        let full_path = root.join(path);
        let text = fs::read_to_string(&full_path).map_err(|err| {
            format!(
                "cannot read test-only reference path {}: {err}",
                full_path.display()
            )
        })?;
        if !text.contains(marker) {
            diagnostics.push(format!(
                "`{path}` is missing test-only reference marker `{marker}` ({reason})"
            ));
        }
    }
    Ok(diagnostics)
}

/// Recursively collects candidate paths.
fn collect_candidate_paths(root: &Path, dir: &Path, paths: &mut Vec<PathBuf>) -> QualityResult<()> {
    for entry in fs::read_dir(dir)
        .map_err(|err| format!("cannot read source directory {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("cannot read source entry: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_candidate_paths(root, &path, paths)?;
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        if is_erlang_backend_candidate(&relative) {
            paths.push(relative);
        }
    }
    Ok(())
}

/// Returns whether a path should be covered by the classification inventory.
fn is_erlang_backend_candidate(path: &Path) -> bool {
    let text = path.to_string_lossy();
    if text.contains("erlang_backend_classification") {
        return false;
    }
    if text.contains("otp_reference_inventory") {
        return false;
    }
    if text.contains("otp_runtime_exit") {
        return false;
    }
    if text.contains("otp_test_pipeline_inventory") {
        return false;
    }
    text.contains("/erlang")
        || text.contains("/beam")
        || text.contains("/otp")
        || text.contains("native_boundary_runtime")
}

/// Returns the classification covering one path.
fn classification_for_path(path: &Path) -> Option<&'static ErlangBackendClassification> {
    let text = path.to_string_lossy();
    CLASSIFICATIONS
        .iter()
        .find(|classification| text == classification.path || text.starts_with(classification.path))
}

/// Builds the success summary.
fn summary() -> ErlangBackendClassificationSummary {
    ErlangBackendClassificationSummary {
        classified_count: CLASSIFICATIONS.len(),
        remove_count: 0,
        reference_only_count: 0,
        temporary_bridge_count: 0,
        historical_artifact_count: 0,
    }
}

/// Renders classification diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[erlang-backend-classification] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "erlang_backend_classification_test.rs"]
#[cfg(test)]
mod erlang_backend_classification_test;
