//! Typed stylesheet evaluation and a private Lightning CSS bundling adapter.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
#[path = "css/projection.rs"]
mod projection;
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use projection::{optional_style_fields, render_stylesheet_with_options};

use super::write_build_file;

/// Bundles manifest styles in declared order. Terlan files export stylesheet/0;
/// CSS files are imported directly. Generated files stay in the build directory.
pub(super) fn bundle_stylesheets(
    sources: &[PathBuf],
    build_root: &Path,
    incremental: bool,
) -> Result<Option<PathBuf>, String> {
    if sources.is_empty() {
        return Ok(None);
    }
    let directory = build_root.join("terlan-css");
    fs::create_dir_all(&directory).map_err(|error| format!("error[css_build]: {error}"))?;
    let directory = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let workspace = directory.join("evaluation");
    let mut imports = String::new();
    for (index, source) in sources.iter().enumerate() {
        let source = source.canonicalize().map_err(|error| {
            format!(
                "error[css_source]: cannot read {}: {error}",
                source.display()
            )
        })?;
        let path = match source.extension().and_then(|value| value.to_str()) {
            Some("css") => source,
            Some("terl") => {
                let css = evaluate_stylesheet(&source, &workspace)
                    .map_err(|error| format!("error[css_source]: {}: {error}", source.display()))?;
                let path = directory.join(format!("source-{index}.css"));
                write_build_file(&path, css.as_bytes(), incremental)?;
                path
            }
            _ => {
                return Err(format!(
                    "error[css_source]: expected .css or .terl: {}",
                    source.display()
                ))
            }
        };
        imports.push_str("@import ");
        cssparser::serialize_string(&path.to_string_lossy(), &mut imports)
            .map_err(|error| error.to_string())?;
        imports.push_str(";\n");
    }
    let entry = directory.join("entry.css");
    write_build_file(&entry, imports.as_bytes(), incremental)?;
    let output = directory.join("styles.css");
    let toolchain = super::web_toolchain::resolve_managed_web_toolchain()?;
    let result = super::web_toolchain::run_managed_bundler(
        Command::new("node")
            .arg(toolchain.root.join("css-bundle.mjs"))
            .arg(&entry)
            .arg(&output),
    )
    .map_err(|error| format!("error[css_bundle]: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "error[css_bundle]: {}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    Ok(Some(output))
}

fn evaluate_stylesheet(source: &Path, workspace: &Path) -> Result<String, String> {
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    {
        super::vm_artifact::source_exports::with_typed_source_exports(
            source,
            workspace,
            &["stylesheet"],
            |_, values, _, interfaces| {
                render_stylesheet_with_options(&values[0], &optional_style_fields(interfaces))
            },
        )
    }
    #[cfg(all(not(test), feature = "serve-runtime-bin"))]
    {
        let _ = (source, workspace);
        Err("stylesheet source compilation requires the compiler executable".into())
    }
}
