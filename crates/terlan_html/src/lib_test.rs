use super::*;

#[test]
fn detects_terlan_template_paths() {
    assert!(is_terlan_template_path("templates/user_card.terl.html"));
    assert!(is_terlan_template_path("templates/user_card.terl.md"));
    assert!(!is_terlan_template_path("templates/user_card.html"));
    assert!(!is_terlan_template_path("templates/user_card.md"));
}

#[test]
fn derives_template_tag_from_underscore_filename() {
    let tag = template_tag_from_path("templates/user_card.terl.html").unwrap();
    assert_eq!(tag, "user-card");
}

#[test]
fn derives_template_tag_from_markdown_template_filename() {
    let tag = template_tag_from_path("templates/welcome_content.terl.md").unwrap();
    assert_eq!(tag, "welcome-content");
}

#[test]
fn derives_template_tag_from_kebab_filename() {
    let tag = template_tag_from_path("templates/main-layout.terl.html").unwrap();
    assert_eq!(tag, "main-layout");
}

#[test]
fn rejects_plain_html_as_template_path() {
    let diagnostic = template_tag_from_path("templates/user_card.html").unwrap_err();
    assert!(diagnostic
        .message
        .contains("template filename must end with `.terl.html` or `.terl.md`"));
}

#[test]
fn rejects_invalid_template_filename_characters() {
    let diagnostic = template_tag_from_path("templates/user.card.terl.html").unwrap_err();
    assert!(diagnostic
        .message
        .contains("invalid template filename character"));
}

#[test]
fn builds_template_with_registered_tag() {
    let template =
        HtmlTemplate::from_terlan_template_path("templates/user_card.terl.html", vec![]).unwrap();

    assert_eq!(template.tag_name.as_deref(), Some("user-card"));
}

#[test]
fn parses_static_template_text_and_elements() {
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
fn parses_template_comments_and_doctype() {
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
fn parses_markdown_templates_as_named_html_templates() {
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
fn reports_template_parse_errors_with_path() {
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
fn parses_text_interpolation_slots() {
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
fn parses_attribute_interpolation_slots() {
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
fn rejects_invalid_interpolation_syntax() {
    let diagnostics =
        parse_html_template("<p>Hello {}</p>", "templates/bad_slot.terl.html").unwrap_err();

    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("template interpolation slot cannot be empty")));
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
