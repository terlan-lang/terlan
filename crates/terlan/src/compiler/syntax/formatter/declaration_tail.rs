use super::*;

/// Formats a raw/unsupported declaration.
///
/// Inputs: raw declaration payload. Output: raw text with terminating period.
/// Transformation: preserves raw declaration text exactly apart from appending
/// the declaration terminator, except for shape declarations whose parser raw
/// scanner stores token-spaced text before shape expansion exists.
pub(in crate::compiler::syntax::formatter) fn format_raw_decl(raw: &UnsupportedDecl) -> String {
    let text = raw.text.trim_end().strip_suffix('.').unwrap_or(&raw.text);
    if raw.kind == "shape" {
        return format!("{}.", normalize_shape_raw_text(text));
    }
    format!("{text}.")
}

/// Formats a reserved shape-synonym declaration.
///
/// Inputs: parsed shape declaration. Output: canonical shape source text.
/// Transformation: reuses raw-shape text normalization until semantic shape
/// expansion owns body and guard formatting.
pub(in crate::compiler::syntax::formatter) fn format_shape_decl(shape: &ShapeDecl) -> String {
    let mut text = String::new();
    if shape.is_public {
        text.push_str("pub ");
    }
    text.push_str("shape ");
    text.push_str(&shape.name);
    text.push('(');
    if !shape.params.is_empty() {
        text.push_str(&shape.params.join(", "));
    }
    text.push(')');
    text.push_str(" = ");
    text.push_str(&shape.body);
    if let Some(guard) = &shape.guard {
        text.push_str(" where ");
        text.push_str(guard);
    }
    format!("{}.", normalize_shape_raw_text(&text))
}

/// Normalizes parse-preserved shape declaration text.
fn normalize_shape_raw_text(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escape_next = false;

    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escape_next {
                escape_next = false;
            } else if ch == '\\' {
                escape_next = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch.is_whitespace() {
            if !out.ends_with(' ')
                && !out.ends_with('(')
                && !out.ends_with('[')
                && !out.ends_with('{')
            {
                out.push(' ');
            }
            continue;
        }

        match ch {
            '(' | '[' => {
                if !out.ends_with("= ") {
                    trim_trailing_space(&mut out);
                }
                out.push(ch);
                consume_following_spaces(&mut chars);
            }
            '{' => {
                out.push(ch);
                consume_following_spaces(&mut chars);
            }
            ')' | ']' | '}' => {
                trim_trailing_space(&mut out);
                out.push(ch);
            }
            ',' => {
                trim_trailing_space(&mut out);
                out.push(',');
                out.push(' ');
                consume_following_spaces(&mut chars);
            }
            _ => out.push(ch),
        }
    }

    out.trim().to_string()
}

fn trim_trailing_space(out: &mut String) {
    if out.ends_with(' ') {
        out.pop();
    }
}

fn consume_following_spaces<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    while chars.peek().is_some_and(|ch| ch.is_whitespace()) {
        chars.next();
    }
}

/// Formats one multi-clause function clause.
pub(super) fn format_function_clause(function: &FunctionDecl, clause: &FunctionClause) -> String {
    let mut out = String::new();
    out.push_str(&function.name);
    out.push('(');
    out.push_str(
        &clause
            .patterns
            .iter()
            .map(format_pattern)
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push(')');

    if let Some(guard) = &clause.guard {
        out.push(' ');
        out.push_str("where");
        out.push(' ');
        out.push_str(&format_expr(guard, 1));
    }

    out.push_str(" ->\n    ");
    out.push_str(&format_expr(&clause.body, 1));
    out
}

/// Returns whether a single function clause duplicates the declaration header.
pub(super) fn single_clause_matches_header(function: &FunctionDecl) -> bool {
    let Some(clause) = function.clauses.first() else {
        return false;
    };

    if clause.patterns.len() != function.params.len() {
        return false;
    }

    clause
        .patterns
        .iter()
        .zip(function.params.iter())
        .all(|(pattern, param)| match pattern {
            Pattern::Var(name) => name == &param.name,
            _ => false,
        })
}

pub(super) fn single_clause_signature_patterns(function: &FunctionDecl) -> Option<&[Pattern]> {
    let clause = function.clauses.first()?;
    if clause.guard.is_some() || clause.patterns.len() != function.params.len() {
        return None;
    }
    let has_non_header_pattern =
        clause
            .patterns
            .iter()
            .zip(function.params.iter())
            .any(|(pattern, param)| match pattern {
                Pattern::Var(name) => name != &param.name,
                _ => true,
            });
    has_non_header_pattern.then_some(clause.patterns.as_slice())
}
