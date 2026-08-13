use std::path::Path;

use super::parse_lint_source;
use crate::commands::lint::diagnostic::{LintDiagnostic, Severity};
use crate::terlan_syntax::syntax_output::SyntaxAnnotationOutput;
use crate::terlan_syntax::{
    SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput, SyntaxFunctionClauseOutput,
};

use property::{has_property_runner_call, is_property_candidate_name};

mod property;

const FAKE_TEST_RULE_ID: &str = "TL0401";
const FAKE_TEST_RULE_NAME: &str = "tests.fake";
const TEST_ASSERTION_VOLUME_RULE_ID: &str = "TL0402";
const TEST_ASSERTION_VOLUME_RULE_NAME: &str = "tests.assertion-volume";
const TEST_TABLE_CANDIDATE_RULE_ID: &str = "TL0403";
const TEST_TABLE_CANDIDATE_RULE_NAME: &str = "tests.table-driven-candidate";
const TEST_PROPERTY_CANDIDATE_RULE_ID: &str = "TL0404";
const TEST_PROPERTY_CANDIDATE_RULE_NAME: &str = "tests.property-candidate";
const MAX_FOCUSED_TEST_ASSERTIONS: usize = 3;
const MIN_TABLE_CANDIDATE_ASSERT_EQUALS: usize = 3;

/// Builds diagnostics for fake test assertions that do not prove behavior.
pub(super) fn fake_test_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    if !is_test_source_path(path) {
        return Vec::new();
    }

    let Ok(module) = parse_lint_source(path, source) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    for declaration in module.declarations {
        if !is_test_declaration(&declaration.annotations) {
            continue;
        }
        let declaration_start = declaration.span.start;
        match declaration.payload {
            SyntaxDeclarationPayload::Function { name, clauses, .. } => {
                collect_declaration_diagnostics(
                    path,
                    source,
                    &name,
                    &clauses,
                    declaration_start,
                    &mut diagnostics,
                );
            }
            SyntaxDeclarationPayload::Method { name, clauses, .. } => {
                collect_declaration_diagnostics(
                    path,
                    source,
                    &name,
                    &clauses,
                    declaration_start,
                    &mut diagnostics,
                );
            }
            _ => {}
        }
    }
    diagnostics
}

/// Collects fake-test and assertion-volume diagnostics for one test declaration.
fn collect_declaration_diagnostics(
    path: &Path,
    source: &str,
    name: &str,
    clauses: &[SyntaxFunctionClauseOutput],
    declaration_start: usize,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if let Some(message) = declaration_only_test_name_message(name) {
        push_fake_test_diagnostic(path, source, declaration_start, message, diagnostics);
    }
    let assertion_count = clauses
        .iter()
        .map(|clause| count_assertion_calls(&clause.body))
        .sum::<usize>();
    if assertion_count > MAX_FOCUSED_TEST_ASSERTIONS {
        push_assertion_volume_diagnostic(path, source, declaration_start, diagnostics);
    }
    let assert_equal_count = clauses
        .iter()
        .map(|clause| count_assert_equal_calls(&clause.body))
        .sum::<usize>();
    if assert_equal_count >= MIN_TABLE_CANDIDATE_ASSERT_EQUALS {
        push_table_candidate_diagnostic(path, source, declaration_start, diagnostics);
    }
    if is_property_candidate_name(name)
        && !clauses
            .iter()
            .any(|clause| has_property_runner_call(&clause.body))
    {
        push_property_candidate_diagnostic(path, source, declaration_start, diagnostics);
    }
    for clause in clauses {
        collect_fake_test_diagnostics(path, source, &clause.body, true, diagnostics);
    }
}

/// Returns the fake-test message for declaration-only test names.
fn declaration_only_test_name_message(name: &str) -> Option<&'static str> {
    let declaration_only_names = [
        "surface_is_declared",
        "generated_surface_is_declared",
        "is_declared",
    ];
    if declaration_only_names.contains(&name)
        || name.ends_with("_is_declared")
        || name.ends_with("_surface_is_declared")
    {
        return Some("declaration-only test name does not prove behavior");
    }
    None
}

/// Returns whether a declaration carries the marker `@test` annotation.
fn is_test_declaration(annotations: &[SyntaxAnnotationOutput]) -> bool {
    annotations.iter().any(|annotation| {
        annotation.path.len() == 1 && annotation.path.first().is_some_and(|name| name == "test")
    })
}

/// Returns whether a file should receive test-only lint rules.
fn is_test_source_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("Test.terl") || name.ends_with("Test.terli"))
}

