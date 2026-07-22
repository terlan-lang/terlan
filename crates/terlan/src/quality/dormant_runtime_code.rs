use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::{render_failure, QualityResult};

const INVENTORY_PATH: &str = "docs/runtime/DORMANT_RUNTIME_CODE.tsv";
const VM_RUNTIME_DIR: &str = "crates/terlan/src/runtime/vm";
const INVENTORY_HEADER: &[&str] = &["path", "classification", "reason", "next_action"];
const ALLOWED_CLASSIFICATIONS: &[&str] = &["design-only", "experimental", "pending-runtime-wiring"];
const PLACEHOLDER_VALUES: &[&str] = &["todo", "tbd", "unknown", "fixme"];

/// Summary produced by the dormant runtime code quality gate.
///
/// Inputs:
/// - Current VM runtime module references.
/// - Checked-in dormant runtime inventory.
///
/// Output:
/// - Counts used by the CLI success message.
///
/// Transformation:
/// - Separates active implementation modules from modules that are only
///   referenced by tests or module declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantRuntimeCodeSummary {
    pub dormant_module_count: usize,
    pub inventory_row_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DormantRuntimeModule {
    path: PathBuf,
    module_name: String,
}

/// Runs the dormant runtime implementation audit.
///
/// Inputs:
/// - `root`: repository root containing `crates/` and `docs/runtime/`.
///
/// Output:
/// - Success summary when every dormant VM module is explicitly inventoried.
/// - Diagnostics for unclassified dormant modules, stale inventory rows, or
///   malformed inventory entries.
///
/// Transformation:
/// - Detects VM implementation modules that have tests but no active non-test
///   runtime reference, so design-only slices cannot silently look complete.
pub fn run_dormant_runtime_code(root: &Path) -> QualityResult<DormantRuntimeCodeSummary> {
    let dormant = discover_dormant_vm_modules(root)?;
    let (inventory, mut diagnostics) = read_inventory(root)?;
    let dormant_paths = dormant
        .iter()
        .map(|module| module.path.clone())
        .collect::<BTreeSet<_>>();

    for module in &dormant {
        if !inventory.contains_key(&module.path) {
            diagnostics.push(format!(
                "{}: dormant VM module `{}` has no `{INVENTORY_PATH}` row",
                module.path.display(),
                module.module_name
            ));
        }
    }

    for path in inventory.keys() {
        if !dormant_paths.contains(path) {
            diagnostics.push(format!(
                "{}: stale dormant runtime inventory row; module now has an active runtime reference or no longer exists",
                path.display()
            ));
        }
    }

    if !diagnostics.is_empty() {
        return Err(render_failure("dormant-runtime-code", &diagnostics));
    }

    Ok(DormantRuntimeCodeSummary {
        dormant_module_count: dormant.len(),
        inventory_row_count: inventory.len(),
    })
}

fn discover_dormant_vm_modules(root: &Path) -> QualityResult<Vec<DormantRuntimeModule>> {
    let runtime_dir = root.join(VM_RUNTIME_DIR);
    let source_files = collect_source_files(root)?;
    let active_sources = source_files
        .iter()
        .filter(|path| !is_test_file(path))
        .map(|path| {
            let text = fs::read_to_string(root.join(path))
                .map_err(|error| format!("{}: failed to read source: {error}", path.display()))?;
            Ok((path.clone(), text))
        })
        .collect::<QualityResult<Vec<_>>>()?;

    let mut modules = Vec::new();
    for entry in fs::read_dir(&runtime_dir).map_err(|error| {
        format!(
            "{}: failed to read directory: {error}",
            runtime_dir.display()
        )
    })? {
        let entry = entry
            .map_err(|error| format!("{}: failed to read entry: {error}", runtime_dir.display()))?;
        let path = entry.path();
        if !path.is_file() || path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if is_test_file(&path) {
            continue;
        }
        let Some(module_name) = file_name.strip_suffix(".rs") else {
            continue;
        };
        let relative = path
            .strip_prefix(root)
            .map_err(|error| format!("{}: failed to relativize path: {error}", path.display()))?
            .to_path_buf();
        if !has_active_reference(&active_sources, &relative, module_name) {
            modules.push(DormantRuntimeModule {
                path: relative,
                module_name: module_name.to_string(),
            });
        }
    }
    modules.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(modules)
}

fn has_active_reference(
    active_sources: &[(PathBuf, String)],
    module_path: &Path,
    module_name: &str,
) -> bool {
    let module_call = format!("{module_name}::");
    let module_use = format!("::{module_name}");
    for (path, text) in active_sources {
        if path == module_path {
            continue;
        }
        for line in text.lines() {
            let trimmed = line.trim();
            if include_target(path, trimmed).is_some_and(|target| target == module_path) {
                return true;
            }
            if path_attribute_target(path, trimmed).is_some_and(|target| target == module_path) {
                return true;
            }
            if is_module_declaration(trimmed, module_name) {
                continue;
            }
            if trimmed.contains(&module_call) || trimmed.contains(&module_use) {
                return true;
            }
        }
    }
    false
}

