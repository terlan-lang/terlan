use super::{escape_html_attr, template_attribute_slot_kind, TemplateAttributeSlotKind};
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

/// Backend-neutral value accepted by typed HTML attribute interpolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateAttributeValue {
    Scalar(String),
    Boolean(bool),
    Tokens(Vec<String>),
    Missing,
    Unsupported,
}

/// Renders one typed HTML attribute, or omits it when its value is absent.
pub fn render_template_attribute(
    name: &str,
    value: TemplateAttributeValue,
) -> Result<Option<String>, BoundaryError> {
    render_template_attribute_untyped(name, value)
}

fn render_template_attribute_untyped(
    name: &str,
    value: TemplateAttributeValue,
) -> Result<Option<String>, BoundaryError> {
    if matches!(value, TemplateAttributeValue::Missing) {
        return Ok(None);
    }

    match (template_attribute_slot_kind(name), value) {
        (TemplateAttributeSlotKind::Boolean, TemplateAttributeValue::Boolean(false)) => Ok(None),
        (TemplateAttributeSlotKind::Boolean, TemplateAttributeValue::Boolean(true)) => {
            Ok(Some(name.to_string()))
        }
        (TemplateAttributeSlotKind::Boolean, _) => Err(attribute_error(format!(
            "template boolean attribute `{name}` requires a Bool value"
        ))),
        (TemplateAttributeSlotKind::TokenList, TemplateAttributeValue::Scalar(text)) => {
            Ok(Some(quoted_attribute(name, &text)))
        }
        (TemplateAttributeSlotKind::TokenList, TemplateAttributeValue::Tokens(tokens)) => {
            validate_tokens(name, &tokens)?;
            Ok(Some(quoted_attribute(name, &tokens.join(" "))))
        }
        (TemplateAttributeSlotKind::TokenList, _) => Err(attribute_error(format!(
            "template token-list attribute `{name}` requires text or a text collection"
        ))),
        (TemplateAttributeSlotKind::Url, TemplateAttributeValue::Scalar(text)) => {
            validate_url(name, &text)?;
            Ok(Some(quoted_attribute(name, &text)))
        }
        (TemplateAttributeSlotKind::Url, _) => Err(attribute_error(format!(
            "template URL attribute `{name}` requires a URL text value"
        ))),
        (TemplateAttributeSlotKind::Scalar, TemplateAttributeValue::Scalar(text)) => {
            Ok(Some(quoted_attribute(name, &text)))
        }
        (TemplateAttributeSlotKind::Scalar, _) => Err(attribute_error(format!(
            "template attribute `{name}` requires a scalar value"
        ))),
    }
}

fn quoted_attribute(name: &str, value: &str) -> String {
    format!("{name}=\"{}\"", escape_html_attr(value))
}

fn validate_tokens(name: &str, tokens: &[String]) -> Result<(), BoundaryError> {
    for (index, token) in tokens.iter().enumerate() {
        if token.is_empty() || token.chars().any(char::is_whitespace) {
            return Err(attribute_error(format!(
                "template token-list attribute `{name}` has invalid token at index {index}"
            )));
        }
    }
    Ok(())
}

fn validate_url(name: &str, value: &str) -> Result<(), BoundaryError> {
    if value.trim() != value || value.chars().any(char::is_control) {
        return Err(attribute_error(unsafe_url_message(name)));
    }

    let base = url::Url::parse("https://template.invalid/").map_err(|error| {
        BoundaryError::sourced(
            ErrorDomain::TemplateRendering,
            "template.attribute.url_policy",
            "validate_url",
            "internal template URL policy configuration is invalid",
            error,
        )
    })?;
    let parsed = url::Url::options()
        .base_url(Some(&base))
        .parse(value)
        .map_err(|error| {
            BoundaryError::sourced(
                ErrorDomain::TemplateRendering,
                "template.attribute.url",
                "validate_url",
                unsafe_url_message(name),
                error,
            )
        })?;
    if matches!(parsed.scheme(), "http" | "https" | "mailto" | "tel") {
        Ok(())
    } else {
        Err(attribute_error(unsafe_url_message(name)))
    }
}

fn unsafe_url_message(name: &str) -> String {
    format!("template URL attribute `{name}` rejects an unsafe URL")
}

fn attribute_error(rendered: String) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::TemplateRendering,
        "render typed template attribute",
        rendered,
    )
}

#[cfg(test)]
#[path = "attribute_test.rs"]
#[cfg(test)]
mod attribute_test;
