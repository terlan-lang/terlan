use std::path::Path;

use crate::commands::lint::diagnostic::{LintDiagnostic, Severity};
use crate::terlan_syntax::{
    parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput,
    SyntaxFunctionClauseOutput, SyntaxParamOutput,
};

const MESSAGE_TAG_EQUALITY_RULE_ID: &str = "TL0905";
const MESSAGE_TAG_EQUALITY_RULE_NAME: &str = "actor-vm.message-tag-equality";
const STATE_PARAMETER_NAME_RULE_ID: &str = "TL0906";
const STATE_PARAMETER_NAME_RULE_NAME: &str = "actor-vm.state-parameter-name";

/// Builds diagnostics for actor-vm style issues in production source.
pub(super) fn actor_vm_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    if is_test_source_path(path) {
        return Vec::new();
    }

    let Ok(module) = parse_module_as_syntax_output(source) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    for declaration in module.declarations {
        match declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                clauses,
                ..
            }
            | SyntaxDeclarationPayload::Method {
                name,
                params,
                clauses,
                ..
            } => {
                if is_actor_handler_name(&name) {
                    collect_clause_diagnostics(path, source, &clauses, &mut diagnostics);
                }
                if is_actor_lifecycle_name(&name) {
                    collect_state_parameter_diagnostics(path, source, &params, &mut diagnostics);
                }
            }
            _ => {}
        }
    }
    diagnostics
}

/// Returns whether production actor lint should skip a source path.
fn is_test_source_path(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|part| part == "tests" || part == "test")
    }) || path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("Test.terl") || name.ends_with("Test.terli"))
}

/// Returns whether a declaration name conventionally handles actor messages.
fn is_actor_handler_name(name: &str) -> bool {
    name == "handle"
        || name.starts_with("handle_")
        || name.ends_with("_handler")
        || matches!(
            name,
            "dispatch" | "on_message" | "receive" | "route_message"
        )
}

/// Returns whether a declaration name conventionally owns actor lifecycle state.
fn is_actor_lifecycle_name(name: &str) -> bool {
    is_actor_handler_name(name)
        || matches!(
            name,
            "init" | "start" | "stop" | "terminate" | "on_start" | "on_stop"
        )
}

/// Collects actor-vm diagnostics from handler clauses.
fn collect_clause_diagnostics(
    path: &Path,
    source: &str,
    clauses: &[SyntaxFunctionClauseOutput],
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    for clause in clauses {
        collect_expr_diagnostics(path, source, &clause.body, diagnostics);
    }
}

/// Collects diagnostics for state-like parameters without canonical names.
fn collect_state_parameter_diagnostics(
    path: &Path,
    source: &str,
    params: &[SyntaxParamOutput],
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    for param in params {
        if is_state_typed_parameter(param) && !is_canonical_state_name(&param.name) {
            push_state_parameter_name_diagnostic(path, source, param.span.start, diagnostics);
        }
    }
}

/// Recursively collects actor-vm diagnostics from an expression tree.
fn collect_expr_diagnostics(
    path: &Path,
    source: &str,
    expr: &SyntaxExprOutput,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if is_manual_message_tag_equality(source, expr) {
        push_message_tag_equality_diagnostic(path, source, expr.span.start, diagnostics);
    }

    for child in &expr.children {
        collect_expr_diagnostics(path, source, child, diagnostics);
    }
    for field in &expr.fields {
        collect_expr_diagnostics(path, source, &field.value, diagnostics);
    }
    for clause in &expr.clauses {
        if let Some(guard) = &clause.guard {
            collect_expr_diagnostics(path, source, guard, diagnostics);
        }
        collect_expr_diagnostics(path, source, &clause.body, diagnostics);
    }
    for clause in &expr.catch_clauses {
        if let Some(guard) = &clause.guard {
            collect_expr_diagnostics(path, source, guard, diagnostics);
        }
        collect_expr_diagnostics(path, source, &clause.body, diagnostics);
    }
    if let Some(after) = &expr.try_after {
        collect_expr_diagnostics(path, source, &after.trigger, diagnostics);
        collect_expr_diagnostics(path, source, &after.body, diagnostics);
    }
}

/// Returns whether a parameter's annotation clearly names actor state.
fn is_state_typed_parameter(param: &SyntaxParamOutput) -> bool {
    let type_name = param.annotation.text.trim();
    type_name == "State"
        || type_name.ends_with(".State")
        || type_name.ends_with("State")
        || type_name.contains("[State]")
}

/// Returns whether a binding name makes actor state explicit.
fn is_canonical_state_name(name: &str) -> bool {
    name == "state" || name.ends_with("_state")
}

/// Returns whether an expression manually compares a message tag to a literal.
fn is_manual_message_tag_equality(source: &str, expr: &SyntaxExprOutput) -> bool {
    if expr.kind != SyntaxExprKind::BinaryOp || expr.operator.as_deref() != Some("==") {
        return false;
    }
    let Some(left) = expr.children.first() else {
        return false;
    };
    let Some(right) = expr.children.get(1) else {
        return false;
    };

    (is_message_tag_access(source, left) && is_tag_literal(source, right))
        || (is_tag_literal(source, left) && is_message_tag_access(source, right))
}

/// Returns whether an expression reads a conventional message tag field.
fn is_message_tag_access(source: &str, expr: &SyntaxExprOutput) -> bool {
    if expr.kind != SyntaxExprKind::FieldAccess {
        return false;
    }
    let field = expr.text.as_deref().unwrap_or_default().trim();
    if !matches!(field, "tag" | "kind" | "type") {
        return false;
    }
    let Some(receiver) = expr.children.first() else {
        return false;
    };
    let receiver = receiver
        .text
        .as_deref()
        .unwrap_or_else(|| expr_source(source, receiver))
        .trim();
    matches!(receiver, "message" | "msg" | "event" | "envelope")
        || receiver.ends_with("_message")
        || receiver.ends_with("_msg")
        || receiver.ends_with("_event")
        || receiver.ends_with("_envelope")
}

/// Returns whether an expression is a literal tag value.
fn is_tag_literal(source: &str, expr: &SyntaxExprOutput) -> bool {
    if matches!(expr.kind, SyntaxExprKind::Binary | SyntaxExprKind::Atom) {
        return true;
    }
    if matches!(expr.kind, SyntaxExprKind::Var) {
        return expr
            .text
            .as_deref()
            .is_some_and(|text| text.chars().next().is_some_and(char::is_uppercase));
    }
    let text = expr_source(source, expr).trim();
    text.starts_with('"') || text.starts_with("Atom[")
}

/// Returns the original source slice for an expression span.
fn expr_source<'a>(source: &'a str, expr: &SyntaxExprOutput) -> &'a str {
    source
        .get(expr.span.start..expr.span.end)
        .unwrap_or_default()
        .trim()
}

/// Adds one actor-vm message-tag equality diagnostic.
fn push_message_tag_equality_diagnostic(
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
        rule_id: MESSAGE_TAG_EQUALITY_RULE_ID,
        rule_name: MESSAGE_TAG_EQUALITY_RULE_NAME,
        severity: Severity::Warning,
        message: "prefer pattern or shape matching over manual actor message tag equality",
        fix_available: false,
    });
}

/// Adds one actor-vm state parameter naming diagnostic.
fn push_state_parameter_name_diagnostic(
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
        rule_id: STATE_PARAMETER_NAME_RULE_ID,
        rule_name: STATE_PARAMETER_NAME_RULE_NAME,
        severity: Severity::Warning,
        message: "actor lifecycle state parameters should be named `state` or end with `_state`",
        fix_available: false,
    });
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
