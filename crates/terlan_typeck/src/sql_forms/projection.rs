use super::scanner::{find_sql_word, find_top_level_sql_word, mask_sql_literals_and_comments};

/// Extracts selected column names from a simple SQL `SELECT` projection.
///
/// Inputs:
/// - `raw`: SQL text preserved from `sql[Row] { ... }`.
///
/// Output:
/// - `Some(Vec<String>)` for plain `SELECT column, table.column AS alias FROM`
///   shapes.
/// - `None` for non-SELECT statements or complex projections that need
///   Postgres-backed validation.
///
/// Transformation:
/// - Locates the top-level `SELECT ... FROM` projection while ignoring quoted
///   strings/comments, splits top-level comma-separated projection items, and
///   converts simple identifiers or aliases into row field names.
pub(crate) fn simple_select_projection_fields(raw: &str) -> Option<Vec<String>> {
    let projection = simple_select_projection_source(raw)?;
    simple_projection_fields_from_source(&projection)
}

/// Extracts row field names from simple row-producing SQL projections.
///
/// Inputs:
/// - `raw`: SQL text preserved from `sql[Row] { ... }`.
///
/// Output:
/// - `Some(Vec<String>)` for simple `SELECT ... FROM` or
///   `... RETURNING ...` projection shapes.
/// - `None` for complex projections that need Postgres-backed validation.
///
/// Transformation:
/// - Reuses the existing simple projection item parser while allowing
///   `RETURNING` statements to participate in the same row-field compatibility
///   checks as simple `SELECT` statements.
pub(crate) fn simple_sql_projection_fields(raw: &str) -> Option<Vec<String>> {
    simple_select_projection_fields(raw).or_else(|| simple_returning_projection_fields(raw))
}

/// Returns the raw source after top-level `SELECT` and before the next clause.
///
/// Inputs:
/// - `raw`: SQL source text captured by the parser.
///
/// Output:
/// - Projection source when the statement starts with a simple top-level
///   `SELECT`.
/// - `None` when the statement is not a simple SELECT shape.
///
/// Transformation:
/// - Masks comments and quoted strings, confirms the first word is `SELECT`,
///   and finds the first top-level clause after the projection. This supports
///   both `SELECT id FROM users` and Postgres expression selects such as
///   `SELECT register_user(...)::text AS id LIMIT 1`.
fn simple_select_projection_source(raw: &str) -> Option<String> {
    let masked = mask_sql_literals_and_comments(raw);
    let select_start = find_sql_word(&masked, "select", 0)?;
    if !masked[..select_start].trim().is_empty() {
        return None;
    }
    let projection_start = select_start + "select".len();
    let projection_end = first_top_level_select_projection_clause(&masked, projection_start)
        .unwrap_or_else(|| raw.len());
    let projection = raw.get(projection_start..projection_end)?;
    let projection = trim_sql_projection_terminator(projection).trim();
    if projection.is_empty() {
        None
    } else {
        Some(projection.to_string())
    }
}

/// Finds the first top-level clause after a SQL `SELECT` projection.
///
/// Inputs:
/// - `masked`: SQL text with strings/comments masked for structural scanning.
/// - `start`: byte offset where projection scanning begins.
///
/// Output:
/// - Byte offset of the first following clause keyword, if any.
///
/// Transformation:
/// - Searches known SQL clause keywords at top level and returns the earliest
///   boundary.
fn first_top_level_select_projection_clause(masked: &str, start: usize) -> Option<usize> {
    [
        "from", "where", "group", "order", "limit", "offset", "fetch", "union",
    ]
    .iter()
    .filter_map(|word| find_top_level_sql_word(masked, word, start))
    .min()
}

/// Returns the raw source after a top-level SQL `RETURNING`.
///
/// Inputs:
/// - `raw`: SQL source text captured by the parser.
///
/// Output:
/// - Projection source when a top-level `RETURNING` clause exists.
/// - `None` when the statement has no clear top-level `RETURNING` clause.
///
/// Transformation:
/// - Masks comments and quoted strings, locates a top-level `RETURNING`, and
///   trims a single statement terminator before projection parsing.
fn simple_returning_projection_source(raw: &str) -> Option<String> {
    let masked = mask_sql_literals_and_comments(raw);
    let returning_start = find_top_level_sql_word(&masked, "returning", 0)?;
    let projection_start = returning_start + "returning".len();
    let projection = raw.get(projection_start..)?;
    let projection = trim_sql_projection_terminator(projection).trim();
    if projection.is_empty() {
        None
    } else {
        Some(projection.to_string())
    }
}

/// Extracts field names from a simple SQL `RETURNING` projection.
///
/// Inputs:
/// - `raw`: SQL source text captured by the parser.
///
/// Output:
/// - `Some(Vec<String>)` for simple `RETURNING id, table.name AS name` shapes.
/// - `None` for complex `RETURNING` projections.
///
/// Transformation:
/// - Locates the `RETURNING` projection and delegates projection item parsing
///   to the same conservative helper used by simple `SELECT` projections.
fn simple_returning_projection_fields(raw: &str) -> Option<Vec<String>> {
    let projection = simple_returning_projection_source(raw)?;
    simple_projection_fields_from_source(&projection)
}

