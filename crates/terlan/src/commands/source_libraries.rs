//! Locates source implementations and standard-library summaries.

pub(crate) use crate::support::library_paths::imported_std_summary_path;
use std::path::{Path, PathBuf};

pub(crate) fn imported_std_source_path(module: &str, active_path: &Path) -> Option<PathBuf> {
    let mut segments = module.split('.').collect::<Vec<_>>();
    let name = segments.pop()?;
    let directory = segments
        .into_iter()
        .map(crate::terlan_hir::source_name_to_terlan_identifier)
        .collect::<Vec<_>>()
        .join("/");
    let relative = PathBuf::from(format!("{directory}/{name}.terl"));
    if !module.starts_with("std.") {
        for root in active_path.ancestors().skip(1) {
            let adjacent = root.join(&relative);
            if adjacent.is_file() {
                return Some(adjacent);
            }
            let manifest_path = root.join("terlan.toml");
            if manifest_path.is_file() {
                if let Ok(manifest) =
                    crate::commands::build::project_manifest::read_project_manifest(&manifest_path)
                {
                    for source in manifest.source_roots {
                        let candidate = root.join(source).join(&relative);
                        if candidate.is_file() {
                            return Some(candidate);
                        }
                    }
                }
            }
        }
        return None;
    }
    // Canonical std directories may use contextual keywords such as `css`.
    // External-name normalization appends an underscore to those names, while
    // the checked-in library keeps the canonical module spelling on disk.
    let canonical = PathBuf::from(module.replace('.', "/")).with_extension("terl");
    crate::support::library_paths::shipped_std_path(&canonical, active_path)
        .or_else(|| crate::support::library_paths::shipped_std_path(&relative, active_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_sources_preserve_contextual_keywords_and_generated_directory_names() {
        let active = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
        for (module, expected) in [
            ("std.css.Values", "std/css/Values.terl"),
            ("std.css.properties.Width", "std/css/properties/Width.terl"),
            ("std.core.Int", "std/core/Int.terl"),
            ("std.js.Dom.Window", "std/js/dom/Window.terl"),
        ] {
            let path = imported_std_source_path(module, &active)
                .unwrap_or_else(|| panic!("missing {module}"));
            assert!(path.ends_with(expected), "{module}: {}", path.display());
        }
    }
}

#[cfg(test)]
#[path = "source_libraries_test.rs"]
mod lookup_regression_tests;
