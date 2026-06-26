use super::{
    render_syntax_module_docs_html, render_syntax_module_docs_json,
    render_syntax_module_docs_markdown,
};

/// Verifies the JSON documentation renderer emits a parseable module model.
///
/// Inputs:
/// - One parsed Terlan module with module and function docs.
///
/// Output:
/// - JSON object containing schema, module name, docs, and declaration
///   signature fields.
///
/// Transformation:
/// - Renders syntax output into the compiler-owned JSON documentation model
///   and parses it back through `serde_json`.
#[test]
fn renders_syntax_module_docs_json_model() {
    let source = r#"/**
 * Math docs.
 *
 * @module mathx
 */
module mathx.

/**
 * Adds one.
 */
pub add(x: Int): Int ->
    x + 1.
"#;
    let module = terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

    let json = render_syntax_module_docs_json(&module);
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse docs json");

    assert_eq!(value["schema"], "terlan-doc-module-v1");
    assert_eq!(value["module"], "mathx");
    assert_eq!(value["docs"][0], "Math docs.\n\n@module mathx");
    assert_eq!(value["declarations"][0]["kind"], "function");
    assert_eq!(value["declarations"][0]["name"], "add");
    assert_eq!(value["declarations"][0]["public"], true);
    assert_eq!(
        value["declarations"][0]["signature"],
        "pub add(x: Int): Int."
    );
}

/// Verifies documentation rendering excludes private declarations.
///
/// Inputs:
/// - One parsed Terlan module with public and private functions.
///
/// Output:
/// - Markdown and JSON outputs containing only the public function.
///
/// Transformation:
/// - Renders through both public docs formats and checks the public-API-only
///   documentation rule.
#[test]
fn renders_only_public_declarations() {
    let source = r#"module mathx.

/**
 * Adds one.
 */
pub add(x: Int): Int ->
    x + 1.

/**
 * Internal helper.
 */
hidden(x: Int): Int ->
    x.

/**
 * Receiver helper.
 */
pub (value: Int) to_string(): String ->
    "1".
"#;
    let module = terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

    let markdown = render_syntax_module_docs_markdown(&module);
    assert!(markdown.contains("add/1"));
    assert!(markdown.contains("Receiver Methods"));
    assert!(markdown.contains("Int.to_string(0)"));
    assert!(!markdown.contains("hidden"));

    let json = render_syntax_module_docs_json(&module);
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse docs json");
    let names = value["declarations"]
        .as_array()
        .expect("decls")
        .iter()
        .map(|decl| decl["name"].as_str().expect("declaration name"))
        .collect::<Vec<_>>();
    assert!(names.contains(&"add"));
    assert!(names.contains(&"to_string"));
    assert!(!names.contains(&"hidden"));
}

/// Verifies HTML documentation renders a usable public module reference.
///
/// Inputs:
/// - One parsed module containing module docs, a struct, and a receiver
///   method.
///
/// Output:
/// - HTML containing the module shell, declaration navigation, field
///   details, method section, and Terlan signature code.
///
/// Transformation:
/// - Renders formal syntax output to static HTML without going through a
///   Markdown validation artifact.
#[test]
fn renders_syntax_module_docs_html_reference_page() {
    let source = r#"/**
 * User module docs.
 */
module std.core.User.

/**
 * User record.
 */
pub struct User {
    name: String
}.

/**
 * Returns the display name.
 *
 * ```terlan
 * user.display_name().
 * ```
 */
pub (user: User) display_name(): String ->
    user.name.
"#;
    let module = terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

    let html = render_syntax_module_docs_html(&module);

    assert!(html.contains("<h1>std.core.User</h1>"));
    assert!(html.contains("User&#32;module&#32;docs."));
    assert!(html.contains("Structs"));
    assert!(html.contains("Receiver&#32;Methods"));
    assert!(html.contains("pub&#32;struct&#32;User"));
    assert!(html.contains("pub&#32;(user:&#32;User)&#32;display_name():&#32;String."));
    assert!(html.contains("user.display_name()."));
}
