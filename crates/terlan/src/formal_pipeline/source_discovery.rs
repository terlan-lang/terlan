//! Deterministic source discovery with project boundaries and typed I/O errors.

use std::fs;
use std::path::{Path, PathBuf};

/// Lists source declarations and clients for interface-first project checking.
pub(crate) fn terlan_check_sources_in_dir(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_terlan_sources_recursive(dir, &mut files, true)?;
    files.sort();
    Ok(files)
}

/// Recursively collects Terlan implementation sources.
///
/// Inputs:
/// - `dir`: directory currently being scanned.
/// - `files`: mutable collection of discovered `.terl` source paths.
///
/// Output:
/// - `Ok(())` when the directory and all nested directories are scanned.
/// - `Err(message)` when a directory entry or metadata read fails.
///
/// Transformation:
/// - Reads one directory level, sorts child paths for stable traversal, appends
///   `.terl` files, and recurses into child directories that are not nested
///   project roots, without following symlinked directories.
pub(super) fn collect_terlan_sources_recursive(
    dir: &Path,
    files: &mut Vec<PathBuf>,
    include_interfaces: bool,
) -> std::io::Result<()> {
    let entries = fs::read_dir(dir).map_err(|err| {
        std::io::Error::new(
            err.kind(),
            format!("failed to read dir {}: {}", dir.display(), err),
        )
    })?;
    let mut children = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| {
            std::io::Error::new(err.kind(), format!("failed to read dir entry: {err}"))
        })?;
        let file_type = entry.file_type().map_err(|err| {
            std::io::Error::new(
                err.kind(),
                format!(
                    "failed to read file type for {}: {err}",
                    entry.path().display()
                ),
            )
        })?;
        children.push((entry.path(), file_type));
    }
    children.sort_by(|left, right| left.0.cmp(&right.0));

    for (path, file_type) in children {
        if file_type.is_dir() {
            if is_nested_terlan_project_root(&path) {
                continue;
            }
            if include_interfaces
                && path.file_name().is_some_and(|name| {
                    matches!(
                        name.to_str(),
                        Some(".git" | ".terlan" | "_build" | "target")
                    )
                })
            {
                continue;
            }
            collect_terlan_sources_recursive(&path, files, include_interfaces)?;
        } else if file_type.is_file()
            && (path.extension().and_then(|ext| ext.to_str()) == Some("terl")
                || (include_interfaces
                    && path.extension().and_then(|ext| ext.to_str()) == Some("terli")))
        {
            files.push(path);
        }
    }
    Ok(())
}

/// Returns whether a directory is a nested Terlan project boundary.
///
/// Inputs:
/// - `dir`: directory discovered while recursively scanning a source root.
///
/// Output:
/// - `true` when the directory owns a `terlan.toml` manifest.
///
/// Transformation:
/// - Checks for the canonical project manifest filename without reading or
///   parsing it, allowing source discovery to avoid crossing project roots.
fn is_nested_terlan_project_root(dir: &Path) -> bool {
    dir.join("terlan.toml").is_file()
}
