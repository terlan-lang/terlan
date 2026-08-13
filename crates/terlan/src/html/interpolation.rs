/// Context surrounding a `${...}` or HTML-native `{...}` interpolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateInterpolationContext {
    Text,
    Attribute { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Variants representing template attribute slot kind.
pub enum TemplateAttributeSlotKind {
    Scalar,
    Url,
    Boolean,
    TokenList,
}

/// Returns template attribute slot kind.
pub fn template_attribute_slot_kind(name: &str) -> TemplateAttributeSlotKind {
    match name.to_ascii_lowercase().as_str() {
        "href" | "src" | "action" | "formaction" | "poster" => TemplateAttributeSlotKind::Url,
        "checked" | "disabled" | "selected" | "readonly" | "required" | "multiple" => {
            TemplateAttributeSlotKind::Boolean
        }
        "class" | "rel" | "sandbox" => TemplateAttributeSlotKind::TokenList,
        _ => TemplateAttributeSlotKind::Scalar,
    }
}

/// One interpolation region using absolute UTF-8 byte offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateInterpolationRegion {
    pub open_start: usize,
    pub expression_start: usize,
    pub expression_end: usize,
    pub close_end: usize,
    pub context: TemplateInterpolationContext,
}

/// Stable scanner failure for malformed interpolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateInterpolationError {
    pub message: String,
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

/// Scans template interpolation regions with balanced brace and quoted-string
/// awareness.
///
/// `${...}` is accepted in every typed artifact target. The shorter `{...}`
/// spelling is accepted only in HTML markup so metadata blocks and braces in
/// JSON, YAML, or ordinary Terlan source cannot be mistaken for slots.
pub fn scan_template_interpolations(
    source: &str,
) -> Result<Vec<TemplateInterpolationRegion>, TemplateInterpolationError> {
    let mut regions = Vec::new();
    let mut cursor = 0usize;
    while let Some((open_start, prefix_len)) = next_interpolation_open(source, cursor) {
        let expression_start = open_start + prefix_len;
        let expression_end =
            interpolation_expression_close(source, expression_start).ok_or_else(|| {
                let (line, start) = line_and_column(source, open_start);
                TemplateInterpolationError {
                    message: "unterminated template interpolation slot".to_string(),
                    line,
                    start,
                    end: start + prefix_len,
                }
            })?;
        let close_end = expression_end + 1;
        if source[expression_start..expression_end].trim().is_empty() {
            let (line, start) = line_and_column(source, open_start);
            return Err(TemplateInterpolationError {
                message: "template interpolation slot cannot be empty".to_string(),
                line,
                start,
                end: start + close_end - open_start,
            });
        }
        regions.push(TemplateInterpolationRegion {
            open_start,
            expression_start,
            expression_end,
            close_end,
            context: interpolation_context(source, open_start),
        });
        cursor = close_end;
    }
    Ok(regions)
}

/// Returns the interpolation expression containing an absolute cursor offset.
pub fn template_interpolation_at_offset(
    source: &str,
    offset: usize,
) -> Result<Option<TemplateInterpolationRegion>, TemplateInterpolationError> {
    Ok(scan_template_interpolations(source)?
        .into_iter()
        .find(|region| offset >= region.expression_start && offset <= region.expression_end))
}

/// Canonicalizes whitespace immediately inside interpolation delimiters while
/// preserving expression spelling and all non-interpolation source bytes.
pub fn format_template_interpolations(source: &str) -> Result<String, TemplateInterpolationError> {
    let regions = scan_template_interpolations(source)?;
    if regions.is_empty() {
        return Ok(source.to_string());
    }

    let mut formatted = String::with_capacity(source.len());
    let mut cursor = 0usize;
    for region in regions {
        formatted.push_str(&source[cursor..region.open_start]);
        formatted.push_str(&source[region.open_start..region.expression_start]);
        formatted.push_str(source[region.expression_start..region.expression_end].trim());
        formatted.push('}');
        cursor = region.close_end;
    }
    formatted.push_str(&source[cursor..]);
    Ok(formatted)
}

/// Finds the next interpolation opening accepted for the current target
/// context.
fn next_interpolation_open(source: &str, cursor: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut index = cursor;
    while index < bytes.len() {
        if bytes[index] == b'$' && bytes.get(index + 1) == Some(&b'{') {
            return Some((index, 2));
        }
        if bytes[index] == b'{' && is_html_native_interpolation_open(source, index) {
            return Some((index, 1));
        }
        index += 1;
    }
    None
}

/// Returns whether a bare brace occurs in HTML text or an HTML tag.
fn is_html_native_interpolation_open(source: &str, open_start: usize) -> bool {
    if open_start > 0 && source.as_bytes().get(open_start - 1) == Some(&b'$') {
        return false;
    }
    let prefix = &source[..open_start];
    let Some(last_tag_open) = prefix.rfind('<') else {
        return false;
    };
    let last_tag_close = prefix.rfind('>');
    if last_tag_close.is_none_or(|close| last_tag_open > close) {
        return true;
    }

    !is_inside_raw_text_element(prefix, "script") && !is_inside_raw_text_element(prefix, "style")
}

/// Keeps braces in HTML raw-text elements available to JavaScript and CSS.
fn is_inside_raw_text_element(prefix: &str, element: &str) -> bool {
    let lower = prefix.to_ascii_lowercase();
    let open = lower.rfind(&format!("<{element}"));
    let close = lower.rfind(&format!("</{element}"));
    open.is_some_and(|open| close.is_none_or(|close| open > close))
}

pub(crate) fn interpolation_expression_close(
    source: &str,
    expression_start: usize,
) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = expression_start;
    let mut brace_depth = 0usize;
    let mut quote = None;
    let mut escaped = false;

    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            cursor += 1;
            continue;
        }

        match byte {
            b'"' | b'\'' => quote = Some(byte),
            b'{' => brace_depth += 1,
            b'}' if brace_depth == 0 => return Some(cursor),
            b'}' => brace_depth -= 1,
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn interpolation_context(source: &str, open_start: usize) -> TemplateInterpolationContext {
    let prefix = &source[..open_start];
    let Some(tag_start) = prefix.rfind('<') else {
        return TemplateInterpolationContext::Text;
    };
    if prefix.rfind('>').is_some_and(|tag_end| tag_end > tag_start) {
        return TemplateInterpolationContext::Text;
    }

    let tag_prefix = &prefix[tag_start + 1..];
    let Some(equals) = tag_prefix.rfind('=') else {
        return TemplateInterpolationContext::Text;
    };
    if !tag_prefix[equals + 1..]
        .chars()
        .all(|character| character.is_whitespace() || matches!(character, '"' | '\''))
    {
        return TemplateInterpolationContext::Text;
    }

    let name_end = tag_prefix[..equals].trim_end().len();
    let name_start = tag_prefix[..name_end]
        .rfind(|character: char| {
            !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ':'))
        })
        .map_or(0, |index| index + 1);
    let name = &tag_prefix[name_start..name_end];
    if name.is_empty() {
        TemplateInterpolationContext::Text
    } else {
        TemplateInterpolationContext::Attribute {
            name: name.to_ascii_lowercase(),
        }
    }
}

fn line_and_column(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..offset.min(source.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let start = prefix.rfind('\n').map_or(prefix.len(), |newline| {
        prefix.len().saturating_sub(newline + 1)
    });
    (line, start)
}

#[cfg(test)]
#[path = "interpolation_test.rs"]
#[cfg(test)]
mod interpolation_test;
