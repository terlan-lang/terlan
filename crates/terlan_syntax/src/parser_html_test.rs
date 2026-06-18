#[cfg(test)]
mod tests {
    use crate::parse_module;
    use crate::parse_tree::{BuiltinBlockMacro, Decl, Expr, HtmlAttrValue, HtmlNode};

    #[test]
    fn parses_html_block_expressions() {
        let source = r#"
module views.

pub view(): Html[none] ->
    html {
        <main><h1>Hello</h1></main>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        match &function.clauses[0].body {
            Expr::HtmlBlock(html) => {
                assert_eq!(html.macro_kind, BuiltinBlockMacro::Html);
                assert_eq!(html.nodes.len(), 1);
                match &html.nodes[0] {
                    HtmlNode::Element(element) => {
                        assert_eq!(element.name, "main");
                        assert_eq!(element.children.len(), 1);
                        match &element.children[0] {
                            HtmlNode::Element(child) => {
                                assert_eq!(child.name, "h1");
                                match &child.children[0] {
                                    HtmlNode::Text(text) => assert_eq!(text, "Hello"),
                                    _ => panic!("expected child text"),
                                }
                            }
                            _ => panic!("expected nested h1 element"),
                        }
                    }
                    _ => panic!("expected main element"),
                }
            }
            _ => panic!("expected html block"),
        }
    }

    #[test]
    fn parses_html_named_slot_children() {
        let source = r#"
module views.

pub view(): Html[none] ->
    html {
        <page-shell title="Markdown">
            @view1 {
                <welcome-content></welcome-content>
            }
        </page-shell>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        match &function.clauses[0].body {
            Expr::HtmlBlock(html) => match &html.nodes[0] {
                HtmlNode::Element(element) => {
                    assert_eq!(element.name, "page-shell");
                    assert_eq!(element.children.len(), 1);
                    match &element.children[0] {
                        HtmlNode::NamedSlot(slot) => {
                            assert_eq!(slot.name, "view1");
                            assert_eq!(slot.children.len(), 1);
                            match &slot.children[0] {
                                HtmlNode::Element(child) => {
                                    assert_eq!(child.name, "welcome-content")
                                }
                                _ => panic!("expected slot child element"),
                            }
                        }
                        _ => panic!("expected named slot child"),
                    }
                }
                _ => panic!("expected page-shell element"),
            },
            _ => panic!("expected html block"),
        }
    }

    #[test]
    fn parses_html_attributes() {
        let source = r#"
module views.

pub view(Primary: Text, Enabled: Bool): Html[none] ->
    html {
        <button class="primary" disabled={true} id='save'>Save</button>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        match &function.clauses[0].body {
            Expr::HtmlBlock(html) => match &html.nodes[0] {
                HtmlNode::Element(element) => {
                    assert_eq!(element.attrs.len(), 3);
                    assert_eq!(element.name, "button");

                    let crate::parse_tree::HtmlAttr { name, value } = &element.attrs[0];
                    assert_eq!(name, "class");
                    match value.as_ref().expect("value") {
                        HtmlAttrValue::Text(value) => assert_eq!(value, "primary"),
                        _ => panic!("expected text value"),
                    }

                    let crate::parse_tree::HtmlAttr { name, value } = &element.attrs[1];
                    assert_eq!(name, "disabled");
                    match value.as_ref().expect("value") {
                        HtmlAttrValue::Expr(Expr::Var(name)) => assert_eq!(name, "true"),
                        _ => panic!("expected expression value"),
                    }

                    let crate::parse_tree::HtmlAttr { name, value } = &element.attrs[2];
                    assert_eq!(name, "id");
                    match value.as_ref().expect("value") {
                        HtmlAttrValue::Text(value) => assert_eq!(value, "save"),
                        _ => panic!("expected text value"),
                    }
                }
                _ => panic!("expected button element"),
            },
            _ => panic!("expected html block"),
        }
    }

    #[test]
    fn parses_html_interpolation_nodes() {
        let source = r#"
module views.

pub view(Title: Text): Html[none] ->
    html {
        <h1>{Title}</h1>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        match &function.clauses[0].body {
            Expr::HtmlBlock(html) => match &html.nodes[0] {
                HtmlNode::Element(element) => {
                    assert_eq!(element.children.len(), 1);
                    match &element.children[0] {
                        HtmlNode::Expr(Expr::Var(name)) => assert_eq!(name, "Title"),
                        _ => panic!("expected interpolated expression"),
                    }
                }
                _ => panic!("expected h1 element"),
            },
            _ => panic!("expected html block"),
        }
    }

    #[test]
    fn parses_html_case_branch_nodes_in_interpolation() {
        let source = r#"
module views.

pub view(Admin: Bool): Html[none] ->
    html {
        <div>
            {case Admin {
                true -> <span class="admin">Admin</span>;
                false -> <span>Viewer</span>
            }}
        </div>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        let div = match &function.clauses[0].body {
            Expr::HtmlBlock(html) => match &html.nodes[0] {
                HtmlNode::Element(element) => element,
                _ => panic!("expected div element"),
            },
            _ => panic!("expected html block"),
        };
        match &div.children[0] {
            HtmlNode::Expr(Expr::Case { clauses, .. }) => {
                assert_eq!(clauses.len(), 2);
                match &clauses[0].body {
                    Expr::HtmlBlock(html) => match &html.nodes[0] {
                        HtmlNode::Element(element) => assert_eq!(element.name, "span"),
                        _ => panic!("expected span element"),
                    },
                    _ => panic!("expected html branch body"),
                }
            }
            _ => panic!("expected case interpolation"),
        }
    }

    #[test]
    fn parses_html_for_nodes_in_interpolation() {
        let source = r#"
module views.

pub view(Users: List[Text]): Html[none] ->
    html {
        <ul>
            {for User <- Users {
                <li>{User}</li>
            }}
        </ul>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        let ul = match &function.clauses[0].body {
            Expr::HtmlBlock(html) => match &html.nodes[0] {
                HtmlNode::Element(element) => element,
                _ => panic!("expected ul element"),
            },
            _ => panic!("expected html block"),
        };
        match &ul.children[0] {
            HtmlNode::Expr(Expr::ListComprehension { expr, .. }) => match expr.as_ref() {
                Expr::HtmlBlock(html) => match &html.nodes[0] {
                    HtmlNode::Element(element) => assert_eq!(element.name, "li"),
                    _ => panic!("expected li element"),
                },
                _ => panic!("expected html list item"),
            },
            _ => panic!("expected list rendering interpolation"),
        }
    }

    #[test]
    fn parses_nested_html_elements() {
        let source = r#"
module views.

pub view(): Html[none] ->
    html {
        <section>
            <article><p>Nested</p></article>
        </section>
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        let element = match &function.clauses[0].body {
            Expr::HtmlBlock(html) => match &html.nodes[0] {
                HtmlNode::Element(element) => element,
                _ => panic!("expected section element"),
            },
            _ => panic!("expected html block"),
        };
        assert_eq!(element.name, "section");
        assert_eq!(element.children.len(), 1);
        let article = match &element.children[0] {
            HtmlNode::Element(element) => element,
            _ => panic!("expected article element"),
        };
        assert_eq!(article.name, "article");
        assert_eq!(article.children.len(), 1);
    }
}
