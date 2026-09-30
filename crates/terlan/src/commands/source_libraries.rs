//! Locates shipped library implementations for all source backends.

use std::path::{Path, PathBuf};

pub(crate) fn imported_std_source_path(module: &str, active_path: &Path) -> Option<PathBuf> {
    if !module.starts_with("std.") {
        return None;
    }
    let relative = PathBuf::from(format!("{}.terl", module.replace('.', "/")));
    let mut candidates = Vec::new();
    if let Some(root) = repository_root_from_std_path(active_path) {
        candidates.push(root.join(&relative));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(&relative));
    }
    if let Some(share_root) = crate::commands::release_layout::installed_share_root() {
        candidates.push(share_root.join(&relative));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(&relative),
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
