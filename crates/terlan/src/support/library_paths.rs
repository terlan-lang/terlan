//! Locates shipped library implementations for all source backends.

use std::path::{Path, PathBuf};

/// Locates an installed or source-tree standard-library summary.
pub(crate) fn imported_std_summary_path(module: &str, active_path: &Path) -> Option<PathBuf> {
    if let Some(summary) = super::js_binding_registry::summary_path(module) {
        return Some(summary);
    }
    if !module.starts_with("std.") {
        return None;
    }
    shipped_std_path(
        &PathBuf::from(format!("std/summaries/{module}.typi")),
        active_path,
    )
}

pub(crate) fn shipped_std_path(relative: &Path, active_path: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(root) = repository_root_from_std_path(active_path) {
        candidates.push(root.join(relative));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(relative));
    }
    if let Some(share_root) = installed_share_root() {
        candidates.push(share_root.join(relative));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(relative),
    );
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn repository_root_from_std_path(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent();
    while let Some(directory) = current {
        if directory.file_name().and_then(|name| name.to_str()) == Some("std") {
            return directory.parent().map(Path::to_path_buf);
        }
        current = directory.parent();
    }
    None
}

/// Finds the installed `share/terlan` directory beside a shipped executable.
pub(crate) fn installed_share_root() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|executable| share_root_for_executable(&executable))
}

/// Resolves an installed share root from one executable path.
pub(crate) fn share_root_for_executable(executable: &Path) -> Option<PathBuf> {
    let parent = executable.parent()?;
    [
        parent.join("share/terlan"),
        parent.parent()?.join("share/terlan"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_dir())
}
