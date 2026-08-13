use super::*;

#[test]
pub(super) fn detects_terlan_template_paths() {
    assert!(is_terlan_template_path("templates/user_card.terl.html"));
    assert!(is_terlan_template_path("templates/user_card.terl.md"));
    assert!(!is_terlan_template_path("templates/user_card.terl.json"));
    assert!(!is_terlan_template_path("templates/user_card.html"));
    assert!(!is_terlan_template_path("templates/user_card.md"));
}

#[test]
pub(super) fn derives_template_tag_from_underscore_filename() {
    let tag = template_tag_from_path("templates/user_card.terl.html").unwrap();
    assert_eq!(tag, "user-card");
}

#[test]
pub(super) fn derives_template_tag_from_markdown_template_filename() {
    let tag = template_tag_from_path("templates/welcome_content.terl.md").unwrap();
    assert_eq!(tag, "welcome-content");
}

#[test]
pub(super) fn derives_template_tag_from_kebab_filename() {
    let tag = template_tag_from_path("templates/main-layout.terl.html").unwrap();
    assert_eq!(tag, "main-layout");
}

#[test]
pub(super) fn rejects_plain_html_as_template_path() {
    let diagnostic = template_tag_from_path("templates/user_card.html").unwrap_err();
    assert!(diagnostic
        .message
        .contains("template filename must end with `.terl.html` or `.terl.md`"));
}

#[test]
pub(super) fn rejects_invalid_template_filename_characters() {
    let diagnostic = template_tag_from_path("templates/user.card.terl.html").unwrap_err();
    assert!(diagnostic
        .message
        .contains("invalid template filename character"));
}

#[test]
pub(super) fn builds_template_with_registered_tag() {
    let template =
        HtmlTemplate::from_terlan_template_path("templates/user_card.terl.html", vec![]).unwrap();

    assert_eq!(template.tag_name.as_deref(), Some("user-card"));
}

#[test]
pub(super) fn parses_static_template_text_and_elements() {
    let template = parse_html_template(
        "<article class=\"card\"><h1>Hello</h1><p>World</p></article>",
        "templates/user_card.terl.html",
    )
    .unwrap();

    assert_eq!(template.tag_name.as_deref(), Some("user-card"));
    assert_eq!(
        template.nodes,
        vec![HtmlNode::Element(HtmlElement {
            name: "article".to_owned(),
            attrs: vec![HtmlAttr {
                name: "class".to_owned(),
                value: Some(HtmlAttrValue::Text("card".to_owned())),
            }],
            children: vec![
                HtmlNode::Element(HtmlElement {
                    name: "h1".to_owned(),
                    attrs: vec![],
                    children: vec![HtmlNode::Text("Hello".to_owned())],
                }),
                HtmlNode::Element(HtmlElement {
                    name: "p".to_owned(),
                    attrs: vec![],
                    children: vec![HtmlNode::Text("World".to_owned())],
                }),
            ],
        })]
    );
}

#[test]
pub(super) fn parses_template_comments_and_doctype() {
    let template = parse_html_template(
        "<!doctype html><!-- note --><main></main>",
        "templates/page_shell.terl.html",
    )
    .unwrap();

    assert_eq!(
        template.nodes,
        vec![
            HtmlNode::Doctype("html".to_owned()),
            HtmlNode::Comment(" note ".to_owned()),
            HtmlNode::Element(HtmlElement {
                name: "main".to_owned(),
                attrs: vec![],
                children: vec![],
            }),
        ]
    );
}

