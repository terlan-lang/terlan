use std::collections::BTreeMap;
use std::path::Path;

use crate::terlan_syntax::parse_module_as_syntax_output;

use super::{render_syntax_static_entrypoint, StaticSyntaxRenderError};

fn render(source: &str, template: &str) -> Result<String, StaticSyntaxRenderError> {
    let module = parse_module_as_syntax_output(source).expect("parse syntax-output module");
    let templates = BTreeMap::from([(
        "Page".to_string(),
        crate::terlan_html::parse_template(template, Path::new("page.terl.html"))
            .expect("parse page template"),
    )]);
    render_syntax_static_entrypoint(&module, &templates, &BTreeMap::new(), "home")
}

#[test]
fn renders_typed_boolean_token_list_and_optional_attributes() {
    let source = r#"
module site.

template Page from "./templates/page.terl.html" {
    disabled: Bool,
    classes: List[String],
    href: Option[String],
    title: Option[String]
}.

pub home(): Html ->
    Page(
        disabled = true,
        classes = ["card", "active"],
        href = Some("/users?x=1&y=2"),
        title = None
    ).
"#;

    assert_eq!(
        render(
            source,
            r#"<a disabled="${disabled}" class="${classes}" href="${href}" title="${title}">users</a>"#
        ),
        Ok(r#"<a disabled class="card active" href="/users?x=1&amp;y=2">users</a>"#.to_string())
    );
}

#[test]
fn omits_false_boolean_and_none_url_attributes() {
    let source = r#"
module site.

template Page from "./templates/page.terl.html" {
    disabled: Bool,
    href: Option[String]
}.

pub home(): Html ->
    Page(disabled = false, href = None).
"#;

    assert_eq!(
        render(
            source,
            r#"<button disabled="${disabled}" href="${href}">save</button>"#
        ),
        Ok("<button>save</button>".to_string())
    );
}

#[test]
fn rejects_unsafe_static_url_interpolation() {
    let source = r#"
module site.

template Page from "./templates/page.terl.html" {
    href: String
}.

pub home(): Html ->
    Page(href = "javascript:alert(1)").
"#;

    assert_eq!(
        render(source, r#"<a href="${href}">unsafe</a>"#),
        Err(StaticSyntaxRenderError::Invalid(
            "template URL attribute `href` rejects an unsafe URL".to_string()
        ))
    );
}

#[test]
fn rejects_non_text_static_token_list_members() {
    let source = r#"
module site.

template Page from "./templates/page.terl.html" {
    classes: List[String]
}.

pub home(): Html ->
    Page(classes = ["card", 7]).
"#;

    assert_eq!(
        render(source, r#"<main class="${classes}"></main>"#),
        Err(StaticSyntaxRenderError::Invalid(
            "template token-list attribute `class` requires text collection members".to_string()
        ))
    );
}
