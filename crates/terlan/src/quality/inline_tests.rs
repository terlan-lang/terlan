/// Returns whether source text contains non-adjacent inline test config.
///
/// Inputs:
/// - `text`: Rust implementation source.
///
/// Output:
/// - `true` when the file contains an inline `#[cfg(test)]` marker.
/// - `false` when every marker belongs to an adjacent `#[path = "*_test.rs"]`
///   module declaration or test-only import/helper hook.
///
/// Transformation:
/// - Scans source lines outside raw string literals and treats approved
///   test-only imports or adjacent test-module hooks as non-debt.
pub(crate) fn has_inline_test_marker(text: &str) -> bool {
    let lines = text.lines().collect::<Vec<_>>();
    let raw_string_lines = raw_string_line_mask(&lines);
    for (index, line) in lines.iter().enumerate() {
        if raw_string_lines[index] {
            continue;
        }
        if line.trim() != "#[cfg(test)]" {
            continue;
        }
        let mut next_index = index + 1;
        while next_index < lines.len() && lines[next_index].trim().is_empty() {
            next_index += 1;
        }
        if next_index < lines.len() {
            let next_line = lines[next_index].trim();
            if next_line.starts_with("#[path = ") && next_line.contains("_test.rs") {
                continue;
            }
            if next_line.starts_with("use ") || next_line.starts_with("pub(crate) use ") {
                continue;
            }
            if next_line.starts_with("pub(crate) mod ") && next_line.ends_with(';') {
                continue;
            }
        }
        return true;
    }
    false
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
