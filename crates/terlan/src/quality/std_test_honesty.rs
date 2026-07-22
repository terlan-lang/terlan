use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_syntax::{
    parse_module_as_syntax_output, SyntaxDeclarationOutput, SyntaxDeclarationPayload,
    SyntaxExprKind, SyntaxExprOutput, SyntaxFunctionClauseOutput,
};

use crate::terlan_quality::{render_failure, QualityResult};

/// Summary produced by the std test honesty gate.
///
/// Inputs:
/// - Canonical std test files discovered under `std`.
///
/// Output:
/// - Counts used by the quality CLI success message.
///
/// Transformation:
/// - Keeps file discovery and structured fake-test detection separate from
///   diagnostic rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdTestHonestySummary {
    pub checked_file_count: usize,
    pub checked_test_count: usize,
}

/// One fake-test finding in a Terlan std test file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StdTestHonestyFinding {
    path: PathBuf,
    test_name: String,
    reason: String,
}

/// Runs the std test honesty gate.
///
/// Inputs:
/// - `root`: repository root containing the `std` directory.
///
/// Output:
/// - Success summary when all discovered std tests have executable behavior.
/// - Stable diagnostics naming fake test patterns.
///
/// Transformation:
/// - Parses each Terlan test module through the formal syntax-output pipeline
///   and inspects `@test` function bodies structurally instead of using source
///   regexes as the gate.
pub fn run_std_test_honesty(root: &Path) -> QualityResult<StdTestHonestySummary> {
    run_std_test_honesty_for_dir(root, Path::new("std"))
}

/// Runs std test honesty against one repository-relative directory.
fn run_std_test_honesty_for_dir(
    root: &Path,
    relative_dir: &Path,
) -> QualityResult<StdTestHonestySummary> {
    let files = collect_std_test_files(root, relative_dir)?;
    let mut checked_test_count = 0;
    let mut diagnostics = Vec::new();

    for path in &files {
        let text = fs::read_to_string(root.join(path))
            .map_err(|err| format!("{}: failed to read test file: {err}", path.display()))?;
        let module = parse_module_as_syntax_output(&text)
            .map_err(|err| format!("{}: failed to parse test module: {err:?}", path.display()))?;
        for declaration in &module.declarations {
            if !has_test_annotation(declaration) {
                continue;
            }
            checked_test_count += 1;
            diagnostics.extend(
                find_fake_test(path, declaration)
                    .into_iter()
                    .map(|finding| {
                        format!(
                            "{}: @test `{}` is fake: {}",
                            finding.path.display(),
                            finding.test_name,
                            finding.reason
                        )
                    }),
            );
        }
    }

    if !diagnostics.is_empty() {
        return Err(render_failure("std-test-honesty", &diagnostics));
    }

    Ok(StdTestHonestySummary {
        checked_file_count: files.len(),
        checked_test_count,
    })
}

/// Finds canonical Terlan std test files below one directory.
fn collect_std_test_files(root: &Path, relative_dir: &Path) -> QualityResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_std_test_files_in_dir(root, relative_dir, &mut files)?;
    files.sort();
    Ok(files)
}

/// Recursively collects Terlan test files in one directory.
fn collect_std_test_files_in_dir(
    root: &Path,
    relative_dir: &Path,
    files: &mut Vec<PathBuf>,
) -> QualityResult<()> {
    let full_path = root.join(relative_dir);
    for entry in fs::read_dir(&full_path).map_err(|err| {
        format!(
            "{}: failed to read directory: {err}",
            relative_dir.display()
        )
    })? {
        let entry = entry.map_err(|err| {
            format!(
                "{}: failed to read directory entry: {err}",
                relative_dir.display()
            )
        })?;
        let child = relative_dir.join(entry.file_name());
        let child_full_path = root.join(&child);
        if child_full_path.is_dir() {
            collect_std_test_files_in_dir(root, &child, files)?;
        } else if child_full_path.is_file() && is_std_test_file(&child) {
            files.push(child);
        }
    }
    Ok(())
}

/// Returns whether a path uses the canonical or compatibility test suffix.
fn is_std_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("Test.terl") || name.ends_with("_Test.terl"))
}

