//! Source interfaces for the selected package's declared dependency graph.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::commands::build::{project_check_source_roots, project_manifest};
use crate::commands::source_layout::validate_module_layout;
use crate::formal_pipeline::{parse_source_as_syntax_output, terlan_check_sources_in_dir};
use crate::terlan_syntax::SyntaxModuleOutput;

/// Reads dependency APIs using the same package resolution as build and test.
/// Handwritten interfaces take precedence over their implementations.
pub(super) fn source_interfaces(
    checked_root: &Path,
) -> Result<Vec<(PathBuf, SyntaxModuleOutput)>, String> {
    let checked_root = checked_root.canonicalize().map_err(|error| {
        format!(
            "cannot resolve check source root {}: {error}",
            checked_root.display()
        )
    })?;
    let Some(project_dir) = checked_root
        .ancestors()
        .find(|parent| parent.join("terlan.toml").is_file())
    else {
        return Ok(Vec::new());
    };
    let manifest = project_manifest::read_project_manifest(&project_dir.join("terlan.toml"))?;
    if manifest.dependencies.is_empty() {
        return Ok(Vec::new());
    }
    let own_roots = manifest
        .source_roots
        .iter()
        .map(|root| project_dir.join(root).canonicalize())
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|error| format!("cannot resolve package source roots: {error}"))?;
    let mut sources = Vec::new();
    for root in project_check_source_roots(project_dir, &manifest)? {
        if own_roots.contains(&root) {
            continue;
        }
        for path in terlan_check_sources_in_dir(&root).map_err(|error| error.to_string())? {
            if path
                .extension()
                .is_some_and(|extension| extension == "terl")
                && path.with_extension("terli").is_file()
            {
                continue;
            }
            let text = crate::support::read_file(&path.to_string_lossy())?;
            let syntax =
                parse_source_as_syntax_output(&path.to_string_lossy(), &text).map_err(|error| {
                    format!("cannot parse dependency {}: {error:?}", path.display())
                })?;
            validate_module_layout(&root, &path, &syntax.module_name)?;
            sources.push((path, syntax));
        }
    }
    Ok(sources)
}
