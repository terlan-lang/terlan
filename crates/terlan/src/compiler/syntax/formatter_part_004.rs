
fn format_string_pattern(segments: &[StringPatternSegment]) -> String {
    let mut payload = String::new();
    for segment in segments {
        match segment {
            StringPatternSegment::Literal(value) => {
                let quoted = super::quoted_string_literal(value);
                payload.push_str(quoted.trim_matches('"'));
            }
            StringPatternSegment::Capture(capture) => {
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
    format!("\"{payload}\"")
}

fn format_constructor_pattern(items: &[Pattern]) -> Option<String> {
    let [Pattern::Atom(name), rest @ ..] = items else {
        return None;
    };
    if !name.chars().next().map(char::is_uppercase).unwrap_or(false) {
        return None;
    }
    if rest.is_empty() {
        return Some(name.clone());
    }
    let args = rest
        .iter()
        .map(format_pattern)
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!("{name}({args})"))
}

/// Formats a record pattern field.
///
/// Inputs: parsed pattern field. Output: `key: pattern` text. Transformation:
/// recursively formats the field pattern value.
fn format_record_pattern_field(field: &MapField) -> String {
    format!("{}: {}", field.key, format_pattern(&field.value))
}

/// Formats a map pattern field.
///
/// Inputs: parsed map pattern field. Output: `key: pattern` text.
/// Transformation: recursively formats the field pattern value.
fn format_map_field(field: &MapField) -> String {
    format!("{}: {}", field.key, format_pattern(&field.value))
}

/// Formats a map expression field.
///
/// Inputs: parsed map expression field. Output: `key: expr` text.
/// Transformation: recursively formats the value expression.
fn format_map_expr_field(field: &MapExprField) -> String {
    format!("{}: {}", field.key, format_expr(&field.value, 0))
}

/// Formats a template or record construction field.
///
/// Inputs: parsed expression field. Output: `key: expr` text. Transformation:
/// recursively formats the value expression.
fn format_template_expr_field(field: &MapExprField) -> String {
    format!("{}: {}", field.key, format_expr(&field.value, 0))
}

/// Formats a descriptor-backed binary layout.
fn format_binary_layout(endian: &str, fields: &[BinaryLayoutField]) -> String {
    let field_text = fields
        .iter()
        .map(|field| format!("{}: {}", field.name, format_type_expr(&field.descriptor)))
        .collect::<Vec<_>>();
    let inline_body = field_text.join(", ");
    let inline = format!("Binary[{endian}] {{{inline_body}}}");
    if inline.chars().count() <= BINARY_LAYOUT_MAX_INLINE_LENGTH {
        return inline;
    }

    let mut out = format!("Binary[{endian}] {{\n");
    for (index, field) in field_text.iter().enumerate() {
        out.push_str("    ");
        out.push_str(field);
        if index + 1 < field_text.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push('}');
    out
}

/// Formats a type expression.
///
/// Inputs: parsed type expression. Output: source type text. Transformation:
/// trims whitespace and substitutes `Dynamic` for empty type text.
pub(super) fn format_type_expr(ty: &TypeExpr) -> String {
    let mut text = normalize_type_text(&ty.text);
    if text.is_empty() {
        text.push_str("Dynamic");
    }
    text
}

fn normalize_type_text(text: &str) -> String {
    let mut normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    for (from, to) in [
        (" ,", ","),
        (" :", ":"),
        (" ]", "]"),
        (" )", ")"),
        (" }", "}"),
        ("[ ", "["),
        ("( ", "("),
        ("{ ", "{"),
    ] {
        while normalized.contains(from) {
            normalized = normalized.replace(from, to);
        }
    }

    let mut out = String::new();
    let mut chars = normalized.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == ',' {
            out.push(',');
            while matches!(chars.peek(), Some(' ')) {
                chars.next();
            }
            if !matches!(chars.peek(), None | Some(']') | Some(')') | Some('}')) {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

/// Formats an expression.
///
/// Inputs: parsed expression and indentation level. Output: canonical
/// expression text. Transformation: recursively formats expression variants and
/// uses indentation for block-like forms.
pub(super) fn format_expr(expr: &Expr, indent: usize) -> String {
    let spacing = "    ".repeat(indent);
    match expr {
        Expr::Int(value) => value.to_string(),
        Expr::Float(value) => format_float_literal(*value),
        Expr::Atom(value) => value.clone(),
        Expr::AtomLiteral(value) => format!("Atom[{}]", super::quoted_string_literal(value)),
        Expr::Binary(value) => value.clone(),
        Expr::Var(name) => name.clone(),
        Expr::Tuple(items) => {
            let body = items
                .iter()
                .map(|item| format_expr(item, 0))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{}}}", body)
        }
        Expr::List(items) => {
            let body = items
                .iter()
                .map(|item| format_expr(item, 0))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", body)
        }
        Expr::FixedArray(items) => {
            let body = items
                .iter()
                .map(|item| format_expr(item, 0))
                .collect::<Vec<_>>()
                .join(", ");
            format!("#[{}]", body)
        }
        Expr::ListCons(head, tail) => {
            format!("[{} | {}]", format_expr(head, 0), format_expr(tail, 0))
        }
        Expr::Index(value, index) => {
            format!("{}[{}]", format_expr(value, 0), format_expr(index, 0))
        }
        Expr::IndexAssign {
            collection,
            index,
            value,
        } => format!(
            "{}[{}] = {}",
            format_expr(collection, 0),
            format_expr(index, 0),
            format_expr(value, 0)
        ),
        Expr::Map(fields) => {
            if fields.is_empty() {
                "{}".to_string()
            } else {
                let body = fields
                    .iter()
                    .map(format_map_expr_field)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", body)
            }
        }
        Expr::RecordAccess { value, name, field } => {
            format!("{}#{}.{}", format_expr(value, 0), name, field)
        }
        Expr::FieldAccess { value, field } => {
            format!("{}.{}", format_expr(value, 0), field)
        }
        Expr::RecordUpdate {
            value,
            name,
            fields,
        } => {
            let body = fields
                .iter()
                .map(format_template_expr_field)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}#{}{{{}}}", format_expr(value, 0), name, body)
        }
        Expr::RecordConstruct { name, fields } => {
            let body = fields
                .iter()
                .map(format_template_expr_field)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} {{{}}}", name, body)
        }
        Expr::BinaryLayout { endian, fields } => format_binary_layout(endian, fields),
        Expr::ConstructorChain { base, record } => {
            format!("{} with {}", format_expr(base, 0), format_expr(record, 0))
        }
        Expr::ListComprehension {
            expr,
            generators,
            guards,
        } => format_list_comprehension(expr, generators, guards),
        Expr::Let {
            bindings,
            else_clauses,
            body,
        } => format_let_expr(bindings, else_clauses, body.as_deref(), indent),
        Expr::Sequence(expressions) => {
            let parts = expressions
                .iter()
                .map(|expr| format_expr(expr, 0))
                .collect::<Vec<_>>();
            if indent > 0 {
                format_statement_parts(parts, indent)
            } else {
                parts.join("; ")
            }
        }
        Expr::Call {
            callee,
            type_args,
            args,
            arg_names,
            remote,
            is_fun_value,
        } => {
            let args_text = args
                .iter()
                .enumerate()
                .map(
                    |(index, arg)| match arg_names.get(index).and_then(Option::as_ref) {
                        Some(name) => format!("{name} = {}", format_expr(arg, 0)),
                        None => format_expr(arg, 0),
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
            if let Some(remote) = remote {
                format!(
                    "{}.{}{}({})",
                    remote,
                    format_expr(callee, 0),
                    rendered_type_args,
                    args_text
                )
            } else if *is_fun_value {
                format!("{}({})", format_expr(callee, 0), args_text)
            } else {
                format!(
                    "{}{}({})",
                    format_expr(callee, 0),
                    rendered_type_args,
                    args_text
                )
            }
        }
        Expr::Case { scrutinee, clauses } => {
            let mut out = String::new();
            out.push_str(&format!("case {} {{\n", format_expr(scrutinee, 0)));
            let clause_spacing = "    ".repeat(indent + 1);
            for (i, clause) in clauses.iter().enumerate() {
                out.push_str(&clause_spacing);
                out.push_str(&format_case_clause(clause, indent + 1));
                if i + 1 < clauses.len() {
                    out.push(';');
                }
                out.push('\n');
            }
            out.push_str(&spacing);
            out.push('}');
            out
        }
        Expr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            let mut out = format!("try {} {{", format_expr(body, indent + 1));
            if !of_clauses.is_empty() {
                out.push('\n');
                let clause_spacing = "    ".repeat(indent + 1);
                for (i, clause) in of_clauses.iter().enumerate() {
                    out.push_str(&clause_spacing);
                    out.push_str(&format_case_clause(clause, indent + 1));
                    if i + 1 < of_clauses.len() {
                        out.push(';');
                    }
                    out.push('\n');
                }
            }
            if !catch_clauses.is_empty() {
                out.push_str("catch\n");
                let clause_spacing = "    ".repeat(indent + 1);
                for (i, clause) in catch_clauses.iter().enumerate() {
                    out.push_str(&clause_spacing);
                    out.push_str(&format_case_clause(clause, indent + 1));
                    if i + 1 < catch_clauses.len() {
                        out.push(';');
                    }
                    out.push('\n');
                }
            }
            if let Some(after) = after_clause {
                out.push_str("after ");
                out.push_str(&spacing);
                out.push_str(&format!(
                    "{} -> {}\n",
                    format_expr(&after.trigger, indent + 1),
                    format_expr(&after.body, indent + 1)
                ));
            }
            out.push_str(&spacing);
            out.push('}');
            out
        }
        Expr::If { clauses } => {
            let mut out = String::from("if {\n");
            for (i, clause) in clauses.iter().enumerate() {
                out.push_str(&spacing);
                out.push_str(&format!(
                    "{} -> {}",
                    format_expr(&clause.condition, 0),
                    format_expr(&clause.body, indent + 1)
                ));
                if i + 1 < clauses.len() {
                    out.push(';');
                }
                out.push('\n');
            }
            out.push_str(&spacing);
            out.push('}');
            out
        }
        Expr::Fun { clauses } => clauses
            .first()
            .map(|clause| {
                format!(
                    "({}) -> {}",
                    clause
                        .patterns
                        .iter()
                        .map(format_pattern)
                        .collect::<Vec<_>>()
                        .join(", "),
                    format_expr(&clause.body, indent + 1)
                )
            })
            .unwrap_or_else(|| "() -> {}".to_string()),
        Expr::MacroCall { name, args } if args.is_empty() => format!("?{}", name),
        Expr::MacroCall { name, args } => format!(
            "?{}({})",
            name,
            args.iter()
                .map(|arg| format_expr(arg, 0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::RawMacro {
            name,
            type_args,
            interpolations: _,
            raw,
        } => {
            let rendered_type_args = if type_args.is_empty() {
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
            };
            format!("{}{} {{{}}}", name, rendered_type_args, raw)
        }
        Expr::BinaryOp { op, left, right } if matches!(op, BinaryOp::PipeForward) && indent > 0 => {
            format_pipe_forward_chain(left, right, indent)
        }
        Expr::BinaryOp { op, left, right } => {
            format!(
                "{} {} {}",
                format_expr(left, 0),
                binary_op_text(op),
                format_expr(right, 0)
            )
        }
        Expr::UnaryOp { op, expr } => match op {
            UnaryOp::Neg => format!("-{}", format_expr(expr, 0)),
            UnaryOp::Not => format!("not {}", format_expr(expr, 0)),
            UnaryOp::Bang => format!("!{}", format_expr(expr, 0)),
        },
        Expr::Cast { expr, target_type } => {
            format!("{} as {}", format_expr(expr, 0), target_type.text)
        }
        Expr::Quote(expr) => format!("quote {}", format_expr(expr, 0)),
        Expr::Unquote(expr) => format!("unquote({})", format_expr(expr, 0)),
        Expr::HtmlBlock(block) => format_html_block(block.macro_kind.name(), &block.nodes, indent),
    }
}

/// Formats an explicit `|>` expression as one pipeline stage per line.
///
/// Inputs:
/// - `left`: left side of the pipe expression.
/// - `right`: right side of the pipe expression.
/// - `indent`: indentation level for continuation pipe stages.
///
/// Output:
/// - Canonical multi-line pipe expression.
///
/// Transformation:
/// - Flattens nested left-associated pipe expressions so source that already
///   uses `|>` converges to one stage per line without rewriting ordinary
///   function calls into pipes.
fn format_pipe_forward_chain(left: &Expr, right: &Expr, indent: usize) -> String {
    let mut stages = Vec::new();
    collect_pipe_forward_parts(left, &mut stages);
    collect_pipe_forward_parts(right, &mut stages);
    let continuation = "    ".repeat(indent);
    let mut out = stages
        .first()
        .cloned()
        .unwrap_or_else(|| format_expr(left, 0));
    for stage in stages.iter().skip(1) {
        out.push('\n');
        out.push_str(&continuation);
        out.push_str("|> ");
        out.push_str(stage);
    }
    out
}

/// Collects formatted stages from a pipe expression.
///
/// Inputs:
/// - `expr`: expression that may be a nested pipe.
/// - `stages`: accumulator of already formatted pipe stages.
///
/// Output: appends formatted stages to `stages`.
///
/// Transformation: recursively flattens `|>` nodes and formats non-pipe
/// expressions without continuation indentation so the caller owns line layout.
fn collect_pipe_forward_parts(expr: &Expr, stages: &mut Vec<String>) {
    match expr {
        Expr::BinaryOp {
            op: BinaryOp::PipeForward,
            left,
            right,
        } => {
            collect_pipe_forward_parts(left, stages);
            collect_pipe_forward_parts(right, stages);
        }
        _ => stages.push(format_expr(expr, 0)),
    }
}

pub(super) fn format_let_binding_assignment(
    prefix: &str,
    pattern: &Pattern,
    value: &Expr,
    indent: usize,
) -> String {
    let value_text = format_let_binding_value(value, indent + 1);
    if value_text.contains('\n') {
        format!(
            "{}{} =\n{}{}",
            prefix,
            format_pattern(pattern),
            "    ".repeat(indent + 1),
            value_text
        )
    } else {
        format!("{}{} = {}", prefix, format_pattern(pattern), value_text)
    }
}

fn format_let_binding_value(value: &Expr, indent: usize) -> String {
    let rendered = format_expr(value, indent);
    match value {
        Expr::Fun { .. } => format!("({rendered})"),
        _ => rendered,
    }
}

pub(super) fn format_statement_parts(parts: Vec<String>, indent: usize) -> String {
    let continuation_indent = "    ".repeat(indent);
    let last_index = parts.len().saturating_sub(1);
    parts
        .into_iter()
        .enumerate()
        .map(|(index, part)| {
            let suffix = if index == last_index { "" } else { ";" };
            if index == 0 {
                format!("{part}{suffix}")
            } else {
                format!("{continuation_indent}{part}{suffix}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Formats a case/try clause.
///
/// Inputs: parsed case clause. Output: `pattern [where guard] -> body` text.
/// Transformation: formats the pattern, optional guard, and body expression.
fn format_case_clause(clause: &CaseClause, clause_indent: usize) -> String {
    let mut out = String::new();
    out.push_str(&format_pattern(&clause.pattern));
    if let Some(guard) = &clause.guard {
        out.push(' ');
        out.push_str("where ");
        out.push_str(&format_expr(guard, 0));
    }
    let body_indent = clause_indent + 1;
    let body = format_expr(&clause.body, body_indent);
    let inline_width = clause_indent * 4 + out.chars().count() + 4 + body.chars().count();
    if body.contains('\n') || inline_width >= DEFAULT_MAX_LINE_LENGTH {
        out.push_str(" ->\n");
        out.push_str(&"    ".repeat(body_indent));
        out.push_str(&body);
    } else {
        out.push_str(" -> ");
        out.push_str(&body);
    }
    out
}

#[cfg(test)]
#[path = "formatter_test.rs"]
mod formatter_test;

#[cfg(test)]
#[path = "formatter_let_else_test.rs"]
mod formatter_let_else_test;
