use super::*;

/// Parses a double-quoted manifest string.
///
/// Inputs:
/// - `value`: trimmed value text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Unescaped string value.
///
/// Transformation:
/// - Accepts a small escape subset needed by package names and source roots:
///   `\"`, `\\`, `\n`, `\r`, and `\t`.
pub(super) fn parse_string(value: &str, path: &Path, line_no: usize) -> Result<String, String> {
    let inner = value
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .ok_or_else(|| {
            format!(
                "{}:{}: project manifest value must be a double-quoted string",
                path.display(),
                line_no
            )
        })?;
    unescape_string(inner, path, line_no)
}

/// Parses an array of double-quoted manifest strings.
///
/// Inputs:
/// - `value`: trimmed value text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Ordered string entries.
///
/// Transformation:
/// - Parses the reviewed one-line `[ "a", "b" ]` subset and rejects empty
///   arrays so source-root discovery remains explicit.
pub(super) fn parse_string_array(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<Vec<String>, String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|text| text.strip_suffix(']'))
        .ok_or_else(|| {
            format!(
                "{}:{}: project manifest value must be an array of strings",
                path.display(),
                line_no
            )
        })?;
    let mut entries = Vec::new();
    for item in split_array_items(inner, path, line_no)? {
        entries.push(parse_string(item.trim(), path, line_no)?);
    }
    if entries.is_empty() {
        return Err(format!(
            "{}:{}: project manifest string array cannot be empty",
            path.display(),
            line_no
        ));
    }
    Ok(entries)
}

/// Splits a manifest array body into item slices.
///
/// Inputs:
/// - `inner`: text inside `[` and `]`.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Slices for each array entry.
///
/// Transformation:
/// - Splits on commas outside strings and nested array brackets, then rejects
///   trailing empty entries.
pub(super) fn split_array_items<'a>(
    inner: &'a str,
    path: &Path,
    line_no: usize,
) -> Result<Vec<&'a str>, String> {
    let mut items = Vec::new();
    let mut start = 0;
    let mut in_string = false;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    for (index, ch) in inner.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '[' if !in_string => bracket_depth += 1,
            ']' if !in_string => {
                bracket_depth = bracket_depth.checked_sub(1).ok_or_else(|| {
                    format!(
                        "{}:{}: project manifest string array has an unmatched closing bracket",
                        path.display(),
                        line_no
                    )
                })?;
            }
            ',' if !in_string && bracket_depth == 0 => {
                let item = inner[start..index].trim();
                if item.is_empty() {
                    return Err(format!(
                        "{}:{}: project manifest string array contains an empty item",
                        path.display(),
                        line_no
                    ));
                }
                items.push(item);
                start = index + 1;
            }
            _ => {}
        }
    }
    if in_string {
        return Err(format!(
            "{}:{}: project manifest string array has an unterminated string",
            path.display(),
            line_no
        ));
    }
    if bracket_depth != 0 {
        return Err(format!(
            "{}:{}: project manifest string array has an unclosed nested array",
            path.display(),
            line_no
        ));
    }
    let tail = inner[start..].trim();
    if !tail.is_empty() {
        items.push(tail);
    }
    Ok(items)
}

/// Unescapes supported manifest string escapes.
///
/// Inputs:
/// - `inner`: text inside double quotes.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Unescaped string.
///
/// Transformation:
/// - Converts the reviewed escape subset and rejects unknown or dangling
///   escapes so manifest text cannot be misread.
fn unescape_string(inner: &str, path: &Path, line_no: usize) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let escaped = chars.next().ok_or_else(|| {
            format!(
                "{}:{}: project manifest string has a dangling escape",
                path.display(),
                line_no
            )
        })?;
        match escaped {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            other => {
                return Err(format!(
                    "{}:{}: unsupported project manifest string escape `\\{}`",
                    path.display(),
                    line_no,
                    other
                ));
            }
        }
    }
    Ok(out)
}
