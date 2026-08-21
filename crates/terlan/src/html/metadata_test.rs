use super::*;

/// Extracts supported `@page` metadata keys.
///
/// Inputs:
/// - A `.terl.md` source with all supported documentation page keys.
///
/// Output:
/// - Test passes when every supported key is preserved in typed page metadata.
///
/// Transformation:
/// - Verifies built-in `@page` schema validation accepts only the documented
///   metadata surface before static route discovery consumes it.
#[test]
fn extracts_page_metadata_with_supported_keys() {
    let metadata = extract_page_metadata(
        "@page { title = \"Home\", route = \"/\", layout = \"Layout\", description = \"Start here\", section = \"Guides\", nav_title = \"Start\", parent = \"/docs\", weight = 10, kind = \"blog\", date = \"2026-08-15\", aliases = [\"/start\", \"/home\"], summary = \"A concise release summary.\", authors = [\"Anatoly\", \"Terlan team\"], tags = [\"release\", \"compiler\"], draft = true }\n\n# Home",
        "content/index.terl.md",
    )
    .expect("extract page metadata");

    assert_eq!(metadata.title.as_deref(), Some("Home"));
    assert_eq!(metadata.route.as_deref(), Some("/"));
    assert_eq!(metadata.layout.as_deref(), Some("Layout"));
    assert_eq!(metadata.description.as_deref(), Some("Start here"));
    assert_eq!(metadata.section.as_deref(), Some("Guides"));
    assert_eq!(metadata.nav_title.as_deref(), Some("Start"));
    assert_eq!(metadata.parent.as_deref(), Some("/docs"));
    assert_eq!(metadata.weight, Some(10));
    assert_eq!(metadata.kind.as_deref(), Some("blog"));
    assert_eq!(metadata.date.as_deref(), Some("2026-08-15"));
    assert_eq!(metadata.aliases, vec!["/start", "/home"]);
    assert_eq!(
        metadata.summary.as_deref(),
        Some("A concise release summary.")
    );
    assert_eq!(metadata.authors, vec!["Anatoly", "Terlan team"]);
    assert_eq!(metadata.tags, vec!["release", "compiler"]);
    assert!(metadata.draft);
}

#[test]
fn extracts_multiline_page_aliases() {
    let metadata = extract_page_metadata(
        "@page {\n  aliases = [\n    \"/old-guide\",\n    \"/guide/start\"\n  ]\n}\n\n# Guide",
        "content/guide.terl.md",
    )
    .expect("extract aliases");

    assert_eq!(metadata.aliases, vec!["/old-guide", "/guide/start"]);
}

#[test]
fn rejects_non_array_page_aliases() {
    let diagnostics = extract_page_metadata(
        "@page { aliases = \"/old-guide\" }\n\n# Guide",
        "content/guide.terl.md",
    )
    .expect_err("non-array aliases should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan @page key `aliases` must be an array of string literals"
    );
}

#[test]
fn rejects_duplicate_page_string_array_values() {
    let diagnostics = extract_page_metadata(
        "@page { tags = [\"compiler\", \"compiler\"] }\n\n# Guide",
        "content/guide.terl.md",
    )
    .expect_err("duplicate tag should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan @page key `tags` contains duplicate value `compiler`"
    );
}

#[test]
fn rejects_non_boolean_page_draft() {
    let diagnostics = extract_page_metadata(
        "@page { draft = \"true\" }\n\n# Guide",
        "content/guide.terl.md",
    )
    .expect_err("string draft should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan @page key `draft` must be a boolean literal"
    );
}

#[test]
fn rejects_non_integer_page_weight() {
    let diagnostics = extract_page_metadata(
        "@page { weight = \"first\" }\n\n# Home",
        "content/index.terl.md",
    )
    .expect_err("non-integer weight should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan @page key `weight` must be a signed 32-bit integer"
    );
}

