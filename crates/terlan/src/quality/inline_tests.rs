/// Returns whether source text contains an embedded Rust test.
///
/// Inputs:
/// - `text`: Rust implementation source.
///
/// Output:
/// - `true` when the file contains an inline test module or test function.
/// - `false` when every `#[cfg(test)]` marker only controls adjacent test
///   modules, imports, support items, enum variants, fields, or match arms.
///
/// Transformation:
/// - Scans source lines outside raw string literals, follows the attribute
///   stack attached to each marker, and distinguishes embedded tests from
///   conditionally compiled test support.
pub(crate) fn has_inline_test_marker(text: &str) -> bool {
    let lines = text.lines().collect::<Vec<_>>();
    let raw_string_lines = raw_string_line_mask(&lines);
    for (index, line) in lines.iter().enumerate() {
        if raw_string_lines[index] {
            continue;
        }
        let trimmed = line.trim();
        if trimmed == "#[test]" || trimmed.starts_with("#[test(") {
            return true;
        }
        if trimmed != "#[cfg(test)]" {
            continue;
        }
        let mut item_index = next_significant_line(&lines, index + 1);
        let mut adjacent_path_module = false;
        while item_index < lines.len() && lines[item_index].trim().starts_with("#[") {
            let attribute = lines[item_index].trim();
            if attribute == "#[test]" || attribute.starts_with("#[test(") {
                return true;
            }
            adjacent_path_module |= attribute.starts_with("#[path = ")
                && (attribute.contains("_test.rs") || attribute.contains("_test_support.rs"));
            item_index = next_significant_line(&lines, item_index + 1);
        }
        if item_index >= lines.len() {
            continue;
        }
        if adjacent_path_module && module_declaration_is_external(&lines, item_index) {
            continue;
        }
        if module_declaration_has_body(&lines, item_index) {
            return true;
        }
    }
    false
}

/// Returns the next non-empty source line index.
fn next_significant_line(lines: &[&str], mut index: usize) -> usize {
    while index < lines.len() && lines[index].trim().is_empty() {
        index += 1;
    }
    index
}

/// Returns whether an item begins with a Rust module declaration.
fn is_module_declaration(line: &str) -> bool {
    let line = line
        .strip_prefix("pub(crate) ")
        .or_else(|| line.strip_prefix("pub(super) "))
        .or_else(|| line.strip_prefix("pub "))
        .unwrap_or(line);
    line.starts_with("mod ")
}

/// Returns whether a module declaration refers to another source file.
fn module_declaration_is_external(lines: &[&str], index: usize) -> bool {
    let declaration = lines[index].trim();
    is_module_declaration(declaration) && declaration.ends_with(';')
}

/// Returns whether a module declaration opens an inline body.
fn module_declaration_has_body(lines: &[&str], index: usize) -> bool {
    let declaration = lines[index].trim();
    if !is_module_declaration(declaration) || declaration.ends_with(';') {
        return false;
    }
    let next_index = next_significant_line(lines, index + 1);
    declaration.contains('{')
        || next_index < lines.len() && lines[next_index].trim().starts_with('{')
}

/// Returns line indexes that are inside Rust raw string literal bodies.
///
/// Inputs:
/// - `lines`: Rust source split into lines.
///
/// Output:
/// - Boolean mask with one entry per input line.
///
/// Transformation:
/// - Tracks ordinary `r"..."` and hash-delimited `r#"..."#` literals well
///   enough for quality scanning, so generated source strings containing
///   `#[cfg(test)]` are not mistaken for inline tests in the host file.
fn raw_string_line_mask(lines: &[&str]) -> Vec<bool> {
    let mut mask = vec![false; lines.len()];
    let mut terminator: Option<String> = None;
    for (index, line) in lines.iter().enumerate() {
        if let Some(end) = terminator.as_deref() {
            mask[index] = true;
            if line.contains(end) {
                terminator = None;
            }
            continue;
        }
        let Some(start) = raw_string_terminator(line) else {
            continue;
        };
        mask[index] = true;
        if !line[start.end_offset..].contains(&start.terminator) {
            terminator = Some(start.terminator);
        }
    }
    mask
}

/// Raw string opener metadata used by the inline-test scanner.
struct RawStringStart {
    terminator: String,
    end_offset: usize,
}

/// Finds a raw string literal opener in one Rust source line.
///
/// Inputs:
/// - `line`: one Rust source line.
///
/// Output:
/// - Terminator and content-start offset for the first raw string literal on
///   the line, if present.
///
/// Transformation:
/// - Recognizes `r"`, `r#"`, `r##"`, and wider hash-delimited forms while
///   ignoring ordinary identifiers containing `r`.
fn raw_string_terminator(line: &str) -> Option<RawStringStart> {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] != b'r' {
            index += 1;
            continue;
        }
        if index > 0 && (bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_') {
            index += 1;
            continue;
        }
        let mut cursor = index + 1;
        let mut hashes = 0;
        while cursor < bytes.len() && bytes[cursor] == b'#' {
            hashes += 1;
            cursor += 1;
        }
        if cursor < bytes.len() && bytes[cursor] == b'"' {
            return Some(RawStringStart {
                terminator: format!("\"{}", "#".repeat(hashes)),
                end_offset: cursor + 1,
            });
        }
        index += 1;
    }
    None
}
