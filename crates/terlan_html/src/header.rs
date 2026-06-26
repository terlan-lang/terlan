use std::path::Path;

use crate::{HtmlDiagnostic, TERLAN_HTML_TEMPLATE_SUFFIX, TERLAN_MARKDOWN_TEMPLATE_SUFFIX};

/// Returns template body source after a Terlan header.
///
/// Inputs:
/// - `source`: template or Markdown source.
/// - `path`: source path used for suffix detection and diagnostics.
///
/// Output:
/// - Body source parsed by the target frontend, or diagnostics for malformed
///   annotation blocks.
///
/// Transformation:
/// - Leaves non-template Markdown files unchanged, while `.terl.html` and
///   `.terl.md` files may start with Terlan imports and annotation metadata
///   that are removed before target parsing.
pub(crate) fn template_body_source(
    source: &str,
    path: &Path,
) -> Result<String, Vec<HtmlDiagnostic>> {
    if !path_uses_terlan_header(path) {
        return Ok(source.to_owned());
    }

    let mut offset = 0usize;
    let lines = source.split_inclusive('\n').collect::<Vec<_>>();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            offset += line.len();
            index += 1;
            continue;
        }
        if trimmed.starts_with("import ") {
            validate_template_header_import(trimmed, path)?;
            offset += line.len();
            index += 1;
            continue;
        }
        if trimmed.starts_with('@') {
            let consumed = consume_template_header_annotation(&lines[index..], path)?;
            for consumed_line in &lines[index..index + consumed] {
                offset += consumed_line.len();
            }
            index += consumed;
            continue;
        }
        break;
    }

    let body = &source[offset..];
    validate_no_template_header_after_body(body, path)?;
    Ok(body.to_owned())
}

/// Walks leading Terlan template header annotations.
///
/// Inputs:
/// - `source`: template source text.
/// - `path`: source path for diagnostics.
/// - `visitor`: callback invoked once for each consumed annotation.
///
/// Output:
/// - `Ok(())` when all header imports and annotations are valid.
///
/// Transformation:
/// - Centralizes template-header traversal so page metadata, template metadata,
///   and body stripping use identical import and annotation rules.
pub(crate) fn walk_template_header(
    source: &str,
    path: &Path,
    mut visitor: impl FnMut(&str, &[&str]) -> Result<(), Vec<HtmlDiagnostic>>,
) -> Result<(), Vec<HtmlDiagnostic>> {
    let lines = source.split_inclusive('\n').collect::<Vec<_>>();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            index += 1;
            continue;
        }
        if trimmed.starts_with("import ") {
            validate_template_header_import(trimmed, path)?;
            index += 1;
            continue;
        }
        if trimmed.starts_with('@') {
            let annotation_name = validate_template_header_annotation_name(trimmed, path)?;
            let consumed = consume_template_header_annotation(&lines[index..], path)?;
            visitor(annotation_name, &lines[index..index + consumed])?;
            index += consumed;
            continue;
        }
        break;
    }

    Ok(())
}

/// Returns whether a path supports a leading Terlan template header.
///
/// Inputs:
/// - `path`: source path used for suffix detection.
///
/// Output:
/// - `true` for `.terl.html` and `.terl.md` files.
///
/// Transformation:
/// - Keeps ordinary `.md` imports literal while enabling headers for canonical
///   Terlan template/content files.
pub(crate) fn path_uses_terlan_header(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(TERLAN_HTML_TEMPLATE_SUFFIX)
                || name.ends_with(TERLAN_MARKDOWN_TEMPLATE_SUFFIX)
        })
}

/// Extracts top-level annotation metadata segments from one source line.
///
/// Inputs:
/// - `line`: source line from an annotation header.
/// - `depth`: running brace depth before the line.
///
/// Output:
/// - Top-level metadata segments found on the line.
///
/// Transformation:
/// - Tracks brace depth and string literals so callers can parse either keys or
///   values without duplicating annotation-header scanning.
pub(crate) fn template_header_metadata_segments_on_line<'a>(
    line: &'a str,
    depth: &mut isize,
) -> Vec<&'a str> {
    let mut segments = Vec::new();
    let mut segment_start = (*depth == 1).then_some(0usize);
    let mut in_string = false;
    let mut escaped = false;

    for (index, ch) in line.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => {
                if *depth == 1 {
                    push_template_header_metadata_segment(
                        line,
                        segment_start,
                        index,
                        &mut segments,
                    );
                }
                *depth += 1;
                segment_start = (*depth == 1).then_some(index + ch.len_utf8());
            }
            '}' => {
                if *depth == 1 {
                    push_template_header_metadata_segment(
                        line,
                        segment_start,
                        index,
                        &mut segments,
                    );
                }
                *depth -= 1;
                segment_start = None;
            }
            ',' if *depth == 1 => {
                push_template_header_metadata_segment(line, segment_start, index, &mut segments);
                segment_start = Some(index + ch.len_utf8());
            }
            _ => {}
        }
    }

    if *depth == 1 {
        push_template_header_metadata_segment(line, segment_start, line.len(), &mut segments);
    }

    segments
}