/// Finds fake-test diagnostics for one annotated declaration.
fn find_fake_test(
    path: &Path,
    declaration: &SyntaxDeclarationOutput,
) -> Vec<StdTestHonestyFinding> {
    let SyntaxDeclarationPayload::Function { name, clauses, .. } = &declaration.payload else {
        return vec![finding(
            path,
            "<non-function>",
            "@test can only mark executable test functions",
        )];
    };

    let mut findings = Vec::new();
    if fake_surface_test_name(name) {
        findings.push(finding(
            path,
            name,
            "test name declares only compile-time surface presence",
        ));
    }
    if let Some(body) = single_test_body(clauses) {
        if literal_bool_or_trivial_bool_conjunction(body).is_some() {
            findings.push(finding(
                path,
                name,
                "body returns a literal boolean instead of exercising behavior",
            ));
        }
        if is_assert_true(body) {
            findings.push(finding(path, name, "body directly asserts true"));
        }
        if is_assert_false_false(body) {
            findings.push(finding(path, name, "body directly asserts false as false"));
        }
        if is_assert_equal_identity(body) {
            findings.push(finding(
                path,
                name,
                "body compares syntactically identical expected and actual expressions",
            ));
        }
        for reason in table_fake_reasons(body) {
            findings.push(finding(path, name, &reason));
        }
    }
    findings
}

/// Builds one fake-test finding.
fn finding(path: &Path, test_name: &str, reason: &str) -> StdTestHonestyFinding {
    StdTestHonestyFinding {
        path: path.to_path_buf(),
        test_name: test_name.to_string(),
        reason: reason.to_string(),
    }
}

/// Returns a single unguarded test body when the function shape is simple.
fn single_test_body(clauses: &[SyntaxFunctionClauseOutput]) -> Option<&SyntaxExprOutput> {
    let [clause] = clauses else {
        return None;
    };
    if clause.has_guard || clause.guard.is_some() || !clause.patterns.is_empty() {
        return None;
    }
    Some(&clause.body)
}

/// Returns whether a declaration carries the source-level `@test` annotation.
fn has_test_annotation(declaration: &SyntaxDeclarationOutput) -> bool {
    declaration
        .annotations
        .iter()
        .any(|annotation| annotation.path.as_slice() == ["test"])
}

/// Returns whether a test name denotes compile-surface conformance only.
fn fake_surface_test_name(name: &str) -> bool {
    name.starts_with("accepts_")
        || name.contains("_surface_")
        || name.ends_with("_is_declared")
        || name == "generated_surface_is_declared"
        || name == "generated_binding_surface_exists"
}

