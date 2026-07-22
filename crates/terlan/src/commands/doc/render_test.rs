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
    let module = crate::terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

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
        "@pure\npub add(x: Int): Int."
    );
}

/// Verifies body-inferred purity is visible without a source annotation.
#[test]
fn renders_inferred_purity_and_excludes_effectful_functions() {
    let source = r#"module inferred_pure_docs.

pub normalize(value: Int): Int ->
    value + 1.

pub replace_first(values: List[Int]): Unit ->
    values[0] = 1.
"#;
    let module = crate::terlan_syntax::parse_module_as_syntax_output(source)
        .expect("parse inferred purity docs module");

    let json = render_syntax_module_docs_json(&module);
    let markdown = render_syntax_module_docs_markdown(&module);
    let html = render_syntax_module_docs_html(&module);

    assert!(json.contains("@pure\\npub normalize(value: Int): Int."));
    assert!(markdown.contains("@pure\npub normalize(value: Int): Int."));
    assert!(html.contains("@pure&#10;pub&#32;normalize(value:&#32;Int):&#32;Int."));
    assert!(!json.contains("@pure\\npub replace_first"));
    assert!(!markdown.contains("@pure\npub replace_first"));
    assert!(!html.contains("@pure&#10;pub&#32;replace_first"));
}

