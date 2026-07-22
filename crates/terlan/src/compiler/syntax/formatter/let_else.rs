use super::{
    format_case_clause, format_expr, format_let_binding_assignment, format_let_binding_value,
    format_pattern, format_statement_parts,
};
use crate::terlan_syntax::parse_tree::{CaseClause, Expr, LetBinding};

/// Formats ordinary nested bindings and grouped refutable bindings.
pub(super) fn format_let_expr(
    bindings: &[LetBinding],
    else_clauses: &[CaseClause],
    body: Option<&Expr>,
    indent: usize,
) -> String {
    if else_clauses.is_empty() {
        return format_ordinary_let(bindings, body, indent);
    }

    let spacing = "    ".repeat(indent);
    let inner_spacing = "    ".repeat(indent + 1);
    let mut out = String::from("let {\n");
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
        out.push_str(&format_expr(body, indent));
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
        parts.push(format_expr(body, indent));
    }
    if indent > 0 || parts.iter().any(|part| part.contains('\n')) {
        format_statement_parts(parts, indent)
    } else {
        parts.join("; ")
    }
}
