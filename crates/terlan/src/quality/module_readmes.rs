use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::{render_failure, QualityResult};

/// Summary produced by the module README coverage check.
///
/// Inputs:
/// - Current module directory set and missing README set.
///
/// Output:
/// - Count used for stable success diagnostics.
///
/// Transformation:
/// - Separates successful scan metrics from diagnostics so CLI output stays
///   stable across Python-to-Rust migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleReadmeSummary {
    pub missing_count: usize,
}

/// Runs module README coverage validation.
///
/// Inputs:
/// - `root`: repository root containing `crates/`, `std/`, and
///   `tools/quality/`.
///
/// Output:
/// - Success summary when README debt has not grown.
/// - Diagnostics when new module directories lack README files or baseline
///   rows are stale.
///
/// Transformation:
/// - Discovers source-owning module directories, reads the checked-in missing
///   README baseline, and rejects new undocumented module directories.
pub fn run_module_readmes(root: &Path) -> QualityResult<ModuleReadmeSummary> {
    let modules = module_directories(root)?;
    let (baseline, mut diagnostics) = read_baseline(root)?;
    diagnostics.extend(check_readmes(root, &modules, &baseline));
    if !diagnostics.is_empty() {
        return Err(render_failure("module-readmes", &diagnostics));
    }
    let missing_count = modules
        .iter()
        .filter(|path| !root.join(path).join("README.md").is_file())
        .count();
    Ok(ModuleReadmeSummary { missing_count })
}

/// Returns module directories that require README coverage.
///
/// Inputs:
/// - `root`: repository root containing `crates/` and `std/`.
///
/// Output:
/// - Set of repository-relative directories with direct Rust or Terlan source
///   ownership.
///
/// Transformation:
/// - Recursively scans crate directories for Rust ownership and standard
///   library directories for Terlan source ownership.
fn module_directories(root: &Path) -> QualityResult<BTreeSet<PathBuf>> {
    let crates = root.join("crates");
    let std = root.join("std");
    let mut modules = BTreeSet::new();
    collect_module_directories(root, &crates, SourceKind::Rust, &mut modules)?;
    if is_module_directory(&crates, SourceKind::Rust)? {
        modules.insert(PathBuf::from("crates"));
    }
    collect_module_directories(root, &std, SourceKind::Terlan, &mut modules)?;
    if is_module_directory(&std, SourceKind::Terlan)? {
        modules.insert(PathBuf::from("std"));
    }
    Ok(modules)
}

/// Source ownership kind used by module discovery.
///
/// Inputs:
/// - Selected by root directory being scanned.
///
/// Output:
/// - Matching direct-child source rule for a directory.
///
/// Transformation:
/// - Keeps Rust and Terlan module ownership rules explicit while sharing the
///   filesystem traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    Rust,
    Terlan,
}

/// Recursively collects module directories.
///
/// Inputs:
/// - `root`: repository root used for relative paths.
/// - `directory`: directory to scan.
/// - `kind`: source ownership rule.
/// - `modules`: output accumulator.
///
/// Output:
/// - `Ok(())` when scanning succeeds.
///
/// Transformation:
/// - Adds directories that directly own source files or crate manifests while
///   still recursing into children.
fn collect_module_directories(
    root: &Path,
    directory: &Path,
    kind: SourceKind,
    modules: &mut BTreeSet<PathBuf>,
) -> QualityResult<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    if is_module_directory(directory, kind)? {
        let relative = directory.strip_prefix(root).map_err(|err| {
            format!(
                "{}: failed to relativize module path: {err}",
                directory.display()
            )
        })?;
        modules.insert(relative.to_path_buf());
    }
    for entry in fs::read_dir(directory)
        .map_err(|err| format!("{}: failed to read directory: {err}", directory.display()))?
    {
        let entry =
            entry.map_err(|err| format!("{}: failed to read entry: {err}", directory.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| format!("{}: failed to read file type: {err}", path.display()))?;
        if file_type.is_dir() {
            collect_module_directories(root, &path, kind, modules)?;
        }
    }
    Ok(())
}

/// Returns whether a directory owns source that requires a README.
///
/// Inputs:
/// - `directory`: directory being classified.
/// - `kind`: Rust or Terlan source ownership rule.
///
/// Output:
/// - `true` when the directory directly contains owned source files.
///
/// Transformation:
/// - Checks direct children only so the README requirement applies to the
///   directory that owns the files, not every ancestor.
fn is_module_directory(directory: &Path, kind: SourceKind) -> QualityResult<bool> {
    if !directory.is_dir() {
        return Ok(false);
    }
    for entry in fs::read_dir(directory)
        .map_err(|err| format!("{}: failed to read directory: {err}", directory.display()))?
    {
        let entry =
            entry.map_err(|err| format!("{}: failed to read entry: {err}", directory.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| format!("{}: failed to read file type: {err}", path.display()))?;
        if !file_type.is_file() {
            continue;
        }
        let owns_source = match kind {
            SourceKind::Rust => {
                path.file_name().and_then(|name| name.to_str()) == Some("Cargo.toml")
                    || path.extension().is_some_and(|extension| extension == "rs")
            }
            SourceKind::Terlan => path
                .extension()
                .is_some_and(|extension| extension == "terl" || extension == "terli"),
        };
        if owns_source {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Reads missing-README migration baseline rows.
///
/// Inputs:
/// - `root`: repository root containing
///   `tools/quality/module_readme_missing_baseline.txt`.
///
/// Output:
/// - Set of repository-relative directories allowed to be missing README.md.
/// - Diagnostics for malformed rows.
///
/// Transformation:
/// - Parses one directory path per line while allowing comments and blanks.
fn read_baseline(root: &Path) -> QualityResult<(BTreeSet<PathBuf>, Vec<String>)> {
    let path = root
        .join("tools")
        .join("quality")
        .join("module_readme_missing_baseline.txt");
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read baseline: {err}", path.display()))?;
    let mut baseline = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.contains('\t') {
            diagnostics.push(format!(
                "{}:{}: expected one directory path per line",
                path.display(),
                index + 1
            ));
            continue;
        }
        baseline.insert(PathBuf::from(line));
    }
    Ok((baseline, diagnostics))
}

/// Validates module-directory README coverage.
///
/// Inputs:
/// - `root`: repository root.
/// - `modules`: current module directories.
/// - `baseline`: missing-README baseline directories.
///
/// Output:
/// - Diagnostics for new missing READMEs and stale baseline rows.
///
/// Transformation:
/// - Allows current documentation debt only while preventing new module
///   directories without README.md.
fn check_readmes(
    root: &Path,
    modules: &BTreeSet<PathBuf>,
    baseline: &BTreeSet<PathBuf>,
) -> Vec<String> {
    let missing = modules
        .iter()
        .filter(|path| !root.join(path).join("README.md").is_file())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut diagnostics = Vec::new();
    for path in baseline {
        if !modules.contains(path) {
            diagnostics.push(format!(
                "{}: stale README baseline row; directory no longer exists",
                path.display()
            ));
        } else if !missing.contains(path) {
            diagnostics.push(format!(
                "{}: stale README baseline row; README.md now exists",
                path.display()
            ));
        }
    }
    for path in missing {
        if !baseline.contains(&path) {
            diagnostics.push(format!(
                "{}: missing README.md; use README_TEMPLATE.md",
                path.display()
            ));
        }
    }
    diagnostics
}

#[cfg(test)]
#[path = "module_readmes_test.rs"]
mod module_readmes_test;
