use super::declaration_formatting::*;
use super::import_analysis::BINARY_LAYOUT_MAX_INLINE_LENGTH;
use super::precedence::*;
use super::*;

pub(super) fn format_string_pattern(segments: &[StringPatternSegment]) -> String {
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

pub(super) fn format_constructor_pattern(items: &[Pattern]) -> Option<String> {
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
pub(super) fn format_record_pattern_field(field: &MapField) -> String {
    format!("{}: {}", field.key, format_pattern(&field.value))
}

/// Formats a map pattern field.
///
/// Inputs: parsed map pattern field. Output: `key: pattern` text.
/// Transformation: recursively formats the field pattern value.
pub(super) fn format_map_field(field: &MapField) -> String {
    format!("{}: {}", field.key, format_pattern(&field.value))
}

/// Formats a map expression field.
///
/// Inputs: parsed map expression field. Output: `key: expr` text.
/// Transformation: recursively formats the value expression.
pub(super) fn format_map_expr_field(field: &MapExprField) -> String {
    format!(
        "{}: {}",
        field.key,
        format_assignment_child(&field.value, 0)
    )
}

/// Formats a template or record construction field.
///
/// Inputs: parsed expression field. Output: `key: expr` text. Transformation:
/// recursively formats the value expression.
pub(super) fn format_template_expr_field(field: &MapExprField) -> String {
    format!(
        "{}: {}",
        field.key,
        format_assignment_child(&field.value, 0)
    )
}

/// Formats a descriptor-backed binary layout.
pub(super) fn format_binary_layout(endian: &str, fields: &[BinaryLayoutField]) -> String {
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
    for field in &field_text {
        out.push_str("    ");
        out.push_str(field);
        out.push(',');
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

pub(super) fn normalize_type_text(text: &str) -> String {
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

/// Formats one comma-delimited expression family using rustfmt-style layout.
///
/// Short, single-line values remain compact. Once the complete form exceeds
/// the canonical line width or any child is multiline, every item receives its
/// own line and a trailing comma so future insertions produce a one-line diff.
fn format_expr_delimited(prefix: &str, suffix: &str, items: &[Expr], indent: usize) -> String {
    let inline_items = items
        .iter()
        .map(|item| format_assignment_child(item, 0))
        .collect::<Vec<_>>();
    format_rendered_delimited(prefix, suffix, inline_items, items, indent)
}

fn format_rendered_delimited(
    prefix: &str,
    suffix: &str,
    inline_items: Vec<String>,
    source_items: &[Expr],
    indent: usize,
) -> String {
    let inline = format!("{prefix}{}{suffix}", inline_items.join(", "));
    if !inline.contains('\n') && indent * 4 + inline.chars().count() <= DEFAULT_MAX_LINE_LENGTH {
        return inline;
    }

    let rendered = source_items
        .iter()
        .map(|item| format_assignment_child(item, indent + 1))
        .collect::<Vec<_>>();
    format_multiline_delimited(prefix, suffix, &rendered, indent)
}

fn format_multiline_delimited(
    prefix: &str,
    suffix: &str,
    items: &[String],
    indent: usize,
) -> String {
    if items.is_empty() {
        return format!("{prefix}{suffix}");
    }
    let item_spacing = "    ".repeat(indent + 1);
    let closing_spacing = "    ".repeat(indent);
    let mut out = String::from(prefix);
    out.push('\n');
    for item in items {
        out.push_str(&item_spacing);
        out.push_str(item);
        out.push_str(",\n");
    }
    out.push_str(&closing_spacing);
    out.push_str(suffix);
    out
}

fn format_tuple(items: &[Expr], indent: usize) -> String {
    let inline_items = items
        .iter()
        .map(|item| format_assignment_child(item, 0))
        .collect::<Vec<_>>();
    let inline = format!("{{{}}}", inline_items.join(", "));
    if !inline.contains('\n') && indent * 4 + inline.chars().count() <= DEFAULT_MAX_LINE_LENGTH {
        return inline;
    }
    let item_spacing = "    ".repeat(indent + 1);
    let closing_spacing = "    ".repeat(indent);
    let mut out = String::from("{\n");
    for item in items {
        out.push_str(&item_spacing);
        out.push_str(&format_assignment_child(item, indent + 1));
        out.push_str(",\n");
    }
    out.push_str(&closing_spacing);
    out.push('}');
    out
}

fn format_named_expr(name: Option<&String>, value: &Expr, indent: usize) -> String {
    match name {
        Some(name) => format!("{name} = {}", format_assignment_child(value, indent)),
        None => format_assignment_child(value, indent),
    }
}

fn rendered_type_arguments(type_args: &[TypeExpr]) -> String {
    if type_args.is_empty() {
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
    }
}

pub(super) fn format_assignment_child(expr: &Expr, indent: usize) -> String {
    let requires_group = matches!(expr, Expr::Let { .. } | Expr::Sequence(_));
    let rendered = if requires_group {
        format_expr(expr, 0)
    } else {
        format_expr(expr, indent)
    };
    parenthesize_if_needed(rendered, requires_group)
}

fn format_clause_body(expr: &Expr, indent: usize) -> String {
    if let Expr::Let {
        bindings,
        else_clauses,
        body,
    } = expr
    {
        return format_function_body_let(bindings, else_clauses, body.as_deref(), indent);
    }

    let requires_group = matches!(expr, Expr::Sequence(_));
    let rendered = if requires_group {
        format_expr(expr, 0)
    } else {
        format_expr(expr, indent)
    };
    parenthesize_if_needed(rendered, requires_group)
}

fn format_call_expression(
    callee: &Expr,
    type_args: &[TypeExpr],
    args: &[Expr],
    arg_names: &[Option<String>],
    remote: Option<&String>,
    is_fun_value: bool,
    indent: usize,
) -> String {
    let type_arguments = rendered_type_arguments(type_args);
    let local_callee = format_postfix_base(callee, 0);
    let head = match remote {
        Some(remote) => format!("{remote}.{local_callee}{type_arguments}("),
        None if is_fun_value => format!("{local_callee}("),
        None => format!("{local_callee}{type_arguments}("),
    };
    let inline_args = args
        .iter()
        .enumerate()
        .map(|(index, arg)| {
            format_named_expr(arg_names.get(index).and_then(Option::as_ref), arg, 0)
        })
        .collect::<Vec<_>>();
    let inline = format!("{head}{})", inline_args.join(", "));
    if !inline.contains('\n') && indent * 4 + inline.chars().count() <= DEFAULT_MAX_LINE_LENGTH {
        return inline;
    }
    let rendered_args = args
        .iter()
        .enumerate()
        .map(|(index, arg)| {
            format_named_expr(
                arg_names.get(index).and_then(Option::as_ref),
                arg,
                indent + 1,
            )
        })
        .collect::<Vec<_>>();
    format_multiline_delimited(&head, ")", &rendered_args, indent)
}

struct MethodCallSegment<'a> {
    field: &'a str,
    type_args: &'a [TypeExpr],
    args: &'a [Expr],
    arg_names: &'a [Option<String>],
}

fn collect_method_call_chain<'a>(
    expr: &'a Expr,
    segments: &mut Vec<MethodCallSegment<'a>>,
) -> &'a Expr {
    let Expr::Call {
        callee,
        type_args,
        args,
        arg_names,
        remote: None,
        is_fun_value: false,
    } = expr
    else {
        return expr;
    };
    let Expr::FieldAccess { value, field } = callee.as_ref() else {
        return expr;
    };
    let base = collect_method_call_chain(value, segments);
    segments.push(MethodCallSegment {
        field,
        type_args,
        args,
        arg_names,
    });
    base
}

fn format_method_call_chain(expr: &Expr, indent: usize) -> Option<String> {
    let mut segments = Vec::new();
    let base = collect_method_call_chain(expr, &mut segments);
    if segments.is_empty() {
        return None;
    }
    let base_text = format_postfix_base(base, 0);
    let mut inline = base_text.clone();
    for segment in &segments {
        inline.push('.');
        inline.push_str(segment.field);
        inline.push_str(&rendered_type_arguments(segment.type_args));
        inline.push('(');
        inline.push_str(
            &segment
                .args
                .iter()
                .enumerate()
                .map(|(index, arg)| {
                    format_named_expr(
                        segment.arg_names.get(index).and_then(Option::as_ref),
                        arg,
                        0,
                    )
                })
                .collect::<Vec<_>>()
                .join(", "),
        );
        inline.push(')');
    }
    if !inline.contains('\n') && indent * 4 + inline.chars().count() <= DEFAULT_MAX_LINE_LENGTH {
        return None;
    }

    let mut out = format_postfix_base(base, indent);
    let chain_spacing = "    ".repeat(indent + 1);
    for segment in segments {
        let type_arguments = rendered_type_arguments(segment.type_args);
        let inline_args = segment
            .args
            .iter()
            .enumerate()
            .map(|(index, arg)| {
                format_named_expr(
                    segment.arg_names.get(index).and_then(Option::as_ref),
                    arg,
                    0,
                )
            })
            .collect::<Vec<_>>();
        let inline_segment = format!(
            ".{}{}({})",
            segment.field,
            type_arguments,
            inline_args.join(", ")
        );
        out.push('\n');
        out.push_str(&chain_spacing);
        if !inline_segment.contains('\n')
            && (indent + 1) * 4 + inline_segment.chars().count() <= DEFAULT_MAX_LINE_LENGTH
        {
            out.push_str(&inline_segment);
            continue;
        }
        let head = format!(".{}{}(", segment.field, type_arguments);
        let rendered_args = segment
            .args
            .iter()
            .enumerate()
            .map(|(index, arg)| {
                format_named_expr(
                    segment.arg_names.get(index).and_then(Option::as_ref),
                    arg,
                    indent + 2,
                )
            })
            .collect::<Vec<_>>();
        out.push_str(&format_multiline_delimited(
            &head,
            ")",
            &rendered_args,
            indent + 1,
        ));
    }
    Some(out)
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
        Expr::Tuple(items) => format_tuple(items, indent),
        Expr::List(items) => format_expr_delimited("[", "]", items, indent),
        Expr::FixedArray(items) => format_expr_delimited("#[", "]", items, indent),
        Expr::ListCons(head, tail) => {
            format!(
                "[{} | {}]",
                format_assignment_child(head, 0),
                format_assignment_child(tail, 0)
            )
        }
        Expr::Index(value, index) => {
            format!(
                "{}[{}]",
                format_postfix_base(value, 0),
                format_expr(index, 0)
            )
        }
        Expr::IndexAssign {
            collection,
            index,
            value,
        } => format!(
            "{}[{}] = {}",
            format_postfix_base(collection, 0),
            format_expr(index, 0),
            format_assignment_child(value, 0)
        ),
        Expr::Map(fields) => {
            if fields.is_empty() {
                "{}".to_string()
            } else {
                let inline_fields = fields.iter().map(format_map_expr_field).collect::<Vec<_>>();
                let inline = format!("{{{}}}", inline_fields.join(", "));
                if !inline.contains('\n')
                    && indent * 4 + inline.chars().count() <= DEFAULT_MAX_LINE_LENGTH
                {
                    inline
                } else {
                    let rendered = fields
                        .iter()
                        .map(|field| {
                            format!(
                                "{}: {}",
                                field.key,
                                format_assignment_child(&field.value, indent + 1)
                            )
                        })
                        .collect::<Vec<_>>();
                    format_multiline_delimited("{", "}", &rendered, indent)
                }
            }
        }
        Expr::RecordAccess { value, name, field } => {
            format!("{}#{}.{}", format_postfix_base(value, 0), name, field)
        }
        Expr::FieldAccess { value, field } => {
            format!("{}.{}", format_postfix_base(value, 0), field)
        }
        Expr::RecordUpdate {
            value,
            name,
            fields,
        } => {
            let inline_fields = fields
                .iter()
                .map(format_template_expr_field)
                .collect::<Vec<_>>();
            let prefix = format!("{}#{}{{", format_postfix_base(value, 0), name);
            let inline = format!("{prefix}{}}}", inline_fields.join(", "));
            if !inline.contains('\n')
                && indent * 4 + inline.chars().count() <= DEFAULT_MAX_LINE_LENGTH
            {
                inline
            } else {
                let rendered = fields
                    .iter()
                    .map(|field| {
                        format!(
                            "{}: {}",
                            field.key,
                            format_assignment_child(&field.value, indent + 1)
                        )
                    })
                    .collect::<Vec<_>>();
                format_multiline_delimited(&prefix, "}", &rendered, indent)
            }
        }
        Expr::RecordConstruct { name, fields } => {
            let inline_fields = fields
                .iter()
                .map(format_template_expr_field)
                .collect::<Vec<_>>();
            let prefix = format!("{name} {{");
            let inline = format!("{prefix}{}}}", inline_fields.join(", "));
            if !inline.contains('\n')
                && indent * 4 + inline.chars().count() <= DEFAULT_MAX_LINE_LENGTH
            {
                inline
            } else {
                let rendered = fields
                    .iter()
                    .map(|field| {
                        format!(
                            "{}: {}",
                            field.key,
                            format_assignment_child(&field.value, indent + 1)
                        )
                    })
                    .collect::<Vec<_>>();
                format_multiline_delimited(&prefix, "}", &rendered, indent)
            }
        }
        Expr::BinaryLayout { endian, fields } => format_binary_layout(endian, fields),
        Expr::ConstructorChain { base, record } => {
            format!(
                "{} with {}",
                format_constructor_chain_operand(base, false),
                format_constructor_chain_operand(record, true)
            )
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
        } => format_method_call_chain(expr, indent).unwrap_or_else(|| {
            format_call_expression(
                callee,
                type_args,
                args,
                arg_names,
                remote.as_ref(),
                *is_fun_value,
                indent,
            )
        }),
        Expr::Case { scrutinee, clauses } => {
            if let Some(grouped) = super::grouped_cases::format_as_grouped_let(expr, indent) {
                return grouped;
            }
            let mut out = String::new();
            out.push_str(&format!(
                "case {} {{\n",
                format_assignment_child(scrutinee, 0)
            ));
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
            let mut out = format!("try {} {{", format_assignment_child(body, indent + 1));
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
            let clause_spacing = "    ".repeat(indent + 1);
            for (i, clause) in clauses.iter().enumerate() {
                out.push_str(&clause_spacing);
                out.push_str(&format!(
                    "{} -> {}",
                    format_assignment_child(&clause.condition, indent + 1),
                    format_clause_body(&clause.body, indent + 1)
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
        Expr::Fun { clauses } => {
            if let Some(reference) = super::function_references::format_forwarding_lambda(expr) {
                return reference;
            }
            clauses
                .first()
                .map(|clause| {
                    let patterns = clause
                        .patterns
                        .iter()
                        .map(format_pattern)
                        .collect::<Vec<_>>()
                        .join(", ");
                    let body_indent = indent + 1;
                    let body = format_clause_body(&clause.body, body_indent);
                    if body.contains('\n') {
                        format!("({patterns}) ->\n{}{body}", "    ".repeat(body_indent))
                    } else {
                        format!("({patterns}) -> {body}")
                    }
                })
                .unwrap_or_else(|| "() -> {}".to_string())
        }
        Expr::MacroCall { name, args } if args.is_empty() => format!("?{}", name),
        Expr::MacroCall { name, args } => {
            format_expr_delimited(&format!("?{name}("), ")", args, indent)
        }
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
        Expr::BinaryOp { op, left, right } if matches!(op, BinaryOp::And | BinaryOp::Or) => {
            format_boolean_chain(*op, left, right, indent)
        }
        Expr::BinaryOp { op, left, right } => {
            format!(
                "{} {} {}",
                format_binary_operand(left, *op, false),
                binary_op_text(op),
                format_binary_operand(right, *op, true)
            )
        }
        Expr::UnaryOp { op, expr } => match op {
            UnaryOp::Neg => format!("-{}", format_unary_operand(expr)),
            UnaryOp::Not => format!("not {}", format_unary_operand(expr)),
            UnaryOp::Bang => format!("!{}", format_unary_operand(expr)),
        },
        Expr::Cast { expr, target_type } => {
            format!("{} as {}", format_cast_operand(expr), target_type.text)
        }
        Expr::Quote(expr) => format!("quote {}", format_expr(expr, 0)),
        Expr::Unquote(expr) => format!("unquote({})", format_expr(expr, 0)),
        Expr::HtmlBlock(block) => format_html_block(block.macro_kind.name(), &block.nodes, indent),
    }
}

/// Formats a long boolean chain with one operator-led continuation per line.
///
/// Short chains remain inline. Only the left-associated spine of one operator
/// is flattened; mixed precedence and explicitly right-associated expressions
/// retain their parentheses through `format_binary_operand`.
fn format_boolean_chain(op: BinaryOp, left: &Expr, right: &Expr, indent: usize) -> String {
    let inline = format!(
        "{} {} {}",
        format_binary_operand(left, op, false),
        binary_op_text(&op),
        format_binary_operand(right, op, true)
    );
    if !inline.contains('\n') && indent * 4 + inline.chars().count() <= DEFAULT_MAX_LINE_LENGTH {
        return inline;
    }

    let mut operands = Vec::new();
    collect_boolean_operands(left, op, &mut operands);
    operands.push(right);
    let Some(first) = operands.first() else {
        return inline;
    };

    let mut out = format_binary_operand(first, op, false);
    let continuation = "    ".repeat(indent + 1);
    for operand in operands.iter().skip(1) {
        out.push('\n');
        out.push_str(&continuation);
        out.push_str(binary_op_text(&op));
        out.push(' ');
        out.push_str(&format_binary_operand(operand, op, true));
    }
    out
}

fn collect_boolean_operands<'a>(expr: &'a Expr, op: BinaryOp, operands: &mut Vec<&'a Expr>) {
    match expr {
        Expr::BinaryOp {
            op: child_op,
            left,
            right,
        } if matches!(
            (child_op, op),
            (BinaryOp::And, BinaryOp::And) | (BinaryOp::Or, BinaryOp::Or)
        ) =>
        {
            collect_boolean_operands(left, op, operands);
            operands.push(right);
        }
        _ => operands.push(expr),
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
pub(super) fn format_pipe_forward_chain(left: &Expr, right: &Expr, indent: usize) -> String {
    let mut stages = Vec::new();
    collect_pipe_forward_parts(left, &mut stages);
    stages.push(format_binary_operand(right, BinaryOp::PipeForward, true));
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
/// Transformation: flattens only the left-associated spine. A pipe nested on
/// the right remains parenthesized so formatting cannot change evaluation order.
pub(super) fn collect_pipe_forward_parts(expr: &Expr, stages: &mut Vec<String>) {
    match expr {
        Expr::BinaryOp {
            op: BinaryOp::PipeForward,
            left,
            right,
        } => {
            collect_pipe_forward_parts(left, stages);
            stages.push(format_binary_operand(right, BinaryOp::PipeForward, true));
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

pub(super) fn format_let_binding_value(value: &Expr, indent: usize) -> String {
    let rendered = format_assignment_child(value, indent);
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
pub(super) fn format_case_clause(clause: &CaseClause, clause_indent: usize) -> String {
    let mut out = String::new();
    out.push_str(&format_pattern(&clause.pattern));
    if let Some(guard) = &clause.guard {
        out.push(' ');
        out.push_str("where ");
        out.push_str(&format_expr(guard, 0));
    }
    let body_indent = clause_indent + 1;
    let body = format_clause_body(&clause.body, body_indent);
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