/// Extracts one metadata key/value pair from a top-level segment.
///
/// Inputs:
/// - `trimmed`: source segment without surrounding whitespace.
///
/// Output:
/// - Key text and raw value text when the segment contains `=` or `:`.
///
/// Transformation:
/// - Finds the first assignment/type separator and returns borrowed slices for
///   shallow schema validation and metadata extraction.
pub(crate) fn template_header_metadata_entry(trimmed: &str) -> Option<(&str, &str)> {
    if trimmed.is_empty() || trimmed.starts_with('}') || trimmed.starts_with('@') {
        return None;
    }
    let separator = trimmed
        .char_indices()
        .find_map(|(index, ch)| matches!(ch, '=' | ':').then_some(index))?;
    Some((trimmed[..separator].trim(), trimmed[separator + 1..].trim()))
}

/// Extracts an object value for one top-level annotation key.
///
/// Inputs:
/// - `source`: full annotation source.
/// - `target_key`: top-level key to find.
///
/// Output:
/// - Source text inside the key's object value.
///
/// Transformation:
/// - Scans at outer annotation depth one and returns the balanced object body
///   while ignoring braces inside strings.
pub(crate) fn annotation_object_value_for_key(source: &str, target_key: &str) -> Option<String> {
    let mut index = 0usize;
    let mut depth = 0isize;
    let mut in_string = false;
    let mut escaped = false;

    while index < source.len() {
        let ch = source[index..].chars().next()?;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += ch.len_utf8();
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                index += ch.len_utf8();
            }
            '{' => {
                depth += 1;
                index += ch.len_utf8();
            }
            '}' => {
                depth -= 1;
                index += ch.len_utf8();
            }
            ch if depth == 1 && (ch.is_ascii_alphabetic() || ch == '_') => {
                let key_start = index;
                index += ch.len_utf8();
                while index < source.len() {
                    let next = source[index..].chars().next()?;
                    if next.is_ascii_alphanumeric() || next == '_' {
                        index += next.len_utf8();
                    } else {
                        break;
                    }
                }
                let key = &source[key_start..index];
                index = skip_ascii_whitespace(source, index);
                let separator = source[index..].chars().next()?;
                if !matches!(separator, '=' | ':') {
                    continue;
                }
                index += separator.len_utf8();
                index = skip_ascii_whitespace(source, index);
                if key == target_key {
                    return balanced_object_body_at(source, index);
                }
            }
            _ => {
                index += ch.len_utf8();
            }
        }
    }

    None
}

/// Validates one Terlan import line in a template header.
///
/// Inputs:
/// - `trimmed`: whitespace-trimmed source line starting with `import`.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when the import uses the normal declaration terminator.
///
/// Transformation:
/// - Keeps template headers aligned with Terlan source syntax instead of
///   silently stripping malformed import-like body text.
fn validate_template_header_import(trimmed: &str, path: &Path) -> Result<(), Vec<HtmlDiagnostic>> {
    if trimmed.ends_with('.') {
        return Ok(());
    }

    Err(vec![HtmlDiagnostic::new(
        Some(path.to_path_buf()),
        "Terlan template header import must end with `.`",
    )])
}

/// Consumes one leading Terlan annotation from a template header.
///
/// Inputs:
/// - `lines`: remaining template source lines starting at an annotation.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - Number of lines consumed by the annotation.
///
/// Transformation:
/// - Supports marker annotations and brace-delimited metadata blocks, stopping
///   once balanced braces close.
fn consume_template_header_annotation(
    lines: &[&str],
    path: &Path,
) -> Result<usize, Vec<HtmlDiagnostic>> {
    let Some(first) = lines.first() else {
        return Ok(0);
    };
    let name = validate_template_header_annotation_name(first.trim(), path)?;
    let mut balance = template_header_brace_delta(first);
    let consumed = if !first.contains('{') {
        1
    } else if balance <= 0 {
        1
    } else {
        let mut consumed = None;
        for (index, line) in lines.iter().enumerate().skip(1) {
            balance += template_header_brace_delta(line);
            if balance <= 0 {
                consumed = Some(index + 1);
                break;
            }
        }
        consumed.ok_or_else(|| {
            vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                "unterminated Terlan template annotation header",
            )]
        })?
    };

    validate_template_header_annotation_keys(name, &lines[..consumed], path)?;
    Ok(consumed)
}

