use super::*;

#[test]
fn typed_attribute_renderer_preserves_boolean_and_optional_semantics() {
    assert_eq!(
        render_template_attribute("disabled", TemplateAttributeValue::Boolean(true)),
        Ok(Some("disabled".to_string()))
    );
    assert_eq!(
        render_template_attribute("disabled", TemplateAttributeValue::Boolean(false)),
        Ok(None)
    );
    assert_eq!(
        render_template_attribute("title", TemplateAttributeValue::Missing),
        Ok(None)
    );
}

#[test]
fn typed_attribute_renderer_escapes_url_and_token_values() {
    assert_eq!(
        render_template_attribute(
            "href",
            TemplateAttributeValue::Scalar("/users?x=1&y=2".to_string())
        ),
        Ok(Some("href=\"/users?x=1&amp;y=2\"".to_string()))
    );
    assert_eq!(
        render_template_attribute(
            "class",
            TemplateAttributeValue::Tokens(vec!["card".to_string(), "active".to_string()])
        ),
        Ok(Some("class=\"card active\"".to_string()))
    );
}

#[test]
fn typed_attribute_renderer_rejects_unsafe_urls_and_invalid_tokens() {
    assert_eq!(
        render_template_attribute(
            "href",
            TemplateAttributeValue::Scalar("javascript:alert(1)".to_string())
        ),
        Err("template URL attribute `href` rejects an unsafe URL".to_string())
    );
    assert_eq!(
        render_template_attribute(
            "class",
            TemplateAttributeValue::Tokens(vec!["two words".to_string()])
        ),
        Err("template token-list attribute `class` has invalid token at index 0".to_string())
    );
}
