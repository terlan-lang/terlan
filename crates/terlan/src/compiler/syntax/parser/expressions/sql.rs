use super::super::*;
use crate::terlan_syntax::sql_opaque_region_end;
use crate::terlan_syntax::sql_regions::sql_interpolation_source;

/// Extracts typed SQL interpolation expressions from a raw SQL body.
///
/// Inputs:
/// - `raw`: SQL text preserved inside `sql[Row] { ... }`.
/// - `fallback_span`: parser span used when interpolation syntax is malformed.
///
/// Output:
/// - Ordered Terlan expressions parsed from unquoted `${...}` islands.
///
/// Transformation:
/// - Uses the shared SQL opaque-region scanner for quoted strings and comments,
///   parses each interpolation body through the ordinary Terlan expression
///   parser, and reports malformed islands before later SQL validation runs.
pub(super) fn parse_sql_interpolations(raw: &str, fallback_span: Span) -> ParseResult<Vec<Expr>> {
    let chars = raw.chars().collect::<Vec<_>>();
    let mut expressions = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        if let Some(next_index) = sql_opaque_region_end(&chars, index) {
            index = next_index;
            continue;
        }

        if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
            let (expr_source, next_index) =
                sql_interpolation_source(&chars, index + 2).ok_or(ParseError {
                    message: "unterminated SQL interpolation expression".to_string(),
                    span: fallback_span,
                })?;
            if expr_source.trim().is_empty() {
                return Err(ParseError {
                    message: "empty SQL interpolation expression".to_string(),
                    span: fallback_span,
                });
            }
            expressions.push(parse_terlan_expr(expr_source.trim())?);
            index = next_index;
            continue;
        }

        index += 1;
    }

    Ok(expressions)
}
