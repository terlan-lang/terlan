//! Source rendering for constants in public interfaces.

use super::*;

pub(in crate::compiler::hir) fn render_constant_expr(expr: &SyntaxExprOutput) -> String {
    match expr.kind {
        SyntaxExprKind::Int | SyntaxExprKind::Float | SyntaxExprKind::Var => {
            expr.text.clone().unwrap_or_default()
        }
        SyntaxExprKind::Atom => expr
            .raw
            .clone()
            .unwrap_or_else(|| expr.text.clone().unwrap_or_default()),
        SyntaxExprKind::Binary => format!(
            "\"{}\"",
            expr.text
                .as_deref()
                .unwrap_or_default()
                .replace('"', "\\\"")
        ),
        SyntaxExprKind::Tuple => format!(
            "{{{}}}",
            expr.children
                .iter()
                .map(render_constant_expr)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SyntaxExprKind::List => format!(
            "[{}]",
            expr.children
                .iter()
                .map(render_constant_expr)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SyntaxExprKind::Array => format!(
            "#[{}]",
            expr.children
                .iter()
                .map(render_constant_expr)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SyntaxExprKind::Map => format!(
            "{{{}}}",
            expr.fields
                .iter()
                .map(|field| format!("{}: {}", field.key, render_constant_expr(&field.value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SyntaxExprKind::RecordConstruct => format!(
            "{} {{{}}}",
            expr.text.as_deref().unwrap_or_default(),
            expr.fields
                .iter()
                .map(|field| format!("{}: {}", field.key, render_constant_expr(&field.value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SyntaxExprKind::ListCons => format!(
            "[{} | {}]",
            render_constant_expr(&expr.children[0]),
            render_constant_expr(&expr.children[1])
        ),
        SyntaxExprKind::UnaryOp => format!(
            "{} {}",
            expr.operator.as_deref().unwrap_or(""),
            render_constant_expr(&expr.children[0])
        ),
        SyntaxExprKind::BinaryOp => format!(
            "({} {} {})",
            render_constant_expr(&expr.children[0]),
            expr.operator.as_deref().unwrap_or(""),
            render_constant_expr(&expr.children[1])
        ),
        SyntaxExprKind::Call | SyntaxExprKind::FunctionCall => {
            let callee = expr
                .children
                .first()
                .map(render_constant_expr)
                .unwrap_or_default();
            let type_args = if expr.type_args.is_empty() {
                String::new()
            } else {
                format!(
                    "[{}]",
                    expr.type_args
                        .iter()
                        .map(|ty| ty.text.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            let args = expr
                .children
                .iter()
                .skip(1)
                .enumerate()
                .map(|(index, arg)| {
                    let value = render_constant_expr(arg);
                    expr.arg_names
                        .get(index)
                        .and_then(|name| name.as_deref())
                        .map_or(value.clone(), |name| format!("{name} = {value}"))
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{callee}{type_args}({args})")
        }
        SyntaxExprKind::FieldAccess => format!(
            "{}.{}",
            render_constant_expr(&expr.children[0]),
            expr.text.as_deref().unwrap_or_default()
        ),
        SyntaxExprKind::Index => format!(
            "{}[{}]",
            render_constant_expr(&expr.children[0]),
            render_constant_expr(&expr.children[1])
        ),
        SyntaxExprKind::Cast => format!(
            "{} as {}",
            render_constant_expr(&expr.children[0]),
            expr.text.as_deref().unwrap_or_default()
        ),
        SyntaxExprKind::Macro => format!(
            "?{}({})",
            expr.text.as_deref().unwrap_or_default(),
            expr.children
                .iter()
                .map(render_constant_expr)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SyntaxExprKind::Quote => format!(
            "quote {}",
            expr.children
                .first()
                .map(render_constant_expr)
                .unwrap_or_else(|| "Unit".to_string())
        ),
        SyntaxExprKind::Unquote => format!(
            "unquote({})",
            expr.children
                .first()
                .map(render_constant_expr)
                .unwrap_or_else(|| "Unit".to_string())
        ),
        SyntaxExprKind::Sequence => expr
            .children
            .iter()
            .map(render_constant_expr)
            .collect::<Vec<_>>()
            .join("; "),
        SyntaxExprKind::Let => {
            let mut parts = expr
                .patterns
                .iter()
                .zip(expr.children.iter())
                .map(|(pattern, value)| {
                    format!(
                        "let {} = {}",
                        render_constant_pattern(pattern),
                        render_constant_expr(value)
                    )
                })
                .collect::<Vec<_>>();
            if let Some(body) = expr.children.get(expr.patterns.len()) {
                parts.push(render_constant_expr(body));
            }
            parts.join("; ")
        }
        SyntaxExprKind::Case => format!(
            "case {} {{ {} }}",
            expr.children
                .first()
                .map(render_constant_expr)
                .unwrap_or_default(),
            expr.clauses
                .iter()
                .map(render_constant_clause)
                .collect::<Vec<_>>()
                .join("; ")
        ),
        _ => expr
            .raw
            .clone()
            .or_else(|| expr.text.clone())
            .unwrap_or_else(|| "Unit".to_string()),
    }
}

fn render_constant_clause(clause: &crate::terlan_syntax::SyntaxClauseOutput) -> String {
    let patterns = clause
        .patterns
        .iter()
        .map(render_constant_pattern)
        .collect::<Vec<_>>()
        .join(", ");
    let guard = clause
        .guard
        .as_ref()
        .map(|guard| format!(" where {}", render_constant_expr(guard)))
        .unwrap_or_default();
    format!(
        "{patterns}{guard} -> {}",
        render_constant_expr(&clause.body)
    )
}

fn render_constant_pattern(pattern: &crate::terlan_syntax::SyntaxPatternOutput) -> String {
    use crate::terlan_syntax::SyntaxPatternKind;
    match pattern.kind {
        SyntaxPatternKind::Wildcard => "_".to_string(),
        SyntaxPatternKind::Var
        | SyntaxPatternKind::Int
        | SyntaxPatternKind::Float
        | SyntaxPatternKind::Atom => pattern.text.clone().unwrap_or_default(),
        SyntaxPatternKind::String => format!(
            "\"{}\"",
            pattern
                .text
                .as_deref()
                .unwrap_or_default()
                .replace('"', "\\\"")
        ),
        SyntaxPatternKind::Tuple => format!(
            "{{{}}}",
            pattern
                .children
                .iter()
                .map(render_constant_pattern)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SyntaxPatternKind::List => format!(
            "[{}]",
            pattern
                .children
                .iter()
                .map(render_constant_pattern)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SyntaxPatternKind::ListCons => format!(
            "[{} | {}]",
            render_constant_pattern(&pattern.children[0]),
            render_constant_pattern(&pattern.children[1])
        ),
        SyntaxPatternKind::Alias => format!(
            "{} = {}",
            pattern.text.as_deref().unwrap_or_default(),
            render_constant_pattern(&pattern.children[0])
        ),
        SyntaxPatternKind::Constructor => format!(
            "{{{}, {}}}",
            pattern.text.as_deref().unwrap_or_default(),
            pattern
                .children
                .iter()
                .map(render_constant_pattern)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => pattern.text.clone().unwrap_or_else(|| "_".to_string()),
    }
}