/// Recursively collects fake-test diagnostics from one expression.
fn collect_fake_test_diagnostics(
    path: &Path,
    source: &str,
    expr: &SyntaxExprOutput,
    allow_literal_body: bool,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if let Some(message) = fake_test_message(expr, allow_literal_body) {
        push_fake_test_diagnostic(path, source, expr.span.start, message, diagnostics);
    }

    for child in &expr.children {
        collect_fake_test_diagnostics(path, source, child, false, diagnostics);
    }
    for field in &expr.fields {
        collect_fake_test_diagnostics(path, source, &field.value, false, diagnostics);
    }
    for clause in &expr.clauses {
        if let Some(guard) = &clause.guard {
            collect_fake_test_diagnostics(path, source, guard, false, diagnostics);
        }
        collect_fake_test_diagnostics(path, source, &clause.body, false, diagnostics);
    }
    for clause in &expr.catch_clauses {
        if let Some(guard) = &clause.guard {
            collect_fake_test_diagnostics(path, source, guard, false, diagnostics);
        }
        collect_fake_test_diagnostics(path, source, &clause.body, false, diagnostics);
    }
    if let Some(after) = &expr.try_after {
        collect_fake_test_diagnostics(path, source, &after.trigger, false, diagnostics);
        collect_fake_test_diagnostics(path, source, &after.body, false, diagnostics);
    }
}

/// Adds one fake-test diagnostic at a source offset.
fn push_fake_test_diagnostic(
    path: &Path,
    source: &str,
    offset: usize,
    message: &'static str,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let (line, column) = source_line_column_at(source, offset);
    diagnostics.push(LintDiagnostic {
        path: path.to_path_buf(),
        line,
        column,
        rule_id: FAKE_TEST_RULE_ID,
        rule_name: FAKE_TEST_RULE_NAME,
        severity: Severity::Error,
        message,
        fix_available: false,
    });
}

/// Adds one oversized-test diagnostic at a source offset.
fn push_assertion_volume_diagnostic(
    path: &Path,
    source: &str,
    offset: usize,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let (line, column) = source_line_column_at(source, offset);
    diagnostics.push(LintDiagnostic {
        path: path.to_path_buf(),
        line,
        column,
        rule_id: TEST_ASSERTION_VOLUME_RULE_ID,
        rule_name: TEST_ASSERTION_VOLUME_RULE_NAME,
        severity: Severity::Suggestion,
        message: "test has too many assertions; split it or use a table-driven test",
        fix_available: false,
    });
}

/// Adds one table-driven-test candidate diagnostic at a source offset.
fn push_table_candidate_diagnostic(
    path: &Path,
    source: &str,
    offset: usize,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let (line, column) = source_line_column_at(source, offset);
    diagnostics.push(LintDiagnostic {
        path: path.to_path_buf(),
        line,
        column,
        rule_id: TEST_TABLE_CANDIDATE_RULE_ID,
        rule_name: TEST_TABLE_CANDIDATE_RULE_NAME,
        severity: Severity::Suggestion,
        message: "repeated assert_equal rows should move to a table-driven test",
        fix_available: false,
    });
}

/// Adds one property-test candidate diagnostic at a source offset.
fn push_property_candidate_diagnostic(
    path: &Path,
    source: &str,
    offset: usize,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let (line, column) = source_line_column_at(source, offset);
    diagnostics.push(LintDiagnostic {
        path: path.to_path_buf(),
        line,
        column,
        rule_id: TEST_PROPERTY_CANDIDATE_RULE_ID,
        rule_name: TEST_PROPERTY_CANDIDATE_RULE_NAME,
        severity: Severity::Suggestion,
        message: "property-shaped test should use std.test.Gen property runners",
        fix_available: false,
    });
}

/// Returns the fake-test diagnostic message for one expression, if any.
fn fake_test_message(expr: &SyntaxExprOutput, allow_literal_body: bool) -> Option<&'static str> {
    if allow_literal_body && is_true_literal(expr) {
        return Some("literal true test body does not prove behavior");
    }
    if is_literal_true_assertion(expr) {
        return Some("literal true assertion does not prove behavior");
    }
    if is_identity_assertion(expr) {
        return Some("identity assertion does not prove behavior");
    }
    None
}

