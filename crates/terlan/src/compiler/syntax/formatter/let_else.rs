use super::{
    format_assignment_child, format_case_clause, format_let_binding_assignment,
    format_let_binding_value, format_pattern, format_statement_parts,
};
use crate::terlan_syntax::parse_tree::{CaseClause, Expr, LetBinding, Pattern};

/// A fallback of exactly `name -> name` is semantically "propagate the
/// unmatched value unchanged" — identical to omitting `else` entirely. This
/// matches both the compiler's own synthesized default clause and any
/// hand-written fallback of the same shape, so both print the same way.
fn is_identity_fallback(clauses: &[CaseClause]) -> bool {
    matches!(
        clauses,
        [CaseClause {
            pattern: Pattern::Var(pattern_name),
            guard: None,
            body: Expr::Var(body_name),
        }] if pattern_name == body_name
    )
}

fn is_ordinary_binding_target(pattern: &Pattern) -> bool {
    matches!(pattern, Pattern::Var(_))
}

/// Formats ordinary declarations, propagating refutable steps, and grouped
/// refutable `if` expressions.
pub(super) fn format_let_expr(
    bindings: &[LetBinding],
    else_clauses: &[CaseClause],
    body: Option<&Expr>,
    indent: usize,
) -> String {
    if else_clauses.is_empty()
        && bindings
            .iter()
            .all(|binding| is_ordinary_binding_target(&binding.pattern))
    {
        return format_ordinary_let(bindings, body, indent);
    }
    if let [binding] = bindings {
        if is_identity_fallback(else_clauses) {
            return format_bare_refutable(binding, body, indent);
        }
    }

    let spacing = "    ".repeat(indent);
    let inner_spacing = "    ".repeat(indent + 1);
    let mut out = String::from("if {\n");
    for (index, binding) in bindings.iter().enumerate() {
        out.push_str(&inner_spacing);
        out.push_str(&format_refutable_binding(binding, indent + 1));
        if index + 1 < bindings.len() {
            out.push_str(";\n");
        }
    }
    out.push('\n');
    out.push_str(&spacing);
    out.push_str("} else {\n");
    for (index, clause) in else_clauses.iter().enumerate() {
        out.push_str(&inner_spacing);
        out.push_str(&format_case_clause(clause, indent + 1));
        if index + 1 < else_clauses.len() {
            out.push(';');
        }
        out.push('\n');
    }
    out.push_str(&spacing);
    out.push_str("};");
    if let Some(body) = body {
        out.push('\n');
        out.push_str(&spacing);
        out.push_str(&format_assignment_child(body, indent));
    }
    out
}

fn format_bare_refutable(binding: &LetBinding, body: Option<&Expr>, indent: usize) -> String {
    let spacing = "    ".repeat(indent);
    let mut out = String::new();
    out.push_str(&format_refutable_binding(binding, indent));
    out.push(';');
    if let Some(body) = body {
        out.push('\n');
        out.push_str(&spacing);
        out.push_str(&super::format_expr(body, indent));
    }
    out
}

fn format_refutable_binding(binding: &LetBinding, indent: usize) -> String {
    let value = format_let_binding_value(&binding.value, indent + 1);
    if value.contains('\n') {
        format!(
            "{} <-\n{}{}",
            format_pattern(&binding.pattern),
            "    ".repeat(indent + 1),
            value
        )
    } else {
        format!("{} <- {}", format_pattern(&binding.pattern), value)
    }
}

fn format_ordinary_let(bindings: &[LetBinding], body: Option<&Expr>, indent: usize) -> String {
    let mut parts = bindings
        .iter()
        .map(|binding| {
            format_let_binding_assignment("let ", &binding.pattern, &binding.value, indent)
        })
        .collect::<Vec<_>>();
    if let Some(body) = body {
        parts.push(format_assignment_child(body, indent));
    }
    if indent > 0 || parts.iter().any(|part| part.contains('\n')) {
        format_statement_parts(parts, indent)
    } else {
        parts.join("; ")
    }
}

/// Formats a let expression used as the complete body of a function clause.
pub(super) fn format_function_body_let(
    bindings: &[LetBinding],
    else_clauses: &[CaseClause],
    body: Option<&Expr>,
    indent: usize,
) -> String {
    if !else_clauses.is_empty()
        || bindings
            .iter()
            .any(|binding| !is_ordinary_binding_target(&binding.pattern))
    {
        return format_let_expr(bindings, else_clauses, body, indent);
    }

    let mut parts = bindings
        .iter()
        .map(|binding| {
            format_let_binding_assignment("let ", &binding.pattern, &binding.value, indent)
        })
        .collect::<Vec<_>>();
    match body {
        Some(Expr::Sequence(expressions)) => parts.extend(
            expressions
                .iter()
                .map(|expression| super::format_expr(expression, indent)),
        ),
        Some(body) => parts.push(super::format_expr(body, indent)),
        None => {}
    }
    format_statement_parts(parts, indent)
}
