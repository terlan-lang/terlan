use std::collections::BTreeMap;

use terlan_syntax::parse_module_as_syntax_output;

#[test]
fn formal_syntax_output_direct_emit_embeds_file_imports() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_file_import_emit.

import file "./hello.html" as HelloHtml.

pub page(): Binary ->
HelloHtml.
"#,
    )
    .expect("parse syntax output file import fixture");

    let mut files = BTreeMap::new();
    files.insert("HelloHtml".to_string(), b"<h1>Hello</h1>".to_vec());

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &files,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("file import subset should lower directly from syntax output")
    .render();

    assert!(output.contains("page() ->\n    <<60,104,49,62,72,101,108,108,111,60,47,104,49,62>>."));
}

#[test]
fn formal_syntax_output_direct_emit_embeds_css_imports() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_css_import_emit.

import css "./style.css" as PageCss.

pub css(): Binary ->
PageCss.
"#,
    )
    .expect("parse syntax output css import fixture");

    let mut files = BTreeMap::new();
    files.insert("PageCss".to_string(), b"main{display:block}".to_vec());

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &files,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("css import subset should lower directly from syntax output")
    .render();

    assert!(output.contains(
        "css() ->\n    <<109,97,105,110,123,100,105,115,112,108,97,121,58,98,108,111,99,107,125>>."
    ));
}

#[test]
fn formal_syntax_output_direct_emit_embeds_markdown_import_fields() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_markdown_import_emit.

import markdown "./posts/hello.md" as HelloPost.

pub raw(): Binary ->
HelloPost.raw.

pub html(): Html[:none] ->
HelloPost.html.
"#,
    )
    .expect("parse syntax output markdown import fixture");

    let mut markdown = BTreeMap::new();
    markdown.insert(
        "HelloPost".to_string(),
        terlan_html::parse_markdown("# Hello\n", "posts/hello.md").expect("markdown"),
    );

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &markdown,
    )
    .expect("markdown import subset should lower directly from syntax output")
    .render();

    assert!(output.contains("raw() ->\n    <<35,32,72,101,108,108,111,10>>."));
    assert!(
        output.contains("html() ->\n    <<60,104,49,62,72,101,108,108,111,60,47,104,49,62,10>>.")
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_template_instantiation() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_template_emit.

template Page from "./page.terl.html" {
title: Text,
url: Text,
body: Html[:none]
}.

pub page(): Html[:none] ->
Page{
    title = "Hi & Bye",
    url = "/posts?tag=a&b=1",
    body = Html.raw("<strong>ok</strong>")
}.
"#,
    )
    .expect("parse syntax output template fixture");

    let mut templates = BTreeMap::new();
    templates.insert(
        "Page".to_string(),
        terlan_html::parse_html_template(
            r#"<a href="{url}">{title}</a><main>{body}</main>"#,
            "page.terl.html",
        )
        .expect("parse template"),
    );

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &templates,
        &BTreeMap::new(),
    )
    .expect("template subset should lower directly from syntax output")
    .render();

    assert!(output.contains("page() ->"));
    assert!(output.contains("typer_html:escape(\"/posts?tag=a&b=1\")"));
    assert!(output.contains("typer_html:escape(\"Hi & Bye\")"));
    assert!(output.contains("\"<strong>ok</strong>\""));
}

/// Verifies generated template function calls lower through the Erlang syntax
/// bridge.
///
/// Inputs:
/// - A template declaration with two properties.
/// - A function returning `Page(title = ..., body = ...)`.
///
/// Output:
/// - Test passes when the named call renders the parsed template body instead
///   of falling through to unsupported uppercase call lowering.
///
/// Transformation:
/// - Routes source-visible template function calls into the same HTML template
///   renderer used by constructor-style template instantiation.
#[test]
fn formal_syntax_output_direct_emit_lowers_named_template_call() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_template_named_call_emit.

template Page from "./page.terl.html" {
title: Binary,
body: Html[:none]
}.

