use std::path::PathBuf;

use crate::terlan_html::{
    extract_template_metadata, template_attribute_slot_kind, template_interpolation_at_offset,
    TemplateAttributeSlotKind, TemplateInterpolationContext,
};
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Documentation, Position, Url};

use super::document::OpenDocument;

pub(super) fn template_completion_items(
    uri: &Url,
    document: &OpenDocument,
    position: Position,
) -> Vec<CompletionItem> {
    let Some(offset) = document.byte_offset_from_position(position) else {
        return Vec::new();
    };
    let Ok(Some(region)) = template_interpolation_at_offset(&document.text, offset) else {
        return Vec::new();
    };
    let path = uri
        .to_file_path()
        .unwrap_or_else(|_| PathBuf::from(uri.path()));
    let Ok(metadata) = extract_template_metadata(&document.text, &path) else {
        return Vec::new();
    };

    metadata
        .params
        .into_iter()
        .map(|param| {
            let slot = expected_slot(&region.context, &param.type_text);
            CompletionItem {
                label: param.name.clone(),
                kind: Some(CompletionItemKind::VARIABLE),
                detail: Some(format!(
                    "template param {}: {} [{slot}]",
                    param.name, param.type_text
                )),
                documentation: Some(Documentation::String(format!(
                    "Declared in `@template.params`; scope: template interpolation; expected context: `{slot}`."
                ))),
                sort_text: Some(format!("0-{}", param.name)),
                ..CompletionItem::default()
            }
        })
        .collect()
}

fn expected_slot(context: &TemplateInterpolationContext, type_text: &str) -> &'static str {
    match context {
        TemplateInterpolationContext::Text if is_trusted_fragment(type_text) => {
            "TrustedFragmentSlot"
        }
        TemplateInterpolationContext::Text => "TextSlot",
        TemplateInterpolationContext::Attribute { name } => {
            match template_attribute_slot_kind(name) {
                TemplateAttributeSlotKind::Url => "UrlSlot",
                TemplateAttributeSlotKind::Boolean => "BoolSlot",
                TemplateAttributeSlotKind::Scalar | TemplateAttributeSlotKind::TokenList => {
                    "AttrSlot"
                }
            }
        }
    }
}

fn is_trusted_fragment(type_text: &str) -> bool {
    matches!(
        type_text.trim(),
        "Html" | "Template.Html" | "std.template.Template.Html"
    )
}

#[cfg(test)]
#[path = "template_completion_test.rs"]
mod template_completion_test;
