#[cfg(test)]
mod tests {
    use crate::parse_module;
    use crate::parse_tree::{BuiltinBlockMacro, Decl, Expr, HtmlAttrValue, HtmlNode};

    /// Verifies release core collection contracts stay parseable.
    ///
    /// Inputs:
    /// - Release source modules for `std.collections.Map`, `std.collections.List`, and
    ///   `std.collections.Set`.
    ///
    /// Output:
    /// - Test passes when all three release modules parse as normal source
    ///   modules and keep their canonical module names.
    ///
    /// Transformation:
    /// - Parses release contracts with compiler intrinsic annotations and
    ///   placeholder bodies without typechecking or backend emission, proving
    ///   the P0.3 release source shape remains grammar-stable.
    #[test]
    fn parses_release_core_collection_contracts() {
        let contracts = [
            (
                "std.collections.Map",
                include_str!("../../../std/collections/map.terl"),
            ),
            (
                "std.collections.List",
                include_str!("../../../std/collections/list.terl"),
            ),
            (
                "std.collections.Set",
                include_str!("../../../std/collections/set.terl"),
            ),
        ];

        for (expected_module, source) in contracts {
            let module = parse_module(source).expect("parse release collection contract");
            assert_eq!(module.name, expected_module);
        }
    }

    /// Verifies release iterator/iterable modules stay parseable.
    ///
    /// Inputs:
    /// - Release source modules for `std.collections.Iterator` and
    ///   `std.collections.Iterable`.
    ///
    /// Output:
    /// - Test passes when both modules parse in source mode and keep their
    ///   canonical module names.
    ///
    /// Transformation:
    /// - Parses release traversal modules without typechecking or backend
    ///   emission, proving P0.4b exposes traversal contracts while allowing
    ///   source-implemented helpers such as `Iterator.each`.
    #[test]
    fn parses_release_traversal_contracts() {
        let contracts = [
            (
                "std.collections.Iterator",
                include_str!("../../../std/collections/iterator.terl"),
            ),
            (
                "std.collections.Iterable",
                include_str!("../../../std/collections/iterable.terl"),
            ),
        ];

        for (expected_module, source) in contracts {
            let module = parse_module(source).expect("parse release collection trait module");
            assert_eq!(module.name, expected_module);
        }
    }

    #[test]
    fn formal_raw_atom_patterns_are_literal_patterns() {
        let module = parse_module(
            r#"
            module atoms.

            value(Status: Status): Int ->
                case Status {
                    :none -> 0;
                    :empty -> 1
                }.
            "#,
        )
        .expect("parse raw atom patterns");

        let Decl::Function(function) = &module.declarations[0] else {
            panic!("expected function");
        };
        let Expr::Case { clauses, .. } = &function.clauses[0].body else {
            panic!("expected case expression");
        };
        assert!(
            matches!(&clauses[0].pattern, crate::parse_tree::Pattern::Atom(name) if name == "none")
        );
        assert!(
            matches!(&clauses[1].pattern, crate::parse_tree::Pattern::Atom(name) if name == "empty")
        );
    }

    /// Verifies expanded pattern families accepted by the A0.25 syntax
    /// baseline.
    ///
    /// Inputs:
    /// - A module containing map, list-cons, literal, tuple, and
    ///   constructor-style patterns.
    ///
    /// Output:
    /// - Test passes when each pattern family is preserved in the syntax-output.
    ///
    /// Transformation:
    /// - Parses the module through the recursive-descent parser, locates case
    ///   clauses, and inspects the pattern variants and selected guard fields.
    #[test]
    fn formal_pattern_expansion_preserves_ast_shapes() {
        let module = parse_module(
            r#"
            module pattern_shapes.

            map_pattern(value: Map): Int ->
              case value {
                #{kind := :ok, count => n} when n > 0 -> n;
                #{} -> 0
              }.

            list_cons_pattern(values: List[Int]): Int ->
              case values {
                [head | tail] when head > 0 -> head;
                [] -> 0
              }.

            literal_patterns(value: Dynamic): Int ->
              case value {
                :none -> 0;
                1.5 -> 1;
                {left, right} -> 2
              }.

            constructor_patterns(value: Dynamic): Int ->
              case value {
                None -> 0;
                Ok(item) -> item
              }.
            "#,
        )
        .expect("parse pattern expansion");

        let Decl::Function(map_function) = &module.declarations[0] else {
            panic!("expected map pattern function");
        };
        let Expr::Case { clauses, .. } = &map_function.clauses[0].body else {
            panic!("expected map pattern case");
        };
        assert!(
            matches!(&clauses[0].pattern, crate::parse_tree::Pattern::Map(fields) if fields.len() == 2)
        );
        assert!(clauses[0].guard.is_some());
        assert!(
            matches!(&clauses[1].pattern, crate::parse_tree::Pattern::Map(fields) if fields.is_empty())
        );

        let Decl::Function(cons_function) = &module.declarations[1] else {
            panic!("expected cons pattern function");
        };
        let Expr::Case { clauses, .. } = &cons_function.clauses[0].body else {
            panic!("expected cons pattern case");
        };
        assert!(matches!(
            &clauses[0].pattern,
            crate::parse_tree::Pattern::ListCons(_, _)
        ));
        assert!(clauses[0].guard.is_some());

        let Decl::Function(literal_function) = &module.declarations[2] else {
            panic!("expected literal pattern function");
        };
        let Expr::Case { clauses, .. } = &literal_function.clauses[0].body else {
            panic!("expected literal pattern case");
        };
        assert!(
            matches!(&clauses[0].pattern, crate::parse_tree::Pattern::Atom(name) if name == "none")
        );
        assert!(
            matches!(&clauses[1].pattern, crate::parse_tree::Pattern::Float(value) if (*value - 1.5).abs() < f64::EPSILON)
        );
        assert!(
            matches!(&clauses[2].pattern, crate::parse_tree::Pattern::Tuple(items) if items.len() == 2)
        );

        let Decl::Function(constructor_function) = &module.declarations[3] else {
            panic!("expected constructor pattern function");
        };
        let Expr::Case { clauses, .. } = &constructor_function.clauses[0].body else {
            panic!("expected constructor pattern case");
        };
        assert!(matches!(
            &clauses[0].pattern,
            crate::parse_tree::Pattern::Tuple(items)
                if matches!(items.as_slice(), [crate::parse_tree::Pattern::Atom(name)] if name == "None")
        ));
        assert!(matches!(
            &clauses[1].pattern,
            crate::parse_tree::Pattern::Tuple(items)
                if matches!(
                    items.as_slice(),
                    [crate::parse_tree::Pattern::Atom(name), crate::parse_tree::Pattern::Var(var)]
            if name == "Ok" && var == "item"
                )
        ));
    }