/// Extracts field names from a comma-separated projection source.
///
/// Inputs:
/// - `projection`: raw projection text after `SELECT` or `RETURNING`.
///
/// Output:
/// - Field names for simple projection items, or `None` when any item is too
///   complex for the lightweight checker.
///
/// Transformation:
/// - Splits top-level comma-separated items and maps each item to the row field
///   name implied by simple identifier or alias syntax.
fn simple_projection_fields_from_source(projection: &str) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    for item in split_top_level_sql_projection_items(projection) {
        fields.push(simple_projection_field_name(item.trim())?);
    }
    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

/// Trims a trailing statement terminator from projection source.
///
/// Inputs:
/// - `projection`: projection source after `RETURNING`.
///
/// Output:
/// - Projection source without one trailing semicolon.
///
/// Transformation:
/// - Keeps simple SQL snippets with conventional trailing semicolons
///   compatible with projection parsing without accepting multi-statement SQL.
fn trim_sql_projection_terminator(projection: &str) -> &str {
    projection
        .trim_end()
        .strip_suffix(';')
        .unwrap_or(projection)
}

/// Splits a SELECT projection into top-level comma-separated items.
///
/// Inputs:
/// - `projection`: raw text between `SELECT` and `FROM`.
///
/// Output:
/// - Projection items preserving original item text.
///
/// Transformation:
/// - Walks the projection once, ignores commas inside quotes or parentheses,
///   and trims item boundaries only after splitting.
fn split_top_level_sql_projection_items(projection: &str) -> Vec<&str> {
    let chars = projection.char_indices().collect::<Vec<_>>();
    let mut items = Vec::new();
    let mut depth = 0usize;
    let mut quote = None;
    let mut start = 0usize;
    let mut index = 0usize;

    while index < chars.len() {
        let (byte_index, ch) = chars[index];
        if let Some(current_quote) = quote {
            if ch == current_quote {
                let next_is_doubled = chars
                    .get(index + 1)
                    .is_some_and(|(_, next)| *next == current_quote);
                if next_is_doubled {
                    index += 2;
                    continue;
                }
                quote = None;
            }
            index += 1;
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                items.push(&projection[start..byte_index]);
                start = byte_index + ch.len_utf8();
            }
            _ => {}
        }
        index += 1;
    }

    items.push(&projection[start..]);
    items
}

/// Returns the row field name represented by one simple projection item.
///
/// Inputs:
/// - `item`: one comma-separated SELECT projection item.
///
/// Output:
/// - Field name for simple identifier, qualified identifier, or alias forms.
/// - `None` for expressions, wildcard projections, or complex SQL.
///
/// Transformation:
/// - Prefers explicit aliases from `AS alias` or `column alias`, otherwise
///   derives the field name from the final segment of a simple dotted column
///   reference.
fn simple_projection_field_name(item: &str) -> Option<String> {
    if item.is_empty() || item == "*" {
        return None;
    }

    let words = item.split_whitespace().collect::<Vec<_>>();
    if words.len() >= 3
        && words
            .get(words.len().saturating_sub(2))
            .is_some_and(|word| word.eq_ignore_ascii_case("as"))
    {
        return simple_projection_alias(words[words.len() - 1]);
    }

    let candidate = match words.as_slice() {
        [column] if is_simple_sql_column_reference(column) => *column,
        [column, alias] if is_simple_sql_column_reference(column) => *alias,
        _ => return None,
    };
    simple_projection_alias(candidate)
}

/// Derives a field alias from a simple SQL projection segment.
///
/// Inputs:
/// - `candidate`: column or alias candidate text.
///
/// Output:
/// - Terlan field name when the candidate is a valid SQL identifier.
///
/// Transformation:
/// - Drops table qualifiers and quotes before validating identifier shape.
fn simple_projection_alias(candidate: &str) -> Option<String> {
    let field = candidate.rsplit('.').next()?.trim_matches('"');
    if is_sql_field_identifier(field) {
        Some(field.to_string())
    } else {
        None
    }
}

/// Returns whether a SQL expression is a simple column reference.
///
/// Inputs:
/// - `value`: projection expression text.
///
/// Output:
/// - `true` for identifier or qualified-identifier column references.
///
/// Transformation:
/// - Rejects wildcard and function-call shapes before validating each dotted
///   identifier segment.
fn is_simple_sql_column_reference(value: &str) -> bool {
    if value == "*" || value.contains('(') || value.contains(')') {
        return false;
    }
    value
        .split('.')
        .all(|segment| is_sql_field_identifier(segment.trim_matches('"')))
}

/// Returns whether text is a supported SQL field identifier.
///
/// Inputs:
/// - `field`: candidate field name.
///
/// Output:
/// - `true` for ASCII lowercase/uppercase identifier spelling used by the
///   current lightweight projection checker.
///
/// Transformation:
/// - Keeps the first projection validator conservative and avoids treating SQL
///   expressions as row fields.
fn is_sql_field_identifier(field: &str) -> bool {
    let mut chars = field.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
