use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

mod cli_exact_selectors;
mod erlang_modernization;
mod internal_docs;
mod module_readmes;
mod oxc_boundary;
mod test_hierarchy;

pub use cli_exact_selectors::{run_cli_exact_selectors, CliExactSelectorSummary};
pub use erlang_modernization::{
    run_erlang_modernization_inventory, run_erlang_runtime_matrix, ErlangModernizationSummary,
    ErlangRuntimeMatrixSummary,
};
pub use internal_docs::{run_internal_docs, InternalDocFinding, InternalDocsSummary};
pub use module_readmes::{run_module_readmes, ModuleReadmeSummary};
pub use oxc_boundary::{run_oxc_boundary, OxcBoundaryFinding, OxcBoundarySummary};
pub use test_hierarchy::{run_test_hierarchy, ScriptInvocation, TestHierarchySummary};

/// Maximum lines allowed in Rust implementation files without a baseline row.
pub const IMPL_LINE_LIMIT: usize = 1000;

/// Maximum lines allowed in adjacent Rust test files without a baseline row.
pub const TEST_LINE_LIMIT: usize = 2000;

/// Result alias used by repository quality checks.
pub(crate) type QualityResult<T> = Result<T, String>;

/// Measured Rust source file.
///
/// Inputs:
/// - `path`: repository-relative Rust source path.
/// - `lines`: number of text lines in the file.
///
/// Output:
/// - Immutable file measurement used by quality checks.
///
/// Transformation:
/// - Keeps path and measured size together so diagnostics can report both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustFile {
    pub path: PathBuf,
    pub lines: usize,
}

impl RustFile {
    /// Returns the configured line limit for this Rust file.
    ///
    /// Inputs:
    /// - The file path.
    ///
    /// Output:
    /// - Test-file line limit for `*_test.rs` files.
    /// - Implementation-file line limit for all other Rust files.
    ///
    /// Transformation:
    /// - Classifies by filename suffix only, matching the project test layout
    ///   rule.
    pub fn limit(&self) -> usize {
        if self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_test.rs"))
        {
            TEST_LINE_LIMIT
        } else {
            IMPL_LINE_LIMIT
        }
    }
}

/// Summary produced by the Rust quality check.
///
/// Inputs:
/// - Current Rust file measurements and inline-test findings.
///
/// Output:
/// - Counts used for stable success diagnostics.
///
/// Transformation:
/// - Separates successful scan metrics from diagnostic failures so callers can
///   render CLI output consistently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustQualitySummary {
    pub oversized_count: usize,
    pub inline_test_count: usize,
}

/// Rust declaration discovered by the documentation checker.
///
/// Inputs:
/// - `path`: repository-relative Rust source path.
/// - `kind`: item category such as `fn`, `struct`, or `trait`.
/// - `name`: declared Rust identifier.
/// - `signature`: normalized declaration line used as a stable baseline key.
/// - `line`: one-based source line for diagnostics.
/// - `documented`: whether adjacent Rustdoc was found.
///
/// Output:
/// - Immutable item record consumed by baseline validation.
///
/// Transformation:
/// - Keeps declaration identity, source location, and documentation state
///   together so quality diagnostics can be precise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustItem {
    pub path: PathBuf,
    pub kind: String,
    pub name: String,
    pub signature: String,
    pub line: usize,
    pub documented: bool,
}

impl RustItem {
    /// Returns the baseline key for this Rust item.
    ///
    /// Inputs:
    /// - The item path, kind, name, and normalized signature.
    ///
    /// Output:
    /// - A tab-separated key suitable for checked-in baseline files.
    ///
    /// Transformation:
    /// - Converts path and declaration identity into stable text without
    ///   embedding source line numbers.
    pub fn key(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}",
            self.path.display(),
            self.kind,
            self.name,
            self.signature
        )
    }
}

/// Summary produced by the Rustdoc coverage check.
///
/// Inputs:
/// - Current undocumented Rust item set.
///
/// Output:
/// - Count used for stable success diagnostics.
///
/// Transformation:
/// - Gives the command wrapper the same success information previously printed
///   by the Python gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustdocSummary {
    pub undocumented_count: usize,
}

