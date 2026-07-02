use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::{render_failure, QualityResult};

const TOKIO_INVENTORY_PATH: &str = "tools/quality/tokio_runtime_inventory.tsv";

const INVENTORY_HEADER: &[&str] = &["path", "classification", "owner", "notes"];

const SCAN_ROOTS: &[&str] = &[
    "Cargo.lock",
    "crates/terlan/Cargo.toml",
    "crates/terlan/src",
    "std",
];

const ALLOWED_CLASSIFICATIONS: &[&str] = &[
    "editor-tooling",
    "generated-summary",
    "lockfile-transitive",
    "maintained-client-boundary",
    "migration-debt",
    "quality-gate",
    "reference-only",
    "test-harness",
];

/// Summary produced by the no-default-Tokio runtime gate.
///
/// Inputs:
/// - Inventory rows and scanned repository files containing Tokio references.
///
/// Output:
/// - Stable counts for CLI reporting.
///
/// Transformation:
/// - Keeps Tokio references visible and classified while preventing Tokio from
///   becoming an implicit default runtime contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoDefaultTokioRuntimeSummary {
    pub inventory_row_count: usize,
    pub scanned_reference_count: usize,
}

/// One Tokio inventory row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TokioInventoryRow {
    path: PathBuf,
    classification: String,
    owner: String,
    notes: String,
}

/// Runs the no-default-Tokio runtime gate.
///
/// Inputs:
/// - `root`: repository root containing Cargo metadata, source, std files, and
///   `tools/quality/tokio_runtime_inventory.tsv`.
///
/// Output:
/// - Success summary when every Tokio reference is explicitly classified.
/// - Stable diagnostics for unclassified, stale, or invalid inventory rows.
///
/// Transformation:
/// - Scans current repository text for Tokio references, compares the result to
///   the checked inventory, and rejects Tokio references inside VM-owned
///   runtime paths.
pub fn run_no_default_tokio_runtime(root: &Path) -> QualityResult<NoDefaultTokioRuntimeSummary> {
    let inventory = read_tokio_inventory(root)?;
    let references = collect_tokio_reference_files(root)?;
    let diagnostics = validate_tokio_inventory(root, &inventory, &references);
    if !diagnostics.is_empty() {
        return Err(render_failure("no-default-tokio-runtime", &diagnostics));
    }

    Ok(NoDefaultTokioRuntimeSummary {
        inventory_row_count: inventory.len(),
        scanned_reference_count: references.len(),
    })
}

/// Reads the Tokio runtime inventory TSV.
fn read_tokio_inventory(root: &Path) -> QualityResult<Vec<TokioInventoryRow>> {
    let text = fs::read_to_string(root.join(TOKIO_INVENTORY_PATH))
        .map_err(|err| format!("{TOKIO_INVENTORY_PATH}: failed to read inventory: {err}"))?;
    parse_tokio_inventory(&text)
}

/// Parses Tokio inventory TSV text.
fn parse_tokio_inventory(text: &str) -> QualityResult<Vec<TokioInventoryRow>> {
    let mut rows = uncommented_tsv_rows(text);
    let Some((line, header)) = rows.next() else {
        return Err(format!("{TOKIO_INVENTORY_PATH}: missing header"));
    };
    if header != INVENTORY_HEADER {
        return Err(format!(
            "{TOKIO_INVENTORY_PATH}:{line}: expected header `{}`, found `{}`",
            INVENTORY_HEADER.join("\t"),
            header.join("\t")
        ));
    }

    let mut inventory = Vec::new();
    for (line, fields) in rows {
        if fields.len() != INVENTORY_HEADER.len() {
            return Err(format!(
                "{TOKIO_INVENTORY_PATH}:{line}: expected {} columns, found {}",
                INVENTORY_HEADER.len(),
                fields.len()
            ));
        }
        inventory.push(TokioInventoryRow {
            path: PathBuf::from(fields[0]),
            classification: fields[1].to_string(),
            owner: fields[2].to_string(),
            notes: fields[3].to_string(),
        });
    }
    Ok(inventory)
}

