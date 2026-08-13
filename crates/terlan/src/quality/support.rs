use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::terlan_quality::QualityResult;

const ACTIVE_ROADMAP: &str = "docs/roadmap/ROADMAP_0_0_7.md";

/// Parses a fixed-width TSV document with one exact header row.
pub(crate) fn parse_tsv_rows(
    text: &str,
    header: &str,
    path: &str,
) -> QualityResult<Vec<Vec<String>>> {
    let mut lines = text.lines();
    let Some(actual_header) = lines.next() else {
        return Err(format!("{path}: missing header"));
    };
    if actual_header != header {
        return Err(format!(
            "{path}: expected header `{header}`, found `{actual_header}`"
        ));
    }
    let expected_columns = header.split('\t').count();
    let mut rows = Vec::new();
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let columns = line.split('\t').map(str::to_string).collect::<Vec<_>>();
        if columns.len() != expected_columns {
            return Err(format!(
                "{path}: row {} has {} columns, expected {expected_columns}",
                index + 2,
                columns.len()
            ));
        }
        rows.push(columns);
    }
    Ok(rows)
}

/// Returns whether `@test` immediately annotates the named public function.
pub(crate) fn has_annotated_public_function(text: &str, function_name: &str) -> bool {
    let mut pending_test = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "@test" {
            pending_test = true;
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if pending_test && starts_public_function(trimmed, function_name) {
            return true;
        }
        if pending_test {
            pending_test = false;
        }
    }
    false
}

/// Returns whether source contains the named public function declaration.
pub(crate) fn has_public_function(text: &str, function_name: &str) -> bool {
    text.lines()
        .map(str::trim)
        .any(|line| starts_public_function(line, function_name))
}

fn starts_public_function(line: &str, function_name: &str) -> bool {
    line.strip_prefix("pub ").is_some_and(|tail| {
        tail.starts_with(function_name) && tail[function_name.len()..].starts_with('(')
    })
}

/// Returns the recipe body of one Make target.
pub(crate) fn make_target_body(make_graph: &str, target: &str) -> Option<String> {
    let target_prefix = format!("{target}:");
    let mut lines = make_graph.lines();
    for line in lines.by_ref() {
        if line.trim_end().starts_with(&target_prefix) {
            break;
        }
    }
    let body = lines
        .take_while(|line| {
            line.starts_with('\t') || line.trim().is_empty() || line.starts_with('#')
        })
        .collect::<Vec<_>>()
        .join("\n");
    (!body.is_empty()).then_some(body)
}

/// Returns the ordered prerequisite list declared by one Make target.
pub(crate) fn make_target_prerequisites(make_graph: &str, target: &str) -> Option<Vec<String>> {
    let target_prefix = format!("{target}:");
    let lines = make_graph.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| line.trim_end().starts_with(&target_prefix))?;
    let mut header = lines[start]
        .trim_end()
        .strip_prefix(&target_prefix)?
        .trim()
        .to_string();
    let mut index = start;
    while header.ends_with('\\') {
        header.pop();
        index += 1;
        let continuation = lines.get(index)?.trim();
        header.push(' ');
        header.push_str(continuation);
    }
    Some(
        header
            .split_whitespace()
            .filter(|token| *token != "\\")
            .map(str::to_owned)
            .collect(),
    )
}

/// Writes one canonical, newline-terminated JSON quality report.
pub(crate) fn write_json_report<T: Serialize>(path: &Path, report: &T) -> QualityResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "{}: failed to create report directory: {error}",
                parent.display()
            )
        })?;
    }
    let text = serde_json::to_string_pretty(report)
        .map_err(|error| format!("{}: failed to serialize report: {error}", path.display()))?;
    fs::write(path, format!("{text}\n"))
        .map_err(|error| format!("{}: failed to write report: {error}", path.display()))
}

/// Validates textual anchors across a Rust module and its path-split children.
pub(crate) fn validate_required_terms(
    root: &Path,
    relative: &str,
    terms: &[&str],
    label: &str,
) -> QualityResult<Vec<String>> {
    let path = root.join(relative);
    let mut text = fs::read_to_string(&path)
        .map_err(|error| format!("{relative}: failed to read {label}: {error}"))?;
    if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
        if let (Some(parent), Some(stem)) = (path.parent(), path.file_stem()) {
            let module_directory = parent.join(stem);
            if module_directory.is_dir() {
                append_rust_tree(&module_directory, &mut text)?;
            }
        }
    }
    Ok(terms
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("{relative}: missing {label} anchor `{term}`"))
        .collect())
}

fn append_rust_tree(directory: &Path, output: &mut String) -> QualityResult<()> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| {
            format!(
                "{}: failed to read module directory: {error}",
                directory.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            format!(
                "{}: failed to inspect module directory: {error}",
                directory.display()
            )
        })?;
    entries.sort_by_key(std::fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            append_rust_tree(&path, output)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            output.push('\n');
            output.push_str(&fs::read_to_string(&path).map_err(|error| {
                format!("{}: failed to read module source: {error}", path.display())
            })?);
        }
    }
    Ok(())
}

/// Collects public Terlan functions immediately preceded by `@test`.
pub(crate) fn annotated_public_test_names(
    root: &Path,
    relative: &str,
    diagnostics: &mut Vec<String>,
) -> BTreeSet<String> {
    let text = match fs::read_to_string(root.join(relative)) {
        Ok(text) => text,
        Err(error) => {
            diagnostics.push(format!(
                "{relative}: failed to read positive test file: {error}"
            ));
            return BTreeSet::new();
        }
    };

    let mut names = BTreeSet::new();
    let mut previous_was_test = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "@test" {
            previous_was_test = true;
            continue;
        }
        if previous_was_test && trimmed.starts_with("pub ") {
            if let Some(name) = public_function_name(trimmed) {
                names.insert(name.to_string());
            } else {
                diagnostics.push(format!(
                    "{relative}: failed to read @test function name from `{trimmed}`"
                ));
            }
            previous_was_test = false;
        } else if !trimmed.is_empty() && !trimmed.starts_with('@') {
            previous_was_test = false;
        }
    }
    names
}

/// Finds the active roadmap from either the workspace or compiler root.
pub(crate) fn find_active_roadmap(root: &Path) -> QualityResult<PathBuf> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("failed to canonicalize repository root: {error}"))?;
    let local = root.join(ACTIVE_ROADMAP);
    if local.exists() {
        return Ok(local);
    }
    let parent = root
        .parent()
        .map(|parent| parent.join(ACTIVE_ROADMAP))
        .ok_or_else(|| "repository root has no parent for roadmap lookup".to_string())?;
    if parent.exists() {
        return Ok(parent);
    }
    Err(format!(
        "missing active roadmap at `{}` or `{}`",
        local.display(),
        parent.display()
    ))
}

fn public_function_name(line: &str) -> Option<&str> {
    let after_pub = line.strip_prefix("pub ")?;
    let name_end = after_pub.find('(')?;
    let name = &after_pub[..name_end];
    (!name.is_empty()).then_some(name)
}
