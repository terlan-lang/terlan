use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::artifacts::collect_syntax_asset_imports_matching;
use crate::terlan_syntax::{SyntaxImportKind, SyntaxModuleOutput};

use super::AssetFilters;

/// Copies file and CSS imports for static output.
///
/// Inputs:
/// - `module`: syntax output containing import declarations.
/// - `source_path`: source file path used to resolve relative imports.
/// - `out_dir`: static output directory.
/// - `filters`: asset include/exclude filters.
///
/// Output:
/// - Copied CSS output paths or an error message.
///
/// Transformation:
/// - Loads and validates shared syntax asset imports, filters them, copies them
///   into the output directory, and tracks copied CSS files for validation.
pub(super) fn copy_syntax_static_asset_imports(
    module: &SyntaxModuleOutput,
    source_path: &Path,
    out_dir: &Path,
    filters: &AssetFilters,
) -> Result<Vec<PathBuf>, String> {
    let mut copied_css_outputs = Vec::new();
    let imports = collect_syntax_asset_imports_matching(module, source_path, |kind, path| {
        matches!(kind, SyntaxImportKind::File | SyntaxImportKind::Css) && filters.allows(path)
    })?;

    for import in imports {
        let Some(file_name) = import.resolved_path.file_name() else {
            return Err(format!(
                "static asset import `{}` has no filename",
                import.resolved_path.display()
            ));
        };
        let target = out_dir.join(file_name);
        fs::copy(&import.resolved_path, &target).map_err(|error| {
            format!(
                "failed to copy static asset `{}` to `{}`: {error}",
                import.resolved_path.display(),
                target.display(),
            )
        })?;
        if crate::terlan_html::is_terlan_artifact_template_path(&import.resolved_path) {
            write_template_telemetry(&import.resolved_path, &import.bytes, &target)?;
        }
        if import.kind == SyntaxImportKind::Css {
            copied_css_outputs.push(target);
        }
    }

    copied_css_outputs.sort();
    Ok(copied_css_outputs)
}

fn write_template_telemetry(
    source_path: &Path,
    source_bytes: &[u8],
    target: &Path,
) -> Result<(), String> {
    let source = std::str::from_utf8(source_bytes).map_err(|error| {
        format!(
            "error[template_backend_encoding]: {}: {error}",
            source_path.display()
        )
    })?;
    let telemetry = crate::terlan_html::structured_template_telemetry(source, source_path)?;
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "error[template_backend_telemetry_path]: {} has no UTF-8 filename",
                target.display()
            )
        })?;
    let telemetry_path = target.with_file_name(format!("{file_name}.telemetry.json"));
    let telemetry_json = serde_json::to_vec_pretty(&telemetry).map_err(|error| {
        format!(
            "error[template_backend_telemetry_encode]: {}: {error}",
            source_path.display()
        )
    })?;
    fs::write(&telemetry_path, telemetry_json).map_err(|error| {
        format!(
            "error[template_backend_telemetry_write]: {}: {error}",
            telemetry_path.display()
        )
    })
}
