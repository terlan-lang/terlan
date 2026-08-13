use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::{render_failure, QualityResult};

const REQUIRED_GENERATED_METADATA: &[&str] = &[
    "@generated true",
    "@do-not-edit true",
    "@generator ",
    "@generator-version",
    "@generator-profile",
    "@artifact-kind",
    "@input-manifest",
    "@source-package",
    "@source-input",
    "@source-interface",
];
const MAX_RENDERED_DIAGNOSTICS: usize = 50;

/// Summary produced by the generated std metadata check.
///
/// Inputs:
/// - `checked_file_count`: generated std files scanned for redundant metadata.
///
/// Output:
/// - Stable success metric rendered by the command-line wrapper.
///
/// Transformation:
/// - Keeps the count separate from diagnostics so release output remains
///   compact when generated headers stay minimal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdGeneratedMetadataSummary {
    pub checked_file_count: usize,
}

/// Runs the generated std metadata check.
///
/// Inputs:
/// - `root`: repository root containing `std/js` and `std/summaries`.
///
/// Output:
/// - Success when generated std JS sources, interfaces, tests, and summaries
///   carry complete provenance with the correct artifact kind.
/// - Stable diagnostics listing missing, duplicate, or mismatched metadata.
///
/// Transformation:
/// - Scans generated `.terl`, `.terli`, and `std.js*.typi` files and validates
///   the generator-owned provenance contract used by drift and review gates.
pub fn run_std_generated_metadata(root: &Path) -> QualityResult<StdGeneratedMetadataSummary> {
    let files = collect_generated_std_files(root)?;
    let mut diagnostics = Vec::new();

    for file in &files {
        diagnostics.extend(check_generated_metadata(root, file)?);
    }

    if !diagnostics.is_empty() {
        let omitted = diagnostics.len().saturating_sub(MAX_RENDERED_DIAGNOSTICS);
        diagnostics.truncate(MAX_RENDERED_DIAGNOSTICS);
        if omitted > 0 {
            diagnostics.push(format!("... {omitted} additional diagnostics omitted"));
        }
        return Err(render_failure("std-generated-metadata", &diagnostics));
    }

    Ok(StdGeneratedMetadataSummary {
        checked_file_count: files.len(),
    })
}

fn collect_generated_std_files(root: &Path) -> QualityResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_js_sources(root, &root.join("std/js"), &mut files)?;
    collect_summary_sources(root, &root.join("std/summaries"), &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_js_sources(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> QualityResult<()> {
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("{}: failed to read directory: {err}", dir.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|err| format!("{}: failed to read entry: {err}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_js_sources(root, &path, files)?;
        } else if is_terlan_source(&path) && is_generated_file(&path)? {
            files.push(path);
        }
    }
    let _ = root;
    Ok(())
}

fn collect_summary_sources(
    _root: &Path,
    dir: &Path,
    files: &mut Vec<PathBuf>,
) -> QualityResult<()> {
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("{}: failed to read directory: {err}", dir.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|err| format!("{}: failed to read entry: {err}", dir.display()))?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("std.js") && name.ends_with(".typi"))
            && is_generated_file(&path)?
        {
            files.push(path);
        }
    }
    Ok(())
}

fn is_terlan_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "terl" | "terli"))
}

fn is_generated_file(path: &Path) -> QualityResult<bool> {
    let content = fs::read_to_string(path)
        .map_err(|err| format!("{}: failed to read: {err}", path.display()))?;
    Ok(content.contains("@generated true"))
}

fn check_generated_metadata(root: &Path, file: &Path) -> QualityResult<Vec<String>> {
    let content = fs::read_to_string(file)
        .map_err(|err| format!("{}: failed to read: {err}", file.display()))?;
    let relative = file.strip_prefix(root).unwrap_or(file);
    let mut diagnostics = Vec::new();

    for required in REQUIRED_GENERATED_METADATA {
        let count = content.matches(required).count();
        if count == 0 {
            diagnostics.push(format!(
                "{}: required generated metadata `{}` is missing",
                relative.display(),
                required.trim()
            ));
        } else if count > 1 {
            diagnostics.push(format!(
                "{}: generated metadata `{}` appears {count} times",
                relative.display(),
                required.trim()
            ));
        }
    }

    let expected_kind = expected_artifact_kind(file);
    if !content.contains(&format!("@artifact-kind {expected_kind}")) {
        diagnostics.push(format!(
            "{}: generated artifact kind must be `{expected_kind}`",
            relative.display()
        ));
    }

    Ok(diagnostics)
}

fn expected_artifact_kind(file: &Path) -> &'static str {
    match file.extension().and_then(|extension| extension.to_str()) {
        Some("terli") => "interface",
        Some("typi") => "summary",
        _ if file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| stem.ends_with("Test")) =>
        {
            "test"
        }
        _ => "source",
    }
}

#[cfg(test)]
#[path = "std_generated_metadata_test.rs"]
#[cfg(test)]
mod std_generated_metadata_test;
