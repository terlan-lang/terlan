use quote::ToTokens;
use syn::visit::{self, Visit};
use syn::Attribute;

#[derive(Clone, Debug)]
pub(super) struct LintAttribute {
    pub(super) path: String,
    pub(super) kind: String,
    pub(super) tokens: String,
    pub(super) lints: String,
    pub(super) inner: bool,
}

pub(super) struct Visitor<'a> {
    pub(super) path: &'a str,
    pub(super) attributes: Vec<LintAttribute>,
}

pub(super) fn normalized_lints(attribute: &Attribute) -> String {
    let normalized = attribute
        .meta
        .to_token_stream()
        .to_string()
        .replace(' ', "");
    normalized
        .strip_prefix("allow(")
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or("")
        .to_owned()
}

impl<'ast> Visit<'ast> for Visitor<'_> {
    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        let kind = if attribute.path().is_ident("allow") {
            Some("allow")
        } else if attribute.path().is_ident("expect") {
            Some("expect")
        } else if attribute.path().is_ident("cfg_attr")
            && attribute
                .meta
                .to_token_stream()
                .to_string()
                .contains("allow")
        {
            Some("cfg-allow")
        } else {
            None
        };
        if let Some(kind) = kind {
            self.attributes.push(LintAttribute {
                path: self.path.to_owned(),
                kind: kind.to_owned(),
                tokens: attribute.meta.to_token_stream().to_string(),
                lints: normalized_lints(attribute),
                inner: matches!(attribute.style, syn::AttrStyle::Inner(_)),
            });
        }
        visit::visit_attribute(self, attribute);
    }
}
