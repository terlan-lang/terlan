use std::path::Path;

use super::parse_lint_source;
use crate::terlan_syntax::{
    SyntaxClauseOutput, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput,
    SyntaxPatternKind, SyntaxPatternOutput,
};

use crate::commands::lint::diagnostic::{LintDiagnostic, Severity};

mod branch;

const DEEP_EXPRESSION_RULE_ID: &str = "TL0002";
const DEEP_EXPRESSION_RULE_NAME: &str = "readability.deep-expression";
const CALLBACK_NAME_RULE_ID: &str = "TL0003";
const CALLBACK_NAME_RULE_NAME: &str = "readability.callback-name";
const UNUSED_DESTRUCTURE_BINDING_RULE_ID: &str = "TL0004";
const UNUSED_DESTRUCTURE_BINDING_RULE_NAME: &str = "readability.unused-destructure-binding";
const REDUNDANT_COMMENT_RULE_ID: &str = "TL0005";
const REDUNDANT_COMMENT_RULE_NAME: &str = "readability.redundant-comment";
const PUBLIC_DOCS_RULE_ID: &str = "TL0006";
const PUBLIC_DOCS_RULE_NAME: &str = "readability.public-docs";
const DOC_COMMENT_SPACING_RULE_ID: &str = "TL0007";
const DOC_COMMENT_SPACING_RULE_NAME: &str = "readability.doc-comment-spacing";
const GROUPED_BINDING_RULE_ID: &str = "TL0009";
const GROUPED_BINDING_RULE_NAME: &str = "readability.grouped-binding";
const FUNCTION_REFERENCE_RULE_ID: &str = "TL0010";
const FUNCTION_REFERENCE_RULE_NAME: &str = "readability.function-reference";
const MAX_EXPRESSION_DEPTH: usize = 8;

/// Builds diagnostics for branch conditions with too many boolean operators.
pub(super) fn boolean_heavy_branch_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    branch::boolean_heavy_branch_diagnostics(path, source)
}