/// Verifies documentation renderers preserve pure function metadata.
///
/// Inputs:
/// - One parsed Terlan module with a public `@pure` function and public
///   `@pure` receiver method.
///
/// Output:
/// - JSON, Markdown, and HTML documentation all include `@pure` in rendered
///   Terlan signatures.
///
/// Transformation:
/// - Exercises the shared documentation signature renderer so public docs stay
///   aligned with HIR summaries and LSP hover metadata.
#[test]
fn renders_pure_function_metadata_in_public_docs() {
    let source = r#"module pure_docs.

pub struct Box {
    value: Int
}.

/**
 * Reads values without effects.
 */
pub trait Read[T] {
    @pure
    read_trait(value: T): Int.
}.

/**
 * Returns the input.
 */
@pure
pub id(value: Int): Int ->
    value.

/**
 * Reads the value.
 */
@pure
pub (box: Box) read(): Int ->
    0.
"#;
    let module = crate::terlan_syntax::parse_module_as_syntax_output(source)
        .expect("parse pure docs module");

    let json = render_syntax_module_docs_json(&module);
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse docs json");
    let signatures = value["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .map(|declaration| declaration["signature"].as_str().expect("signature"))
        .collect::<Vec<_>>();
    assert!(signatures.contains(&"@pure\npub id(value: Int): Int."));
    assert!(signatures.contains(&"@pure\npub (box: Box) read(): Int."));

    let markdown = render_syntax_module_docs_markdown(&module);
    assert!(markdown.contains("@pure\npub id(value: Int): Int."));
    assert!(markdown.contains("@pure\npub (box: Box) read(): Int."));
    assert!(markdown.contains("    @pure\n    read_trait(value: T): Int."));

    let html = render_syntax_module_docs_html(&module);
    assert!(html.contains("@pure&#10;pub&#32;id(value:&#32;Int):&#32;Int."));
    assert!(html.contains("@pure&#10;pub&#32;(box:&#32;Box)&#32;read():&#32;Int."));
    assert!(html.contains("@pure&#10;&#32;&#32;&#32;&#32;read_trait(value:&#32;T):&#32;Int."));
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
    let module = crate::terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

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

#[test]
fn value_lifecycle_docs_render_public_constants_and_valued_unions_only() {
    let source = r#"module docs.ValueLifecycle.

/// Stable exported value.
pub const ANSWER: Int = 42.
const PRIVATE_VALUE: Int = 7.

/// Compile-time helper.
pub const bump(value: Int): Int -> value + 1.

/// Closed status values.
pub type Status: Int = OK = 200 | NOT_FOUND = 404.
"#;
    let module = crate::terlan_syntax::parse_module_as_syntax_output(source).expect("parse docs");

    let markdown = render_syntax_module_docs_markdown(&module);
    assert!(
        markdown.contains("pub const ANSWER: Int = 42."),
        "{markdown}"
    );
    assert!(
        markdown.contains("pub const bump(value: Int): Int."),
        "{markdown}"
    );
    assert!(
        markdown.contains("pub type Status: Int = OK = 200 | NOT_FOUND = 404."),
        "{markdown}"
    );
    assert!(!markdown.contains("PRIVATE_VALUE"));

    let json = render_syntax_module_docs_json(&module);
    assert!(json.contains("pub type Status: Int = OK = 200 | NOT_FOUND = 404."));
    assert!(!json.contains("PRIVATE_VALUE"));

    let html = render_syntax_module_docs_html(&module);
    assert!(html.contains("Status"));
    assert!(html.contains("NOT_FOUND"));
    assert!(!html.contains("PRIVATE_VALUE"));
}

/// Verifies public raw shape declarations are included in generated docs.
///
/// Inputs:
/// - One parsed module with documented public and private shape declarations.
///
/// Output:
/// - Markdown, JSON, and HTML docs include the public shape and omit the
///   private shape.
///
/// Transformation:
/// - Exercises the documentation renderer over parse-preserved shape syntax
///   without requiring semantic shape expansion.
#[test]
fn renders_public_shape_declarations() {
    let source = r#"module docs.Shapes.

/**
 * Matches a user asset path.
 */
pub shape UserAsset(id, file) =
    "users/${id: Int}/assets/${file}".

/**
 * Internal response shape.
 */
shape InternalResponse(body) =
    {status, body} where status in 200..299.
"#;
    let module = crate::terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

    let markdown = render_syntax_module_docs_markdown(&module);
    assert!(markdown.contains("## Shapes"));
    assert!(markdown.contains("### `UserAsset`"));
    assert!(markdown.contains("Matches a user asset path."));
    assert!(markdown.contains("pub shape UserAsset"));
    assert!(!markdown.contains("InternalResponse"));

    let json = render_syntax_module_docs_json(&module);
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse docs json");
    assert_eq!(value["declarations"][0]["kind"], "shape");
    assert_eq!(value["declarations"][0]["name"], "UserAsset");
    assert_eq!(value["declarations"][0]["public"], true);
    assert!(value["declarations"][0]["signature"]
        .as_str()
        .expect("shape signature")
        .contains("pub shape UserAsset"));

    let html = render_syntax_module_docs_html(&module);
    assert!(html.contains("Shapes"));
    assert!(html.contains("UserAsset"));
    assert!(html.contains("Matches&#32;a&#32;user&#32;asset&#32;path."));
    assert!(!html.contains("InternalResponse"));
}

/// Verifies public negative trait facts retain their polarity in every doc format.
#[test]
fn renders_public_negative_trait_impl_declarations() {
    let source = r#"module docs.NegativeTraits.

pub opaque type SecretKey.

/**
 * Prevents accidental JSON serialization.
 */
pub impl not JsonEncode[SecretKey].
"#;
    let module = crate::terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

    let markdown = render_syntax_module_docs_markdown(&module);
    assert!(markdown.contains("### `not JsonEncode[SecretKey]`"));
    assert!(markdown.contains("pub impl not JsonEncode[SecretKey]."));
    assert!(!markdown.contains("JsonEncode for SecretKey"));

    let json = render_syntax_module_docs_json(&module);
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse docs json");
    let declaration = value["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .find(|declaration| declaration["signature"] == "pub impl not JsonEncode[SecretKey].")
        .expect("negative impl declaration");
    assert_eq!(
        declaration["signature"],
        "pub impl not JsonEncode[SecretKey]."
    );

    let html = render_syntax_module_docs_html(&module);
    assert!(html.contains("not&#32;JsonEncode[SecretKey]"));
    assert!(!html.contains("JsonEncode&#32;for&#32;SecretKey"));
}

/// Verifies generic impl implications retain their source form in every
/// public documentation format.
#[test]
fn renders_generic_trait_impl_implications() {
    let source = r#"module implication_docs.

pub trait Render[T] {
    render(value: T): String.
}.

pub impl Render[T => {title: String}] for T {
    render(value: T): String -> value.title.
}.
"#;
    let module = crate::terlan_syntax::parse_module_as_syntax_output(source)
        .expect("parse generic impl docs module");

    let markdown = render_syntax_module_docs_markdown(&module);
    assert!(markdown.contains("pub impl Render[T => {title: String}] for T"));

    let json = render_syntax_module_docs_json(&module);
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse docs json");
    assert!(value["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .any(|declaration| {
            declaration["signature"] == "pub impl Render[T => {title: String}] for T."
        }));

    let html = render_syntax_module_docs_html(&module);
    assert!(
        html.contains("Render[T&#32;&#61;&gt;&#32;{title:&#32;String}]"),
        "rendered HTML:\n{html}"
    );
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
    let module = crate::terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

    let html = render_syntax_module_docs_html(&module);

    assert!(html.contains("<h1>std.core.User</h1>"));
    assert!(html.contains("User&#32;module&#32;docs."));
    assert!(html.contains("Structs"));
    assert!(html.contains("Receiver&#32;Methods"));
    assert!(html.contains("pub&#32;struct&#32;User"));
    assert!(html.contains("pub&#32;(user:&#32;User)&#32;display_name():&#32;String."));
    assert!(html.contains("user.display_name()."));
}
