/// Returns the cursor after an opaque SQL region beginning at `start`.
///
/// Inputs:
/// - `chars`: SQL source characters.
/// - `start`: candidate region start.
///
/// Output:
/// - `Some(index)` after a comment, quoted segment, or PostgreSQL dollar-quoted
///   string; `None` when ordinary SQL text begins at `start`.
///
/// Transformation:
/// - Identifies regions where Terlan `${...}` syntax is SQL data rather than a
///   parameter interpolation. Statement parsing remains owned by `sqlparser`.
pub(crate) fn sql_opaque_region_end(chars: &[char], start: usize) -> Option<usize> {
    let current = *chars.get(start)?;
    let next = chars.get(start + 1).copied();

    match (current, next) {
        ('-', Some('-')) => Some(sql_line_comment_end(chars, start)),
        ('/', Some('*')) => Some(sql_block_comment_end(chars, start)),
        ('\'', _) | ('"', _) => Some(sql_quoted_segment_end(chars, start, current)),
        ('$', _) if has_sql_parameter_boundary(chars, start) => {
            sql_dollar_quoted_segment_end(chars, start)
        }
        _ => None,
    }
}

/// Returns whether `$` begins a standalone PostgreSQL parameter-like token.
pub(crate) fn has_sql_parameter_boundary(chars: &[char], start: usize) -> bool {
    start == 0 || !is_postgres_identifier_char(chars[start - 1])
}

/// Reads one Terlan expression source inside a SQL interpolation.
///
/// Inputs:
/// - `chars`: SQL source characters.
/// - `start`: index immediately after `${`.
///
/// Output:
/// - Source text and the cursor after the closing brace, or `None` when the
///   interpolation is unterminated.
///
/// Transformation:
/// - Tracks nested braces and quoted Terlan strings without parsing either the
///   surrounding SQL statement or the extracted Terlan expression.
pub(crate) fn sql_interpolation_source(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut index = start;
    let mut depth = 1usize;
    let mut quote = None;

    while index < chars.len() {
        let current = chars[index];
        if let Some(current_quote) = quote {
            if current == '\\' && current_quote == '"' && index + 1 < chars.len() {
                index += 2;
                continue;
            }
            if current == current_quote {
                quote = None;
            }
            index += 1;
            continue;
        }

        if current == '"' || current == '\'' {
            quote = Some(current);
            index += 1;
            continue;
        }

        match current {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((chars[start..index].iter().collect(), index + 1));
                }
            }
            _ => {}
        }
        index += 1;
    }

    None
}

fn sql_line_comment_end(chars: &[char], start: usize) -> usize {
    let mut index = start + 2;
    while index < chars.len() {
        if chars[index] == '\n' {
            return index + 1;
        }
        index += 1;
    }
    chars.len()
}

fn sql_block_comment_end(chars: &[char], start: usize) -> usize {
    let mut index = start + 2;
    let mut depth = 1usize;
    while index + 1 < chars.len() {
        match (chars[index], chars[index + 1]) {
            ('/', '*') => {
                depth += 1;
                index += 2;
            }
            ('*', '/') => {
                depth -= 1;
                index += 2;
                if depth == 0 {
                    return index;
                }
            }
            _ => index += 1,
        }
    }
    chars.len()
}

fn sql_quoted_segment_end(chars: &[char], start: usize, quote: char) -> usize {
    let mut index = start + 1;
    while index < chars.len() {
        if chars[index] == quote {
            if chars.get(index + 1) == Some(&quote) {
                index += 2;
                continue;
            }
            return index + 1;
        }
        index += 1;
    }
    chars.len()
}

fn sql_dollar_quoted_segment_end(chars: &[char], start: usize) -> Option<usize> {
    let delimiter_end = dollar_quote_delimiter_end(chars, start)?;
    let delimiter = &chars[start..=delimiter_end];
    let mut index = delimiter_end + 1;

    while index < chars.len() {
        if chars[index..].starts_with(delimiter) {
            return Some(index + delimiter.len());
        }
        index += 1;
    }
    Some(chars.len())
}

fn dollar_quote_delimiter_end(chars: &[char], start: usize) -> Option<usize> {
    let first = *chars.get(start + 1)?;
    if first == '$' {
        return Some(start + 1);
    }
    if !first.is_ascii_alphabetic() && first != '_' {
        return None;
    }

    let mut index = start + 2;
    while chars
        .get(index)
        .is_some_and(|character| character.is_ascii_alphanumeric() || *character == '_')
    {
        index += 1;
    }
    (chars.get(index) == Some(&'$')).then_some(index)
}

fn is_postgres_identifier_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '$'
}
