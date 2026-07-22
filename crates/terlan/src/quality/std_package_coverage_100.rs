use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::{render_failure, QualityResult};

const RELEASE_API_TESTS: &str = "tests/std/RELEASE_API_TESTS.tsv";
const RELEASE_MANIFEST: &str = "std/RELEASE_MANIFEST.tsv";

const UNCOVERED_RELEASE_MODULE_BASELINE: &[&str] = &[];

/// Summary produced by the std package coverage gate.
///
/// Inputs:
/// - `api_row_count`: number of release API manifest rows checked.
/// - `executable_test_row_count`: number of rows backed by `@test`.
/// - `generated_contract_row_count`: number of generated interface rows backed
///   by unannotated contract functions.
/// - `release_module_count`: number of release-owned std modules checked.
/// - `uncovered_module_baseline_count`: number of known release modules that
///   still need executable coverage rows.
///
/// Output:
/// - Stable success metrics for the quality CLI.
///
/// Transformation:
/// - Keeps manifest-level coverage accounting explicit while the deeper
///   declaration-by-declaration 100% inventory is expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdPackageCoverage100Summary {
    pub api_row_count: usize,
    pub executable_test_row_count: usize,
    pub generated_contract_row_count: usize,
    pub release_module_count: usize,
    pub uncovered_module_baseline_count: usize,
}

/// One row from the std release API test manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseApiTestRow {
    line_no: usize,
    api_id: String,
    test_file: PathBuf,
    test_function: String,
}

/// Runs the std package coverage gate.
///
/// Inputs:
/// - `root`: repository root containing `tests/std/RELEASE_API_TESTS.tsv`.
///
/// Output:
/// - Success when every manifest row points at an existing executable test or
///   generated JS contract function.
/// - Stable diagnostics for stale test names, generated-contract/test
///   category mismatches, malformed rows, duplicate API ids, and missing files.
///
/// Transformation:
/// - Promotes the release API manifest from a shell-only consistency check to a
///   permanent Rust quality gate. This is the first enforced slice of the full
///   std package 100% coverage contract.
pub fn run_std_package_coverage_100(root: &Path) -> QualityResult<StdPackageCoverage100Summary> {
    let rows = read_release_api_tests(root)?;
    let release_modules = read_release_modules(root)?;
    let mut diagnostics = Vec::new();
    diagnostics.extend(check_duplicate_api_ids(&rows));
    diagnostics.extend(check_release_module_coverage(&release_modules, &rows));

    let mut executable_test_row_count = 0;
    let mut generated_contract_row_count = 0;
    for row in &rows {
        match check_release_api_row(root, row) {
            Ok(ReleaseApiRowKind::ExecutableTest) => executable_test_row_count += 1,
            Ok(ReleaseApiRowKind::GeneratedContract) => generated_contract_row_count += 1,
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }

    if !diagnostics.is_empty() {
        return Err(render_failure("std-package-coverage-100", &diagnostics));
    }

    Ok(StdPackageCoverage100Summary {
        api_row_count: rows.len(),
        executable_test_row_count,
        generated_contract_row_count,
        release_module_count: release_modules.len(),
        uncovered_module_baseline_count: UNCOVERED_RELEASE_MODULE_BASELINE.len(),
    })
}

/// Valid manifest row coverage category.
enum ReleaseApiRowKind {
    ExecutableTest,
    GeneratedContract,
}

/// Reads release API manifest rows.
fn read_release_api_tests(root: &Path) -> QualityResult<Vec<ReleaseApiTestRow>> {
    let path = root.join(RELEASE_API_TESTS);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read release API manifest: {err}",
            path.display()
        )
    })?;
    parse_release_api_tests(&text)
}

/// Reads release-owned std module ids.
fn read_release_modules(root: &Path) -> QualityResult<Vec<String>> {
    let path = root.join(RELEASE_MANIFEST);
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read release manifest: {err}", path.display()))?;
    parse_release_modules(&text)
}

/// Parses release-owned std module ids from the release manifest.
fn parse_release_modules(text: &str) -> QualityResult<Vec<String>> {
    let mut modules = Vec::new();
    let mut seen = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_no = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("kind\t") {
            continue;
        }
        let columns = line.split('\t').collect::<Vec<_>>();
        if columns.len() != 6 {
            diagnostics.push(format!(
                "{RELEASE_MANIFEST}:{line_no}: malformed manifest row"
            ));
            continue;
        }
        if columns[0] == "module" {
            if !seen.insert(columns[1].to_string()) {
                diagnostics.push(format!(
                    "{RELEASE_MANIFEST}:{line_no}: duplicate release module `{}`",
                    columns[1]
                ));
            }
            modules.push(columns[1].to_string());
        }
    }
    if diagnostics.is_empty() {
        Ok(modules)
    } else {
        Err(render_failure("std-package-coverage-100", &diagnostics))
    }
}

/// Parses release API manifest text.
fn parse_release_api_tests(text: &str) -> QualityResult<Vec<ReleaseApiTestRow>> {
    let mut rows = Vec::new();
    let mut diagnostics = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_no = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let columns = line.split('\t').collect::<Vec<_>>();
        if columns.len() != 3 || columns.iter().any(|column| column.trim().is_empty()) {
            diagnostics.push(format!(
                "{RELEASE_API_TESTS}:{line_no}: malformed manifest row"
            ));
            continue;
        }
        rows.push(ReleaseApiTestRow {
            line_no,
            api_id: columns[0].to_string(),
            test_file: PathBuf::from(columns[1]),
            test_function: columns[2].to_string(),
        });
    }
    if diagnostics.is_empty() {
        Ok(rows)
    } else {
        Err(render_failure("std-package-coverage-100", &diagnostics))
    }
}

