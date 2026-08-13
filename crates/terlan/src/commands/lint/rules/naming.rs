use std::collections::BTreeMap;
use std::path::Path;

use crate::terlan_syntax::{
    SyntaxClauseOutput, SyntaxDeclarationPayload, SyntaxExprOutput, SyntaxParamOutput,
    SyntaxPatternKind, SyntaxPatternOutput,
};

use super::super::diagnostic::{LintDiagnostic, Severity};
use super::parse_lint_source;

const FUNCTION_SNAKE_CASE_RULE_ID: &str = "TL0301";
const FUNCTION_SNAKE_CASE_RULE_NAME: &str = "naming.function-snake-case";
const TYPE_UPPER_CAMEL_RULE_ID: &str = "TL0302";
const TYPE_UPPER_CAMEL_RULE_NAME: &str = "naming.type-upper-camel";
const NAME_COLLISION_RULE_ID: &str = "TL0303";
const NAME_COLLISION_RULE_NAME: &str = "naming.case-underscore-collision";
const BINDING_SNAKE_CASE_RULE_ID: &str = "TL0304";
const BINDING_SNAKE_CASE_RULE_NAME: &str = "naming.binding-snake-case";

/// Builds diagnostics for function-like declarations outside snake case.
pub(super) fn function_snake_case_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let Ok(module) = parse_lint_source(path, source) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    for declaration in module.declarations {
        let name = match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, .. }
            | SyntaxDeclarationPayload::Method { name, .. } => name,
            _ => continue,
        };
        if is_lower_snake_case(name) {
            continue;
        }
        let (line, column) = source_line_column_at(source, declaration.span.start);
        diagnostics.push(LintDiagnostic {
            path: path.to_path_buf(),
            line,
            column,
            rule_id: FUNCTION_SNAKE_CASE_RULE_ID,
            rule_name: FUNCTION_SNAKE_CASE_RULE_NAME,
            severity: Severity::Warning,
            message: "function and method names should use lower_snake_case",
            fix_available: false,
        });
    }

    diagnostics
}

/// Builds diagnostics for type-like declarations outside UpperCamelCase.
pub(super) fn type_upper_camel_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let Ok(module) = parse_lint_source(path, source) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    for declaration in module.declarations {
        let name = match &declaration.payload {
            SyntaxDeclarationPayload::Type { name, .. }
            | SyntaxDeclarationPayload::Struct { name, .. }
            | SyntaxDeclarationPayload::Constructor { name, .. }
            | SyntaxDeclarationPayload::Trait { name, .. } => name,
            _ => continue,
        };
        if is_upper_camel_case(name) {
            continue;
        }
        let (line, column) = source_line_column_at(source, declaration.span.start);
        diagnostics.push(LintDiagnostic {
            path: path.to_path_buf(),
            line,
            column,
            rule_id: TYPE_UPPER_CAMEL_RULE_ID,
            rule_name: TYPE_UPPER_CAMEL_RULE_NAME,
            severity: Severity::Warning,
            message: "type, struct, trait, and constructor names should use UpperCamelCase",
            fix_available: false,
        });
    }

    diagnostics
}

/// Builds diagnostics for declarations that differ only by case or underscores.
pub(super) fn case_underscore_collision_diagnostics(
    path: &Path,
    source: &str,
) -> Vec<LintDiagnostic> {
    let Ok(module) = parse_lint_source(path, source) else {
        return Vec::new();
    };

    let mut seen_names = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for declaration in module.declarations {
        let Some(name) = declaration_name(&declaration.payload) else {
            continue;
        };
        let collision_key = case_underscore_key(name);
        if let Some(previous_name) = seen_names.insert(collision_key, name.to_string()) {
            if previous_name != name {
                let (line, column) = source_line_column_at(source, declaration.span.start);
                diagnostics.push(LintDiagnostic {
                    path: path.to_path_buf(),
                    line,
                    column,
                    rule_id: NAME_COLLISION_RULE_ID,
                    rule_name: NAME_COLLISION_RULE_NAME,
                    severity: Severity::Warning,
                    message: "declaration names should not differ only by case or underscores",
                    fix_available: false,
                });
            }
        }
    }

    diagnostics
}