/// Runs the Rust quality baseline checks.
///
/// Inputs:
/// - `root`: repository root containing `crates/` and `tools/quality/`.
///
/// Output:
/// - Success summary when quality debt has not grown.
/// - Diagnostics when file-size debt grows, inline-test debt grows, or
///   baselines are stale/malformed.
///
/// Transformation:
/// - Combines Rust file-size and inline-test validation into one permanent
///   repository quality gate.
pub fn run_rust_quality(root: &Path) -> QualityResult<RustQualitySummary> {
    let files = iter_rust_files(root)?;
    let (size_baseline, mut diagnostics) = read_size_baseline(root)?;
    let (inline_baseline, inline_diagnostics) = read_inline_test_baseline(root)?;
    diagnostics.extend(inline_diagnostics);
    diagnostics.extend(check_file_sizes(&files, &size_baseline));
    let inline_tests = files_with_inline_tests(root, &files)?;
    diagnostics.extend(check_inline_tests(&inline_tests, &inline_baseline));

    if !diagnostics.is_empty() {
        return Err(render_failure("rust-quality", &diagnostics));
    }

    let oversized_count = files
        .iter()
        .filter(|file| file.lines > file.limit())
        .count();
    Ok(RustQualitySummary {
        oversized_count,
        inline_test_count: inline_tests.len(),
    })
}

/// Runs Rustdoc coverage validation.
///
/// Inputs:
/// - `root`: repository root containing `crates/` and `tools/quality/`.
///
/// Output:
/// - Success summary when undocumented Rust items match the baseline.
/// - Diagnostics when documentation coverage regresses or baselines are stale.
///
/// Transformation:
/// - Discovers Rust functions/types, filters undocumented declarations, and
///   compares them to the checked-in migration baseline.
pub fn run_rustdoc(root: &Path) -> QualityResult<RustdocSummary> {
    let current = undocumented_items(&discover_rustdoc_items(root)?);
    let (baseline, mut diagnostics) = read_rustdoc_baseline(root)?;
    diagnostics.extend(check_rustdoc_baseline(&current, &baseline));
    if !diagnostics.is_empty() {
        return Err(render_failure("rustdoc", &diagnostics));
    }
    Ok(RustdocSummary {
        undocumented_count: current.len(),
    })
}

/// Rewrites the undocumented Rustdoc baseline.
///
/// Inputs:
/// - `root`: repository root containing `crates/` and `tools/quality/`.
///
/// Output:
/// - Number of undocumented items written to the baseline.
///
/// Transformation:
/// - Discovers current undocumented items and serializes their stable keys with
///   the same header used by the previous Python gate.
pub fn write_rustdoc_baseline(root: &Path) -> QualityResult<usize> {
    let current = undocumented_items(&discover_rustdoc_items(root)?);
    let path = rustdoc_baseline_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("{}: failed to create baseline dir: {err}", parent.display()))?;
    }
    let mut lines = vec![
        "# Existing undocumented Rust items allowed during 0.0.4 consolidation.".to_string(),
        "# New Rust functions and types must add Rustdoc instead of extending this file."
            .to_string(),
    ];
    lines.extend(current.keys().cloned());
    fs::write(&path, format!("{}\n", lines.join("\n")))
        .map_err(|err| format!("{}: failed to write baseline: {err}", path.display()))?;
    Ok(current.len())
}