pub page(): Html[:none] ->
Page(
    title = "Hi & Bye",
    body = Html.raw("<strong>ok</strong>")
).
"#,
    )
    .expect("parse syntax output named template call fixture");

    let mut templates = BTreeMap::new();
    templates.insert(
        "Page".to_string(),
        terlan_html::parse_html_template(
            r#"<h1>{title}</h1><main>{body}</main>"#,
            "page.terl.html",
        )
        .expect("parse template"),
    );

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &templates,
        &BTreeMap::new(),
    )
    .expect("named template call should lower directly from syntax output")
    .render();

    assert!(output.contains("page() ->"));
    assert!(output.contains("typer_html:escape(\"Hi & Bye\")"));
    assert!(output.contains("\"<strong>ok</strong>\""));
    assert!(!output.contains("Page("), "output:\n{}", output);
}

/// Verifies positional template calls apply declaration order and defaults.
///
/// Inputs:
/// - A template declaration with one required and one defaulted property.
/// - A function returning `Page("Hello")`.
///
/// Output:
/// - Test passes when the first argument maps to `title` and the omitted
///   `subtitle` property lowers from its default expression.
///
/// Transformation:
/// - Exercises the generated template callable path used by ordinary BEAM
///   project builds.
#[test]
fn formal_syntax_output_direct_emit_lowers_positional_template_call_with_default() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_template_positional_call_emit.

template Page from "./page.terl.html" {
title: Binary,
subtitle: Binary = "Ready"
}.

pub page(): Html[:none] ->
Page("Hello").
"#,
    )
    .expect("parse syntax output positional template call fixture");

    let mut templates = BTreeMap::new();
    templates.insert(
        "Page".to_string(),
        terlan_html::parse_html_template(r#"<h1>{title}</h1><p>{subtitle}</p>"#, "page.terl.html")
            .expect("parse template"),
    );

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &templates,
        &BTreeMap::new(),
    )
    .expect("positional template call should lower directly from syntax output")
    .render();

    assert!(output.contains("typer_html:escape(\"Hello\")"));
    assert!(output.contains("typer_html:escape(\"Ready\")"));
}

/// Verifies template instantiation lowering inserts omitted default properties.
///
/// Inputs:
/// - A template declaration whose `title` property has a binary default.
/// - A template instantiation that supplies no explicit fields.
/// - A parsed template body that reads `{title}`.
///
/// Output:
/// - Test passes when emitted Erlang escapes the default title value.
///
/// Transformation:
/// - Completes the template slot-value map from declaration defaults before
///   lowering parsed template nodes.
#[test]
fn formal_syntax_output_direct_emit_inserts_template_default_properties() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_template_default_emit.

template Page from "./page.terl.html" {
title: Binary = "Untitled"
}.

pub page(): Html[:none] ->
Page{}.
"#,
    )
    .expect("parse syntax output template default fixture");

    let mut templates = BTreeMap::new();
    templates.insert(
        "Page".to_string(),
        terlan_html::parse_html_template(r#"<h1>{title}</h1>"#, "page.terl.html")
            .expect("parse template"),
    );

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &templates,
        &BTreeMap::new(),
    )
    .expect("template default subset should lower directly from syntax output")
    .render();

    assert!(output.contains("page() ->"));
    assert!(output.contains("typer_html:escape(\"Untitled\")"));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_template_field_access_from_param_type() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_template_field_emit.

pub struct User {
name: Text
}.

template Page from "./page.terl.html" {
title: Text
}.

