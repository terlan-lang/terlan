//! Width-aware ordinary calls and fluent method chains.

use super::{
    format_multiline_delimited, format_named_expr, format_postfix_base, rendered_type_arguments,
    Expr, TypeExpr, DEFAULT_MAX_LINE_LENGTH,
};

/// Formats a call with width-aware argument placement.
pub(super) fn format_call_expression(
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

/// Places a long method chain on successive indented lines.
pub(super) fn format_method_call_chain(expr: &Expr, indent: usize) -> Option<String> {
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
