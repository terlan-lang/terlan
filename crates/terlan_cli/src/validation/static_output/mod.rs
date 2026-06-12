use std::fs;
use std::path::{Path, PathBuf};

/// Validates one generated static HTML artifact.
///
/// Inputs:
/// - `html`: rendered HTML text produced by the static-site command.
/// - `target`: generated output path used only for diagnostic context.
///
/// Output:
/// - `Ok(())` when `terlan_html` accepts the generated HTML.
/// - `Err(String)` containing CLI-ready diagnostics when validation fails.
///
/// Transformation:
/// - Delegates HTML checking to `terlan_html` and converts structured
///   diagnostics into newline-separated CLI text.
pub(crate) fn validate_static_html_output(html: &str, target: &Path) -> Result<(), String> {
    terlan_html::validate_html_output(html, target).map_err(format_html_diagnostics)
}

/// Validates generated or copied CSS output files.
///
/// Inputs:
/// - `css_files`: output paths for CSS assets selected by the static command.
///
/// Output:
/// - `Ok(())` when every CSS file can be read and validated.
/// - `Err(String)` when a file cannot be read or CSS validation reports
///   diagnostics.
///
/// Transformation:
/// - Reads each CSS file from disk, delegates CSS parsing/validation to
///   `terlan_html`, and formats any structured diagnostics for CLI output.
pub(crate) fn validate_static_css_output_files(css_files: &[PathBuf]) -> Result<(), String> {
    for path in css_files {
        let source = fs::read_to_string(path).map_err(|err| {
            format!(
                "failed to read static CSS output `{}`: {}",
                path.display(),
                err
            )
        })?;
        terlan_html::validate_css(&source, path).map_err(format_html_diagnostics)?;
    }

    Ok(())
}

/// Formats HTML/CSS diagnostics for CLI display.
///
/// Inputs:
/// - `diagnostics`: structured diagnostics returned by `terlan_html`.
///
/// Output:
/// - A newline-separated message string suitable for `stderr`.
///
/// Transformation:
/// - Preserves diagnostic paths when present and falls back to the diagnostic
///   message when a path is unavailable.
fn format_html_diagnostics(diagnostics: Vec<terlan_html::HtmlDiagnostic>) -> String {
    diagnostics
        .into_iter()
        .map(|diagnostic| match diagnostic.path {
            Some(path) => format!("{}: {}", path.display(), diagnostic.message),
            None => diagnostic.message,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
