use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::{render_failure, QualityResult};

const INVENTORY_PATH: &str = "tools/quality/vm_hashmap_inventory.tsv";
const INVENTORY_HEADER: &[&str] = &["path", "classification", "owner", "notes"];
const VM_SCAN_ROOTS: &[&str] = &[
    "crates/terlan/src/runtime/vm.rs",
    "crates/terlan/src/runtime/vm",
];
const RANDOMIZED_HASH_TOKENS: &[&str] = &["HashMap", "RandomState"];

const ALLOWED_CLASSIFICATIONS: &[&str] = &[
    "lexical-env",
    "lookup-table",
    "handle-registry",
    "transport-registry",
    "migration-debt",
];
const PLACEHOLDER_INVENTORY_VALUES: &[&str] = &["todo", "tbd", "unknown", "fixme"];

/// Summary produced by the VM deterministic HashMap quality gate.
///
/// Inputs:
/// - Inventory rows and runtime VM source files that mention Rust `HashMap`.
///
/// Output:
/// - Stable counts for CLI reporting.
///
/// Transformation:
/// - Keeps randomized Rust `HashMap` use visible in VM-owned runtime code so
///   nondeterministic iteration cannot silently leak into VM semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmDeterministicHashMapSummary {
    pub inventory_row_count: usize,
    pub scanned_reference_count: usize,
}

/// One VM HashMap inventory row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct VmHashMapInventoryRow {
    path: PathBuf,
    classification: String,
    owner: String,
    notes: String,
}

/// Runs the VM deterministic HashMap quality gate.
///
/// Inputs:
/// - `root`: repository root containing VM runtime sources and
///   `tools/quality/vm_hashmap_inventory.tsv`.
///
/// Output:
/// - Success when every VM runtime `HashMap` use is inventoried.
/// - Stable diagnostics for unclassified, stale, duplicate, or invalid rows.
///
/// Transformation:
/// - Scans implementation files under the VM runtime and requires explicit
///   ownership for randomized hash tables.
pub fn run_vm_deterministic_hashmap(root: &Path) -> QualityResult<VmDeterministicHashMapSummary> {
    let inventory = read_vm_hashmap_inventory(root)?;
    let references = collect_vm_hashmap_reference_files(root)?;
    let mut diagnostics = validate_allowed_classifications_have_no_placeholders();
    diagnostics.extend(validate_vm_hashmap_inventory(root, &inventory, &references));
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-deterministic-hashmap", &diagnostics));
    }

    Ok(VmDeterministicHashMapSummary {
        inventory_row_count: inventory.len(),
        scanned_reference_count: references.len(),
    })
}

/// Reads the VM HashMap inventory TSV.
fn read_vm_hashmap_inventory(root: &Path) -> QualityResult<Vec<VmHashMapInventoryRow>> {
    let text = fs::read_to_string(root.join(INVENTORY_PATH))
        .map_err(|err| format!("{INVENTORY_PATH}: failed to read inventory: {err}"))?;
    parse_vm_hashmap_inventory(&text)
}

/// Parses VM HashMap inventory TSV text.
fn parse_vm_hashmap_inventory(text: &str) -> QualityResult<Vec<VmHashMapInventoryRow>> {
    let mut rows = uncommented_tsv_rows(text);
    let Some((line, header)) = rows.next() else {
        return Err(format!("{INVENTORY_PATH}: missing header"));
    };
    if header != INVENTORY_HEADER {
        return Err(format!(
            "{INVENTORY_PATH}:{line}: expected header `{}`, found `{}`",
            INVENTORY_HEADER.join("\t"),
            header.join("\t")
        ));
    }

    let mut inventory = Vec::new();
    for (line, fields) in rows {
        if fields.len() != INVENTORY_HEADER.len() {
            return Err(format!(
                "{INVENTORY_PATH}:{line}: expected {} columns, found {}",
                INVENTORY_HEADER.len(),
                fields.len()
            ));
        }
        inventory.push(VmHashMapInventoryRow {
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

/// Collects VM runtime implementation files that mention Rust `HashMap`.
fn collect_vm_hashmap_reference_files(root: &Path) -> QualityResult<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    for relative in VM_SCAN_ROOTS {
        let path = root.join(relative);
        if path.is_file() {
            maybe_insert_hashmap_file(root, Path::new(relative), &mut files)?;
            collect_adjacent_part_files(root, Path::new(relative), &mut files)?;
        } else if path.is_dir() {
            collect_vm_hashmap_reference_files_in_dir(root, Path::new(relative), &mut files)?;
        }
    }
    Ok(files)
}

/// Scans numbered include fragments adjacent to a file scan root.
fn collect_adjacent_part_files(
    root: &Path,
    relative: &Path,
    files: &mut BTreeSet<PathBuf>,
) -> QualityResult<()> {
    let Some(parent) = relative.parent() else {
        return Ok(());
    };
    let Some(stem) = relative.file_stem().and_then(|stem| stem.to_str()) else {
        return Ok(());
    };
    let prefix = format!("{stem}_part_");
    for entry in fs::read_dir(root.join(parent))
        .map_err(|err| format!("{}: failed to read directory: {err}", parent.display()))?
    {
        let entry = entry.map_err(|err| {
            format!(
                "{}: failed to read directory entry: {err}",
                parent.display()
            )
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(&prefix) && name.ends_with(".rs") {
            maybe_insert_hashmap_file(root, &parent.join(name), files)?;
        }
    }
    Ok(())
}

/// Recursively collects VM runtime implementation files mentioning `HashMap`.
fn collect_vm_hashmap_reference_files_in_dir(
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
            collect_vm_hashmap_reference_files_in_dir(root, &child, files)?;
        } else if child_full_path.is_file() {
            maybe_insert_hashmap_file(root, &child, files)?;
        }
    }
    Ok(())
}

/// Inserts one file into the reference set when it mentions randomized hash use.
fn maybe_insert_hashmap_file(
    root: &Path,
    relative: &Path,
    files: &mut BTreeSet<PathBuf>,
) -> QualityResult<()> {
    let logical = logical_source_path(root, relative);
    if !is_rust_implementation_file(&logical) {
        return Ok(());
    }
    let text = match fs::read_to_string(root.join(relative)) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::InvalidData => return Ok(()),
        Err(err) => {
            return Err(format!(
                "{}: failed to read scanned file: {err}",
                relative.display()
            ));
        }
    };
    if RANDOMIZED_HASH_TOKENS
        .iter()
        .any(|token| text.contains(token))
    {
        files.insert(logical);
    }
    Ok(())
}

/// Attributes numbered include fragments to their adjacent wrapper module.
fn logical_source_path(root: &Path, relative: &Path) -> PathBuf {
    let Some(stem) = relative.file_stem().and_then(|stem| stem.to_str()) else {
        return relative.to_path_buf();
    };
    let Some((owner, part)) = stem.rsplit_once("_part_") else {
        return relative.to_path_buf();
    };
    if part.len() != 3 || !part.bytes().all(|byte| byte.is_ascii_digit()) {
        return relative.to_path_buf();
    }
    let wrapper = relative.with_file_name(format!("{owner}.rs"));
    if root.join(&wrapper).is_file() {
        wrapper
    } else {
        relative.to_path_buf()
    }
}

/// Returns whether a path is a production Rust implementation file.
fn is_rust_implementation_file(relative: &Path) -> bool {
    relative
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".rs") && !name.ends_with("_test.rs"))
}

