use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::terlan_quality::{render_failure, QualityResult};

/// Summary produced by the std source naming check.
///
/// Inputs:
/// - `checked_source_count`: hand-authored std sources that were validated.
///
/// Output:
/// - Stable success metric rendered by the command-line wrapper.
///
/// Transformation:
/// - Keeps the count separate from diagnostics so release output remains
///   compact when the convention is satisfied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdSourceNamingSummary {
    pub checked_source_count: usize,
}

/// Runs the hand-authored std source naming check.
///
/// Inputs:
/// - `root`: repository root containing `std/`.
///
/// Output:
/// - Success when each hand-authored std source file basename matches the final
///   module segment declared in the file.
/// - Stable diagnostics for extension mistakes, missing module declarations, or
///   basename mismatches.
///
/// Transformation:
/// - Recursively scans `std/`, skips generated JavaScript bindings and summary
///   artifacts, then compares module declarations with source filenames.
pub fn run_std_source_naming(root: &Path) -> QualityResult<StdSourceNamingSummary> {
    let sources = collect_hand_authored_std_sources(root)?;
    let mut diagnostics = Vec::new();

    for source in &sources {
        diagnostics.extend(check_source_name(root, source));
    }

    if !diagnostics.is_empty() {
        return Err(render_failure("std-source-naming", &diagnostics));
    }

    Ok(StdSourceNamingSummary {
        checked_source_count: sources.len(),
    })
}

/// Collects hand-authored standard-library sources covered by the naming rule.
///
/// Inputs:
/// - `root`: repository root containing `std/`.
///
/// Output:
/// - Sorted absolute paths to `.terl` and `.terli` files.
///
/// Transformation:
/// - Skips cached summaries, disabled scratch material, and generated docs
///   fixtures while keeping implementation and interface files, including
///   generated `std/js` bindings.
fn collect_hand_authored_std_sources(root: &Path) -> QualityResult<Vec<PathBuf>> {
    let std = root.join("std");
    let mut sources = Vec::new();
    collect_sources(root, &std, &mut sources)?;
    sources.sort();
    Ok(sources)
}

/// Recursively scans one directory for Terlan source files.
///
/// Inputs:
/// - `root`: repository root used for relative-path diagnostics.
/// - `dir`: directory currently being scanned.
/// - `sources`: output accumulator.
///
/// Output:
/// - `Ok(())` when the directory was scanned.
///
/// Transformation:
/// - Visits children in filesystem order and pushes supported source files into
///   the accumulator.
fn collect_sources(root: &Path, dir: &Path, sources: &mut Vec<PathBuf>) -> QualityResult<()> {
    if should_skip_dir(root, dir) {
        return Ok(());
    }

    let entries = fs::read_dir(dir)
        .map_err(|err| format!("{}: failed to read directory: {err}", dir.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|err| format!("{}: failed to read entry: {err}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_sources(root, &path, sources)?;
        } else if is_terlan_source(&path) {
            sources.push(path);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == "tert")
        {
            sources.push(path);
        }
    }
    Ok(())
}

/// Returns whether a std directory is outside the hand-authored naming rule.
///
/// Inputs:
/// - `root`: repository root.
/// - `dir`: candidate directory.
///
/// Output:
/// - `true` when the directory should be skipped.
///
/// Transformation:
/// - Uses repository-relative path segments so the rule is stable across
///   workspaces.
fn should_skip_dir(root: &Path, dir: &Path) -> bool {
    let Ok(relative) = dir.strip_prefix(root) else {
        return false;
    };
    let parts = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    parts.contains(&"summaries") || parts.contains(&"disabled") || parts.contains(&"fixtures")
}

/// Returns whether a path is a supported Terlan source extension.
///
/// Inputs:
/// - `path`: candidate file path.
///
/// Output:
/// - `true` for `.terl` and `.terli`.
///
/// Transformation:
/// - Checks the extension only; malformed `.tert` files are handled as
///   diagnostics by `check_source_name`.
fn is_terlan_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "terl" | "terli"))
}

/// Checks one std source filename against its module declaration.
///
/// Inputs:
/// - `root`: repository root.
/// - `source`: absolute source path.
///
/// Output:
/// - Empty diagnostics when the file satisfies the convention.
///
/// Transformation:
/// - Reads the first module declaration and compares the basename with the
///   final module segment plus the original file extension.
fn check_source_name(root: &Path, source: &Path) -> Vec<String> {
    let relative = source.strip_prefix(root).unwrap_or(source);
    let Some(extension) = source.extension().and_then(|extension| extension.to_str()) else {
        return vec![format!(
            "{}: missing Terlan source extension",
            relative.display()
        )];
    };
    if extension == "tert" {
        return vec![format!(
            "{}: unsupported Terlan source extension `.tert`; use `.terl`",
            relative.display()
        )];
    }

    let text = match fs::read_to_string(source) {
        Ok(text) => text,
        Err(err) => {
            return vec![format!(
                "{}: failed to read source: {err}",
                relative.display()
            )];
        }
    };
    let Some(module) = read_module_name(&text) else {
        return vec![format!(
            "{}: missing module declaration",
            relative.display()
        )];
    };
    let Some(last_segment) = module.rsplit('.').next() else {
        return vec![format!(
            "{}: invalid module declaration `{module}`",
            relative.display()
        )];
    };
    let expected = format!("{last_segment}.{extension}");
    let actual = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if actual == expected {
        return Vec::new();
    }

    vec![format!(
        "{}: source filename `{actual}` does not match module `{module}`; expected `{expected}`",
        relative.display()
    )]
}

/// Reads the first module declaration from Terlan source text.
///
/// Inputs:
/// - `text`: Terlan source text.
///
/// Output:
/// - Module path without the trailing period when present.
///
/// Transformation:
/// - Applies the same lightweight declaration scan used by other quality
///   inventory gates without parsing the full language.
fn read_module_name(text: &str) -> Option<String> {
    let re = Regex::new(r"(?m)^\s*module\s+([A-Za-z_][A-Za-z0-9_.]*)\s*\.\s*$")
        .expect("module regex should compile");
    re.captures(text)
        .and_then(|captures| captures.get(1))
        .map(|match_| match_.as_str().to_owned())
}

#[cfg(test)]
#[path = "std_source_naming_test.rs"]
mod std_source_naming_test;
