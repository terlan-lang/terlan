/// Removes insignificant whitespace from a type expression.
///
/// Inputs:
/// - `input`: raw type-expression text.
///
/// Output:
/// - Text with whitespace outside quoted strings removed.
///
/// Transformation:
/// - Preserves quoted payloads and escape sequences while compacting structural
///   type syntax.
pub(crate) fn compact_spaces(input: &str) -> String {
    let mut output = String::new();
    let mut quote = None;
    let mut escape = false;

    for ch in input.chars() {
        if let Some(quote_ch) = quote {
            output.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == quote_ch {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                output.push(ch);
            }
            ch if ch.is_whitespace() => {}
            _ => output.push(ch),
        }
    }

    output
}

/// Removes one pair of parentheses that wraps an entire expression.
///
/// Inputs:
/// - `input`: candidate parenthesized text.
///
/// Output:
/// - Inner text when the outer parentheses enclose the whole input.
///
/// Transformation:
/// - Tracks parenthesis depth and rejects partial wrapping.
pub(crate) fn strip_wrapping_parens(input: &str) -> Option<&str> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'(') || bytes.last() != Some(&b')') {
        return None;
    }

    let mut depth = 0usize;
    for (idx, byte) in bytes.iter().enumerate() {
        match *byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && idx != bytes.len() - 1 {
                    return None;
                }
            }
            _ => {}
        }
    }

    Some(&input[1..input.len() - 1])
}

/// Splits a function type at a top-level `->`.
///
/// Inputs:
/// - `input`: compacted type-expression text.
///
/// Output:
/// - Parameter-side and return-side text when a top-level arrow exists.
///
/// Transformation:
/// - Ignores arrows nested inside parentheses, brackets, or braces.
pub(crate) fn split_top_level_arrow(input: &str) -> Option<(String, String)> {
    let bytes = input.as_bytes();
    let mut p_depth = 0usize;
    let mut b_depth = 0usize;
    let mut t_depth = 0usize;

    for i in 0..bytes.len() {
        match bytes[i] {
            b'(' => p_depth += 1,
            b')' => p_depth = p_depth.saturating_sub(1),
            b'[' => b_depth += 1,
            b']' => b_depth = b_depth.saturating_sub(1),
            b'{' => t_depth += 1,
            b'}' => t_depth = t_depth.saturating_sub(1),
            b'-' if i + 1 < bytes.len()
                && bytes[i + 1] == b'>'
                && p_depth == 0
                && b_depth == 0
                && t_depth == 0 =>
            {
                let left = String::from_utf8_lossy(&bytes[..i]).to_string();
                let right = String::from_utf8_lossy(&bytes[i + 2..]).to_string();
                return Some((left.trim().to_string(), right.trim().to_string()));
            }
            _ => {}
        }
    }

    None
}

/// Splits comma-separated text at top-level commas.
///
/// Inputs:
/// - `input`: list text without surrounding delimiters.
///
/// Output:
/// - Non-empty trimmed items.
///
/// Transformation:
/// - Tracks nested delimiters so commas inside nested type forms are preserved.
pub(crate) fn split_top_level_csv(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut p_depth = 0usize;
    let mut b_depth = 0usize;
    let mut t_depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in bytes.iter().enumerate() {
        match *ch {
            b'(' => p_depth += 1,
            b')' => p_depth = p_depth.saturating_sub(1),
            b'[' => b_depth += 1,
            b']' => b_depth = b_depth.saturating_sub(1),
            b'{' => t_depth += 1,
            b'}' => t_depth = t_depth.saturating_sub(1),
            b',' if p_depth == 0 && b_depth == 0 && t_depth == 0 => {
                out.push(String::from_utf8_lossy(&bytes[start..idx]).to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }

    out.push(String::from_utf8_lossy(&bytes[start..]).to_string());
    out.into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Splits text at top-level plus signs.
///
/// Inputs:
/// - `input`: expression-like text to split.
///
/// Output:
/// - Non-empty trimmed items.
///
/// Transformation:
/// - Tracks nested delimiters so plus signs inside nested forms are preserved.
pub(crate) fn split_top_level_plus(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut p_depth = 0usize;
    let mut b_depth = 0usize;
    let mut t_depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in bytes.iter().enumerate() {
        match *ch {
            b'(' => p_depth += 1,
            b')' => p_depth = p_depth.saturating_sub(1),
            b'[' => b_depth += 1,
            b']' => b_depth = b_depth.saturating_sub(1),
            b'{' => t_depth += 1,
            b'}' => t_depth = t_depth.saturating_sub(1),
            b'+' if p_depth == 0 && b_depth == 0 && t_depth == 0 => {
                out.push(String::from_utf8_lossy(&bytes[start..idx]).to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }

    out.push(String::from_utf8_lossy(&bytes[start..]).to_string());
    out.into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Splits type text at top-level union separators.
///
/// Inputs:
/// - `input`: type-expression text.
///
/// Output:
/// - Non-empty trimmed union variant text.
///
/// Transformation:
/// - Tracks nested delimiters so `|` inside lists, tuples, maps, or function
///   parameters does not split the outer union.
pub(crate) fn split_top_level_union(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut p_depth = 0usize;
    let mut b_depth = 0usize;
    let mut t_depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in bytes.iter().enumerate() {
        match *ch {
            b'(' => p_depth += 1,
            b')' => p_depth = p_depth.saturating_sub(1),
            b'[' => b_depth += 1,
            b']' => b_depth = b_depth.saturating_sub(1),
            b'{' => t_depth += 1,
            b'}' => t_depth = t_depth.saturating_sub(1),
            b'|' if p_depth == 0 && b_depth == 0 && t_depth == 0 => {
                out.push(String::from_utf8_lossy(&bytes[start..idx]).to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }

    out.push(String::from_utf8_lossy(&bytes[start..]).to_string());
    out.into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Splits a qualified type name into module path and base name.
///
/// Inputs:
/// - `name`: type name that may contain dots.
///
/// Output:
/// - Optional module path and unqualified base name.
///
/// Transformation:
/// - Uses the final dot as the boundary so nested module paths remain intact.
pub(crate) fn split_module_name(name: &str) -> (Option<String>, String) {
    if let Some((module, base)) = name.rsplit_once('.') {
        (Some(module.to_string()), base.to_string())
    } else {
        (None, name.to_string())
    }
}

/// Reports whether compacted text has list type syntax.
///
/// Inputs:
/// - `input`: compacted type-expression text.
///
/// Output:
/// - `true` for bracketed list type syntax that is not a list-comprehension
///   marker.
///
/// Transformation:
/// - Checks only the outer delimiters and excludes `||`.
pub(crate) fn is_list_type(input: &str) -> bool {
    input.starts_with('[') && input.ends_with(']') && !input.contains("||")
}

/// Reports whether compacted text has tuple type syntax.
///
/// Inputs:
/// - `input`: compacted type-expression text.
///
/// Output:
/// - `true` when the text is wrapped in tuple braces.
///
/// Transformation:
/// - Performs a delimiter-shape check before full tuple parsing.
pub(crate) fn is_tuple_type(input: &str) -> bool {
    input.starts_with('{') && input.ends_with('}')
}

/// Reports whether compacted text has map type syntax.
///
/// Inputs:
/// - `input`: compacted type-expression text.
///
/// Output:
/// - `true` for `#{...}` map type expressions.
///
/// Transformation:
/// - Performs a delimiter-shape check before full map-field parsing.
pub(crate) fn is_map_type_expr(input: &str) -> bool {
    input.starts_with("#{") && input.ends_with('}') && input.len() >= 3
}