fn path_attribute_target(source_path: &Path, line: &str) -> Option<PathBuf> {
    let relative = line.strip_prefix("#[path = \"")?.strip_suffix("\"]")?;
    source_path.parent().map(|parent| parent.join(relative))
}

fn include_target(source_path: &Path, line: &str) -> Option<PathBuf> {
    let relative = line.strip_prefix("include!(\"")?.strip_suffix("\");")?;
    source_path.parent().map(|parent| parent.join(relative))
}

fn is_module_declaration(line: &str, module_name: &str) -> bool {
    line == format!("mod {module_name};")
        || line == format!("pub mod {module_name};")
        || line == format!("pub(crate) mod {module_name};")
        || line == format!("pub(super) mod {module_name};")
}

fn read_inventory(root: &Path) -> QualityResult<(BTreeMap<PathBuf, String>, Vec<String>)> {
    let path = root.join(INVENTORY_PATH);
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("{}: failed to read inventory: {error}", path.display()))?;
    let mut rows = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let mut saw_header = false;
    let mut previous_path: Option<String> = None;

    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let columns = trimmed.split('\t').collect::<Vec<_>>();
        if !saw_header {
            saw_header = true;
            if columns != INVENTORY_HEADER {
                diagnostics.push(format!(
                    "{INVENTORY_PATH}:{}: expected header `{}`, found `{}`",
                    index + 1,
                    INVENTORY_HEADER.join("\t"),
                    columns.join("\t")
                ));
            }
            continue;
        }
        if columns.len() != 4 {
            diagnostics.push(format!(
                "{INVENTORY_PATH}:{}: expected 4 tab-separated columns",
                index + 1
            ));
            continue;
        }
        let row_path = PathBuf::from(columns[0]);
        if let Some(previous) = previous_path.as_deref() {
            if previous > columns[0] {
                diagnostics.push(format!(
                    "{INVENTORY_PATH}:{}: dormant runtime inventory rows must be sorted by path; previous row was `{previous}`",
                    index + 1
                ));
            }
        }
        previous_path = Some(columns[0].to_string());
        if !ALLOWED_CLASSIFICATIONS.contains(&columns[1]) {
            diagnostics.push(format!(
                "{INVENTORY_PATH}:{}: unsupported dormant runtime classification `{}`",
                index + 1,
                columns[1]
            ));
        }
        for (label, value) in [
            ("classification", columns[1]),
            ("reason", columns[2]),
            ("next_action", columns[3]),
        ] {
            if is_placeholder_inventory_value(value) {
                diagnostics.push(format!(
                    "{INVENTORY_PATH}:{}: dormant runtime inventory {label} `{value}` must not use placeholder values",
                    index + 1
                ));
            }
        }
        if rows
            .insert(row_path.clone(), columns[1..].join("\t"))
            .is_some()
        {
            diagnostics.push(format!(
                "{INVENTORY_PATH}:{}: duplicate row for {}",
                index + 1,
                row_path.display()
            ));
        }
    }
    if !saw_header {
        diagnostics.push(format!("{INVENTORY_PATH}: missing header"));
    }
    Ok((rows, diagnostics))
}

fn is_placeholder_inventory_value(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    PLACEHOLDER_VALUES
        .iter()
        .any(|placeholder| normalized == *placeholder || normalized.contains(placeholder))
}

fn collect_source_files(root: &Path) -> QualityResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_source_files_in(root, &root.join("crates/terlan/src"), &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_source_files_in(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
) -> QualityResult<()> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("{}: failed to read directory: {error}", directory.display()))?
    {
        let entry = entry
            .map_err(|error| format!("{}: failed to read entry: {error}", directory.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_source_files_in(root, &path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(
                path.strip_prefix(root)
                    .map_err(|error| {
                        format!("{}: failed to relativize path: {error}", path.display())
                    })?
                    .to_path_buf(),
            );
        }
    }
    Ok(())
}

fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            if name.ends_with("_test.rs") {
                return true;
            }
            let Some(stem) = name.strip_suffix(".rs") else {
                return false;
            };
            let Some((owner, part)) = stem.rsplit_once("_part_") else {
                return false;
            };
            owner.ends_with("_test")
                && part.len() == 3
                && part.bytes().all(|byte| byte.is_ascii_digit())
        })
}

#[cfg(test)]
#[path = "dormant_runtime_code_test.rs"]
mod dormant_runtime_code_test;