/// Rejects unknown `@page` metadata keys.
///
/// Inputs:
/// - A `.terl.md` source with misspelled `titel` metadata.
///
/// Output:
/// - Test passes when metadata extraction reports a stable schema diagnostic.
///
/// Transformation:
/// - Prevents page metadata typos from silently falling back to inferred static
///   route/title behavior.
#[test]
fn rejects_unknown_page_metadata_key() {
    let diagnostics = extract_page_metadata(
        "@page { titel = \"Home\" }\n\n# Home",
        "content/index.terl.md",
    )
    .expect_err("unknown page key should fail");

    assert_eq!(diagnostics[0].message, "unknown Terlan @page key `titel`");
}

/// Rejects duplicate `@page` metadata keys.
///
/// Inputs:
/// - A `.terl.md` source with two `title` entries.
///
/// Output:
/// - Test passes when metadata extraction reports a stable duplicate-key
///   diagnostic.
///
/// Transformation:
/// - Prevents later page metadata values from silently overwriting earlier
///   values in the same built-in annotation block.
#[test]
fn rejects_duplicate_page_metadata_key() {
    let diagnostics = extract_page_metadata(
        "@page { title = \"Home\", title = \"Index\" }\n\n# Home",
        "content/index.terl.md",
    )
    .expect_err("duplicate page key should fail");

    assert_eq!(diagnostics[0].message, "duplicate Terlan @page key `title`");
}

/// Extracts multiline `@template` metadata.
///
/// Inputs:
/// - A `.terl.html` template header with `name` and a multiline `params`
///   object.
///
/// Output:
/// - Test passes when template name and parameter order are preserved.
///
/// Transformation:
/// - Verifies the annotation-backed template signature surface before
///   generated template functions consume it.
#[test]
fn extracts_multiline_template_metadata() {
    let metadata = extract_template_metadata(
        "@template {\n  name = \"Layout\"\n  params = {\n    title: String\n    body: Template.Html\n  }\n}\n\n<main>${body}</main>",
        "templates/layout.terl.html",
    )
    .expect("extract template metadata");

    assert_eq!(metadata.name.as_deref(), Some("Layout"));
    assert_eq!(
        metadata.params,
        vec![
            TemplateParamMetadata {
                name: "title".to_string(),
                type_text: "String".to_string(),
            },
            TemplateParamMetadata {
                name: "body".to_string(),
                type_text: "Template.Html".to_string(),
            },
        ]
    );
}

/// Extracts compact `@template.params` metadata.
///
/// Inputs:
/// - A `.terl.html` template header with comma-separated params.
///
/// Output:
/// - Test passes when compact params are parsed like multiline params.
///
/// Transformation:
/// - Keeps one-line generated or hand-authored template signatures equivalent
///   to block-style metadata.
#[test]
fn extracts_compact_template_metadata() {
    let metadata = extract_template_metadata(
        "@template { params = { title: String, items: List[String] } }\n\n<main></main>",
        "templates/list.terl.html",
    )
    .expect("extract compact template metadata");

    assert_eq!(metadata.name, None);
    assert_eq!(
        metadata.params,
        vec![
            TemplateParamMetadata {
                name: "title".to_string(),
                type_text: "String".to_string(),
            },
            TemplateParamMetadata {
                name: "items".to_string(),
                type_text: "List[String]".to_string(),
            },
        ]
    );
}

/// Extracts an explicit empty `@template.params` signature.
///
/// Inputs:
/// - A `.terl.html` template header with an empty `params` object.
///
/// Output:
/// - Test passes when the template is accepted with no parameters while still
///   recording that its signature was declared.
///
/// Transformation:
/// - Verifies zero-argument templates use the same explicit signature contract
///   as parameterized templates instead of relying on hidden caller context.
#[test]
fn extracts_explicit_empty_template_params() {
    let metadata = extract_template_metadata(
        "@template { params = {} }\n\n<main></main>",
        "templates/empty.terl.html",
    )
    .expect("extract empty template metadata");

    assert!(metadata.params_declared);
    assert!(metadata.params.is_empty());
}