/// Builds diagnostics for source-owned value bindings outside snake case.
pub(super) fn binding_snake_case_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let Ok(module) = parse_lint_source(path, source) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    for declaration in module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Constructor { clauses, .. } => {
                for clause in clauses {
                    for param in &clause.params {
                        push_binding_diagnostic(
                            path,
                            source,
                            &param.name,
                            param.span.start,
                            &mut diagnostics,
                        );
                    }
                    collect_expr_binding_diagnostics(
                        path,
                        source,
                        &clause.body,
                        clause.span.start,
                        &mut diagnostics,
                    );
                }
            }
            SyntaxDeclarationPayload::Function {
                params, clauses, ..
            } => {
                collect_param_binding_diagnostics(path, source, params, &mut diagnostics);
                for clause in clauses {
                    collect_expr_binding_diagnostics(
                        path,
                        source,
                        &clause.body,
                        clause.span.start,
                        &mut diagnostics,
                    );
                }
            }
            SyntaxDeclarationPayload::Method {
                receiver,
                params,
                clauses,
                ..
            } => {
                push_binding_diagnostic(
                    path,
                    source,
                    &receiver.name,
                    receiver.span.start,
                    &mut diagnostics,
                );
                collect_param_binding_diagnostics(path, source, params, &mut diagnostics);
                for clause in clauses {
                    collect_expr_binding_diagnostics(
                        path,
                        source,
                        &clause.body,
                        clause.span.start,
                        &mut diagnostics,
                    );
                }
            }
            SyntaxDeclarationPayload::Trait { methods, .. } => {
                for method in methods {
                    collect_param_binding_diagnostics(
                        path,
                        source,
                        &method.params,
                        &mut diagnostics,
                    );
                    if let Some(default_body) = &method.default_body {
                        collect_expr_binding_diagnostics(
                            path,
                            source,
                            default_body,
                            method.span.start,
                            &mut diagnostics,
                        );
                    }
                }
            }
            SyntaxDeclarationPayload::TraitImpl { methods, .. } => {
                for method in methods {
                    collect_param_binding_diagnostics(
                        path,
                        source,
                        &method.params,
                        &mut diagnostics,
                    );
                    for clause in &method.clauses {
                        collect_expr_binding_diagnostics(
                            path,
                            source,
                            &clause.body,
                            clause.span.start,
                            &mut diagnostics,
                        );
                    }
                }
            }
            SyntaxDeclarationPayload::Template { props, .. } => {
                for prop in props {
                    push_binding_diagnostic(
                        path,
                        source,
                        &prop.name,
                        prop.span.start,
                        &mut diagnostics,
                    );
                    if let Some(default) = &prop.default {
                        collect_expr_binding_diagnostics(
                            path,
                            source,
                            default,
                            prop.span.start,
                            &mut diagnostics,
                        );
                    }
                }
            }
            SyntaxDeclarationPayload::Struct { fields, .. } => {
                for field in fields {
                    if let Some(default) = &field.default {
                        collect_expr_binding_diagnostics(
                            path,
                            source,
                            default,
                            field.span.start,
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

/// Returns the declaration name for lint-owned naming checks.
fn declaration_name(payload: &SyntaxDeclarationPayload) -> Option<&str> {
    match payload {
        SyntaxDeclarationPayload::Type { name, .. }
        | SyntaxDeclarationPayload::Struct { name, .. }
        | SyntaxDeclarationPayload::Constructor { name, .. }
        | SyntaxDeclarationPayload::Function { name, .. }
        | SyntaxDeclarationPayload::Method { name, .. }
        | SyntaxDeclarationPayload::Trait { name, .. } => Some(name),
        _ => None,
    }
}

/// Adds diagnostics for callable parameter bindings.
fn collect_param_binding_diagnostics(
    path: &Path,
    source: &str,
    params: &[SyntaxParamOutput],
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    for param in params {
        push_binding_diagnostic(path, source, &param.name, param.span.start, diagnostics);
        if let Some(default) = &param.default {
            collect_expr_binding_diagnostics(path, source, default, param.span.start, diagnostics);
        }
    }
}

/// Traverses expression-local binding sites.
fn collect_expr_binding_diagnostics(
    path: &Path,
    source: &str,
    expr: &SyntaxExprOutput,
    fallback_start: usize,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    collect_pattern_binding_diagnostics(path, source, &expr.patterns, fallback_start, diagnostics);
    for clause in &expr.clauses {
        collect_clause_binding_diagnostics(path, source, clause, fallback_start, diagnostics);
    }
    for clause in &expr.catch_clauses {
        collect_clause_binding_diagnostics(path, source, clause, fallback_start, diagnostics);
    }
    for child in &expr.children {
        collect_expr_binding_diagnostics(path, source, child, fallback_start, diagnostics);
    }
    for field in &expr.fields {
        collect_expr_binding_diagnostics(path, source, &field.value, fallback_start, diagnostics);
    }
}

/// Adds diagnostics for pattern bindings in a clause.
fn collect_clause_binding_diagnostics(
    path: &Path,
    source: &str,
    clause: &SyntaxClauseOutput,
    fallback_start: usize,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    collect_pattern_binding_diagnostics(
        path,
        source,
        &clause.patterns,
        fallback_start,
        diagnostics,
    );
    if let Some(guard) = &clause.guard {
        collect_expr_binding_diagnostics(path, source, guard, fallback_start, diagnostics);
    }
    collect_expr_binding_diagnostics(path, source, &clause.body, fallback_start, diagnostics);
}

/// Adds diagnostics for variable bindings inside pattern trees.
fn collect_pattern_binding_diagnostics(
    path: &Path,
    source: &str,
    patterns: &[SyntaxPatternOutput],
    fallback_start: usize,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    for pattern in patterns {
        if pattern.kind == SyntaxPatternKind::Var {
            if let Some(name) = &pattern.text {
                push_binding_diagnostic(path, source, name, fallback_start, diagnostics);
            }
        }
        collect_pattern_binding_diagnostics(
            path,
            source,
            &pattern.children,
            fallback_start,
            diagnostics,
        );
        for field in &pattern.fields {
            collect_pattern_binding_diagnostics(
                path,
                source,
                std::slice::from_ref(field.value.as_ref()),
                fallback_start,
                diagnostics,
            );
        }
    }
}

/// Adds one binding-name diagnostic when a binding is not lower_snake_case.
fn push_binding_diagnostic(
    path: &Path,
    source: &str,
    name: &str,
    span_start: usize,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if is_lower_snake_binding(name) {
        return;
    }
    let (line, column) = source_line_column_at(source, span_start);
    diagnostics.push(LintDiagnostic {
        path: path.to_path_buf(),
        line,
        column,
        rule_id: BINDING_SNAKE_CASE_RULE_ID,
        rule_name: BINDING_SNAKE_CASE_RULE_NAME,
        severity: Severity::Warning,
        message: "value bindings should use lower_snake_case",
        fix_available: false,
    });
}

/// Builds a normalized key that erases case and underscores.
fn case_underscore_key(name: &str) -> String {
    name.chars()
        .filter(|ch| *ch != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Returns whether a language-owned function name is lower_snake_case.
fn is_lower_snake_case(name: &str) -> bool {
    let mut previous_was_underscore = false;
    let mut saw_letter_or_digit = false;

    for (index, ch) in name.chars().enumerate() {
        if ch == '_' {
            if index == 0 || previous_was_underscore {
                return false;
            }
            previous_was_underscore = true;
            continue;
        }
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            saw_letter_or_digit = true;
            previous_was_underscore = false;
            continue;
        }
        return false;
    }

    saw_letter_or_digit && !previous_was_underscore
}

/// Returns whether a value binding is lower_snake_case or intentionally unused.
fn is_lower_snake_binding(name: &str) -> bool {
    name.strip_prefix('_')
        .map_or_else(|| is_lower_snake_case(name), is_lower_snake_case)
}

/// Returns whether a language-owned type-like name is UpperCamelCase.
fn is_upper_camel_case(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_uppercase() {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric())
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
