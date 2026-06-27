use std::collections::BTreeMap;
use std::path::Path;

use crate::terlan_syntax::parse_module_as_syntax_output;

use super::{render_syntax_static_entrypoint, StaticSyntaxRenderError};

/// Parses a syntax-output module for static renderer tests.
///
/// Inputs:
/// - `source`: Terlan source containing template declarations and an entrypoint.
///
/// Output:
/// - Formal syntax-output module used by the renderer.
///
/// Transformation:
/// - Routes source through the same parser/output path used by static commands.
fn syntax_module(source: &str) -> crate::terlan_syntax::SyntaxModuleOutput {
    parse_module_as_syntax_output(source).expect("parse syntax-output module")
}

/// Parses one external HTML template for static renderer tests.
///
/// Inputs:
/// - `source`: `.terl.html` source body.
///
/// Output:
/// - Parsed HTML template keyed by the `Page` template declaration name.
///
/// Transformation:
/// - Uses the public Terlan HTML parser so tests exercise real slot parsing.
fn page_template(source: &str) -> BTreeMap<String, crate::terlan_html::HtmlTemplate> {
    BTreeMap::from([(
        "Page".to_string(),
        crate::terlan_html::parse_template(source, Path::new("page.terl.html"))
            .expect("parse page template"),
    )])
}

/// Verifies named template calls render as generated template functions.
///
/// Inputs:
/// - A template declaration with one prop.
/// - An entrypoint returning `Page(title = "Home")`.
///
/// Output:
/// - Rendered HTML with the named prop substituted.
///
/// Transformation:
/// - Confirms the static renderer normalizes a known direct call into the
///   existing template-instantiation field model.
#[test]
fn renders_named_template_call() {
    let module = syntax_module(
        r#"
module site.

template Page from "./templates/page.terl.html" {
    title: Text
}.

pub home(): Html ->
    Page(title = "Home").
"#,
    );
    let html = render_syntax_static_entrypoint(
        &module,
        &page_template("<main>${title}</main>"),
        &BTreeMap::new(),
        "home",
    )
    .expect("render named template call");

    assert_eq!(html, "<main>Home</main>");
}

/// Verifies static template text slots are escaped as HTML text.
///
/// Inputs:
/// - A template declaration with one `Text` prop.
/// - An entrypoint passing text that looks like HTML/script markup.
///
/// Output:
/// - Rendered HTML with text escaped rather than interpreted as markup.
///
/// Transformation:
/// - Routes untrusted text through the static renderer and confirms escaping
///   is delegated by the render-value layer before template output is emitted.
#[test]
fn renders_template_text_slot_as_escaped_html_text() {
    let module = syntax_module(
        r#"
module site.

template Page from "./templates/page.terl.html" {
    title: Text
}.

pub home(): Html ->
    Page(title = "<script>alert(1)</script>&").
"#,
    );
    let html = render_syntax_static_entrypoint(
        &module,
        &page_template("<main>${title}</main>"),
        &BTreeMap::new(),
        "home",
    )
    .expect("render escaped text slot");

    assert_eq!(
        html,
        "<main>&lt;script&gt;alert(1)&lt;&#47;script&gt;&amp;</main>"
    );
}

/// Verifies positional template calls use declaration prop order and defaults.
///
/// Inputs:
/// - A template declaration with one required prop and one defaulted prop.
/// - An entrypoint returning `Page("Home")`.
///
/// Output:
/// - Rendered HTML containing the positional and defaulted prop values.
///
/// Transformation:
/// - Confirms callable template generation follows declaration order and fills
///   omitted trailing properties from template defaults.
#[test]
fn renders_positional_template_call_with_default_prop() {
    let module = syntax_module(
        r#"
module site.

template Page from "./templates/page.terl.html" {
    title: Text,
    subtitle: Text = "Ready"
}.

pub home(): Html ->
    Page("Home").
"#,
    );
    let html = render_syntax_static_entrypoint(
        &module,
        &page_template("<main><h1>${title}</h1><p>${subtitle}</p></main>"),
        &BTreeMap::new(),
        "home",
    )
    .expect("render positional template call");

    assert_eq!(html, "<main><h1>Home</h1><p>Ready</p></main>");
}

