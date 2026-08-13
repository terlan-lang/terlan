use std::collections::BTreeSet;

use super::{
    super::parse_tree::BinaryLayoutField, LalrpopLoweringContext, LalrpopLoweringError,
    LalrpopLoweringResult,
};
use crate::terlan_syntax::lalrpop_syntax::LalrpopSyntaxNode;

pub(super) fn validate(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    endian: &str,
    fields: &[BinaryLayoutField],
) -> LalrpopLoweringResult<()> {
    if !matches!(endian, "big" | "little") {
        return Err(context.error(node, "binary layout endian must be `big` or `little`"));
    }
    if fields.is_empty() {
        return Err(context.error(node, "binary layouts require at least one descriptor field"));
    }
    let mut names = BTreeSet::new();
    let mut rest_count = 0usize;
    for field in fields {
        validate_descriptor(field)?;
        if !names.insert(field.name.as_str()) {
            return Err(LalrpopLoweringError {
                message: format!("duplicate binary layout field `{}`", field.name),
                span: field.descriptor.span,
            });
        }
        if is_rest(&field.descriptor.text) {
            rest_count += 1;
            if rest_count > 1 {
                return Err(LalrpopLoweringError {
                    message: "binary layouts allow only one Rest field".to_string(),
                    span: field.descriptor.span,
                });
            }
        }
    }
    if let Some(field) = fields
        .iter()
        .take(fields.len().saturating_sub(1))
        .find(|field| is_rest(&field.descriptor.text))
    {
        return Err(LalrpopLoweringError {
            message: "binary layout Rest field must be terminal".to_string(),
            span: field.descriptor.span,
        });
    }
    Ok(())
}

fn validate_descriptor(field: &BinaryLayoutField) -> LalrpopLoweringResult<()> {
    let text = field.descriptor.text.trim();
    let valid = is_rest(text)
        || ["Utf8", "Utf16", "Utf32"]
            .into_iter()
            .any(|name| matches_name(text, name))
        || ["UInt", "IntBits", "Bytes", "Bits"]
            .into_iter()
            .any(|name| has_width(text, name));
    if valid {
        Ok(())
    } else {
        Err(LalrpopLoweringError {
            message: format!("binary layout field uses unsupported descriptor `{text}`"),
            span: field.descriptor.span,
        })
    }
}

fn is_rest(text: &str) -> bool {
    matches!(
        text.trim(),
        "Rest" | "std.binary.Binary.Rest" | "std.binary.Rest"
    )
}

fn has_width(text: &str, name: &str) -> bool {
    let text = text.trim();
    let Some(open) = text.find('[') else {
        return false;
    };
    let Some(close) = text.strip_suffix(']') else {
        return false;
    };
    if !matches_name(text[..open].trim(), name) {
        return false;
    }
    let width = close[open + 1..].trim();
    !width.is_empty() && width != "0" && width.chars().all(|character| character.is_ascii_digit())
}

fn matches_name(head: &str, name: &str) -> bool {
    head == name
        || head
            .strip_prefix("std.binary.Binary.")
            .is_some_and(|suffix| suffix == name)
        || head
            .strip_prefix("std.binary.")
            .is_some_and(|suffix| suffix == name)
}