/// Checks one release API manifest row.
fn check_release_api_row(
    root: &Path,
    row: &ReleaseApiTestRow,
) -> Result<ReleaseApiRowKind, String> {
    if !is_adjacent_std_test_file(&row.test_file) {
        return Err(format!(
            "{}:{}: API `{}` test path must be adjacent std `*Test.terl`, got `{}`",
            RELEASE_API_TESTS,
            row.line_no,
            row.api_id,
            row.test_file.display()
        ));
    }

    let full_path = root.join(&row.test_file);
    let text = fs::read_to_string(&full_path).map_err(|err| {
        format!(
            "{}:{}: API `{}` references unreadable test file `{}`: {err}",
            RELEASE_API_TESTS,
            row.line_no,
            row.api_id,
            row.test_file.display()
        )
    })?;

    if is_generated_surface_api(&row.api_id) {
        if has_annotated_test_function(&text, &row.test_function) {
            return Err(format!(
                "{}:{}: generated API `{}` must use an unannotated contract, not `@test`",
                RELEASE_API_TESTS, row.line_no, row.api_id
            ));
        }
        if !has_public_function(&text, &row.test_function) {
            return Err(format!(
                "{}:{}: generated API `{}` is missing contract function `{}` in `{}`",
                RELEASE_API_TESTS,
                row.line_no,
                row.api_id,
                row.test_function,
                row.test_file.display()
            ));
        }
        return Ok(ReleaseApiRowKind::GeneratedContract);
    }

    if is_surface_only_function_name(&row.test_function) {
        return Err(format!(
            "{}:{}: API `{}` points at surface-only test function `{}`",
            RELEASE_API_TESTS, row.line_no, row.api_id, row.test_function
        ));
    }
    if !has_annotated_test_function(&text, &row.test_function) {
        return Err(format!(
            "{}:{}: API `{}` is missing @test function `{}` in `{}`",
            RELEASE_API_TESTS,
            row.line_no,
            row.api_id,
            row.test_function,
            row.test_file.display()
        ));
    }
    Ok(ReleaseApiRowKind::ExecutableTest)
}

/// Checks duplicate API identifiers.
fn check_duplicate_api_ids(rows: &[ReleaseApiTestRow]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut duplicates = Vec::new();
    for row in rows {
        if !seen.insert(row.api_id.clone()) {
            duplicates.push(format!(
                "{}:{}: duplicate API id `{}`",
                RELEASE_API_TESTS, row.line_no, row.api_id
            ));
        }
    }
    duplicates
}

/// Checks release modules against executable coverage rows.
fn check_release_module_coverage(
    release_modules: &[String],
    rows: &[ReleaseApiTestRow],
) -> Vec<String> {
    let baseline = UNCOVERED_RELEASE_MODULE_BASELINE
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let release_module_set = release_modules
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let release_baseline = baseline
        .intersection(&release_module_set)
        .copied()
        .collect::<BTreeSet<_>>();
    let missing = release_modules
        .iter()
        .filter(|module| !module_has_coverage_row(module, rows))
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    let mut diagnostics = Vec::new();
    for module in missing.difference(&release_baseline) {
        diagnostics.push(format!(
            "{RELEASE_MANIFEST}: release module `{module}` has no release API coverage row"
        ));
    }
    for module in release_baseline.difference(&missing) {
        diagnostics.push(format!(
            "{RELEASE_MANIFEST}: baseline module `{module}` now has coverage; remove it from `UNCOVERED_RELEASE_MODULE_BASELINE`"
        ));
    }
    diagnostics
}

/// Returns whether a release module has at least one matching API row.
fn module_has_coverage_row(module: &str, rows: &[ReleaseApiTestRow]) -> bool {
    rows.iter()
        .any(|row| row.api_id == module || row.api_id.starts_with(&format!("{module}.")))
}

/// Returns whether a path is an adjacent std test source path.
fn is_adjacent_std_test_file(path: &Path) -> bool {
    let Some(text) = path.to_str() else {
        return false;
    };
    text.starts_with("std/")
        && text.ends_with("Test.terl")
        && !text.contains("/../")
        && !text.contains("/negative/")
}

/// Returns whether a row is a generated JS surface contract row.
fn is_generated_surface_api(api_id: &str) -> bool {
    api_id.starts_with("std.js.") && api_id.ends_with(".generated_surface")
}

/// Returns whether a function name is a forbidden surface-only test name.
fn is_surface_only_function_name(name: &str) -> bool {
    name.contains("_surface_")
        || name.ends_with("_is_declared")
        || name == "generated_surface_is_declared"
        || name == "generated_binding_surface_exists"
}

/// Returns whether a file contains an `@test`-annotated public function.
fn has_annotated_test_function(text: &str, function_name: &str) -> bool {
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

/// Returns whether a file contains a public function.
fn has_public_function(text: &str, function_name: &str) -> bool {
    text.lines()
        .map(str::trim)
        .any(|line| starts_public_function(line, function_name))
}

/// Returns whether a trimmed line starts `pub function_name(`.
fn starts_public_function(line: &str, function_name: &str) -> bool {
    line.strip_prefix("pub ").is_some_and(|tail| {
        tail.starts_with(function_name) && tail[function_name.len()..].starts_with('(')
    })
}

#[cfg(test)]
#[path = "std_package_coverage_100_test.rs"]
mod std_package_coverage_100_test;