/// Counts assertion calls in one expression tree.
fn count_assertion_calls(expr: &SyntaxExprOutput) -> usize {
    let self_count = usize::from(
        assertion_call_args(expr, "assert", 1).is_some()
            || assertion_call_args(expr, "assert_equal", 2).is_some(),
    );

    self_count
        + expr
            .children
            .iter()
            .map(count_assertion_calls)
            .sum::<usize>()
        + expr
            .fields
            .iter()
            .map(|field| count_assertion_calls(&field.value))
            .sum::<usize>()
        + expr
            .clauses
            .iter()
            .map(|clause| {
                clause
                    .guard
                    .as_ref()
                    .map_or(0, |guard| count_assertion_calls(guard))
                    + count_assertion_calls(&clause.body)
            })
            .sum::<usize>()
        + expr
            .catch_clauses
            .iter()
            .map(|clause| {
                clause
                    .guard
                    .as_ref()
                    .map_or(0, |guard| count_assertion_calls(guard))
                    + count_assertion_calls(&clause.body)
            })
            .sum::<usize>()
        + expr.try_after.as_ref().map_or(0, |after| {
            count_assertion_calls(&after.trigger) + count_assertion_calls(&after.body)
        })
}

/// Counts equality assertion rows in one expression tree.
fn count_assert_equal_calls(expr: &SyntaxExprOutput) -> usize {
    let self_count = usize::from(assertion_call_args(expr, "assert_equal", 2).is_some());

    self_count
        + expr
            .children
            .iter()
            .map(count_assert_equal_calls)
            .sum::<usize>()
        + expr
            .fields
            .iter()
            .map(|field| count_assert_equal_calls(&field.value))
            .sum::<usize>()
        + expr
            .clauses
            .iter()
            .map(|clause| {
                clause
                    .guard
                    .as_ref()
                    .map_or(0, |guard| count_assert_equal_calls(guard))
                    + count_assert_equal_calls(&clause.body)
            })
            .sum::<usize>()
        + expr
            .catch_clauses
            .iter()
            .map(|clause| {
                clause
                    .guard
                    .as_ref()
                    .map_or(0, |guard| count_assert_equal_calls(guard))
                    + count_assert_equal_calls(&clause.body)
            })
            .sum::<usize>()
        + expr.try_after.as_ref().map_or(0, |after| {
            count_assert_equal_calls(&after.trigger) + count_assert_equal_calls(&after.body)
        })
}

/// Returns whether a call is `assert(true)`.
fn is_literal_true_assertion(expr: &SyntaxExprOutput) -> bool {
    let Some(args) = assertion_call_args(expr, "assert", 1) else {
        return false;
    };
    let Some(value) = args.first() else {
        return false;
    };
    is_true_literal(value)
}

/// Returns whether an expression is the literal truth value.
fn is_true_literal(expr: &SyntaxExprOutput) -> bool {
    matches!(expr.kind, SyntaxExprKind::Atom | SyntaxExprKind::Var)
        && expr.text.as_deref() == Some("true")
}

/// Returns whether a call is `assert_equal(x, x)`.
fn is_identity_assertion(expr: &SyntaxExprOutput) -> bool {
    let Some(args) = assertion_call_args(expr, "assert_equal", 2) else {
        return false;
    };

    let Some(left) = args.first().and_then(identity_expr_key) else {
        return false;
    };
    let Some(right) = args.get(1).and_then(identity_expr_key) else {
        return false;
    };
    left == right
}

/// Returns assertion call arguments for selected or fully qualified std tests.
fn assertion_call_args<'a>(
    expr: &'a SyntaxExprOutput,
    name: &str,
    arity: usize,
) -> Option<&'a [SyntaxExprOutput]> {
    if expr.kind != SyntaxExprKind::Call || expr.children.len() != arity + 1 {
        return None;
    }
    let callee = expr.children.first()?;
    if callee.text.as_deref() != Some(name) {
        return None;
    }
    if expr
        .remote
        .as_deref()
        .is_some_and(|remote| remote != "std.test.Test")
    {
        return None;
    }
    Some(&expr.children[1..])
}

/// Returns a source-span-independent key for fake-test expression comparison.
fn identity_expr_key(expr: &SyntaxExprOutput) -> Option<String> {
    let mut key = format!(
        "{:?}:{:?}:{:?}:{:?}",
        expr.kind, expr.text, expr.operator, expr.remote
    );
    if !expr.type_args.is_empty() || expr.arg_names.iter().any(Option::is_some) {
        return None;
    }
    for child in &expr.children {
        key.push('(');
        key.push_str(&identity_expr_key(child)?);
        key.push(')');
    }
    for field in &expr.fields {
        key.push('{');
        key.push_str(&field.key);
        key.push('=');
        key.push_str(&identity_expr_key(&field.value)?);
        key.push('}');
    }
    Some(key)
}

/// Returns the one-based line and column for a source byte offset.
fn source_line_column_at(source: &str, index: usize) -> (usize, usize) {
    let prefix = &source[..index];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.len(), |(_, tail)| tail.len())
        + 1;
    (line, column)
}