/// Builds diagnostics for expression trees that are too deeply nested.
pub(super) fn deep_expression_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    if is_property_test_source_path(path) {
        return Vec::new();
    }

    let Ok(module) = parse_lint_source(path, source) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    for declaration in module.declarations {
        match declaration.payload {
            SyntaxDeclarationPayload::Function { clauses, .. }
            | SyntaxDeclarationPayload::Method { clauses, .. } => {
                for clause in clauses {
                    if let Some(expr) = first_deep_expression(&clause.body, 1) {
                        let (line, column) = source_line_column_at(source, expr.span.start);
                        diagnostics.push(LintDiagnostic {
                            path: path.to_path_buf(),
                            line,
                            column,
                            rule_id: DEEP_EXPRESSION_RULE_ID,
                            rule_name: DEEP_EXPRESSION_RULE_NAME,
                            severity: Severity::Warning,
                            message: "deep expression tree should be split with let bindings, a pipe, or a named helper",
                            fix_available: false,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    diagnostics
}

/// Builds diagnostics for linear nested cases that repeat one fallback.
pub(super) fn grouped_binding_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    selected_binding_diagnostics(path, source, true, false)
}

/// Builds diagnostics for lambdas that only forward their parameters.
pub(super) fn function_reference_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    selected_binding_diagnostics(path, source, false, true)
}

/// Builds grouped-binding and forwarding-lambda diagnostics from one parse.
pub(super) fn grouped_binding_and_function_reference_diagnostics(
    path: &Path,
    source: &str,
) -> Vec<LintDiagnostic> {
    selected_binding_diagnostics(path, source, true, true)
}

fn selected_binding_diagnostics(
    path: &Path,
    source: &str,
    grouped_binding: bool,
    function_reference: bool,
) -> Vec<LintDiagnostic> {
    let Ok(module) = parse_lint_source(path, source) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    for declaration in module.declarations {
        match declaration.payload {
            SyntaxDeclarationPayload::Function { clauses, .. }
            | SyntaxDeclarationPayload::Method { clauses, .. } => {
                for clause in clauses {
                    if grouped_binding {
                        collect_grouped_binding_diagnostics(
                            path,
                            source,
                            &clause.body,
                            &mut diagnostics,
                        );
                    }
                    if function_reference {
                        collect_function_reference_diagnostics(
                            path,
                            source,
                            &clause.body,
                            &mut diagnostics,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    diagnostics
}

fn collect_function_reference_diagnostics(
    path: &Path,
    source: &str,
    expr: &SyntaxExprOutput,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if is_forwarding_lambda(expr) {
        let (line, column) = source_line_column_at(source, expr.span.start);
        diagnostics.push(LintDiagnostic {
            path: path.to_path_buf(),
            line,
            column,
            rule_id: FUNCTION_REFERENCE_RULE_ID,
            rule_name: FUNCTION_REFERENCE_RULE_NAME,
            severity: Severity::Error,
            message:
                "a lambda that only forwards its parameters should be a direct function reference",
            fix_available: false,
        });
        return;
    }
    for child in expression_children(expr) {
        collect_function_reference_diagnostics(path, source, child, diagnostics);
    }
}

fn is_forwarding_lambda(expr: &SyntaxExprOutput) -> bool {
    if expr.kind != SyntaxExprKind::Fun || expr.clauses.len() != 1 {
        return false;
    }
    let clause = &expr.clauses[0];
    if clause.guard.is_some()
        || !matches!(
            clause.body.kind,
            SyntaxExprKind::Call | SyntaxExprKind::FunctionCall
        )
        || !clause.body.type_args.is_empty()
        || clause.body.arg_names.iter().any(Option::is_some)
    {
        return false;
    }
    let Some((callee, arguments)) = clause.body.children.split_first() else {
        return false;
    };
    if callee.kind != SyntaxExprKind::Var || clause.patterns.len() != arguments.len() {
        return false;
    }
    clause
        .patterns
        .iter()
        .zip(arguments)
        .all(|(pattern, argument)| {
            pattern.kind == SyntaxPatternKind::Var
                && argument.kind == SyntaxExprKind::Var
                && pattern.text == argument.text
        })
}

fn collect_grouped_binding_diagnostics(
    path: &Path,
    source: &str,
    expr: &SyntaxExprOutput,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if repeated_fallback_case_pair(expr) {
        let (line, column) = source_line_column_at(source, expr.span.start);
        diagnostics.push(LintDiagnostic {
            path: path.to_path_buf(),
            line,
            column,
            rule_id: GROUPED_BINDING_RULE_ID,
            rule_name: GROUPED_BINDING_RULE_NAME,
            severity: Severity::Error,
            message: "linear nested cases with one repeated fallback should use grouped `let { ... } else { ... }` bindings",
            fix_available: false,
        });
        return;
    }
    for child in expression_children(expr) {
        collect_grouped_binding_diagnostics(path, source, child, diagnostics);
    }
}

fn repeated_fallback_case_pair(expr: &SyntaxExprOutput) -> bool {
    if expr.kind != SyntaxExprKind::Case || expr.clauses.len() != 2 {
        return false;
    }
    for nested_index in 0..2 {
        if expr.clauses[nested_index].guard.is_some()
            || expr.clauses[1 - nested_index].guard.is_some()
        {
            continue;
        }
        let nested = expr.clauses[nested_index].body.as_ref();
        if nested.kind != SyntaxExprKind::Case || nested.clauses.len() != 2 {
            continue;
        }
        let fallback = expr.clauses[1 - nested_index].body.as_ref();
        if nested.clauses.iter().any(|clause| {
            clause.guard.is_none()
                && !expression_reads_pattern_binding(&clause.body, &clause.patterns)
                && expressions_equal_ignoring_spans(fallback, &clause.body)
        }) {
            return true;
        }
    }
    false
}

fn expression_reads_pattern_binding(
    expr: &SyntaxExprOutput,
    patterns: &[SyntaxPatternOutput],
) -> bool {
    let mut names = Vec::new();
    for pattern in patterns {
        collect_pattern_variable_names(pattern, &mut names);
    }
    expression_reads_any_name(expr, &names)
}

fn expression_reads_any_name(expr: &SyntaxExprOutput, names: &[&str]) -> bool {
    (expr.kind == SyntaxExprKind::Var
        && expr
            .text
            .as_deref()
            .is_some_and(|name| names.contains(&name)))
        || expression_children(expr)
            .into_iter()
            .any(|child| expression_reads_any_name(child, names))
}

fn expressions_equal_ignoring_spans(left: &SyntaxExprOutput, right: &SyntaxExprOutput) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    clear_expression_spans(&mut left);
    clear_expression_spans(&mut right);
    left == right
}

fn clear_expression_spans(expr: &mut SyntaxExprOutput) {
    expr.span = Default::default();
    for child in &mut expr.children {
        clear_expression_spans(child);
    }
    for guard in expr.let_guards.iter_mut().filter_map(Option::as_deref_mut) {
        clear_expression_spans(guard);
    }
    for field in &mut expr.fields {
        clear_expression_spans(&mut field.value);
    }
    for clause in expr.clauses.iter_mut().chain(&mut expr.catch_clauses) {
        if let Some(guard) = clause.guard.as_deref_mut() {
            clear_expression_spans(guard);
        }
        clear_expression_spans(&mut clause.body);
    }
    if let Some(after) = &mut expr.try_after {
        clear_expression_spans(&mut after.trigger);
        clear_expression_spans(&mut after.body);
    }
}

/// Builds diagnostics for multi-expression callbacks with throwaway names.
pub(super) fn callback_name_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let Ok(module) = parse_lint_source(path, source) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    for declaration in module.declarations {
        match declaration.payload {
            SyntaxDeclarationPayload::Function { clauses, .. }
            | SyntaxDeclarationPayload::Method { clauses, .. } => {
                for clause in clauses {
                    collect_callback_name_diagnostics(path, source, &clause.body, &mut diagnostics);
                }
            }
            _ => {}
        }
    }
    diagnostics
}

/// Builds diagnostics for destructured pattern bindings that are never used.
pub(super) fn unused_destructure_binding_diagnostics(
    path: &Path,
    source: &str,
) -> Vec<LintDiagnostic> {
    let Ok(module) = parse_lint_source(path, source) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    for declaration in module.declarations {
        match declaration.payload {
            SyntaxDeclarationPayload::Function { clauses, .. }
            | SyntaxDeclarationPayload::Method { clauses, .. } => {
                for clause in clauses {
                    collect_unused_destructure_binding_diagnostics(
                        path,
                        source,
                        &clause.body,
                        &mut diagnostics,
                    );
                }
            }
            _ => {}
        }
    }
    diagnostics
}

/// Builds diagnostics for comments that repeat the next source expression.
pub(super) fn redundant_comment_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut diagnostics = Vec::new();

    for (line_index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !is_plain_line_comment(trimmed) {
            continue;
        }
        let Some(next_code_line) = next_nonempty_noncomment_line(&lines, line_index + 1) else {
            continue;
        };
        let comment_text = trimmed.trim_start_matches("//").trim();
        if is_comment_exempt(comment_text) {
            continue;
        }
        if normalize_restatement(comment_text) == normalize_restatement(next_code_line.trim()) {
            diagnostics.push(LintDiagnostic {
                path: path.to_path_buf(),
                line: line_index + 1,
                column: line.len() - trimmed.len() + 1,
                rule_id: REDUNDANT_COMMENT_RULE_ID,
                rule_name: REDUNDANT_COMMENT_RULE_NAME,
                severity: Severity::Warning,
                message: "comments should explain intent instead of restating the next expression",
                fix_available: false,
            });
        }
    }

    diagnostics
}

/// Builds diagnostics for public declarations missing API documentation.
pub(super) fn public_docs_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    if is_test_source_path(path)
        || path.extension().and_then(|extension| extension.to_str()) == Some("terls")
    {
        return Vec::new();
    }

    let Ok(module) = parse_lint_source(path, source) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    for declaration in module.declarations {
        let Some(()) = public_declaration_marker(&declaration.payload) else {
            continue;
        };
        if declaration.docs.is_empty() {
            push_public_docs_diagnostic(path, source, declaration.span.start, &mut diagnostics);
        }
        if let SyntaxDeclarationPayload::Trait { methods, .. } = declaration.payload {
            for method in methods {
                if method.is_public && method.docs.is_empty() {
                    push_public_docs_diagnostic(path, source, method.span.start, &mut diagnostics);
                }
            }
        }
    }

    diagnostics
}

/// Builds diagnostics for malformed block-doc star spacing.
pub(super) fn doc_comment_spacing_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_doc_block = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("/**") {
            in_doc_block = true;
            continue;
        }
        if !in_doc_block {
            continue;
        }
        if is_bad_doc_star_spacing(trimmed) {
            diagnostics.push(LintDiagnostic {
                path: path.to_path_buf(),
                line: line_index + 1,
                column: line.len() - trimmed.len() + 1,
                rule_id: DOC_COMMENT_SPACING_RULE_ID,
                rule_name: DOC_COMMENT_SPACING_RULE_NAME,
                severity: Severity::Warning,
                message: "block doc lines should use ` * ` spacing",
                fix_available: false,
            });
        }
        if trimmed.contains("*/") {
            in_doc_block = false;
        }
    }

    diagnostics
}

/// Finds the first expression whose structural depth exceeds the threshold.
fn first_deep_expression(expr: &SyntaxExprOutput, depth: usize) -> Option<&SyntaxExprOutput> {
    if depth > MAX_EXPRESSION_DEPTH {
        return Some(expr);
    }

    for child in expression_children(expr) {
        if let Some(deep) = first_deep_expression(child, depth + 1) {
            return Some(deep);
        }
    }
    None
}

/// Returns whether a trimmed source line is a plain non-doc line comment.
fn is_plain_line_comment(trimmed: &str) -> bool {
    trimmed.starts_with("//") && !trimmed.starts_with("///") && !trimmed.starts_with("//!")
}

/// Returns whether a block-doc body line is missing the space after `*`.
fn is_bad_doc_star_spacing(trimmed: &str) -> bool {
    trimmed.starts_with('*')
        && !trimmed.starts_with("*/")
        && trimmed != "*"
        && !trimmed.starts_with("* ")
}

/// Returns whether a source file is test-only for public-doc linting.
fn is_test_source_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("Test.terl") || name.ends_with("Test.terli"))
}

/// Returns whether a source file is a property-test module.
fn is_property_test_source_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with("PropertyTest.terl") || name.ends_with("PropertyTest.terli")
        })
}