/// Validates one Terlan template-header annotation path.
///
/// Inputs:
/// - `trimmed`: whitespace-trimmed annotation line.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - Built-in annotation name when it is valid.
///
/// Transformation:
/// - Rejects unknown header annotations until custom annotation schemas are
///   available for template files.
fn validate_template_header_annotation_name<'a>(
    trimmed: &'a str,
    path: &Path,
) -> Result<&'a str, Vec<HtmlDiagnostic>> {
    let Some(name) = template_header_annotation_name(trimmed) else {
        return Err(vec![HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            "Terlan template annotation is missing a name",
        )]);
    };
    if matches!(name, "page" | "template") {
        return Ok(name);
    }

    Err(vec![HtmlDiagnostic::new(
        Some(path.to_path_buf()),
        format!("unknown Terlan template annotation `@{name}`"),
    )])
}

/// Validates top-level keys for a template-header annotation.
///
/// Inputs:
/// - `name`: validated annotation name.
/// - `lines`: source lines consumed by the annotation.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when every top-level key belongs to the built-in schema.
///
/// Transformation:
/// - Scans only brace-depth-one metadata keys so nested `params` entries can
///   keep their own type-like syntax until full template schema parsing lands.
///   Duplicate top-level keys are rejected before generated template metadata
///   starts relying on first/last-write behavior.
fn validate_template_header_annotation_keys(
    name: &str,
    lines: &[&str],
    path: &Path,
) -> Result<(), Vec<HtmlDiagnostic>> {
    let mut depth = 0isize;
    let mut seen_keys = Vec::new();
    for line in lines {
        for key in template_header_metadata_keys_on_line(line, &mut depth) {
            if !template_header_key_is_allowed(name, key) {
                return Err(vec![HtmlDiagnostic::new(
                    Some(path.to_path_buf()),
                    format!("unknown Terlan @{name} key `{key}`"),
                )]);
            }
            if seen_keys.contains(&key) {
                return Err(vec![HtmlDiagnostic::new(
                    Some(path.to_path_buf()),
                    format!("duplicate Terlan @{name} key `{key}`"),
                )]);
            }
            seen_keys.push(key);
        }
    }
    Ok(())
}

/// Extracts top-level annotation metadata keys from one source line.
///
/// Inputs:
/// - `line`: source line from an annotation header.
/// - `depth`: running brace depth before the line.
///
/// Output:
/// - Top-level metadata keys found on the line.
///
/// Transformation:
/// - Scans comma and brace boundaries at depth one so compact annotations such
///   as `@page { title = "Home" }` are validated the same as multiline blocks.
fn template_header_metadata_keys_on_line<'a>(line: &'a str, depth: &mut isize) -> Vec<&'a str> {
    template_header_metadata_segments_on_line(line, depth)
        .into_iter()
        .filter_map(template_header_metadata_key)
        .collect()
}

/// Pushes a metadata segment from one top-level source span.
///
/// Inputs:
/// - `line`: source line.
/// - `segment_start`: optional byte offset where a depth-one segment starts.
/// - `segment_end`: byte offset where the segment ends.
/// - `segments`: accumulator for extracted metadata segments.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Trims the segment and appends it only when it contains a key/value
///   separator.
fn push_template_header_metadata_segment<'a>(
    line: &'a str,
    segment_start: Option<usize>,
    segment_end: usize,
    segments: &mut Vec<&'a str>,
) {
    let Some(start) = segment_start else {
        return;
    };
    if start > segment_end {
        return;
    }
    let segment = line[start..segment_end].trim();
    if template_header_metadata_key(segment).is_some() {
        segments.push(segment);
    }
}

