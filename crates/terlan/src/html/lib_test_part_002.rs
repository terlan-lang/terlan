
/// Allows compact page metadata strings containing braces.
///
/// Inputs:
/// - A `.terl.md` template whose compact `@page` title contains brace
///   characters inside a quoted string.
///
/// Output:
/// - Test passes when parsing consumes the header and renders the Markdown
///   body normally.
///
/// Transformation:
/// - Ensures header brace balancing ignores quoted string contents.
#[test]
fn parses_compact_markdown_page_header_string_braces() {
    let template = parse_markdown_template(
        "@page { title = \"{Home}\" }\n\n# Body\n",
        "templates/body.terl.md",
    )
    .expect("quoted braces should not affect compact header parsing");

    let HtmlNode::Element(heading) = &template.nodes[0] else {
        panic!("expected heading element");
    };
    assert_eq!(heading.name, "h1");
}

/// Allows multiline page metadata strings containing closing braces.
///
/// Inputs:
/// - A `.terl.md` template whose multiline `@page` title contains `}` inside a
///   quoted string.
///
/// Output:
/// - Test passes when the annotation does not close early.
///
/// Transformation:
/// - Locks annotation-block scanning to structural braces outside strings.
#[test]
fn parses_multiline_markdown_page_header_string_braces() {
    let template = parse_markdown_template(
        "@page {\n  title = \"}\"\n}\n\n# Body\n",
        "templates/body.terl.md",
    )
    .expect("quoted closing brace should not close header early");

    let HtmlNode::Element(heading) = &template.nodes[0] else {
        panic!("expected heading element");
    };
    assert_eq!(heading.name, "h1");
}

/// Extracts multiline page metadata.
///
/// Inputs:
/// - A `.terl.md` source with `title`, `route`, and `layout` in a multiline
///   `@page` header.
///
/// Output:
/// - Test passes when all page metadata fields are extracted.
///
/// Transformation:
/// - Provides the static-site route discovery layer with typed metadata instead
///   of raw annotation source.
#[test]
fn extracts_multiline_page_metadata() {
    let metadata = extract_page_metadata(
        "@page {\n  title = \"Install\"\n  route = \"/install\"\n  layout = \"docs\"\n}\n\n# Body\n",
        "templates/body.terl.md",
    )
    .expect("extract page metadata");

    assert_eq!(metadata.title.as_deref(), Some("Install"));
    assert_eq!(metadata.route.as_deref(), Some("/install"));
    assert_eq!(metadata.layout.as_deref(), Some("docs"));
}

/// Extracts compact page metadata.
///
/// Inputs:
/// - A `.terl.md` source with compact one-line `@page` metadata.
///
/// Output:
/// - Test passes when comma-separated metadata fields are extracted.
///
/// Transformation:
/// - Reuses the compact schema scanner for static-site metadata extraction.
#[test]
fn extracts_compact_page_metadata() {
    let metadata = extract_page_metadata(
        "@page { title = \"Install\", route = \"/install\" }\n\n# Body\n",
        "templates/body.terl.md",
    )
    .expect("extract compact page metadata");

    assert_eq!(metadata.title.as_deref(), Some("Install"));
    assert_eq!(metadata.route.as_deref(), Some("/install"));
    assert_eq!(metadata.layout, None);
}

/// Extracts escaped page metadata string values.
///
/// Inputs:
/// - A `.terl.md` source whose `@page.title` contains escaped quotes.
///
/// Output:
/// - Test passes when the escaped value is unescaped in metadata.
///
/// Transformation:
/// - Keeps metadata extraction useful for human-facing titles without exposing
///   arbitrary expression evaluation in annotations.
#[test]
fn extracts_escaped_page_metadata_string() {
    let metadata = extract_page_metadata(
        "@page { title = \"Install \\\"Terlan\\\"\" }\n\n# Body\n",
        "templates/body.terl.md",
    )
    .expect("extract escaped page metadata");

    assert_eq!(metadata.title.as_deref(), Some("Install \"Terlan\""));
}