/// Returns whether a declaration is public and requires docs.
fn public_declaration_marker(payload: &SyntaxDeclarationPayload) -> Option<()> {
    match payload {
        SyntaxDeclarationPayload::Type {
            is_public: true, ..
        }
        | SyntaxDeclarationPayload::Struct {
            is_public: true, ..
        }
        | SyntaxDeclarationPayload::Constructor {
            is_public: true, ..
        }
        | SyntaxDeclarationPayload::Function {
            is_public: true, ..
        }
        | SyntaxDeclarationPayload::Method {
            is_public: true, ..
        }
        | SyntaxDeclarationPayload::Trait {
            is_public: true, ..
        }
        | SyntaxDeclarationPayload::TraitImpl {
            is_public: true, ..
        }
        | SyntaxDeclarationPayload::AnnotationSchema {
            is_public: true, ..
        } => Some(()),
        _ => None,
    }
}

/// Adds one public documentation diagnostic at a source offset.
fn push_public_docs_diagnostic(
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
        rule_id: PUBLIC_DOCS_RULE_ID,
        rule_name: PUBLIC_DOCS_RULE_NAME,
        severity: Severity::Warning,
        message: "public API declarations should have documentation",
        fix_available: false,
    });
}

/// Returns whether a comment should be exempt from restatement checks.
fn is_comment_exempt(comment: &str) -> bool {
    let lower = comment.to_ascii_lowercase();
    lower.starts_with("todo")
        || lower.starts_with("fixme")
        || lower.starts_with("safety")
        || lower.starts_with("why")
}