    #[test]
    fn formal_nullary_constructor_pattern_call_is_rejected() {
        let err = parse_module(
            r#"
            module bad_constructor_pattern.

            value(Option: Option): Int ->
                case Option {
                    None() -> 0
                }.
            "#,
        )
        .expect_err("reject nullary constructor pattern call");
        assert_eq!(
            err.message,
            "constructor patterns require at least one argument"
        );
    }

    #[test]
    fn parses_quote_and_unquote_expressions() {
        let source = r#"
module sym.

pub macro expand(C: Ast, X: Expr): Expr ->
    quote unquote(X).
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };

        let expr = &function.clauses[0].body;
        match expr {
            Expr::Quote(inner) => match inner.as_ref() {
                Expr::Unquote(_) => {}
                _ => panic!("expected unquote inside quote"),
            },
            _ => panic!("expected quoted expression"),
        }
    }

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

    #[test]
    fn parses_typed_fun_parameters() {
        let source = r#"
module callbackx.

pub run(X: Int): Int ->
    apply((N: Int) -> N + 1, X).
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        match &function.clauses[0].body {
            Expr::Call { args, .. } => match &args[0] {
                Expr::Fun { clauses } => assert_eq!(clauses[0].patterns.len(), 1),
                _ => panic!("expected fun argument"),
            },
            _ => panic!("expected call"),
        }
    }

    #[test]
    fn parses_constructor_style_patterns() {
        let source = r#"
module syntax.

pub simplify(E: Expr): Expr ->
    case E {
        Call(:atom, [x, y]) ->
            call(x, y);
        _ ->
            E
    }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        let expr = &function.clauses[0].body;
        let case_clauses = match expr {
            Expr::Case { clauses, .. } => clauses,
            _ => panic!("expected case"),
        };
        let first = &case_clauses[0].pattern;
        match first {
            crate::parse_tree::Pattern::Tuple(items) => {
                assert_eq!(items.len(), 3);
                match &items[0] {
                    crate::parse_tree::Pattern::Atom(name) => assert_eq!(name, "Call"),
                    _ => panic!("expected constructor atom"),
                }
                match &items[1] {
                    crate::parse_tree::Pattern::Atom(name) => assert_eq!(name, "atom"),
                    _ => panic!("expected raw atom argument"),
                }
            }
            _ => panic!("expected tuple pattern"),
        }
    }

    #[test]
    fn parses_remote_call_expression() {
        let source = r#"
module remote.

pub add(): Int ->
    io_lib:format("~p", []).
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        let expr = &function.clauses[0].body;
        match expr {
            Expr::Call {
                remote: Some(module),
                ..
            } => assert_eq!(module, "io_lib"),
            _ => panic!("expected remote call"),
        }
    }

    /// Verifies explicit trait-target method calls parse as remote calls.
    ///
    /// Inputs:
    /// - A module using `Parse[Int].from_string("42")`.
    ///
    /// Output:
    /// - Test passes when the call is preserved with `Parse[Int]` as the
    ///   remote qualifier and `from_string` as the method name.
    ///
    /// Transformation:
    /// - Parses bracketed type arguments in expression qualifier position
    ///   without introducing general postfix generic call syntax.
    #[test]
    fn parses_explicit_trait_target_call_expression() {
        let source = r#"
module traits.parse_target.

pub parse(): Option[Int] ->
    Parse[Int].from_string("42").
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        let expr = &function.clauses[0].body;
        match expr {
            Expr::Call {
                callee,
                remote: Some(module),
                ..
            } => {
                assert_eq!(module, "Parse[Int]");
                assert!(matches!(callee.as_ref(), Expr::Atom(name) if name == "from_string"));
            }
            _ => panic!("expected explicit trait-target call"),
        }
    }

    #[test]
    fn parses_struct_field_access_sugar() {
        let source = r#"
module fields.

pub name(User: User): Text ->
    User.name.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        let expr = &function.clauses[0].body;
        match expr {
            Expr::FieldAccess { value, field } => {
                assert_eq!(field, "name");
                match value.as_ref() {
                    Expr::Var(name) => assert_eq!(name, "User"),
                    _ => panic!("expected field receiver"),
                }
            }
            _ => panic!("expected field access"),
        }
    }

    #[test]
    fn parses_template_instantiation_expr() {
        let source = r#"
module template_instantiation.

pub view(Title: Text, User: User): Html[none] ->
    Page{ title = Title, user = User }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        match &function.clauses[0].body {
            Expr::TemplateInstantiate { name, fields } => {
                assert_eq!(name, "Page");
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].key, "title");
                assert!(matches!(fields[0].value.as_ref(), Expr::Var(name) if name == "Title"));
                assert_eq!(fields[1].key, "user");
                assert!(matches!(fields[1].value.as_ref(), Expr::Var(name) if name == "User"));
            }
            _ => panic!("expected template instantiation"),
        }
    }

    #[test]
    fn parses_eqeq_and_divrem_operators() {
        let source = r#"
module ops.

pub add(X: Int, Y: Int): Int ->
    X == Y + X div Y.
"#;

        let tokens = crate::lexer::lex(source).unwrap();
        for token in tokens {
            println!("{:?} {:?} {:?}", token.kind, token.text, token.span());
        }

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        match &function.clauses[0].body {
            Expr::BinaryOp { op, .. } => {
                assert_eq!(format!("{:?}", op), "EqEq");
            }
            _ => panic!("expected binary op"),
        }
    }

    #[test]
    fn parses_greater_than_or_equal_operator() {
        let source = r#"
module compare.

pub non_negative(X: Int): Bool ->
    X >= 0.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        match &function.clauses[0].body {
            Expr::BinaryOp { op, .. } => {
                assert!(matches!(op, crate::parse_tree::BinaryOp::GtEq));
            }
            _ => panic!("expected binary op"),
        }
    }

    /// Verifies that the old Kleisli composition operator is not A0 syntax.
    ///
    /// Inputs:
    /// - A module body containing the removed `>=>` operator.
    ///
    /// Output:
    /// - Test passes when parsing rejects the source.
    ///
    /// Transformation:
    /// - Exercises the recursive-descent parser after the canonical EBNF
    ///   removed `>=>` from `CmpOp`.
    #[test]
    fn rejects_kleisli_compose_operator_from_canonical_syntax() {
        let source = r#"
module kleisli_demo.

pub authenticate(): Kleisli[AuthResult, Text, User] ->
    decode_token() >=> load_user() >=> require_admin().
"#;

        parse_module(source).expect_err("kleisli composition operator should be rejected");
    }

    #[test]
    fn parses_pipe_forward_operator() {
        let source = r#"
module pipe_demo.

pub demo(X: Int): Int ->
    X |> add(1).
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        match &function.clauses[0].body {
            Expr::BinaryOp { op, .. } => {
                assert!(matches!(op, crate::parse_tree::BinaryOp::PipeForward));
            }
            _ => panic!("expected pipe forward binary op"),
        }
    }

    /// Verifies binary `!` is rejected as process-message syntax.
    ///
    /// Inputs:
    /// - A module body that attempts to use `P ! inc`.
    ///
    /// Output:
    /// - Test passes when parsing rejects the module.
    ///
    /// Transformation:
    /// - Parses source through the normal module parser and confirms the
    ///   removed BEAM-shaped binary operator cannot produce an expression.
    #[test]
    fn rejects_binary_send_operator_as_noncanonical_source() {
        let source = r#"
module protocol_ok.

pub inc(P: Pid[Counter]): ok ->
    P ! inc,
    ok.
"#;

        parse_module(source).expect_err("binary send operator is not canonical Terlan source");
    }

    #[test]
    fn parses_fixed_array_expression_syntax() {
        let source = r#"
module arrays.

pub rgb(): FixedArray[3, Int] ->
    #[255, 128, 0].
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function declaration"),
        };

        match &function.clauses[0].body {
            Expr::FixedArray(elements) => {
                assert_eq!(elements.len(), 3);
            }
            _ => panic!("expected fixed array expression"),
        }
    }

    #[test]
    fn rejects_bodyless_let_expression() {
        let source = r#"
module let_requires_result.

pub total(price: Int, tax: Int): Int ->
    let subtotal = price; total = subtotal + tax.
"#;

        let error = parse_module(source).expect_err("bodyless let should fail");
        assert!(
            error
                .message
                .contains("let expression requires an explicit result expression"),
            "unexpected diagnostic: {:?}",
            error
        );
    }
}
