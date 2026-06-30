/// Returns the identifier span under a byte offset.
///
/// Inputs:
/// - `source`: source text.
/// - `offset`: byte offset.
///
/// Output:
/// - Start/end byte offsets, or `None`.
///
/// Transformation:
/// - Expands backward and forward across ASCII identifier bytes.
pub(crate) fn ident_span_at_offset(source: &str, offset: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    if offset > bytes.len() {
        return None;
    }

    let mut start = offset;
    while start > 0 {
        let byte = bytes[start - 1];
        if is_identifier_byte(byte) {
            start -= 1;
        } else {
            break;
        }
    }

    let mut end = offset;
    while let Some(byte) = bytes.get(end) {
        if is_identifier_byte(*byte) {
            end += 1;
        } else {
            break;
        }
    }

    if start == end {
        return None;
    }
    Some((start, end))
}

/// Returns whether a byte is an identifier byte.
///
/// Inputs:
/// - `byte`: candidate byte.
///
/// Output:
/// - `true` for ASCII alphanumeric bytes and `_`.
///
/// Transformation:
/// - Defines the local hover identifier scanner character set.
pub(super) fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Detects record access syntax under a hover position.
///
/// Inputs:
/// - `source`: source text.
/// - `offset`: hover byte offset.
///
/// Output:
/// - Record name and field name, or `None`.
///
/// Transformation:
/// - Scans `#` occurrences and reads `Record#field` spans that include the
///   requested field offset.
pub(crate) fn record_access_at(source: &str, offset: usize) -> Option<(String, String)> {
    for (hash, _) in source.match_indices('#') {
        let mut cursor = hash + 1;
        let Some((name, next)) = read_ident_at(source, cursor) else {
            continue;
        };
        cursor = next;
        if source.as_bytes().get(cursor).copied() != Some(b'.') {
            continue;
        }
        cursor += 1;
        let field_start = cursor;
        let Some((field, field_end)) = read_ident_at(source, cursor) else {
            continue;
        };
        if offset >= hash && offset <= field_end && offset >= field_start {
            return Some((name, field));
        }
    }
    None
}

/// Reads an identifier at a byte offset.
///
/// Inputs:
/// - `source`: source text.
/// - `start`: first candidate identifier byte.
///
/// Output:
/// - Identifier text and first byte after it, or `None`.
///
/// Transformation:
/// - Scans forward across ASCII identifier bytes.
pub(crate) fn read_ident_at(source: &str, start: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    let mut end = start;
    while let Some(byte) = bytes.get(end) {
        if is_identifier_byte(*byte) {
            end += 1;
        } else {
            break;
        }
    }

    if end == start {
        None
    } else {
        Some((source[start..end].to_string(), end))
    }
}