/// Returns the next source line that can be compared against a comment.
fn next_nonempty_noncomment_line<'a>(lines: &'a [&str], start: usize) -> Option<&'a str> {
    lines
        .iter()
        .skip(start)
        .map(|line| line.trim())
        .find(|trimmed| !trimmed.is_empty() && !trimmed.starts_with("//"))
}

/// Normalizes a comment or expression line for conservative restatement checks.
fn normalize_restatement(text: &str) -> String {
    text.trim()
        .trim_end_matches(['.', ';'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Appends unused destructured binding diagnostics nested under `expr`.
fn collect_unused_destructure_binding_diagnostics(
    path: &Path,
    source: &str,
    expr: &SyntaxExprOutput,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if expr.kind == SyntaxExprKind::Let {
        collect_let_unused_destructure_binding_diagnostics(path, source, expr, diagnostics);
    }

    for clause in &expr.clauses {
        collect_clause_unused_destructure_binding_diagnostics(path, source, clause, diagnostics);
    }
    for clause in &expr.catch_clauses {
        collect_clause_unused_destructure_binding_diagnostics(path, source, clause, diagnostics);
    }
    for child in non_clause_expression_children(expr) {
        collect_unused_destructure_binding_diagnostics(path, source, child, diagnostics);
    }
}

/// Appends diagnostics for destructured let bindings unused by the let body.
fn collect_let_unused_destructure_binding_diagnostics(
    path: &Path,
    source: &str,
    expr: &SyntaxExprOutput,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let Some(body) = expr.children.get(expr.patterns.len()) else {
        return;
    };
    for (index, pattern) in expr.patterns.iter().enumerate() {
        let mut uses = vec![body];
        if let Some(guard) = expr.let_guards.get(index).and_then(Option::as_deref) {
            uses.push(guard);
        }
        collect_unused_pattern_bindings(path, source, pattern, &uses, diagnostics);
    }
}

/// Appends diagnostics for destructured clause bindings unused by guard/body.
fn collect_clause_unused_destructure_binding_diagnostics(
    path: &Path,
    source: &str,
    clause: &SyntaxClauseOutput,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let mut use_sites = vec![clause.body.as_ref()];
    if let Some(guard) = &clause.guard {
        use_sites.push(guard.as_ref());
    }
    for pattern in &clause.patterns {
        collect_unused_pattern_bindings(path, source, pattern, &use_sites, diagnostics);
    }
}

/// Appends diagnostics for unused variables inside one destructured pattern.
fn collect_unused_pattern_bindings(
    path: &Path,
    source: &str,
    pattern: &SyntaxPatternOutput,
    use_sites: &[&SyntaxExprOutput],
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if !is_destructured_pattern(pattern) {
        return;
    }

    let mut names = Vec::new();
    collect_pattern_variable_names(pattern, &mut names);
    for name in names {
        if is_intentionally_unused_name(name)
            || use_sites.iter().any(|expr| expr_uses_name(expr, name))
        {
            continue;
        }
        let (line, column) =
            source_line_column_at(source, use_sites.first().map_or(0, |expr| expr.span.start));
        diagnostics.push(LintDiagnostic {
            path: path.to_path_buf(),
            line,
            column,
            rule_id: UNUSED_DESTRUCTURE_BINDING_RULE_ID,
            rule_name: UNUSED_DESTRUCTURE_BINDING_RULE_NAME,
            severity: Severity::Warning,
            message: "unused destructured bindings should use `_` or a meaningful used name",
            fix_available: false,
        });
    }
}

/// Returns whether a pattern owns nested binding structure.
fn is_destructured_pattern(pattern: &SyntaxPatternOutput) -> bool {
    !matches!(
        pattern.kind,
        SyntaxPatternKind::Var | SyntaxPatternKind::Wildcard | SyntaxPatternKind::Ignore
    )
}

/// Collects variable names bound below one pattern.
fn collect_pattern_variable_names<'a>(pattern: &'a SyntaxPatternOutput, names: &mut Vec<&'a str>) {
    if pattern.kind == SyntaxPatternKind::Var {
        if let Some(name) = pattern.text.as_deref() {
            names.push(name);
        }
    }
    for child in &pattern.children {
        collect_pattern_variable_names(child, names);
    }
    for field in &pattern.fields {
        collect_pattern_variable_names(&field.value, names);
    }
}

/// Returns whether a binding name explicitly marks an unused value.
fn is_intentionally_unused_name(name: &str) -> bool {
    name == "_" || name.starts_with('_')
}

/// Returns whether an expression tree uses a variable name.
fn expr_uses_name(expr: &SyntaxExprOutput, name: &str) -> bool {
    if expr.kind == SyntaxExprKind::Var && expr.text.as_deref() == Some(name) {
        return true;
    }
    expression_children(expr)
        .into_iter()
        .any(|child| expr_uses_name(child, name))
}

/// Returns expression children without clause bodies already handled by a rule.
fn non_clause_expression_children(expr: &SyntaxExprOutput) -> Vec<&SyntaxExprOutput> {
    let mut children = Vec::new();
    children.extend(expr.children.iter());
    children.extend(expr.let_guards.iter().filter_map(|guard| guard.as_deref()));
    children.extend(expr.fields.iter().map(|field| field.value.as_ref()));
    if let Some(after) = &expr.try_after {
        children.push(after.trigger.as_ref());
        children.push(after.body.as_ref());
    }
    children
}

/// Appends diagnostics for all callback expressions nested under `expr`.
fn collect_callback_name_diagnostics(
    path: &Path,
    source: &str,
    expr: &SyntaxExprOutput,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if expr.kind == SyntaxExprKind::Fun {
        collect_fun_callback_name_diagnostics(path, source, expr, diagnostics);
    }

    for child in expression_children(expr) {
        collect_callback_name_diagnostics(path, source, child, diagnostics);
    }
}

/// Appends diagnostics for one lambda expression.
fn collect_fun_callback_name_diagnostics(
    path: &Path,
    source: &str,
    expr: &SyntaxExprOutput,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    for clause in &expr.clauses {
        if !is_multi_expression_callback_body(&clause.body) {
            continue;
        }
        for pattern in &clause.patterns {
            if let Some(name) = direct_pattern_name(pattern) {
                if is_weak_callback_name(name) {
                    let (line, column) = source_line_column_at(source, expr.span.start);
                    diagnostics.push(LintDiagnostic {
                        path: path.to_path_buf(),
                        line,
                        column,
                        rule_id: CALLBACK_NAME_RULE_ID,
                        rule_name: CALLBACK_NAME_RULE_NAME,
                        severity: Severity::Warning,
                        message: "multi-expression callbacks should use meaningful parameter names",
                        fix_available: false,
                    });
                }
            }
        }
    }
}

/// Returns whether the callback body contains a semicolon-backed expression list.
fn is_multi_expression_callback_body(expr: &SyntaxExprOutput) -> bool {
    matches!(expr.kind, SyntaxExprKind::Let | SyntaxExprKind::Sequence) && expr.children.len() > 1
}

/// Returns a simple callback parameter name when the pattern is direct.
fn direct_pattern_name(pattern: &SyntaxPatternOutput) -> Option<&str> {
    if pattern.kind == SyntaxPatternKind::Var {
        return pattern.text.as_deref();
    }
    None
}

/// Returns whether a callback parameter name is too weak for a multi-step body.
fn is_weak_callback_name(name: &str) -> bool {
    let bare_name = name.strip_prefix('_').unwrap_or(name);
    if bare_name.len() == 1 && bare_name.chars().all(|ch| ch.is_ascii_lowercase()) {
        return true;
    }
    matches!(bare_name, "arg" | "it" | "tmp" | "val")
}

/// Returns direct expression children across expression-node storage fields.
fn expression_children(expr: &SyntaxExprOutput) -> Vec<&SyntaxExprOutput> {
    let mut children = Vec::new();
    children.extend(expr.children.iter());
    children.extend(expr.let_guards.iter().filter_map(|guard| guard.as_deref()));
    children.extend(expr.fields.iter().map(|field| field.value.as_ref()));
    for clause in &expr.clauses {
        if let Some(guard) = &clause.guard {
            children.push(guard.as_ref());
        }
        children.push(clause.body.as_ref());
    }
    for clause in &expr.catch_clauses {
        if let Some(guard) = &clause.guard {
            children.push(guard.as_ref());
        }
        children.push(clause.body.as_ref());
    }
    if let Some(after) = &expr.try_after {
        children.push(after.trigger.as_ref());
        children.push(after.body.as_ref());
    }
    children
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
