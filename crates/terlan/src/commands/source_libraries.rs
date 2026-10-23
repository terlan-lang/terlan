//! Locates source implementations and standard-library summaries.

pub(crate) use crate::support::library_paths::imported_std_summary_path;
use std::path::{Path, PathBuf};

pub(crate) fn imported_std_source_path(module: &str, active_path: &Path) -> Option<PathBuf> {
    if !module.starts_with("std.") {
        return None;
    }
    let literal = PathBuf::from(format!("{}.terl", module.replace('.', "/")));
    if let Some(path) = crate::support::library_paths::shipped_std_path(&literal, active_path) {
        return Some(path);
    }
    let mut segments = module.split('.').collect::<Vec<_>>();
    let name = segments.pop()?;
    let directory = segments
        .into_iter()
        .map(crate::terlan_hir::source_name_to_terlan_identifier)
        .collect::<Vec<_>>()
        .join("/");
    let relative = PathBuf::from(format!("{directory}/{name}.terl"));
    crate::support::library_paths::shipped_std_path(&relative, active_path)
}

#[cfg(test)]
#[path = "source_libraries_test.rs"]
mod tests;
