//! String interpolation scanning and source-preserving parser representation.

use super::parse_tree::Expr;

const MAX_NESTING: usize = 128;

/// Returns the exclusive end of a quoted string, including expression islands.
pub(super) fn string_end(chars: &[char], start: usize, depth: usize) -> Option<usize> {
    if depth >= MAX_NESTING || chars.get(start) != Some(&'"') {
        return None;
    }
    let mut index = start + 1;
    while let Some(ch) = chars.get(index) {
        match ch {
            '"' => return Some(index + 1),
            '\\' => index += 2,
            '$' if chars.get(index + 1) == Some(&'{') => {
                index = expression_end(chars, index + 2, depth + 1)?;
            }
            _ => index += 1,
        }
    }
    None
}

/// Braces in nested strings and comments do not close expression islands.
fn expression_end(chars: &[char], mut index: usize, depth: usize) -> Option<usize> {
    let mut braces = 1usize;
    while let Some(&ch) = chars.get(index) {
        match ch {
            '"' => index = string_end(chars, index, depth)?,
            '/' if chars.get(index + 1) == Some(&'*') => {
                index += 2;
                while chars.get(index..index + 2) != Some(&['*', '/']) {
                    chars.get(index)?;
                    index += 1;
                }
                index += 2;
            }
            '%' | '/' if ch == '%' || chars.get(index + 1) == Some(&'/') => {
                while chars.get(index).is_some_and(|ch| *ch != '\n') {
                    index += 1;
                }
            }
            '{' => {
                braces += 1;
                index += 1;
            }
            '}' => {
                braces -= 1;
                index += 1;
                if braces == 0 {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }
    None
}

/// Keeps literal spellings separate from parsed, ordinary Terlan expressions.
pub(super) fn parse(source: &str) -> Result<Expr, String> {
    let chars = source.chars().collect::<Vec<_>>();
    let mut segments = Vec::new();
    let mut expressions = Vec::new();
    let mut start = 1;
    let mut index = start;
    while index + 1 < chars.len() {
        if chars[index] == '\\' {
            index += 2;
        } else if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
            let end =
                expression_end(&chars, index + 2, 1).ok_or("unterminated string interpolation")?;
            segments.push(chars[start..index].iter().collect());
            let expression = chars[index + 2..end - 1].iter().collect::<String>();
            if expression.trim().is_empty() {
                return Err("empty string interpolation expression".into());
            }
            expressions.push(
                super::parser::parse_terlan_expr(&expression)
                    .map_err(|error| format!("invalid string interpolation: {}", error.message))?,
            );
            index = end;
            start = end;
        } else {
            index += 1;
        }
    }
    if expressions.is_empty() && !source.contains("\\${") {
        return Ok(Expr::Binary(source.into()));
    }
    segments.push(chars[start..chars.len() - 1].iter().collect());
    Ok(Expr::InterpolatedString {
        segments,
        expressions,
    })
}

pub(super) fn render(
    segments: &[String],
    expressions: &[Expr],
    format: impl Fn(&Expr) -> String,
) -> String {
    let mut output = String::from("\"");
    for (index, segment) in segments.iter().enumerate() {
        output.push_str(segment);
        if let Some(expression) = expressions.get(index) {
            output.push_str("${");
            output.push_str(&format(expression));
            output.push('}');
        }
    }
    output.push('"');
    output
}

/// Only the interpolation escape is consumed here; normal string decoding is
/// still owned by the ordinary literal path. Paired backslashes stay paired.
pub(super) fn literal(segment: &str) -> String {
    let mut chars = segment.chars().peekable();
    let mut output = String::from("\"");
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('$') if chars.peek() == Some(&'{') => output.push('$'),
                Some(next) => {
                    output.push('\\');
                    output.push(next);
                }
                None => output.push('\\'),
            }
        } else {
            output.push(ch);
        }
    }
    output.push('"');
    output
}

/// Each list element evaluates its expression and Show conversion exactly once,
/// in source order. The generic helper enforces the ordinary Show[T] bound.
pub(super) fn lower(segments: &[String], expressions: &[Expr]) -> Expr {
    let mut pieces = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        if !segment.is_empty() {
            pieces.push(Expr::Binary(literal(segment)));
        }
        if let Some(expression) = expressions.get(index) {
            pieces.push(call("show", vec![expression.clone()]));
        }
    }
    if expressions.is_empty() {
        return Expr::Binary(literal(&segments.concat()));
    }
    call("concat", vec![Expr::List(pieces)])
}

fn call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::Call {
        callee: Box::new(Expr::Atom(name.into())),
        type_args: Vec::new(),
        arg_names: vec![None; args.len()],
        args,
        remote: Some("std.core.String".into()),
        is_fun_value: false,
    }
}

#[cfg(test)]
#[path = "string_interpolation_test.rs"]
mod tests;
