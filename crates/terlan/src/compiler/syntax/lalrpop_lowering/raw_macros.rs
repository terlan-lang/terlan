use super::{
    super::{
        html_syntax::parse_html_nodes,
        lalrpop_boundary::parse_lalrpop_expression,
        parse_tree::{BuiltinBlockMacro, Expr, HtmlBlockExpr, TypeExpr},
        span::Span,
        sql_regions::{sql_interpolation_source, sql_opaque_region_end},
    },
    LalrpopLoweringContext, LalrpopLoweringError, LalrpopLoweringResult,
};
use crate::terlan_syntax::lalrpop_syntax::LalrpopSyntaxNode;

pub(super) fn lower(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Expr> {
    let source = context.text(node.span);
    let opening = payload_opening(source)
        .ok_or_else(|| context.error(node, "raw macro is missing its payload"))?;
    let closing = source
        .rfind('}')
        .filter(|closing| *closing > opening)
        .ok_or_else(|| context.error(node, "raw macro payload is unterminated"))?;
    let head = source[..opening].trim();
    let raw = source[opening + 1..closing].to_string();
    let (name, type_args) = parse_head(node, head)?;
    if name == "html" {
        return Ok(Expr::HtmlBlock(HtmlBlockExpr {
            macro_kind: BuiltinBlockMacro::Html,
            nodes: parse_html_nodes(&raw),
            raw,
        }));
    }
    let interpolations = if name == "sql" && !type_args.is_empty() {
        sql_interpolations(context, node, &raw)?
    } else {
        Vec::new()
    };
    Ok(Expr::RawMacro {
        name,
        type_args,
        interpolations,
        raw,
    })
}

fn payload_opening(source: &str) -> Option<usize> {
    let mut brackets = 0usize;
    for (index, character) in source.char_indices() {
        match character {
            '[' => brackets += 1,
            ']' => brackets = brackets.saturating_sub(1),
            '{' if brackets == 0 => return Some(index),
            _ => {}
        }
    }
    None
}

fn parse_head(
    node: &LalrpopSyntaxNode,
    head: &str,
) -> LalrpopLoweringResult<(String, Vec<TypeExpr>)> {
    if let Some(opening) = head.find('[') {
        let closing = head.rfind(']').ok_or_else(|| LalrpopLoweringError {
            message: "raw macro type arguments are unterminated".to_string(),
            span: node.span,
        })?;
        let name = head[..opening].trim().to_string();
        let type_source = head[opening + 1..closing].trim();
        if name != "sql" || type_source.is_empty() {
            return Err(LalrpopLoweringError {
                message: "typed raw macro syntax is reserved for sql".to_string(),
                span: node.span,
            });
        }
        return Ok((
            name,
            vec![TypeExpr {
                text: type_source.to_string(),
                span: node.span,
            }],
        ));
    }
    Ok((head.to_string(), Vec::new()))
}

fn sql_interpolations(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    raw: &str,
) -> LalrpopLoweringResult<Vec<Expr>> {
    let chars = raw.chars().collect::<Vec<_>>();
    let mut expressions = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        if let Some(next) = sql_opaque_region_end(&chars, index) {
            index = next;
            continue;
        }
        if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
            let (source, next) = sql_interpolation_source(&chars, index + 2)
                .ok_or_else(|| context.error(node, "unterminated SQL interpolation expression"))?;
            if source.trim().is_empty() {
                return Err(context.error(node, "empty SQL interpolation expression"));
            }
            let generated =
                parse_lalrpop_expression(source.trim()).map_err(|error| LalrpopLoweringError {
                    message: error.message,
                    span: Span::new(
                        node.span.start + error.span.start,
                        node.span.start + error.span.end,
                    ),
                })?;
            expressions
                .push(LalrpopLoweringContext::new(source.trim()).expression(&generated.root)?);
            index = next;
            continue;
        }
        index += 1;
    }
    Ok(expressions)
}