#[test]
pub(super) fn parses_markdown_templates_as_named_html_templates() {
    let template = parse_template(
        "# Hello {name}\n\nThis came from **Markdown**.\n",
        "templates/welcome_content.terl.md",
    )
    .unwrap();

    assert_eq!(template.tag_name.as_deref(), Some("welcome-content"));
    assert_eq!(
        template.nodes,
        vec![
            HtmlNode::Element(HtmlElement {
                name: "h1".to_owned(),
                attrs: vec![],
                children: vec![
                    HtmlNode::Text("Hello ".to_owned()),
                    HtmlNode::Slot(HtmlSlot {
                        expression: "name".to_owned(),
                        path: vec!["name".to_owned()],
                        span: Some(HtmlSpan {
                            line: 1,
                            start: 6,
                            end: 12,
                        }),
                    }),
                ],
            }),
            HtmlNode::Text("\n".to_owned()),
            HtmlNode::Element(HtmlElement {
                name: "p".to_owned(),
                attrs: vec![],
                children: vec![
                    HtmlNode::Text("This came from ".to_owned()),
                    HtmlNode::Element(HtmlElement {
                        name: "strong".to_owned(),
                        attrs: vec![],
                        children: vec![HtmlNode::Text("Markdown".to_owned())],
                    }),
                    HtmlNode::Text(".".to_owned()),
                ],
            }),
            HtmlNode::Text("\n".to_owned()),
        ]
    );
}

#[test]
pub(super) fn reports_template_parse_errors_with_path() {
    let diagnostics = parse_html_template(
        "<article><h1>Broken</article>",
        "templates/bad_card.terl.html",
    )
    .unwrap_err();

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("mismatched closing tag")));
    assert!(diagnostics
        .iter()
        .all(|diagnostic| diagnostic.path.as_deref()
            == Some(Path::new("templates/bad_card.terl.html"))));
}

#[test]
pub(super) fn parses_text_interpolation_slots() {
    let template = parse_html_template(
        "<p>Hello {user.name}</p>",
        "templates/user_greeting.terl.html",
    )
    .unwrap();

    assert_eq!(
        template.nodes,
        vec![HtmlNode::Element(HtmlElement {
            name: "p".to_owned(),
            attrs: vec![],
            children: vec![
                HtmlNode::Text("Hello ".to_owned()),
                HtmlNode::Slot(HtmlSlot {
                    expression: "user.name".to_owned(),
                    path: vec!["user".to_owned(), "name".to_owned()],
                    span: Some(HtmlSpan {
                        line: 1,
                        start: 6,
                        end: 17,
                    }),
                }),
            ],
        })]
    );
}

#[test]
pub(super) fn parses_dollar_text_interpolation_slots() {
    let template = parse_html_template(
        "<p>Hello ${user.name}</p>",
        "templates/user_greeting.terl.html",
    )
    .unwrap();

    assert_eq!(
        template.nodes,
        vec![HtmlNode::Element(HtmlElement {
            name: "p".to_owned(),
            attrs: vec![],
            children: vec![
                HtmlNode::Text("Hello ".to_owned()),
                HtmlNode::Slot(HtmlSlot {
                    expression: "user.name".to_owned(),
                    path: vec!["user".to_owned(), "name".to_owned()],
                    span: Some(HtmlSpan {
                        line: 1,
                        start: 6,
                        end: 18,
                    }),
                }),
            ],
        })]
    );
}

/// Verifies template interpolation preserves non-path Terlan expressions.
///
/// Inputs:
/// - A `.terl.html` body containing a receiver-method interpolation.
///
/// Output:
/// - A slot whose expression keeps the method call and whose path metadata is
///   empty because the expression is not a static dotted path.
///
/// Transformation:
/// - Exercises the parser-side split between expression preservation and
///   static path metadata used by older renderers.
#[test]
pub(super) fn parses_dollar_text_interpolation_expression_slots() {
    let template = parse_html_template(
        "<p>Hello ${count.to_string()}</p>",
        "templates/user_greeting.terl.html",
    )
    .unwrap();

    assert_eq!(
        template.nodes,
        vec![HtmlNode::Element(HtmlElement {
            name: "p".to_owned(),
            attrs: vec![],
            children: vec![
                HtmlNode::Text("Hello ".to_owned()),
                HtmlNode::Slot(HtmlSlot {
                    expression: "count.to_string()".to_owned(),
                    path: vec![],
                    span: Some(HtmlSpan {
                        line: 1,
                        start: 6,
                        end: 26,
                    }),
                }),
            ],
        })]
    );
}