/// Extracts one top-level annotation metadata key from a line.
///
/// Inputs:
/// - `trimmed`: source line without surrounding whitespace.
///
/// Output:
/// - Key text before `=` or `:` when the line appears to declare metadata.
///
/// Transformation:
/// - Ignores braces and comments; this is intentionally a shallow key check,
///   not a full annotation value parser.
fn template_header_metadata_key(trimmed: &str) -> Option<&str> {
    if trimmed.is_empty() || trimmed.starts_with('}') || trimmed.starts_with('@') {
        return None;
    }
    let (key, _) = template_header_metadata_entry(trimmed)?;
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

/// Extracts a balanced object body beginning at an opening brace.
///
/// Inputs:
/// - `source`: full annotation source.
/// - `open_index`: byte index expected to point at `{`.
///
/// Output:
/// - Text inside the balanced object braces.
///
/// Transformation:
/// - Tracks nested braces and strings so compact generic metadata values do not
///   terminate the object early.
fn balanced_object_body_at(source: &str, open_index: usize) -> Option<String> {
    if source[open_index..].chars().next()? != '{' {
        return None;
    }
    let body_start = open_index + 1;
    let mut index = body_start;
    let mut depth = 1isize;
    let mut in_string = false;
    let mut escaped = false;

    while index < source.len() {
        let ch = source[index..].chars().next()?;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += ch.len_utf8();
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                index += ch.len_utf8();
            }
            '{' => {
                depth += 1;
                index += ch.len_utf8();
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(source[body_start..index].to_string());
                }
                index += ch.len_utf8();
            }
            _ => {
                index += ch.len_utf8();
            }
        }
    }

    None
}

/// Skips ASCII whitespace from a byte index.
///
/// Inputs:
/// - `source`: source text.
/// - `index`: starting byte offset.
///
/// Output:
/// - First byte offset after whitespace.
///
/// Transformation:
/// - Advances over ASCII whitespace only because Terlan header syntax uses
///   ASCII punctuation and identifiers.
fn skip_ascii_whitespace(source: &str, mut index: usize) -> usize {
    while index < source.len() {
        let Some(ch) = source[index..].chars().next() else {
            break;
        };
        if !ch.is_ascii_whitespace() {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

/// Returns whether a top-level metadata key belongs to an annotation schema.
///
/// Inputs:
/// - `name`: built-in annotation name.
/// - `key`: metadata key.
///
/// Output:
/// - `true` when the key is accepted by the first template-header schema.
///
/// Transformation:
/// - Encodes the v0.0.5 built-in `@page` and `@template` key surface in one
///   place for diagnostics.
fn template_header_key_is_allowed(name: &str, key: &str) -> bool {
    match name {
        "page" => matches!(key, "title" | "route" | "layout"),
        "template" => matches!(key, "name" | "params"),
        _ => false,
    }
}

/// Extracts the annotation name from a template-header annotation line.
///
/// Inputs:
/// - `trimmed`: annotation line without surrounding whitespace.
///
/// Output:
/// - Annotation name after `@`, or `None` when no name is present.
///
/// Transformation:
/// - Reads lowercase/ascii identifier characters until metadata whitespace or
///   `{` starts.
fn template_header_annotation_name(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix('@')?;
    let end = rest
        .char_indices()
        .find_map(|(index, ch)| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                None
            } else {
                Some(index)
            }
        })
        .unwrap_or(rest.len());
    if end == 0 {
        None
    } else {
        Some(&rest[..end])
    }
}

/// Computes a brace balance delta for a template header line.
///
/// Inputs:
/// - `line`: one source line from a Terlan template header.
///
/// Output:
/// - Opening braces minus closing braces.
///
/// Transformation:
/// - Provides a conservative annotation-block boundary scan for static-site
///   metadata without parsing annotation values yet.
fn template_header_brace_delta(line: &str) -> isize {
    let mut balance = 0isize;
    let mut in_string = false;
    let mut escaped = false;

    for ch in line.chars() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => balance += 1,
            '}' => balance -= 1,
            _ => {}
        }
    }

    balance
}

/// Rejects Terlan header syntax after template body content begins.
///
/// Inputs:
/// - `body`: target body source after leading header stripping.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when no top-level header-looking line appears after body start.
///
/// Transformation:
/// - Scans unindented body lines and rejects late imports or annotations so
///   Terlan template headers stay a single leading block.
fn validate_no_template_header_after_body(
    body: &str,
    path: &Path,
) -> Result<(), Vec<HtmlDiagnostic>> {
    let mut body_started = false;
    for line in body.lines() {
        if !body_started {
            if line.trim().is_empty() {
                continue;
            }
            body_started = true;
            continue;
        }
        if is_terlan_template_header_line(line) {
            return Err(vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                "Terlan template imports and annotations must appear before body content",
            )]);
        }
    }
    Ok(())
}

/// Returns whether a body line looks like Terlan template header syntax.
///
/// Inputs:
/// - `line`: one Markdown body line.
///
/// Output:
/// - `true` for unindented Terlan import or annotation syntax.
///
/// Transformation:
/// - Keeps indented code blocks free to contain literal text while rejecting
///   top-level source metadata after the body has begun.
fn is_terlan_template_header_line(line: &str) -> bool {
    if line.starts_with(char::is_whitespace) {
        return false;
    }
    line.starts_with("import ") || line.starts_with('@')
}