/// Detects literal booleans and trivial boolean conjunctions.
fn literal_bool_or_trivial_bool_conjunction(expr: &SyntaxExprOutput) -> Option<bool> {
    match expr.kind {
        SyntaxExprKind::Atom | SyntaxExprKind::Var => match expr.text.as_deref() {
            Some("true") | Some("True") => Some(true),
            Some("false") | Some("False") => Some(false),
            _ => None,
        },
        SyntaxExprKind::BinaryOp if is_boolean_conjunction(expr.operator.as_deref()) => {
            let [left, right] = expr.children.as_slice() else {
                return None;
            };
            match (
                literal_bool_or_trivial_bool_conjunction(left),
                literal_bool_or_trivial_bool_conjunction(right),
            ) {
                (Some(left), Some(right)) => Some(left && right),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Returns whether a binary operator is a boolean conjunction spelling.
fn is_boolean_conjunction(operator: Option<&str>) -> bool {
    matches!(operator, Some("&&") | Some("and"))
}

/// Returns whether an expression is `assert(true)`.
fn is_assert_true(expr: &SyntaxExprOutput) -> bool {
    call_name(expr).is_some_and(|name| name == "assert")
        && expr
            .children
            .get(1)
            .and_then(literal_bool_or_trivial_bool_conjunction)
            == Some(true)
}

/// Returns whether an expression is `assert_false(false)`.
fn is_assert_false_false(expr: &SyntaxExprOutput) -> bool {
    call_name(expr).is_some_and(|name| name == "assert_false")
        && expr
            .children
            .get(1)
            .and_then(literal_bool_or_trivial_bool_conjunction)
            == Some(false)
}

/// Returns whether an expression is `assert_equal(x, x)`.
fn is_assert_equal_identity(expr: &SyntaxExprOutput) -> bool {
    call_name(expr).is_some_and(|name| name == "assert_equal")
        && expr.children.len() == 3
        && expr.children[1] == expr.children[2]
}

/// Returns table-specific fake-test reasons for one test body.
fn table_fake_reasons(expr: &SyntaxExprOutput) -> Vec<String> {
    let Some(name) = call_name(expr) else {
        return Vec::new();
    };
    if !matches!(name, "each" | "each_result") || expr.children.len() != 3 {
        return Vec::new();
    }

    let mut reasons = Vec::new();
    let rows = &expr.children[1];
    let callback = &expr.children[2];
    if table_rows_are_empty(rows) {
        reasons.push("table-driven test has zero rows".to_string());
    }
    if let Some(duplicates) = duplicate_row_names(rows) {
        reasons.push(format!("table-driven test repeats row name `{duplicates}`"));
    }
    if callback_returns_literal_true(callback) {
        reasons.push("table callback returns literal true".to_string());
    }
    if callback_contains_identity_assertion(callback) {
        reasons.push("table callback contains an identity assertion".to_string());
    }
    reasons
}

/// Returns whether a table row source is visibly empty.
fn table_rows_are_empty(expr: &SyntaxExprOutput) -> bool {
    match expr.kind {
        SyntaxExprKind::List => expr.children.is_empty(),
        SyntaxExprKind::Call => match call_name(expr) {
            Some("List") => expr.children.len() == 1,
            Some("cases") => expr.children.get(1).is_some_and(table_rows_are_empty),
            _ => false,
        },
        _ => false,
    }
}

/// Returns the first duplicate row name when row literals are visible.
fn duplicate_row_names(expr: &SyntaxExprOutput) -> Option<String> {
    let mut names = Vec::new();
    collect_row_names(expr, &mut names);
    for (index, name) in names.iter().enumerate() {
        if names.iter().skip(index + 1).any(|other| other == name) {
            return Some(name.clone());
        }
    }
    None
}

/// Collects visible row names from literal table row expressions.
fn collect_row_names(expr: &SyntaxExprOutput, names: &mut Vec<String>) {
    match expr.kind {
        SyntaxExprKind::List => {
            for child in &expr.children {
                collect_row_names(child, names);
            }
        }
        SyntaxExprKind::Tuple => {
            if let Some(name) = expr.children.first().and_then(binary_literal_text) {
                names.push(name.to_string());
            }
        }
        SyntaxExprKind::Call => match call_name(expr) {
            Some("List") => {
                for child in expr.children.iter().skip(1) {
                    collect_row_names(child, names);
                }
            }
            Some("cases") => {
                if let Some(rows) = expr.children.get(1) {
                    collect_row_names(rows, names);
                }
            }
            Some("row") => {
                if let Some(name) = expr.children.get(1).and_then(binary_literal_text) {
                    names.push(name.to_string());
                }
            }
            _ => {}
        },
        _ => {}
    }
}

/// Returns a binary literal's text payload.
fn binary_literal_text(expr: &SyntaxExprOutput) -> Option<&str> {
    if expr.kind == SyntaxExprKind::Binary {
        expr.text.as_deref()
    } else {
        None
    }
}

/// Returns whether a callback always returns literal true.
fn callback_returns_literal_true(expr: &SyntaxExprOutput) -> bool {
    callback_body(expr).and_then(literal_bool_or_trivial_bool_conjunction) == Some(true)
}

/// Returns whether a callback body contains a known fake assertion shape.
fn callback_contains_identity_assertion(expr: &SyntaxExprOutput) -> bool {
    callback_body(expr).is_some_and(expr_contains_fake_assertion)
}

/// Returns whether an expression tree contains a known fake assertion shape.
fn expr_contains_fake_assertion(expr: &SyntaxExprOutput) -> bool {
    is_assert_true(expr)
        || is_assert_false_false(expr)
        || is_assert_equal_identity(expr)
        || expr.children.iter().any(expr_contains_fake_assertion)
        || expr
            .clauses
            .iter()
            .any(|clause| expr_contains_fake_assertion(&clause.body))
        || expr
            .catch_clauses
            .iter()
            .any(|clause| expr_contains_fake_assertion(&clause.body))
        || expr
            .try_after
            .as_ref()
            .is_some_and(|try_after| expr_contains_fake_assertion(&try_after.body))
}

/// Returns the body of a simple function-value callback.
fn callback_body(expr: &SyntaxExprOutput) -> Option<&SyntaxExprOutput> {
    if expr.kind != SyntaxExprKind::Fun {
        return None;
    }
    let [clause] = expr.clauses.as_slice() else {
        return None;
    };
    Some(&clause.body)
}

/// Returns the final callable name for a call expression.
fn call_name(expr: &SyntaxExprOutput) -> Option<&str> {
    if expr.kind != SyntaxExprKind::Call {
        return None;
    }
    expr.children.first().and_then(expr_name)
}

/// Returns the final name segment represented by an expression.
fn expr_name(expr: &SyntaxExprOutput) -> Option<&str> {
    match expr.kind {
        SyntaxExprKind::Var | SyntaxExprKind::Atom => expr.text.as_deref(),
        SyntaxExprKind::FieldAccess | SyntaxExprKind::RecordAccess => expr.text.as_deref(),
        _ => None,
    }
}

#[cfg(test)]
#[path = "std_test_honesty_test.rs"]
mod std_test_honesty_test;