/// Returns whether a directory should be skipped during recursive scans.
fn should_skip_dir(relative: &Path) -> bool {
    relative
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, "target" | ".git"))
}

/// Validates inventory rows against scanned VM HashMap references.
fn validate_vm_hashmap_inventory(
    root: &Path,
    inventory: &[VmHashMapInventoryRow],
    references: &BTreeSet<PathBuf>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut by_path = BTreeMap::new();
    let mut previous_path: Option<String> = None;

    for row in inventory {
        let current_path = row.path.to_string_lossy().into_owned();
        if let Some(previous) = previous_path.as_deref() {
            if previous > current_path.as_str() {
                diagnostics.push(format!(
                    "{}: VM HashMap inventory rows must be sorted by path; previous row was `{}`",
                    row.path.display(),
                    previous
                ));
            }
        }
        previous_path = Some(current_path);

        if by_path.insert(row.path.clone(), row).is_some() {
            diagnostics.push(format!(
                "{}: duplicate VM HashMap inventory row",
                row.path.display()
            ));
        }
        if !ALLOWED_CLASSIFICATIONS.contains(&row.classification.as_str()) {
            diagnostics.push(format!(
                "{}: unsupported VM HashMap classification `{}`",
                row.path.display(),
                row.classification
            ));
        }
        if row.owner.trim().is_empty() || row.notes.trim().is_empty() {
            diagnostics.push(format!(
                "{}: VM HashMap inventory rows require owner and notes",
                row.path.display()
            ));
        }
        if is_placeholder_inventory_value(&row.owner) || is_placeholder_inventory_value(&row.notes)
        {
            diagnostics.push(format!(
                "{}: VM HashMap inventory owner and notes must not use placeholder values",
                row.path.display()
            ));
        }
        if !is_vm_runtime_path(&row.path) {
            diagnostics.push(format!(
                "{}: VM HashMap inventory rows must stay under VM runtime sources",
                row.path.display()
            ));
        }
        if row
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_test.rs"))
        {
            diagnostics.push(format!(
                "{}: VM HashMap inventory must not include test files",
                row.path.display()
            ));
        }
        if !root.join(&row.path).exists() {
            diagnostics.push(format!(
                "{}: stale VM HashMap inventory path",
                row.path.display()
            ));
        }
    }

    for reference in references {
        if !by_path.contains_key(reference) {
            diagnostics.push(format!(
                "{}: unclassified VM HashMap/RandomState reference; use BTreeMap/IndexMap/sorted output or add an explicit inventory row",
                reference.display()
            ));
        }
    }

    for row in inventory {
        if root.join(&row.path).exists() && !references.contains(&row.path) {
            diagnostics.push(format!(
                "{}: stale VM HashMap inventory row; file no longer mentions HashMap/RandomState",
                row.path.display()
            ));
        }
    }

    diagnostics
}

fn validate_allowed_classifications_have_no_placeholders() -> Vec<String> {
    ALLOWED_CLASSIFICATIONS
        .iter()
        .flat_map(|classification| {
            validate_text_has_no_placeholder_value(
                "allowed VM HashMap classification",
                classification,
            )
        })
        .collect()
}

fn validate_text_has_no_placeholder_value(label: &str, value: &str) -> Vec<String> {
    if is_placeholder_inventory_value(value) {
        vec![format!(
            "{label} `{value}` must not use placeholder inventory values"
        )]
    } else {
        Vec::new()
    }
}

fn is_placeholder_inventory_value(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    PLACEHOLDER_INVENTORY_VALUES
        .iter()
        .any(|placeholder| normalized == *placeholder || normalized.contains(placeholder))
}

/// Returns whether a path belongs to the VM runtime implementation scan.
fn is_vm_runtime_path(path: &Path) -> bool {
    path == Path::new("crates/terlan/src/runtime/vm.rs")
        || path.starts_with("crates/terlan/src/runtime/vm")
}

#[cfg(test)]
#[path = "vm_deterministic_hashmap_test.rs"]
#[cfg(test)]
mod vm_deterministic_hashmap_test;
