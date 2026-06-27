use super::super::*;

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
/// - Scans SQL text, skips basic quoted SQL strings and SQL comments, parses
///   each interpolation body through the ordinary Terlan expression parser, and
///   reports malformed islands as parse errors before later SQL
///   validation/lowering runs.
pub(super) fn parse_sql_interpolations(raw: &str, fallback_span: Span) -> ParseResult<Vec<Expr>> {
    let chars = raw.chars().collect::<Vec<_>>();
    let mut expressions = Vec::new();
    let mut index = 0usize;
    let mut quote = None;

    while index < chars.len() {
        let ch = chars[index];
        if let Some(current_quote) = quote {
            if ch == current_quote {
                if current_quote == '\'' && chars.get(index + 1) == Some(&'\'') {
                    index += 2;
                    continue;
                }
                quote = None;
            }
            index += 1;
            continue;
        }

        if ch == '-' && chars.get(index + 1) == Some(&'-') {
            index = skip_sql_line_comment(&chars, index);
            continue;
        }

        if ch == '/' && chars.get(index + 1) == Some(&'*') {
            index = skip_sql_block_comment(&chars, index);
            continue;
        }

        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            index += 1;
            continue;
        }

        if ch == '$' && chars.get(index + 1) == Some(&'{') {
            let (expr_source, next_index) =
                read_sql_interpolation_source(&chars, index + 2, fallback_span)?;
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

/// Skips a SQL line comment while scanning interpolation islands.
///
/// Inputs:
/// - `chars`: SQL body characters.
/// - `start`: index of the first `-` in a `--` comment opener.
///
/// Output:
/// - Index immediately after the newline or at end of input.
///
/// Transformation:
/// - Advances across comment text so `${...}` inside SQL comments is not parsed
///   as a Terlan parameter expression.
fn skip_sql_line_comment(chars: &[char], start: usize) -> usize {
    let mut index = start + 2;
    while index < chars.len() {
        if chars[index] == '\n' {
            return index + 1;
        }
        index += 1;
    }
    chars.len()
}

/// Skips a SQL block comment while scanning interpolation islands.
///
/// Inputs:
/// - `chars`: SQL body characters.
/// - `start`: index of the `/` in a `/*` comment opener.
///
/// Output:
/// - Index immediately after the closing `*/`, or at end of input when the
///   comment is unterminated.
///
/// Transformation:
/// - Advances across block comment text so `${...}` inside SQL comments is not
///   parsed as a Terlan parameter expression.
fn skip_sql_block_comment(chars: &[char], start: usize) -> usize {
    let mut index = start + 2;
    while index + 1 < chars.len() {
        if chars[index] == '*' && chars[index + 1] == '/' {
            return index + 2;
        }
        index += 1;
    }
    chars.len()
}

/// Reads the source inside one SQL `${...}` interpolation.
///
/// Inputs:
/// - `chars`: raw SQL body as characters.
/// - `start`: cursor just after the `${` opener.
/// - `fallback_span`: parser span used for diagnostics.
///
/// Output:
/// - Interpolation source text and the next cursor after its closing brace.
///
/// Transformation:
/// - Tracks nested braces and quoted Terlan strings so expression bodies can
///   contain ordinary Terlan expression syntax before being parsed separately.
fn read_sql_interpolation_source(
    chars: &[char],
    start: usize,
    fallback_span: Span,
) -> ParseResult<(String, usize)> {
    let mut index = start;
    let mut depth = 1usize;
    let mut quote = None;

    while index < chars.len() {
        let ch = chars[index];
        if let Some(current_quote) = quote {
            if ch == '\\' && current_quote == '"' && index + 1 < chars.len() {
                index += 2;
                continue;
            }
            if ch == current_quote {
                quote = None;
            }
            index += 1;
            continue;
        }

        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            index += 1;
            continue;
        }

        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let source = chars[start..index].iter().collect::<String>();
                    return Ok((source, index + 1));
                }
            }
            _ => {}
        }
        index += 1;
    }

    Err(ParseError {
        message: "unterminated SQL interpolation expression".to_string(),
        span: fallback_span,
    })
}