/// Returns non-comment TSV rows with one-based line numbers.
fn uncommented_tsv_rows(text: &str) -> impl Iterator<Item = (usize, Vec<&str>)> {
    text.lines().enumerate().filter_map(|(index, line)| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            None
        } else {
            Some((index + 1, line.split('\t').collect()))
        }
    })
}

/// Collects repository files that mention Tokio.
fn collect_tokio_reference_files(root: &Path) -> QualityResult<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    for relative in SCAN_ROOTS {
        let path = root.join(relative);
        if path.is_file() {
            maybe_insert_tokio_file(root, Path::new(relative), &mut files)?;
        } else if path.is_dir() {
            collect_tokio_reference_files_in_dir(root, Path::new(relative), &mut files)?;
        }
    }
    Ok(files)
}

/// Recursively collects files that mention Tokio under one directory.
fn collect_tokio_reference_files_in_dir(
    root: &Path,
    relative: &Path,
    files: &mut BTreeSet<PathBuf>,
) -> QualityResult<()> {
    let full_path = root.join(relative);
    for entry in fs::read_dir(&full_path)
        .map_err(|err| format!("{}: failed to read directory: {err}", relative.display()))?
    {
        let entry = entry.map_err(|err| {
            format!(
                "{}: failed to read directory entry: {err}",
                relative.display()
            )
        })?;
        let child = relative.join(entry.file_name());
        let child_full_path = root.join(&child);
        if child_full_path.is_dir() {
            if should_skip_dir(&child) {
                continue;
            }
            collect_tokio_reference_files_in_dir(root, &child, files)?;
        } else if child_full_path.is_file() {
            maybe_insert_tokio_file(root, &child, files)?;
        }
    }
    Ok(())
}

/// Inserts one file into the reference set when its text mentions Tokio.
fn maybe_insert_tokio_file(
    root: &Path,
    relative: &Path,
    files: &mut BTreeSet<PathBuf>,
) -> QualityResult<()> {
    let text = match fs::read_to_string(root.join(relative)) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::InvalidData => return Ok(()),
        Err(err) => {
            return Err(format!(
                "{}: failed to read scanned file: {err}",
                relative.display()
            ))
        }
    };
    if text.to_lowercase().contains("tokio") {
        files.insert(relative.to_path_buf());
    }
    Ok(())
}

/// Validates inventory rows against scanned Tokio references.
fn validate_tokio_inventory(
    root: &Path,
    inventory: &[TokioInventoryRow],
    references: &BTreeSet<PathBuf>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut by_path = BTreeMap::new();

    for row in inventory {
        if by_path.insert(row.path.clone(), row).is_some() {
            diagnostics.push(format!(
                "{}: duplicate Tokio inventory row",
                row.path.display()
            ));
        }
        if !ALLOWED_CLASSIFICATIONS.contains(&row.classification.as_str()) {
            diagnostics.push(format!(
                "{}: unsupported Tokio classification `{}`",
                row.path.display(),
                row.classification
            ));
        }
        if row.owner.trim().is_empty() || row.notes.trim().is_empty() {
            diagnostics.push(format!(
                "{}: Tokio inventory rows require owner and notes",
                row.path.display()
            ));
        }
        if !root.join(&row.path).exists() {
            diagnostics.push(format!(
                "{}: stale Tokio inventory path",
                row.path.display()
            ));
        }
        if row.path.starts_with("crates/terlan/src/vm")
            || row.path.starts_with("crates/terlan/src/runtime/vm")
        {
            diagnostics.push(format!(
                "{}: VM-owned runtime paths must not depend on Tokio",
                row.path.display()
            ));
        }
    }

    for reference in references {
        if !by_path.contains_key(reference) {
            diagnostics.push(format!(
                "{}: unclassified Tokio reference",
                reference.display()
            ));
        }
    }

    for row in inventory {
        if root.join(&row.path).exists() && !references.contains(&row.path) {
            diagnostics.push(format!(
                "{}: stale Tokio inventory row; file no longer mentions Tokio",
                row.path.display()
            ));
        }
    }

    diagnostics
}

/// Returns whether a directory should be excluded from Tokio scanning.
fn should_skip_dir(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        matches!(name.as_ref(), "target" | "node_modules" | ".git")
    })
}

#[cfg(test)]
#[path = "no_default_tokio_runtime_test.rs"]
mod no_default_tokio_runtime_test;