#[test]
pub(super) fn parses_nested_dollar_interpolation_expression_slots() {
    let template = parse_html_template(
        r#"<p>${render(Map { body = "}" })}</p>"#,
        "templates/nested_expression.terl.html",
    )
    .unwrap();

    let HtmlNode::Element(element) = &template.nodes[0] else {
        panic!("expected root element");
    };
    let HtmlNode::Slot(slot) = &element.children[0] else {
        panic!("expected nested expression slot");
    };
    assert_eq!(slot.expression, r#"render(Map { body = "}" })"#);
}

#[test]
pub(super) fn parses_attribute_interpolation_slots() {
    let template = parse_html_template(
        "<a href=\"{url}\">Link</a>",
        "templates/link_card.terl.html",
    )
    .unwrap();

    assert_eq!(
        template.nodes,
        vec![HtmlNode::Element(HtmlElement {
            name: "a".to_owned(),
            attrs: vec![HtmlAttr {
                name: "href".to_owned(),
                value: Some(HtmlAttrValue::Slot(HtmlSlot {
                    expression: "url".to_owned(),
                    path: vec!["url".to_owned()],
                    span: Some(HtmlSpan {
                        line: 1,
                        start: 0,
                        end: 5,
                    }),
                })),
            }],
            children: vec![HtmlNode::Text("Link".to_owned())],
        })]
    );
}

#[test]
pub(super) fn parses_dollar_attribute_interpolation_slots() {
    let template = parse_html_template(
        "<a href=\"${url}\">Link</a>",
        "templates/link_card.terl.html",
    )
    .unwrap();

    assert_eq!(
        template.nodes,
        vec![HtmlNode::Element(HtmlElement {
            name: "a".to_owned(),
            attrs: vec![HtmlAttr {
                name: "href".to_owned(),
                value: Some(HtmlAttrValue::Slot(HtmlSlot {
                    expression: "url".to_owned(),
                    path: vec!["url".to_owned()],
                    span: Some(HtmlSpan {
                        line: 1,
                        start: 0,
                        end: 6,
                    }),
                })),
            }],
            children: vec![HtmlNode::Text("Link".to_owned())],
        })]
    );
}

/// Strips Terlan imports and annotations before HTML template parsing.
///
/// Inputs:
/// - A `.terl.html` template with a leading import and `@template` metadata
///   block.
///
/// Output:
/// - Test passes when only the HTML body is parsed into template nodes.
///
/// Transformation:
/// - Exercises the shared Terlan template header rule for HTML targets.
#[test]
pub(super) fn parses_html_template_after_terlan_header() {
    let template = parse_html_template(
        "import std.core.String.\n\n@template {\n  params = {\n    title: String\n  }\n}\n\n<main>${title}</main>",
        "templates/page.terl.html",
    )
    .unwrap();

    assert_eq!(
        template.nodes,
        vec![HtmlNode::Element(HtmlElement {
            name: "main".to_owned(),
            attrs: vec![],
            children: vec![HtmlNode::Slot(HtmlSlot {
                expression: "title".to_owned(),
                path: vec!["title".to_owned()],
                span: Some(HtmlSpan {
                    line: 1,
                    start: 0,
                    end: 8,
                }),
            })],
        })]
    );
}

/// Rejects imports after HTML body content.
///
/// Inputs:
/// - A `.terl.html` template with HTML before an import line.
///
/// Output:
/// - Test passes when parsing reports the late-header diagnostic.
///
/// Transformation:
/// - Locks Terlan imports to the leading template header region for HTML files.
#[test]
pub(super) fn rejects_html_template_import_after_body_content() {
    let diagnostics = parse_html_template(
        "<main></main>\nimport std.core.String.\n",
        "templates/page.terl.html",
    )
    .expect_err("late import should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan template imports and annotations must appear before body content"
    );
}

/// Rejects malformed imports in HTML template headers.
///
/// Inputs:
/// - A `.terl.html` template whose leading import is missing the terminating
///   dot.
///
/// Output:
/// - Test passes when parsing reports the import terminator diagnostic.
///
/// Transformation:
/// - Prevents malformed Terlan header syntax from being stripped silently.
#[test]
pub(super) fn rejects_html_template_header_import_without_dot() {
    let diagnostics = parse_html_template(
        "import std.core.String\n\n<main></main>",
        "templates/page.terl.html",
    )
    .expect_err("malformed import should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan template header import must end with `.`"
    );
}

/// Rejects annotations after HTML body content.
///
/// Inputs:
/// - A `.terl.html` template with HTML before an annotation block.
///
/// Output:
/// - Test passes when parsing reports the late-header diagnostic.
///
/// Transformation:
/// - Locks Terlan annotations to the leading template header region for HTML
///   files.
#[test]
pub(super) fn rejects_html_template_annotation_after_body_content() {
    let diagnostics = parse_html_template(
        "<main></main>\n@template {\n  params = {}\n}\n",
        "templates/page.terl.html",
    )
    .expect_err("late annotation should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan template imports and annotations must appear before body content"
    );
}

#[test]
pub(super) fn parses_dollar_markdown_interpolation_slots() {
    let template = parse_markdown_template(
        "# Hello ${name}\n\nThis came from **Markdown**.\n",
        "templates/welcome_content.terl.md",
    )
    .unwrap();

    let HtmlNode::Element(heading) = &template.nodes[0] else {
        panic!("expected heading element");
    };
    assert_eq!(heading.name, "h1");
    assert_eq!(
        heading.children,
        vec![
            HtmlNode::Text("Hello ".to_owned()),
            HtmlNode::Slot(HtmlSlot {
                expression: "name".to_owned(),
                path: vec!["name".to_owned()],
                span: Some(HtmlSpan {
                    line: 1,
                    start: 6,
                    end: 13,
                }),
            }),
        ]
    );
}

/// Rejects unknown annotations in HTML template headers.
///
/// Inputs:
/// - A `.terl.html` template whose leading header uses an unsupported
///   annotation name.
///
/// Output:
/// - Test passes when parsing reports a stable unknown-annotation diagnostic.
///
/// Transformation:
/// - Keeps the first template-header annotation surface limited to built-in
///   `@template` and `@page` metadata.
#[test]
pub(super) fn rejects_unknown_html_template_header_annotation() {
    let diagnostics =
        parse_html_template("@unknown {}\n\n<main></main>", "templates/page.terl.html")
            .expect_err("unknown annotation should fail");

    assert_eq!(
        diagnostics[0].message,
        "unknown Terlan template annotation `@unknown`"
    );
}

/// Rejects unknown top-level keys in HTML template metadata.
///
/// Inputs:
/// - A `.terl.html` template whose `@template` header has an unsupported
///   top-level key.
///
/// Output:
/// - Test passes when parsing reports a stable unknown-key diagnostic.
///
/// Transformation:
/// - Locks the first built-in `@template` schema to `name` and `params` while
///   custom annotation schemas are deferred.
#[test]
pub(super) fn rejects_unknown_html_template_header_key() {
    let diagnostics = parse_html_template(
        "@template {\n  props = {}\n}\n\n<main></main>",
        "templates/page.terl.html",
    )
    .expect_err("unknown template key should fail");

    assert_eq!(
        diagnostics[0].message,
        "unknown Terlan @template key `props`"
    );
}

/// Rejects duplicate top-level keys in HTML template metadata.
///
/// Inputs:
/// - A `.terl.html` template whose `@template` header repeats `params`.
///
/// Output:
/// - Test passes when parsing reports a stable duplicate-key diagnostic.
///
/// Transformation:
/// - Prevents generated template metadata from depending on ambiguous
///   first-write or last-write behavior.
#[test]
pub(super) fn rejects_duplicate_html_template_header_key() {
    let diagnostics = parse_html_template(
        "@template {\n  params = {}\n  params = {}\n}\n\n<main></main>",
        "templates/page.terl.html",
    )
    .expect_err("duplicate template key should fail");

    assert_eq!(
        diagnostics[0].message,
        "duplicate Terlan @template key `params`"
    );
}

/// Allows nested template parameter keys inside `params`.
///
/// Inputs:
/// - A `.terl.html` template whose `@template` header declares nested params.
///
/// Output:
/// - Test passes when nested parameter entries are not treated as top-level
///   annotation keys.
///
/// Transformation:
/// - Keeps the first header schema shallow so `params` can carry typed Terlan
///   parameter metadata before full annotation value parsing lands.
#[test]
pub(super) fn parses_html_template_header_nested_params() {
    let template = parse_html_template(
        "@template {\n  params = {\n    title: String\n  }\n}\n\n<main>${title}</main>",
        "templates/page.terl.html",
    )
    .expect("nested params should be allowed");

    assert_eq!(template.tag_name.as_deref(), Some("page"));
}

/// Strips Terlan imports and annotations before Markdown template rendering.
///
/// Inputs:
/// - A `.terl.md` template with a leading import and `@page` metadata block.
///
/// Output:
/// - Test passes when only the Markdown body is rendered into HTML nodes.
///
/// Transformation:
/// - Exercises the template frontend rule that Terlan headers configure
///   template files but are not Markdown body content.
#[test]
pub(super) fn parses_markdown_template_after_terlan_header() {
    let template = parse_markdown_template(
        "import docs.Version.\n\n@page {\n  title = \"Welcome\"\n}\n\n# Hello ${name}\n",
        "templates/welcome_content.terl.md",
    )
    .unwrap();

    let HtmlNode::Element(heading) = &template.nodes[0] else {
        panic!("expected heading element");
    };
    assert_eq!(heading.name, "h1");
    assert_eq!(
        heading.children,
        vec![
            HtmlNode::Text("Hello ".to_owned()),
            HtmlNode::Slot(HtmlSlot {
                expression: "name".to_owned(),
                path: vec!["name".to_owned()],
                span: Some(HtmlSpan {
                    line: 1,
                    start: 6,
                    end: 13,
                }),
            }),
        ]
    );
}

/// Rejects unterminated Terlan annotation headers in Markdown templates.
///
/// Inputs:
/// - A `.terl.md` template whose `@page` block never closes.
///
/// Output:
/// - Test passes when parsing returns a stable header diagnostic.
///
/// Transformation:
/// - Prevents malformed Terlan metadata from being rendered as ordinary
///   Markdown text.
#[test]
pub(super) fn rejects_unterminated_markdown_template_header_annotation() {
    let diagnostics = parse_markdown_template(
        "@page {\n  title = \"Welcome\"\n\n# Body\n",
        "templates/welcome_content.terl.md",
    )
    .expect_err("unterminated annotation header should fail");

    assert_eq!(
        diagnostics[0].message,
        "unterminated Terlan template annotation header"
    );
}

/// Rejects imports after Markdown body content.
///
/// Inputs:
/// - A `.terl.md` template with body text before an import line.
///
/// Output:
/// - Test passes when parsing reports the late-header diagnostic.
///
/// Transformation:
/// - Locks Terlan imports to the leading Markdown header region.
#[test]
pub(super) fn rejects_markdown_template_import_after_body_content() {
    let diagnostics = parse_markdown_template(
        "# Body\n\nimport docs.Version.\n",
        "templates/welcome_content.terl.md",
    )
    .expect_err("late import should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan template imports and annotations must appear before body content"
    );
}

/// Rejects malformed imports in Markdown template headers.
///
/// Inputs:
/// - A `.terl.md` template whose leading import is missing the terminating dot.
///
/// Output:
/// - Test passes when parsing reports the import terminator diagnostic.
///
/// Transformation:
/// - Applies the same Terlan header import rule to Markdown templates.
#[test]
pub(super) fn rejects_markdown_template_header_import_without_dot() {
    let diagnostics = parse_markdown_template(
        "import docs.Version\n\n# Body\n",
        "templates/welcome_content.terl.md",
    )
    .expect_err("malformed import should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan template header import must end with `.`"
    );
}

/// Rejects annotations after Markdown body content.
///
/// Inputs:
/// - A `.terl.md` template with body text before an annotation block.
///
/// Output:
/// - Test passes when parsing reports the late-header diagnostic.
///
/// Transformation:
/// - Locks Terlan annotations to the leading Markdown header region.
#[test]
pub(super) fn rejects_markdown_template_annotation_after_body_content() {
    let diagnostics = parse_markdown_template(
        "# Body\n\n@page {\n  title = \"Late\"\n}\n",
        "templates/welcome_content.terl.md",
    )
    .expect_err("late annotation should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan template imports and annotations must appear before body content"
    );
}

/// Rejects nameless annotations in Markdown template headers.
///
/// Inputs:
/// - A `.terl.md` template whose header contains `@ { ... }`.
///
/// Output:
/// - Test passes when parsing reports a missing-name diagnostic.
///
/// Transformation:
/// - Keeps malformed annotation prefixes from being silently stripped before
///   Markdown rendering.
#[test]
pub(super) fn rejects_nameless_markdown_template_header_annotation() {
    let diagnostics = parse_markdown_template(
        "@ { title = \"Body\" }\n\n# Body\n",
        "templates/body.terl.md",
    )
    .expect_err("nameless annotation should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan template annotation is missing a name"
    );
}

/// Rejects unknown top-level keys in Markdown page metadata.
///
/// Inputs:
/// - A `.terl.md` template whose `@page` header has a misspelled key.
///
/// Output:
/// - Test passes when parsing reports a stable unknown-key diagnostic.
///
/// Transformation:
/// - Locks the first built-in `@page` schema to known page metadata keys.
#[test]
pub(super) fn rejects_unknown_markdown_page_header_key() {
    let diagnostics = parse_markdown_template(
        "@page {\n  titel = \"Typo\"\n}\n\n# Body\n",
        "templates/body.terl.md",
    )
    .expect_err("unknown page key should fail");

    assert_eq!(diagnostics[0].message, "unknown Terlan @page key `titel`");
}

/// Rejects unknown keys in compact page metadata.
///
/// Inputs:
/// - A `.terl.md` template whose `@page` header keeps metadata on one line.
///
/// Output:
/// - Test passes when parsing reports the same unknown-key diagnostic as the
///   multiline form.
///
/// Transformation:
/// - Keeps compact annotation headers under the same schema validation path.
#[test]
pub(super) fn rejects_unknown_compact_markdown_page_header_key() {
    let diagnostics = parse_markdown_template(
        "@page { titel = \"Typo\" }\n\n# Body\n",
        "templates/body.terl.md",
    )
    .expect_err("unknown compact page key should fail");

    assert_eq!(diagnostics[0].message, "unknown Terlan @page key `titel`");
}

/// Rejects duplicate top-level keys in Markdown page metadata.
///
/// Inputs:
/// - A `.terl.md` template whose `@page` header repeats `title`.
///
/// Output:
/// - Test passes when parsing reports a stable duplicate-key diagnostic.
///
/// Transformation:
/// - Keeps static-page metadata deterministic before page discovery consumes
///   annotation values.
#[test]
pub(super) fn rejects_duplicate_markdown_page_header_key() {
    let diagnostics = parse_markdown_template(
        "@page {\n  title = \"One\"\n  title = \"Two\"\n}\n\n# Body\n",
        "templates/body.terl.md",
    )
    .expect_err("duplicate page key should fail");

    assert_eq!(diagnostics[0].message, "duplicate Terlan @page key `title`");
}

/// Rejects duplicate keys in compact page metadata.
///
/// Inputs:
/// - A `.terl.md` template whose `@page` header repeats `title` on one line.
///
/// Output:
/// - Test passes when parsing reports the same duplicate-key diagnostic as the
///   multiline form.
///
/// Transformation:
/// - Validates comma-separated compact metadata without adding a separate
///   annotation syntax.
#[test]
pub(super) fn rejects_duplicate_compact_markdown_page_header_key() {
    let diagnostics = parse_markdown_template(
        "@page { title = \"One\", title = \"Two\" }\n\n# Body\n",
        "templates/body.terl.md",
    )
    .expect_err("duplicate compact page key should fail");

    assert_eq!(diagnostics[0].message, "duplicate Terlan @page key `title`");
}
