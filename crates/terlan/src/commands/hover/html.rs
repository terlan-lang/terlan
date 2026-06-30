use crate::terlan_hir::identifier_to_snake;
use crate::terlan_syntax::{SyntaxDeclarationPayload, SyntaxModuleOutput};

use super::{ident_span_at_offset, read_ident_at};

/// Returns the type of a component property under a hover position.
///
/// Inputs:
/// - `module`: parsed module containing component functions.
/// - `source`: source text containing inline HTML.
/// - `offset`: hover byte offset.
///
/// Output:
/// - `prop: Type` text when the offset is on a known component attribute.
///
/// Transformation:
/// - Detects uppercase HTML component tags and maps attributes to matching
///   function parameters.
pub(crate) fn hover_component_prop_type(
    module: &SyntaxModuleOutput,
    source: &str,
    offset: usize,
) -> Option<String> {
    let (prop_start, prop_end) = ident_span_at_offset(source, offset)?;
    let prop_name = &source[prop_start..prop_end];
    let (tag_name, attr_names) = html_start_tag_at(source, prop_start)?;
    if !hover_is_component_element_name(&tag_name)
        || !attr_names.iter().any(|name| name == prop_name)
    {
        return None;
    }

    let arity = attr_names.len();
    hover_component_function_names(&tag_name)
        .into_iter()
        .find_map(|function_name| {
            module.declarations.iter().find_map(|decl| {
                let SyntaxDeclarationPayload::Function { name, params, .. } = &decl.payload else {
                    return None;
                };
                if name != &function_name || params.len() != arity {
                    return None;
                }
                params
                    .iter()
                    .find(|param| hover_component_prop_matches_param(prop_name, &param.name))
                    .map(|param| format!("{}: {}", prop_name, param.annotation.text))
            })
        })
}

/// Finds the enclosing HTML start tag at an offset.
pub(crate) fn html_start_tag_at(source: &str, offset: usize) -> Option<(String, Vec<String>)> {
    let bytes = source.as_bytes();
    if offset > bytes.len() {
        return None;
    }

    let cursor = find_html_start_tag_start(source, offset)?;
    if bytes.get(cursor).copied() != Some(b'<')
        || matches!(bytes.get(cursor + 1).copied(), Some(b'/') | Some(b'!'))
    {
        return None;
    }

    let (tag_name, tag_end) = read_ident_at(source, cursor + 1)?;
    let tag_close = find_html_start_tag_end(source, tag_end)?;
    if offset > tag_close {
        return None;
    }

    Some((
        tag_name,
        html_attr_names_in_start_tag(source, tag_end, tag_close),
    ))
}

/// Finds the start of an HTML start tag containing an offset.
fn find_html_start_tag_start(source: &str, offset: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = 0usize;
    let mut tag_start = None;
    let mut quote = None;
    let mut brace_depth = 0usize;

    while cursor < offset {
        let byte = bytes[cursor];
        match (tag_start, quote, byte) {
            (None, _, b'<') => {
                tag_start = Some(cursor);
                quote = None;
                brace_depth = 0;
            }
            (None, _, _) => {}
            (Some(_), Some(q), b) if b == q => quote = None,
            (Some(_), Some(_), _) => {}
            (Some(_), None, b'"' | b'\'') => quote = Some(byte),
            (Some(_), None, b'{') => brace_depth += 1,
            (Some(_), None, b'}') if brace_depth > 0 => brace_depth -= 1,
            (Some(_), None, b'>') if brace_depth == 0 => tag_start = None,
            _ => {}
        }
        cursor += 1;
    }

    tag_start
}

/// Finds the closing `>` for an HTML start tag.
fn find_html_start_tag_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = start;
    let mut quote = None;
    let mut brace_depth = 0usize;

    while let Some(byte) = bytes.get(cursor).copied() {
        match (quote, byte) {
            (Some(q), b) if b == q => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(byte),
            (None, b'{') => brace_depth += 1,
            (None, b'}') if brace_depth > 0 => brace_depth -= 1,
            (None, b'>') if brace_depth == 0 => return Some(cursor),
            _ => {}
        }
        cursor += 1;
    }

    None
}

/// Extracts attribute names from an HTML start tag range.
fn html_attr_names_in_start_tag(source: &str, start: usize, end: usize) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut cursor = start;
    let mut names = Vec::new();

    while cursor < end {
        while cursor < end && (bytes[cursor].is_ascii_whitespace() || bytes[cursor] == b'/') {
            cursor += 1;
        }
        if cursor >= end {
            break;
        }

        let name_start = cursor;
        while cursor < end && is_html_attr_name_byte(bytes[cursor]) {
            cursor += 1;
        }
        if name_start == cursor {
            cursor += 1;
            continue;
        }
        names.push(source[name_start..cursor].to_string());

        while cursor < end && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor < end && bytes[cursor] == b'=' {
            cursor = skip_html_attr_value(source, cursor + 1, end);
        }
    }

    names
}

/// Skips an HTML attribute value.
fn skip_html_attr_value(source: &str, start: usize, end: usize) -> usize {
    let bytes = source.as_bytes();
    let mut cursor = start;
    while cursor < end && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if cursor >= end {
        return cursor;
    }

    match bytes[cursor] {
        b'"' | b'\'' => {
            let quote = bytes[cursor];
            cursor += 1;
            while cursor < end && bytes[cursor] != quote {
                cursor += 1;
            }
            (cursor + 1).min(end)
        }
        b'{' => {
            let mut depth = 1usize;
            cursor += 1;
            while cursor < end && depth > 0 {
                match bytes[cursor] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    _ => {}
                }
                cursor += 1;
            }
            cursor
        }
        _ => {
            while cursor < end && !bytes[cursor].is_ascii_whitespace() && bytes[cursor] != b'/' {
                cursor += 1;
            }
            cursor
        }
    }
}

/// Returns whether a byte can appear in an HTML attribute name.
fn is_html_attr_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':')
}

/// Returns whether an HTML tag name is treated as a component.
fn hover_is_component_element_name(name: &str) -> bool {
    matches!(name.chars().next(), Some(ch) if ch.is_ascii_uppercase())
}

/// Returns candidate function names for a component tag.
fn hover_component_function_names(tag_name: &str) -> Vec<String> {
    let snake_case = identifier_to_snake(tag_name);
    if snake_case == tag_name {
        vec![tag_name.to_string()]
    } else {
        vec![tag_name.to_string(), snake_case]
    }
}

/// Returns whether a component prop name matches a function parameter name.
fn hover_component_prop_matches_param(prop_name: &str, param_name: &str) -> bool {
    prop_name == param_name
        || prop_name.eq_ignore_ascii_case(param_name)
        || prop_name == identifier_to_snake(param_name)
}
