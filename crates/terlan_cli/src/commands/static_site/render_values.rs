use std::collections::BTreeMap;

use super::{is_static_html_return_type, TEMPLATE_CHILDREN_SLOT};

/// Runtime value model for static template rendering.
///
/// Inputs:
/// - Produced by evaluating syntax-output expressions used as template props.
///
/// Output:
/// - A constrained static value that can be rendered, passed to component
///   templates, or inspected by slot paths.
///
/// Transformation:
/// - Keeps only compile-time-renderable values and rejects dynamic expression
///   shapes before HTML output is written.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum StaticTemplateValue {
    Text(String),
    Int(i64),
    Bool(bool),
    Html(String),
    Record {
        name: String,
        fields: BTreeMap<String, StaticTemplateValue>,
    },
}

/// Resolves a slot path to a static template value.
///
/// Inputs:
/// - `values`: evaluated template prop values.
/// - `slot`: parsed slot path.
///
/// Output:
/// - Cloned static value at the slot path, or an error message.
///
/// Transformation:
/// - Resolves the root value and walks record fields for dotted slot paths.
pub(super) fn static_template_slot_value(
    values: &BTreeMap<String, StaticTemplateValue>,
    slot: &terlan_html::HtmlSlot,
) -> Result<StaticTemplateValue, String> {
    let root = slot
        .path
        .first()
        .ok_or_else(|| "empty static template slot".to_string())?;
    let mut value = values
        .get(root)
        .cloned()
        .ok_or_else(|| format!("missing static template slot value `{}`", root))?;
    for field in slot.path.iter().skip(1) {
        match value {
            StaticTemplateValue::Record { ref fields, .. } => {
                value = fields.get(field).cloned().ok_or_else(|| {
                    format!(
                        "missing static template field `{}` in slot `{}`",
                        field,
                        slot.path.join(".")
                    )
                })?;
            }
            _ => {
                return Err(format!(
                    "static template slot `{}` does not reference a record field",
                    slot.path.join(".")
                ))
            }
        }
    }
    Ok(value)
}

/// Returns whether a slot path refers to the reserved `children` slot.
///
/// Inputs:
/// - `slot`: parsed slot path.
///
/// Output:
/// - `true` when the path is exactly `children`.
///
/// Transformation:
/// - Compares the single path segment with the reserved slot name.
pub(super) fn is_template_children_slot(slot: &terlan_html::HtmlSlot) -> bool {
    slot.path.len() == 1
        && slot
            .path
            .first()
            .is_some_and(|root| root == TEMPLATE_CHILDREN_SLOT)
}

/// Converts a static template value into text.
///
/// Inputs:
/// - `value`: static template value.
///
/// Output:
/// - Text representation or an error when the value is not text-renderable.
///
/// Transformation:
/// - Stringifies scalar values and HTML fragments, and rejects records.
pub(super) fn static_template_value_text(value: &StaticTemplateValue) -> Result<String, String> {
    match value {
        StaticTemplateValue::Text(text) => Ok(text.clone()),
        StaticTemplateValue::Int(value) => Ok(value.to_string()),
        StaticTemplateValue::Bool(value) => Ok(value.to_string()),
        StaticTemplateValue::Html(html) => Ok(html.clone()),
        StaticTemplateValue::Record { name, .. } => {
            Err(format!("cannot render static record `{}` as text", name))
        }
    }
}

/// Decodes supported static string and binary string literals.
///
/// Inputs:
/// - `text`: literal text from syntax output.
///
/// Output:
/// - Literal contents with simple escapes decoded.
///
/// Transformation:
/// - Strips `<<"...">>` or `"..."` delimiters when present, then unescapes the
///   inner text.
pub(super) fn static_literal_text(text: &str) -> String {
    if let Some(inner) = text
        .strip_prefix("<<\"")
        .and_then(|text| text.strip_suffix("\">>"))
        .or_else(|| {
            text.strip_prefix('"')
                .and_then(|text| text.strip_suffix('"'))
        })
    {
        return unescape_static_literal_text(inner);
    }
    text.to_string()
}

/// Unescapes the supported static literal escape sequences.
///
/// Inputs:
/// - `text`: literal contents without delimiters.
///
/// Output:
/// - String with supported escapes decoded.
///
/// Transformation:
/// - Replaces newline, carriage-return, tab, quote, and backslash escapes while
///   preserving unknown escapes verbatim.
fn unescape_static_literal_text(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Returns whether a template prop annotation denotes HTML.
///
/// Inputs:
/// - `type_text`: annotation text from syntax output.
///
/// Output:
/// - `true` for public template HTML annotations and older internal HTML
///   annotations accepted by the static-site compiler.
///
/// Transformation:
/// - Delegates to the route HTML type checker and keeps bare `Html` accepted
///   for external template declarations that have not imported the std module.
pub(super) fn is_static_template_html_type(type_text: &str) -> bool {
    let trimmed = type_text.trim();
    trimmed == "Html" || is_static_html_return_type(trimmed)
}