pub page(user: User): Html[:none] ->
Page{
    title = user.name
}.
"#,
    )
    .expect("parse syntax output template field fixture");

    let mut templates = BTreeMap::new();
    templates.insert(
        "Page".to_string(),
        terlan_html::parse_html_template(r#"<h1>{title}</h1>"#, "page.terl.html")
            .expect("parse template"),
    );

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &templates,
        &BTreeMap::new(),
    )
    .expect("template field access should lower directly from syntax output")
    .render();

    assert!(output.contains("typer_html:escape(User#user.name)"));
    assert!(!output.contains("User#name.name"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_html_blocks() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_html_block_emit.

pub view(Title: Text, Body: Binary): Html[:none] ->
html {
    <section class={["hero", "compact"]} data-title={Title}>
        <h1>{Title}</h1>
        {Html.raw(Body)}
    </section>
}.
"#,
    )
    .expect("parse syntax output html block fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("html block subset should lower directly from syntax output")
    .render();

    assert!(output.contains("view(Title, Body) ->"));
    assert!(output.contains("<<\"<section class=\\\"hero compact\\\"\">>"));
    assert!(output.contains("typer_html:escape(Title)"));
    assert!(output.contains("Body"));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_dynamic_html_attrs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_html_dynamic_attr_emit.

pub type Route = :home.

pub link(to: Route): Html[:none] ->
html { <a href={route.to_path(to)}>Open</a> }.
"#,
    )
    .expect("parse syntax output html dynamic attr fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("html dynamic attributes should lower directly from syntax output")
    .render();

    assert!(output.contains("<<\"<a\">>"));
    assert!(output.contains("<<\" href=\\\"\">>"));
    assert!(
        output.contains("typer_html:escape(route:to_path(To))"),
        "output:\n{}",
        output
    );
    assert!(output.contains("<<\"\\\"\">>"));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_raw_html_children_without_escape() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_html_raw_child_emit.

pub view(trusted: Binary): Html[:none] ->
html {
    <main>{Html.raw(trusted)}</main>
}.
"#,
    )
    .expect("parse syntax output html raw child fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("raw html children should lower directly from syntax output")
    .render();

    assert!(
        output.contains("view(Trusted) ->\n    [<<\"<main>\">>, Trusted, <<\"</main>\">>]."),
        "output:\n{}",
        output
    );
    assert!(
        !output.contains("typer_html:escape(Trusted)"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_html_list_comprehension_children() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_html_for_emit.

pub view(users: List[Text]): Html[:none] ->
html {
    <ul>{for user <- users {
        <li>{user}</li>
    }}</ul>
}.
"#,
    )
    .expect("parse syntax output html list comprehension fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("html list comprehension should lower directly from syntax output")
    .render();

    assert!(output
        .contains("[[<<\"<li>\">>, typer_html:escape(User), <<\"</li>\">>] || User <- Users]"));
    assert!(
        !output.contains("typer_html:escape(["),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_html_case_children_per_branch() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_html_case_emit.

pub view(admin: Bool, name: Text): Html[:none] ->
html {
    <main>{case admin {
        true -> <span>Admin</span>;
        false -> name
    }}</main>
}.
"#,
    )
    .expect("parse syntax output html case fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("html case child should lower directly from syntax output")
    .render();

    assert!(
        output.contains("true -> [<<\"<span>\">>, <<\"Admin\">>, <<\"</span>\">>]"),
        "output:\n{}",
        output
    );
    assert!(output.contains("false -> typer_html:escape(Name)"));
    assert!(
        !output.contains("typer_html:escape(case Admin of"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_html_field_access_from_param_type() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_html_field_emit.

pub struct User {
name: Text
}.

pub view(user: User): Html[:none] ->
html {
    <section data-name={user.name}>
        <h1>{user.name}</h1>
    </section>
}.
"#,
    )
    .expect("parse syntax output html field fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("html field access should lower directly from syntax output")
    .render();

    assert!(output.contains("typer_html:escape(User#user.name)"));
    assert!(!output.contains("User#name.name"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_handles_trait_and_template_decls() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_noop_decls_emit.

pub trait Show[A] {
show(value: A): Text.
}.

template Page from "./page.terl.html" {
title: Text
}.

pub id(value: Int): Int ->
value.
"#,
    )
    .expect("parse syntax output no-op decl fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("trait and template declarations should lower directly from syntax output")
    .render();

    assert!(output.contains("%% trait Show."));
    assert!(output.contains("-export([id/1])."));
    assert!(output.contains("id(Value) ->\n    Value."));
}