/// Returns measured Rust files under `crates/`.
///
/// Inputs:
/// - `root`: repository root containing the `crates/` directory.
///
/// Output:
/// - Sorted Rust file measurements.
///
/// Transformation:
/// - Recursively scans `.rs` files, counts lines, and stores paths relative to
///   the repository root for stable baseline matching.
fn iter_rust_files(root: &Path) -> QualityResult<Vec<RustFile>> {
    let crates = root.join("crates");
    let mut files = Vec::new();
    collect_rust_files(root, &crates, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

/// Discovers Rust functions and types under implementation files in `crates/`.
///
/// Inputs:
/// - `root`: repository root containing Rust source files.
///
/// Output:
/// - Sorted Rust item records.
///
/// Transformation:
/// - Skips adjacent `*_test.rs` modules because the Rustdoc rule protects
///   compiler implementation files, not test bodies.
/// - Reads each implementation Rust file, matches declaration lines with
///   conservative regexes, and records whether each declaration has adjacent
///   Rustdoc.
fn discover_rustdoc_items(root: &Path) -> QualityResult<Vec<RustItem>> {
    let function_pattern = Regex::new(
        r#"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|const|unsafe|extern(?:\s+"[^"]+")?)\s+)*fn\s+([A-Za-z_][A-Za-z0-9_]*)\b"#,
    )
    .expect("function regex");
    let type_pattern = Regex::new(
        r"^\s*(?:pub(?:\([^)]*\))?\s+)?(struct|enum|union|trait|type)\s+([A-Za-z_][A-Za-z0-9_]*)\b",
    )
    .expect("type regex");
    let raw_string_open_pattern = Regex::new(r#"b?r(#+)?""#).expect("raw string regex");
    let mut files = iter_rust_files(root)?
        .into_iter()
        .filter(|file| {
            file.path
                .file_name()
                .and_then(|name| name.to_str())
                .is_none_or(|name| !name.ends_with("_test.rs"))
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.path.cmp(&right.path));

    let mut items = Vec::new();
    for file in files {
        let path = root.join(&file.path);
        let text = fs::read_to_string(&path)
            .map_err(|err| format!("{}: failed to read source: {err}", path.display()))?;
        let lines = text.lines().map(str::to_string).collect::<Vec<_>>();
        let mut in_escaped_string = false;
        let mut raw_string_terminator = None::<String>;
        for (index, line) in lines.iter().enumerate() {
            let (next_raw_terminator, skip_raw_string) = raw_string_state(
                line,
                raw_string_terminator.as_deref(),
                &raw_string_open_pattern,
            );
            raw_string_terminator = next_raw_terminator;
            if skip_raw_string {
                continue;
            }

            let (next_escaped, skip_line) = escaped_string_state(line, in_escaped_string);
            in_escaped_string = next_escaped;
            if skip_line {
                continue;
            }

            if let Some(captures) = function_pattern.captures(line) {
                items.push(RustItem {
                    path: file.path.clone(),
                    kind: "fn".to_string(),
                    name: captures
                        .get(1)
                        .map(|item| item.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    signature: normalized_signature(line),
                    line: index + 1,
                    documented: line_has_rustdoc(&lines, index),
                });
                continue;
            }
            if let Some(captures) = type_pattern.captures(line) {
                items.push(RustItem {
                    path: file.path.clone(),
                    kind: captures
                        .get(1)
                        .map(|item| item.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    name: captures
                        .get(2)
                        .map(|item| item.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    signature: normalized_signature(line),
                    line: index + 1,
                    documented: line_has_rustdoc(&lines, index),
                });
            }
        }
    }
    Ok(items)
}

/// Recursively collects Rust file measurements.
///
/// Inputs:
/// - `root`: repository root used for relative paths.
/// - `directory`: directory to scan.
/// - `files`: output accumulator.
///
/// Output:
/// - `Ok(())` when scanning succeeds.
/// - Error string when a directory or file cannot be read.
///
/// Transformation:
/// - Walks the filesystem using `std::fs` so the quality crate has no external
///   dependency for this simple repository scan.
fn collect_rust_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<RustFile>,
) -> QualityResult<()> {
    let entries = fs::read_dir(directory)
        .map_err(|err| format!("{}: failed to read directory: {err}", directory.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|err| format!("{}: failed to read entry: {err}", directory.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| format!("{}: failed to read file type: {err}", path.display()))?;
        if file_type.is_dir() {
            collect_rust_files(root, &path, files)?;
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            let text = fs::read_to_string(&path)
                .map_err(|err| format!("{}: failed to read source: {err}", path.display()))?;
            let relative = path
                .strip_prefix(root)
                .map_err(|err| format!("{}: failed to relativize path: {err}", path.display()))?
                .to_path_buf();
            files.push(RustFile {
                path: relative,
                lines: text.lines().count(),
            });
        }
    }
    Ok(())
}

/// Returns a whitespace-normalized declaration line.
///
/// Inputs:
/// - `line`: raw source line containing a Rust item declaration.
///
/// Output:
/// - Single-line signature text for baseline comparison.
///
/// Transformation:
/// - Trims leading/trailing whitespace, collapses internal whitespace, and
///   removes trailing body/opening markers that are not part of the signature.
fn normalized_signature(line: &str) -> String {
    line.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches([' ', '{'])
        .to_string()
}

/// Returns whether an item has adjacent Rustdoc.
///
/// Inputs:
/// - `lines`: source file split into lines.
/// - `item_index`: zero-based index of the item declaration line.
///
/// Output:
/// - `true` when `///`, `/** ... */`, or `//!` documentation is adjacent.
/// - `false` otherwise.
///
/// Transformation:
/// - Walks upward past attributes attached to the item and checks the closest
///   documentation comment block.
fn line_has_rustdoc(lines: &[String], item_index: usize) -> bool {
    let mut index = item_index;
    while index > 0 {
        let previous_index = index - 1;
        if lines[previous_index].trim().starts_with("#[") {
            index = previous_index;
        } else {
            break;
        }
    }
    if index == 0 {
        return false;
    }

    let mut doc_index = index - 1;
    let previous = lines[doc_index].trim();
    if previous.starts_with("///") || previous.starts_with("//!") {
        return true;
    }
    if previous.ends_with("*/") {
        loop {
            let text = lines[doc_index].trim();
            if text.starts_with("/**") || text.starts_with("/*!") {
                return true;
            }
            if text.starts_with("/*") {
                return false;
            }
            if doc_index == 0 {
                break;
            }
            doc_index -= 1;
        }
    }
    previous.starts_with("/**") || previous.starts_with("/*!")
}

/// Returns whether a line should be skipped as an escaped fixture string.
///
/// Inputs:
/// - `line`: current source line.
/// - `active`: whether the previous source line opened an escaped string.
///
/// Output:
/// - Updated escaped-string state.
/// - `true` when the current line is part of the string and should be skipped.
///
/// Transformation:
/// - Tracks Rust test fixtures written as `"\` followed by source-like lines
///   ending in `\n\`, which otherwise look like real Rust declarations.
fn escaped_string_state(line: &str, active: bool) -> (bool, bool) {
    let stripped = line.trim();
    if active {
        return (!stripped.ends_with("\","), true);
    }
    if stripped == r#""\"# {
        return (true, true);
    }
    (false, false)
}

/// Returns whether a line should be skipped as a Rust raw string literal.
///
/// Inputs:
/// - `line`: current source line.
/// - `terminator`: active raw-string terminator such as `"#`, or `None`.
/// - `raw_string_open_pattern`: compiled raw-string opener pattern.
///
/// Output:
/// - Updated raw-string terminator state.
/// - `true` when the current line is part of a raw string and should be
///   skipped.
///
/// Transformation:
/// - Tracks raw strings such as `r#"..."#` and `r###"..."###` so embedded
///   Terlan/Rust-like fixture text is not counted as real Rust declarations.
fn raw_string_state(
    line: &str,
    terminator: Option<&str>,
    raw_string_open_pattern: &Regex,
) -> (Option<String>, bool) {
    if let Some(terminator) = terminator {
        return (
            if line.contains(terminator) {
                None
            } else {
                Some(terminator.to_string())
            },
            true,
        );
    }

    let Some(raw_start) = raw_string_open_pattern.captures(line) else {
        return (None, false);
    };
    let Some(full_match) = raw_start.get(0) else {
        return (None, false);
    };
    let hashes = raw_start.get(1).map(|item| item.as_str()).unwrap_or("");
    let raw_terminator = format!("\"{hashes}");
    let remainder = &line[full_match.end()..];
    (
        if remainder.contains(&raw_terminator) {
            None
        } else {
            Some(raw_terminator)
        },
        true,
    )
}

/// Returns undocumented Rust items keyed by baseline identity.
///
/// Inputs:
/// - `items`: discovered Rust items.
///
/// Output:
/// - Mapping from baseline key to undocumented item.
///
/// Transformation:
/// - Filters documented declarations away and keeps the remaining item records
///   for diagnostics and baseline writing.
fn undocumented_items(items: &[RustItem]) -> BTreeMap<String, RustItem> {
    items
        .iter()
        .filter(|item| !item.documented)
        .map(|item| (item.key(), item.clone()))
        .collect()
}

/// Reads the undocumented Rustdoc migration baseline.
///
/// Inputs:
/// - `root`: repository root containing `tools/quality/rustdoc_missing_baseline.tsv`.
///
/// Output:
/// - Set of item keys allowed to remain undocumented.
/// - Diagnostics for malformed rows.
///
/// Transformation:
/// - Parses tab-separated path/kind/name/signature rows into comparable keys.
fn read_rustdoc_baseline(root: &Path) -> QualityResult<(BTreeSet<String>, Vec<String>)> {
    let path = rustdoc_baseline_path(root);
    let mut baseline = BTreeSet::new();
    let mut diagnostics = Vec::new();
    if !path.exists() {
        diagnostics.push(format!(
            "{}: missing baseline; run with --write-baseline",
            path.display()
        ));
        return Ok((baseline, diagnostics));
    }

    let text = fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read baseline: {err}", path.display()))?;
    for (index, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.split('\t').count() != 4 {
            diagnostics.push(format!(
                "{}:{}: expected path<TAB>kind<TAB>name<TAB>signature",
                path.display(),
                index + 1
            ));
            continue;
        }
        baseline.insert(line.to_string());
    }
    Ok((baseline, diagnostics))
}

/// Validates undocumented items against the baseline.
///
/// Inputs:
/// - `current`: current undocumented Rust items.
/// - `baseline`: checked-in undocumented-item baseline keys.
///
/// Output:
/// - Diagnostics for new undocumented items and stale baseline entries.
///
/// Transformation:
/// - Treats existing undocumented declarations as migration debt while
///   blocking new undocumented functions or types from entering the tree.
fn check_rustdoc_baseline(
    current: &BTreeMap<String, RustItem>,
    baseline: &BTreeSet<String>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for key in baseline {
        if !current.contains_key(key) {
            diagnostics.push(format!("{key}: stale Rustdoc baseline row"));
        }
    }
    for (key, item) in current {
        if !baseline.contains(key) {
            diagnostics.push(format!(
                "{}:{}: undocumented {} `{}`; add Rustdoc or update reviewed baseline",
                item.path.display(),
                item.line,
                item.kind,
                item.name
            ));
        }
    }
    diagnostics
}

/// Returns the Rustdoc baseline path.
///
/// Inputs:
/// - `root`: repository root.
///
/// Output:
/// - Path to `tools/quality/rustdoc_missing_baseline.tsv`.
///
/// Transformation:
/// - Centralizes the baseline path used by read and write paths.
fn rustdoc_baseline_path(root: &Path) -> PathBuf {
    root.join("tools")
        .join("quality")
        .join("rustdoc_missing_baseline.tsv")
}

/// Reads the file-size quality baseline.
///
/// Inputs:
/// - `root`: repository root containing `tools/quality/rust_file_size_baseline.tsv`.
///
/// Output:
/// - Mapping from repository-relative path to maximum allowed line count.
/// - Diagnostics for malformed rows.
///
/// Transformation:
/// - Parses tab-separated path/count rows into typed baseline values.
fn read_size_baseline(root: &Path) -> QualityResult<(BTreeMap<PathBuf, usize>, Vec<String>)> {
    let path = root
        .join("tools")
        .join("quality")
        .join("rust_file_size_baseline.tsv");
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read baseline: {err}", path.display()))?;
    let mut baseline = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 2 {
            diagnostics.push(format!(
                "{}:{}: expected path<TAB>lines",
                path.display(),
                index + 1
            ));
            continue;
        }
        match fields[1].parse::<usize>() {
            Ok(lines) => {
                baseline.insert(PathBuf::from(fields[0]), lines);
            }
            Err(_) => diagnostics.push(format!(
                "{}:{}: invalid line count `{}`",
                path.display(),
                index + 1,
                fields[1]
            )),
        }
    }
    Ok((baseline, diagnostics))
}

/// Reads the inline-test quality baseline.
///
/// Inputs:
/// - `root`: repository root containing `tools/quality/rust_inline_test_baseline.txt`.
///
/// Output:
/// - Set of repository-relative paths allowed to contain `#[cfg(test)]`.
/// - Diagnostics for malformed rows.
///
/// Transformation:
/// - Parses one path per line while allowing comments and blank lines.
fn read_inline_test_baseline(root: &Path) -> QualityResult<(BTreeSet<PathBuf>, Vec<String>)> {
    let path = root
        .join("tools")
        .join("quality")
        .join("rust_inline_test_baseline.txt");
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
                "{}:{}: expected one path per line",
                path.display(),
                index + 1
            ));
            continue;
        }
        baseline.insert(PathBuf::from(line));
    }
    Ok((baseline, diagnostics))
}

/// Returns whether source text contains non-adjacent inline test config.
///
/// Inputs:
/// - `text`: Rust implementation source.
///
/// Output:
/// - `true` when the file contains an inline `#[cfg(test)]` marker.
/// - `false` when every marker belongs to an adjacent `#[path = "*_test.rs"]`
///   module declaration.
///
/// Transformation:
/// - Scans source lines and treats `#[cfg(test)]` followed by optional blank
///   lines and a `#[path = "..._test.rs"]` attribute as the approved adjacent
///   test-module pattern.
fn has_inline_test_marker(text: &str) -> bool {
    let lines = text.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        if line.trim() != "#[cfg(test)]" {
            continue;
        }
        let mut next_index = index + 1;
        while next_index < lines.len() && lines[next_index].trim().is_empty() {
            next_index += 1;
        }
        if next_index < lines.len() {
            let next_line = lines[next_index].trim();
            if next_line.starts_with("#[path = ") && next_line.contains("_test.rs") {
                continue;
            }
        }
        return true;
    }
    false
}

