use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::QualityResult;

/// Rejects reintroduced Erlang/BEAM backend source paths.
pub fn run_erlang_backend_classification(root: &Path) -> QualityResult<()> {
    let diagnostics = discovered_erlang_backend_paths(root)?
        .into_iter()
        .map(|path| format!("forbidden Erlang/BEAM backend path `{}`", path.display()))
        .collect::<Vec<_>>();
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(render_failure(&diagnostics))
    }
}

/// Discovers source paths likely owned by the Erlang/BEAM migration.
fn discovered_erlang_backend_paths(root: &Path) -> QualityResult<Vec<PathBuf>> {
    let source_root = root.join("crates/terlan/src");
    let mut paths = Vec::new();
    collect_candidate_paths(root, &source_root, &mut paths)?;
    paths.sort();
    Ok(paths)
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
    if text.contains("otp_test_pipeline_inventory") {
        return false;
    }
    text.contains("/erlang")
        || text.contains("/beam")
        || text.contains("/otp")
        || text.contains("native_boundary_runtime")
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