/// Rejects `@template` metadata with no `params` declaration.
///
/// Inputs:
/// - A `.terl.html` template header that names a template but omits its
///   signature object.
///
/// Output:
/// - Test passes when metadata extraction reports a stable required-key
///   diagnostic.
///
/// Transformation:
/// - Prevents reusable template declarations from entering later codegen with
///   hidden or inferred parameter signatures.
#[test]
fn rejects_template_metadata_without_params() {
    let diagnostics = extract_template_metadata(
        "@template { name = \"Card\" }\n\n<main></main>",
        "templates/card.terl.html",
    )
    .expect_err("template params should be required");

    assert_eq!(
        diagnostics[0].message,
        "Terlan @template annotation requires `params`"
    );
}

/// Rejects a non-object `@template.params` value.
///
/// Inputs:
/// - A `.terl.html` template header whose `params` key is not an object.
///
/// Output:
/// - Test passes when metadata extraction reports a stable diagnostic.
///
/// Transformation:
/// - Prevents generated template signatures from accepting ambiguous metadata
///   forms.
#[test]
fn rejects_non_object_template_params() {
    let diagnostics = extract_template_metadata(
        "@template { params = \"title\" }\n\n<main></main>",
        "templates/page.terl.html",
    )
    .expect_err("non-object params should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan @template key `params` must be an object"
    );
}

/// Rejects unknown `@template` metadata keys.
///
/// Inputs:
/// - A `.terl.html` template header with an unsupported `slot` key.
///
/// Output:
/// - Test passes when metadata extraction reports a stable schema diagnostic.
///
/// Transformation:
/// - Keeps built-in template metadata schema-checked before generated template
///   declarations consume it.
#[test]
fn rejects_unknown_template_metadata_key() {
    let diagnostics = extract_template_metadata(
        "@template { slot = \"main\" }\n\n<main></main>",
        "templates/page.terl.html",
    )
    .expect_err("unknown template key should fail");

    assert_eq!(
        diagnostics[0].message,
        "unknown Terlan @template key `slot`"
    );
}

/// Rejects duplicate `@template` metadata keys.
///
/// Inputs:
/// - A `.terl.html` template header with two `name` entries.
///
/// Output:
/// - Test passes when metadata extraction reports a stable duplicate-key
///   diagnostic.
///
/// Transformation:
/// - Prevents later template metadata values from silently overwriting earlier
///   generated-template names.
#[test]
fn rejects_duplicate_template_metadata_key() {
    let diagnostics = extract_template_metadata(
        "@template { name = \"Card\", name = \"Panel\" }\n\n<main></main>",
        "templates/page.terl.html",
    )
    .expect_err("duplicate template key should fail");

    assert_eq!(
        diagnostics[0].message,
        "duplicate Terlan @template key `name`"
    );
}

/// Rejects duplicate template parameters.
///
/// Inputs:
/// - A `.terl.html` template header whose params object repeats a key.
///
/// Output:
/// - Test passes when duplicate params are rejected before code generation.
///
/// Transformation:
/// - Keeps generated template function signatures deterministic.
#[test]
fn rejects_duplicate_template_params() {
    let diagnostics = extract_template_metadata(
        "@template {\n  params = {\n    title: String\n    title: String\n  }\n}\n\n<main></main>",
        "templates/page.terl.html",
    )
    .expect_err("duplicate params should fail");

    assert_eq!(
        diagnostics[0].message,
        "duplicate Terlan @template param `title`"
    );
}

/// Rejects reserved template parameters.
///
/// Inputs:
/// - A `.terl.html` template header that declares `children` explicitly.
///
/// Output:
/// - Test passes when the reserved child-content prop is rejected.
///
/// Transformation:
/// - Aligns annotation-backed signatures with source template declaration
///   validation.
#[test]
fn rejects_reserved_template_param() {
    let diagnostics = extract_template_metadata(
        "@template { params = { children: Template.Html } }\n\n<main></main>",
        "templates/page.terl.html",
    )
    .expect_err("reserved children should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan @template param `children` is reserved"
    );
}