/// Returns implementation files that contain inline Rust test configuration.
///
/// Inputs:
/// - `root`: repository root.
/// - `files`: measured Rust files.
///
/// Output:
/// - Repository-relative implementation paths containing `#[cfg(test)]`.
///
/// Transformation:
/// - Ignores adjacent `*_test.rs` test modules because those are the required
///   test layout.
/// - Reads implementation Rust source files and allows adjacent path-based
///   test modules while rejecting other inline test configuration markers.
fn files_with_inline_tests(root: &Path, files: &[RustFile]) -> QualityResult<BTreeSet<PathBuf>> {
    let mut paths = BTreeSet::new();
    for file in files {
        if file
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_test.rs"))
        {
            continue;
        }
        let text = fs::read_to_string(root.join(&file.path)).map_err(|err| {
            format!(
                "{}: failed to read source: {err}",
                root.join(&file.path).display()
            )
        })?;
        if has_inline_test_marker(&text) {
            paths.insert(file.path.clone());
        }
    }
    Ok(paths)
}

/// Validates line-count limits against the baseline.
///
/// Inputs:
/// - Current measured Rust files.
/// - Baseline maximum line counts.
///
/// Output:
/// - Diagnostics for new oversized files, baseline growth, and stale baseline
///   rows.
///
/// Transformation:
/// - Enforces hard limits for new files while allowing existing debt only up to
///   the recorded baseline line count.
fn check_file_sizes(files: &[RustFile], baseline: &BTreeMap<PathBuf, usize>) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let current = files
        .iter()
        .map(|file| (file.path.clone(), file))
        .collect::<BTreeMap<_, _>>();

    for path in baseline.keys() {
        if !current.contains_key(path) {
            diagnostics.push(format!("{}: stale file-size baseline row", path.display()));
        }
    }

    for file in files {
        let limit = file.limit();
        if file.lines <= limit {
            continue;
        }
        match baseline.get(&file.path) {
            None => diagnostics.push(format!(
                "{}: {} lines exceeds {limit}; split file or add reviewed baseline",
                file.path.display(),
                file.lines
            )),
            Some(allowed) if file.lines > *allowed => diagnostics.push(format!(
                "{}: {} lines exceeds baseline {allowed}; split before adding code",
                file.path.display(),
                file.lines
            )),
            Some(_) => {}
        }
    }
    diagnostics
}