/// Rejects non-string page metadata values.
///
/// Inputs:
/// - A `.terl.md` source whose `@page.route` is not a string literal.
///
/// Output:
/// - Test passes when extraction reports a stable type diagnostic.
///
/// Transformation:
/// - Prevents static route discovery from accepting untyped annotation values.
#[test]
fn rejects_non_string_page_metadata_value() {
    let diagnostics =
        extract_page_metadata("@page { route = 42 }\n\n# Body\n", "templates/body.terl.md")
            .expect_err("non-string route should fail");

    assert_eq!(
        diagnostics[0].message,
        "Terlan @page key `route` must be a string literal"
    );
}

/// Allows indented literal text that resembles header syntax.
///
/// Inputs:
/// - A `.terl.md` template with indented lines after body content.
///
/// Output:
/// - Test passes when the indented lines render as Markdown code text.
///
/// Transformation:
/// - Keeps code-block-like Markdown content legal while rejecting only
///   top-level late header syntax.
#[test]
fn parses_markdown_template_indented_header_like_body_text() {
    let template = parse_markdown_template(
        "# Body\n\n    import docs.Version.\n",
        "templates/welcome_content.terl.md",
    )
    .expect("indented header-looking text should be body content");

    assert!(template
        .nodes
        .iter()
        .any(|node| matches!(node, HtmlNode::Element(element) if element.name == "pre")));
}

#[test]
fn rejects_invalid_interpolation_syntax() {
    let diagnostics =
        parse_html_template("<p>Hello {}</p>", "templates/bad_slot.terl.html").unwrap_err();

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("template interpolation slot cannot be empty")
    }));
}

/// Verifies oversized unterminated template slots fail deterministically.
///
/// Inputs:
/// - HTML source containing a large `${...` interpolation without a closing
///   brace.
///
/// Output:
/// - Test passes when the template parser returns the slot diagnostic instead
///   of treating the remainder as valid static text.
///
/// Transformation:
/// - Exercises the template-slot scanner against hostile generated input while
///   keeping the fixture deterministic.
#[test]
fn adversarial_template_slot_rejects_oversized_unterminated_interpolation() {
    let slot_body = "user.profile.".repeat(1024);
    let source = ["<p>${", &slot_body, "</p>"].concat();

    let diagnostics = parse_html_template(source, "templates/bad_slot.terl.html").unwrap_err();

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("unterminated template interpolation slot")
    }));
}

#[test]
fn does_not_parse_interpolation_inside_script_or_style_text() {
    let template = parse_html_template(
        "<script>let value = {raw};</script><style>.x { color: red; }</style>",
        "templates/raw_text.terl.html",
    )
    .unwrap();

    assert_eq!(
        template.nodes,
        vec![
            HtmlNode::Element(HtmlElement {
                name: "script".to_owned(),
                attrs: vec![],
                children: vec![HtmlNode::Text("let value = {raw};".to_owned())],
            }),
            HtmlNode::Element(HtmlElement {
                name: "style".to_owned(),
                attrs: vec![],
                children: vec![HtmlNode::Text(".x { color: red; }".to_owned())],
            }),
        ]
    );
}

#[test]
fn validates_css_sources() {
    validate_css(
        "body { color: red; }\n.card { display: block; }",
        "styles/page.css",
    )
    .expect("valid css");
}

#[test]
fn reports_css_parse_errors() {
    let diagnostics = validate_css("body { color: '\n'; }", "styles/bad.css").unwrap_err();

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("CSS parse error")));
    assert!(diagnostics
        .iter()
        .all(|diagnostic| diagnostic.path.as_deref() == Some(Path::new("styles/bad.css"))));
}

#[test]
fn validates_html_output_without_template_slots() {
    validate_html_output("<main>{literal}</main>", "public/page.html").expect("valid html");
}

#[test]
fn validates_html_output_with_standard_void_elements() {
    validate_html_output(
        "<head><base href=\"/docs/\"><meta charset=\"utf-8\"><link rel=\"stylesheet\" href=\"/app.css\"></head>",
        "public/index.html",
    )
    .expect("valid html with void tags");
}

#[test]
fn reports_html_output_validation_errors() {
    let diagnostics = validate_html_output("<main></section>", "public/bad.html").unwrap_err();

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("mismatched closing tag")));
    assert!(diagnostics
        .iter()
        .all(|diagnostic| diagnostic.path.as_deref() == Some(Path::new("public/bad.html"))));
}

