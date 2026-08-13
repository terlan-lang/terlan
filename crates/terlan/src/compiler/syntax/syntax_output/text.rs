use super::SyntaxTypeOutput;
use crate::terlan_syntax::parse_tree::{BinaryOp, Expr, Pattern, TypeExpr, UnaryOp};
use crate::terlan_syntax::quoted_string_literal;

/// Renders parser expression text for syntax-output summaries.
///
/// Inputs: parsed expression. Output: compact source-like text. Transformation:
/// recursively formats expression variants used by declarations that preserve
/// body/default text for diagnostics and contracts.
pub(super) fn expr_to_output_text(expr: &Expr) -> String {
    match expr {
        Expr::Int(value) => value.to_string(),
        Expr::Float(value) => value.to_string(),
        Expr::Atom(name) | Expr::AtomLiteral(name) | Expr::Var(name) => name.clone(),
        Expr::Binary(value) => value.clone(),
        Expr::Tuple(items) => format!(
            "{{{}}}",
            items
                .iter()
                .map(expr_to_output_text)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::List(items) => format!(
            "[{}]",
            items
                .iter()
                .map(expr_to_output_text)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::ListCons(head, tail) => format!(
            "[{} | {}]",
            expr_to_output_text(head),
            expr_to_output_text(tail)
        ),
        Expr::IndexAssign {
            collection,
            index,
            value,
        } => format!(
            "{}[{}] = {}",
            expr_to_output_text(collection),
            expr_to_output_text(index),
            expr_to_output_text(value)
        ),
        Expr::Let {
            bindings,
            else_clauses,
            body,
        } => {
            let mut parts = if else_clauses.is_empty() {
                bindings
                    .iter()
                    .map(|binding| {
                        format!(
                            "let {} = {}",
                            pattern_to_output_text(&binding.pattern),
                            expr_to_output_text(&binding.value)
                        )
                    })
                    .collect::<Vec<_>>()
            } else {
                let rendered_bindings = bindings
                    .iter()
                    .map(|binding| {
                        format!(
                            "{} <- {}",
                            pattern_to_output_text(&binding.pattern),
                            expr_to_output_text(&binding.value)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                let clauses = else_clauses
                    .iter()
                    .map(case_clause_to_output_text)
                    .collect::<Vec<_>>()
                    .join("; ");
                if bindings.len() == 1 {
                    vec![format!("let {rendered_bindings} else {{ {clauses} }}")]
                } else {
                    vec![format!(
                        "let {{ {rendered_bindings} }} else {{ {clauses} }}"
                    )]
                }
            };
            if let Some(body) = body {
                parts.push(expr_to_output_text(body));
            }
            parts.join("; ")
        }
        Expr::Call {
            callee,
            type_args,
            args,
            arg_names,
            remote,
            is_fun_value,
        } => {
            let args = args
                .iter()
                .enumerate()
                .map(
                    |(index, arg)| match arg_names.get(index).and_then(Option::as_ref) {
                        Some(name) => format!("{name} = {}", expr_to_output_text(arg)),
                        None => expr_to_output_text(arg),
                    },
                )
                .collect::<Vec<_>>()
                .join(", ");
            let rendered_type_args = if type_args.is_empty() {
                String::new()
            } else {
                format!(
                    "[{}]",
                    type_args
                        .iter()
                        .map(|type_arg| type_arg.text.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            match remote {
                Some(module) => format!(
                    "{}.{}{}({})",
                    module,
                    expr_to_output_text(callee),
                    rendered_type_args,
                    args
                ),
                None if *is_fun_value => format!("{}({})", expr_to_output_text(callee), args),
                None => format!(
                    "{}{}({})",
                    expr_to_output_text(callee),
                    rendered_type_args,
                    args
                ),
            }
        }
        Expr::FieldAccess { value, field } => {
            format!("{}.{}", expr_to_output_text(value), field)
        }
        Expr::Map(fields) => {
            let body = fields
                .iter()
                .map(|field| format!("{}: {}", field.key, expr_to_output_text(&field.value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{body}}}")
        }
        Expr::RecordConstruct { name, fields } => {
            let body = fields
                .iter()
                .map(|field| format!("{}: {}", field.key, expr_to_output_text(&field.value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name} {{{body}}}")
        }
        Expr::RecordUpdate {
            value,
            name,
            fields,
        } => {
            let body = fields
                .iter()
                .map(|field| format!("{}: {}", field.key, expr_to_output_text(&field.value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}#{name}{{{body}}}", expr_to_output_text(value))
        }
        Expr::ConstructorChain { base, record } => {
            format!(
                "{} with {}",
                expr_to_output_text(base),
                expr_to_output_text(record)
            )
        }
        Expr::UnaryOp { op, expr } => {
            format!("{} {}", unary_op_text(op), expr_to_output_text(expr))
        }
        Expr::Cast { expr, target_type } => {
            format!("{} as {}", expr_to_output_text(expr), target_type.text)
        }
        Expr::BinaryOp { op, left, right } => format!(
            "{} {} {}",
            expr_to_output_text(left),
            binary_op_text(op),
            expr_to_output_text(right)
        ),
        Expr::MacroCall { name, args } if args.is_empty() => format!("?{}", name),
        Expr::MacroCall { name, args } => format!(
            "?{}({})",
            name,
            args.iter()
                .map(expr_to_output_text)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::RawMacro {
            name,
            type_args,
            interpolations: _,
            raw,
        } => {
            let rendered_type_args = format_raw_macro_type_args(type_args);
            format!("{}{} {{{}}}", name, rendered_type_args, raw)
        }
        _ => "terlan_interface_constructor".to_string(),
    }
}

/// Renders a pattern as source-like text for syntax-output summaries.
///
/// Inputs:
/// - `pattern`: parse-tree pattern from source.
///
/// Output:
/// - Stable source-like pattern text.
///
/// Transformation:
/// - Recursively serializes the currently supported pattern forms so generated
///   let-expression text can preserve destructuring bindings.
fn pattern_to_output_text(pattern: &Pattern) -> String {
    match pattern {
        Pattern::Wildcard => "_".to_string(),
        Pattern::Var(name) => name.clone(),
        Pattern::Int(value) => value.to_string(),
        Pattern::Float(value) => value.to_string(),
        Pattern::String(value) => quoted_string_literal(value),
        Pattern::StringSegments(segments) => {
            let mut payload = String::new();
            for segment in segments {
                match segment {
                    crate::terlan_syntax::parse_tree::StringPatternSegment::Literal(value) => {
                        payload.push_str(value);
                    }
                    crate::terlan_syntax::parse_tree::StringPatternSegment::Capture(capture) => {
                        payload.push_str("${");
                        payload.push_str(&capture.name);
                        if let Some(annotation) = &capture.annotation {
                            payload.push_str(": ");
                            payload.push_str(&annotation.text);
                        }
                        payload.push('}');
                    }
                }
            }
            quoted_string_literal(&payload)
        }
        Pattern::Atom(value) => value.clone(),
        Pattern::AtomLiteral(value) => format!("Atom[{}]", quoted_string_literal(value)),
        Pattern::NullaryConstructorCall(value) => format!("{value}()"),
        Pattern::Tuple(items) => {
            let parts = items
                .iter()
                .map(pattern_to_output_text)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{parts}}}")
        }
        Pattern::Alias { alias, pattern } => {
            format!("{} = {}", pattern_to_output_text(pattern), alias)
        }
        Pattern::List(items) => {
            let parts = items
                .iter()
                .map(pattern_to_output_text)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{parts}]")
        }
        Pattern::ListCons(head, tail) => {
            format!(
                "[{} | {}]",
                pattern_to_output_text(head),
                pattern_to_output_text(tail)
            )
        }
        Pattern::Map(fields) => {
            let parts = fields
                .iter()
                .map(|field| format!("{}: {}", field.key, pattern_to_output_text(&field.value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{parts}}}")
        }
        Pattern::Record { name, fields } => {
            let parts = fields
                .iter()
                .map(|field| format!("{}: {}", field.key, pattern_to_output_text(&field.value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name} {{{parts}}}")
        }
        Pattern::BinaryLayout { endian, fields } => {
            let parts = fields
                .iter()
                .map(|field| format!("{}: {}", field.name, field.descriptor.text))
                .collect::<Vec<_>>()
                .join(", ");
            format!("Binary[{endian}] {{{parts}}}")
        }
    }
}

fn case_clause_to_output_text(clause: &crate::terlan_syntax::parse_tree::CaseClause) -> String {
    let guard = clause
        .guard
        .as_deref()
        .map(|guard| format!(" where {}", expr_to_output_text(guard)))
        .unwrap_or_default();
    format!(
        "{}{guard} -> {}",
        pattern_to_output_text(&clause.pattern),
        expr_to_output_text(&clause.body)
    )
}

/// Converts parser type text into syntax-output type metadata.
///
/// Inputs:
/// - `ty`: parsed type-expression payload.
///
/// Output:
/// - Stable syntax-output type payload retaining text and source span.
///
/// Transformation:
/// - Copies the parser text/span without invoking semantic type parsing.
pub(super) fn type_expr_output(ty: &TypeExpr) -> SyntaxTypeOutput {
    SyntaxTypeOutput {
        text: ty.text.clone(),
        span: ty.span.into(),
    }
}

/// Renders raw-macro type arguments for source-like expression text.
///
/// Inputs:
/// - `type_args`: optional parsed raw-macro type arguments.
///
/// Output:
/// - Empty text when no type arguments exist, otherwise `[T, U]`.
///
/// Transformation:
/// - Joins preserved type-expression text without semantic normalization.
fn format_raw_macro_type_args(type_args: &[TypeExpr]) -> String {
    if type_args.is_empty() {
        String::new()
    } else {
        format!(
            "[{}]",
            type_args
                .iter()
                .map(|ty| ty.text.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// Returns the source spelling for a unary operator.
///
/// Inputs:
/// - `op`: parsed unary operator.
///
/// Output:
/// - Canonical Terlan source text for the operator.
///
/// Transformation:
/// - Maps syntax enum variants back to their stable textual representation.
pub(super) fn unary_op_text(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "not",
        UnaryOp::Bang => "!",
    }
}

/// Returns the source spelling for a binary operator.
///
/// Inputs: parser binary operator. Output: canonical operator text.
/// Transformation: maps the closed operator enum to its syntax spelling.
pub(crate) fn binary_op_text(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::EqEq => "==",
        BinaryOp::NotEq => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::LtEq => "<=",
        BinaryOp::GtEq => ">=",
        BinaryOp::DivRem => "div",
        BinaryOp::Rem => "rem",
        BinaryOp::Range => "..",
        BinaryOp::In => "in",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::PipeForward => "|>",
    }
}