/// Validates inline test usage against the baseline.
///
/// Inputs:
/// - Current files containing `#[cfg(test)]`.
/// - Baseline files allowed to contain inline tests.
///
/// Output:
/// - Diagnostics for new inline test files and stale baseline rows.
///
/// Transformation:
/// - Prevents new inline-test debt while allowing current debt to be migrated
///   out over time.
fn check_inline_tests(current: &BTreeSet<PathBuf>, baseline: &BTreeSet<PathBuf>) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for path in baseline {
        if !current.contains(path) {
            diagnostics.push(format!(
                "{}: stale inline-test baseline row",
                path.display()
            ));
        }
    }
    for path in current {
        if !baseline.contains(path) {
            diagnostics.push(format!(
                "{}: new inline #[cfg(test)] block; move tests to adjacent *_test.rs",
                path.display()
            ));
        }
    }
    diagnostics
}

/// Renders a named failure block.
///
/// Inputs:
/// - `name`: check label.
/// - `diagnostics`: diagnostic messages.
///
/// Output:
/// - Stable multi-line failure message.
///
/// Transformation:
/// - Preserves the previous Python script's output shape so Makefile and CI
///   logs remain familiar.
fn render_failure(name: &str, diagnostics: &[String]) -> String {
    let mut message = format!("[{name}] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "erlang_modernization_test.rs"]
mod erlang_modernization_test;

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