#[test]
fn renders_markdown_to_valid_html_nodes() {
    let document = parse_markdown("# Hello\n\n- one\n- two\n", "posts/hello.md").unwrap();

    assert_eq!(document.raw_source, "# Hello\n\n- one\n- two\n");
    assert_eq!(
        document.rendered_html,
        "<h1>Hello</h1>\n<ul>\n<li>one</li>\n<li>two</li>\n</ul>\n"
    );
    assert_eq!(
        document.nodes,
        vec![
            HtmlNode::Element(HtmlElement {
                name: "h1".to_owned(),
                attrs: vec![],
                children: vec![HtmlNode::Text("Hello".to_owned())],
            }),
            HtmlNode::Text("\n".to_owned()),
            HtmlNode::Element(HtmlElement {
                name: "ul".to_owned(),
                attrs: vec![],
                children: vec![
                    HtmlNode::Text("\n".to_owned()),
                    HtmlNode::Element(HtmlElement {
                        name: "li".to_owned(),
                        attrs: vec![],
                        children: vec![HtmlNode::Text("one".to_owned())],
                    }),
                    HtmlNode::Text("\n".to_owned()),
                    HtmlNode::Element(HtmlElement {
                        name: "li".to_owned(),
                        attrs: vec![],
                        children: vec![HtmlNode::Text("two".to_owned())],
                    }),
                    HtmlNode::Text("\n".to_owned()),
                ],
            }),
            HtmlNode::Text("\n".to_owned()),
        ]
    );
}

/// Strips Terlan imports and annotations before Markdown document rendering.
///
/// Inputs:
/// - A `.terl.md` document with a Terlan header and Markdown body.
///
/// Output:
/// - Test passes when `raw_source` and rendered HTML contain only the body.
///
/// Transformation:
/// - Applies the same header stripping used by static Markdown imports before
///   the Markdown renderer runs.
#[test]
fn renders_terlan_markdown_document_after_header() {
    let document = parse_markdown(
        "import docs.Version.\n\n@page {\n  title = \"Welcome\"\n}\n\n# Welcome\n",
        "posts/welcome.terl.md",
    )
    .unwrap();

    assert_eq!(document.raw_source, "# Welcome\n");
    assert_eq!(document.rendered_html, "<h1>Welcome</h1>\n");
}

/// Keeps ordinary Markdown files unchanged.
///
/// Inputs:
/// - A `.md` file whose first line happens to look like a Terlan import.
///
/// Output:
/// - Test passes when non-`.terl.md` Markdown renders the text literally.
///
/// Transformation:
/// - Restricts Terlan header stripping to canonical Terlan Markdown templates
///   and content files.
#[test]
fn renders_plain_markdown_without_terlan_header_stripping() {
    let document = parse_markdown("import docs.Version.\n\n# Body\n", "posts/plain.md").unwrap();

    assert!(document.raw_source.starts_with("import docs.Version."));
    assert!(document.rendered_html.contains("import docs.Version."));
}

#[test]
fn validates_markdown_rendered_html_with_path() {
    let document = parse_markdown("[safe](javascript:alert(1))", "posts/safe.md").unwrap();

    assert_eq!(
        document.source_path.as_deref(),
        Some(Path::new("posts/safe.md"))
    );
    assert!(!document.rendered_html.contains("javascript:alert"));
    assert!(document
        .nodes
        .iter()
        .any(|node| matches!(node, HtmlNode::Element(element) if element.name == "p")));
}

#[test]
fn validates_markdown_derived_html_output() {
    let document = parse_markdown(
        "# Links\n\n[good](https://example.com)\n\n[bad](javascript:alert(1))\n",
        "posts/links.md",
    )
    .unwrap();

    assert!(document.rendered_html.contains("<h1>Links</h1>"));
    assert!(document.rendered_html.contains("https://example.com"));
    assert!(!document.rendered_html.contains("javascript:alert"));
    assert!(document.nodes.iter().any(|node| {
        matches!(
            node,
            HtmlNode::Element(HtmlElement { name, .. }) if name == "h1"
        )
    }));
    assert!(document.nodes.iter().any(|node| {
        matches!(
            node,
            HtmlNode::Element(HtmlElement { name, .. }) if name == "p"
        )
    }));
}