/// Verifies template calls reject missing required props early.
///
/// Inputs:
/// - A template declaration with one required prop.
/// - An entrypoint returning `Page()`.
///
/// Output:
/// - Static render error naming the missing prop.
///
/// Transformation:
/// - Confirms generated template functions validate their full call signature
///   before rendering slot substitutions.
#[test]
fn rejects_template_call_missing_required_prop() {
    let module = syntax_module(
        r#"
module site.

template Page from "./templates/page.terl.html" {
    title: Text
}.

pub home(): Html ->
    Page().
"#,
    );
    let err = render_syntax_static_entrypoint(
        &module,
        &page_template("<main>${title}</main>"),
        &BTreeMap::new(),
        "home",
    )
    .expect_err("reject missing template prop");

    assert_eq!(
        err,
        StaticSyntaxRenderError::Invalid(
            "static template `Page` is missing required prop `title`".to_string()
        )
    );
}

/// Verifies template calls reject unknown named props.
///
/// Inputs:
/// - A template declaration with one known prop.
/// - An entrypoint returning `Page(header = "Home")`.
///
/// Output:
/// - Static render error naming the unknown prop.
///
/// Transformation:
/// - Confirms generated template functions validate named arguments against
///   the declared template signature before rendering slot substitutions.
#[test]
fn rejects_template_call_unknown_named_prop() {
    let module = syntax_module(
        r#"
module site.

template Page from "./templates/page.terl.html" {
    title: Text
}.

pub home(): Html ->
    Page(header = "Home").
"#,
    );
    let err = render_syntax_static_entrypoint(
        &module,
        &page_template("<main>${title}</main>"),
        &BTreeMap::new(),
        "home",
    )
    .expect_err("reject unknown template prop");

    assert_eq!(
        err,
        StaticSyntaxRenderError::Invalid("static template `Page` has no prop `header`".to_string())
    );
}

/// Verifies template calls reject duplicate named props.
///
/// Inputs:
/// - A template declaration with one prop.
/// - An entrypoint returning `Page(title = "Home", title = "Again")`.
///
/// Output:
/// - Static render error naming the duplicated prop.
///
/// Transformation:
/// - Confirms generated template functions keep named argument binding
///   single-sourced before rendering slot substitutions.
#[test]
fn rejects_template_call_duplicate_named_prop() {
    let module = syntax_module(
        r#"
module site.

template Page from "./templates/page.terl.html" {
    title: Text
}.

pub home(): Html ->
    Page(title = "Home", title = "Again").
"#,
    );
    let err = render_syntax_static_entrypoint(
        &module,
        &page_template("<main>${title}</main>"),
        &BTreeMap::new(),
        "home",
    )
    .expect_err("reject duplicate template prop");

    assert_eq!(
        err,
        StaticSyntaxRenderError::Invalid(
            "static template `Page` prop `title` was provided more than once".to_string()
        )
    );
}

/// Verifies template text slots escape HTML by default.
///
/// Inputs:
/// - A template declaration with one text prop.
/// - An entrypoint passing text containing angle brackets and ampersands.
///
/// Output:
/// - Rendered HTML with the prop escaped in a text-node context.
///
/// Transformation:
/// - Confirms static template rendering treats interpolated text as data unless
///   a later explicit trusted-HTML path is used.
#[test]
fn renders_template_text_slot_escaped_by_default() {
    let module = syntax_module(
        r#"
module site.

template Page from "./templates/page.terl.html" {
    title: Text
}.

pub home(): Html ->
    Page(title = "<script>&").
"#,
    );
    let html = render_syntax_static_entrypoint(
        &module,
        &page_template("<main>${title}</main>"),
        &BTreeMap::new(),
        "home",
    )
    .expect("render escaped text slot");

    assert_eq!(html, "<main>&lt;script&gt;&amp;</main>");
}

/// Verifies template attribute slots escape attribute-sensitive characters.
///
/// Inputs:
/// - A template declaration with one text prop.
/// - An entrypoint passing text containing a quote, angle brackets, and an
///   ampersand.
///
/// Output:
/// - Rendered HTML with the prop escaped in an attribute context.
///
/// Transformation:
/// - Confirms static template rendering uses stricter escaping for attributes
///   than for text nodes.
#[test]
fn renders_template_attribute_slot_escaped_by_default() {
    let module = syntax_module(
        r#"
module site.

template Page from "./templates/page.terl.html" {
    title: Text
}.

pub home(): Html ->
    Page(title = "\"<admin>&").
"#,
    );
    let html = render_syntax_static_entrypoint(
        &module,
        &page_template(r#"<a href="${title}">link</a>"#),
        &BTreeMap::new(),
        "home",
    )
    .expect("render escaped attribute slot");

    assert_eq!(html, r#"<a href="&quot;&lt;admin&gt;&amp;">link</a>"#);
}
