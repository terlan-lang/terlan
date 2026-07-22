use std::fs;
use std::path::{Path, PathBuf};

/// Collects lintable Terlan files in deterministic order.
pub(super) fn collect_lint_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    if root.is_file() {
        return if is_lint_source_path(root) {
            Ok(vec![root.to_path_buf()])
        } else {
            Err(format!(
                "lint input is not a Terlan source file: {}",
                root.display()
            ))
        };
    }
    if !root.is_dir() {
        return Err(format!("lint input does not exist: {}", root.display()));
    }

    let mut paths = Vec::new();
    collect_lint_paths_into(root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

/// Recursively collects lintable Terlan files.
fn collect_lint_paths_into(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read directory {}: {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| {
            format!(
                "failed to read directory entry under {}: {err}",
                dir.display()
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_lint_paths_into(&path, paths)?;
        } else if is_lint_source_path(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

/// Returns whether a path is lintable Terlan source.
fn is_lint_source_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("terl" | "terli")
    )
}
