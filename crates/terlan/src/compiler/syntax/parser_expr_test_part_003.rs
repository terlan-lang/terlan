
    #[test]
    fn formal_remote_fun_ref_is_not_source_syntax() {
        let err = parse_terlan_expr("fun math:double/1 |> inspect()")
            .expect_err("remote fun refs are not canonical source syntax");

        assert!(
            err.message.contains("unexpected tokens after expression")
                || err.message.contains("expected"),
            "unexpected diagnostic: {}",
            err.message
        );
    }

    #[test]
    fn formal_macro_expr_parses_as_primary_expr() {
        let expr = parse_terlan_expr("?MODULE |> inspect()").expect("parse macro expr in pipe");
        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(
            op,
            crate::terlan_syntax::parse_tree::BinaryOp::PipeForward
        ));
        assert!(matches!(
            left.as_ref(),
            Expr::MacroCall { name, args } if name == "MODULE" && args.is_empty()
        ));

        let expr = parse_terlan_expr("?assert_equal(A, B)").expect("parse macro call expr");
        assert!(matches!(
            expr,
            Expr::MacroCall { name, args } if name == "assert_equal" && args.len() == 2
        ));
    }

    #[test]
    fn formal_raw_macro_expr_requires_immediate_raw_block() {
        let expr = parse_terlan_expr("sql{select * from users} |> inspect()")
            .expect("parse raw macro expr in pipe");
        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(
            op,
            crate::terlan_syntax::parse_tree::BinaryOp::PipeForward
        ));
        assert!(matches!(
            left.as_ref(),
            Expr::RawMacro { name, type_args, interpolations, raw }
                if name == "sql"
                    && type_args.is_empty()
                    && interpolations.is_empty()
                    && raw == "select * from users"
        ));

        let spaced = parse_terlan_expr("sql {select * from users}");
        assert!(
            spaced.is_err(),
            "spaced raw macro should not parse as expression"
        );
    }

    #[test]
    fn formal_typed_sql_raw_macro_expr_parses_result_type() {
        let expr = parse_terlan_expr("sql[UserRow] {select * from users}")
            .expect("parse typed sql raw macro expr");
        let Expr::RawMacro {
            name,
            type_args,
            interpolations,
            raw,
        } = expr
        else {
            panic!("expected typed sql raw macro expression");
        };

        assert_eq!(name, "sql");
        assert_eq!(type_args.len(), 1);
        assert_eq!(type_args[0].text, "UserRow");
        assert!(interpolations.is_empty());
        assert_eq!(raw, "select * from users");
    }

    #[test]
    fn formal_typed_sql_raw_macro_expr_parses_interpolation_expressions() {
        let expr = parse_terlan_expr("sql[UserRow] {select * from users where id = ${user.id}}")
            .expect("parse typed sql raw macro interpolation");
        let Expr::RawMacro { interpolations, .. } = expr else {
            panic!("expected typed sql raw macro expression");
        };

        assert_eq!(interpolations.len(), 1);
        assert!(matches!(
            &interpolations[0],
            Expr::FieldAccess { field, .. } if field == "id"
        ));
    }

    #[test]
    fn formal_typed_sql_raw_macro_expr_ignores_comment_interpolation_text() {
        let expr = parse_terlan_expr(
            "sql[UserRow] {/* ${ignored} */ select * from users where id = ${user.id} /* ${also_ignored} */}",
        )
        .expect("parse typed sql raw macro comment interpolation text");
        let Expr::RawMacro { interpolations, .. } = expr else {
            panic!("expected typed sql raw macro expression");
        };

        assert_eq!(interpolations.len(), 1);
        assert!(matches!(
            &interpolations[0],
            Expr::FieldAccess { field, .. } if field == "id"
        ));
    }

    #[test]
    fn formal_typed_sql_raw_macro_expr_rejects_bad_interpolation() {
        let err = parse_terlan_expr("sql[UserRow] {select * from users where id = ${}}")
            .expect_err("empty sql interpolation should fail");

        assert!(err.message.contains("empty SQL interpolation expression"));
    }

    #[test]
    fn formal_constructor_chain_expr_parses_with_record_expr() {
        let expr = parse_terlan_expr("User(id, name) with Admin { id: id, name: name }")
            .expect("parse constructor chain expr");

        let Expr::ConstructorChain { base, record } = expr else {
            panic!("expected constructor chain expression");
        };
        assert!(matches!(
            base.as_ref(),
            Expr::Call {
                remote: None,
                args,
                ..
            } if args.len() == 2
        ));
        assert!(matches!(
            record.as_ref(),
            Expr::RecordConstruct { name, fields } if name == "Admin" && fields.len() == 2
        ));
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

    /// Verifies private struct field access syntax.
    ///
    /// Inputs:
    /// - A function body containing `receiver.#field`.
    ///
    /// Output:
    /// - Test passes when the parser preserves the private marker on the field
    ///   access expression.
    ///
    /// Transformation:
    /// - Reuses the normal field-access expression node while keeping `#` in
    ///   the field text for later privacy enforcement.
    #[test]
    fn parses_private_struct_field_access_sugar() {
        let source = r#"
module private_fields.

pub email(user: User): String ->
    user.#email.
"#;

        let module = parse_module(source).expect("parse private field access");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        match &function.clauses[0].body {
            Expr::FieldAccess { value, field } => {
                assert_eq!(field, "#email");
                assert!(matches!(value.as_ref(), Expr::Var(name) if name == "user"));
            }
            _ => panic!("expected private field access"),
        }
    }

    #[test]
    fn parses_template_instantiation_expr() {
        let source = r#"
module template_instantiation.

pub view(Title: Text, User: User): Html[none] ->
    Page { title: Title, user: User }.
"#;

        let module = parse_module(source).expect("parse");
        let function = match &module.declarations[0] {
            Decl::Function(function) => function,
            _ => panic!("expected function"),
        };
        match &function.clauses[0].body {
            Expr::RecordConstruct { name, fields } => {
                assert_eq!(name, "Page");
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].key, "title");
                assert!(matches!(fields[0].value.as_ref(), Expr::Var(name) if name == "Title"));
                assert_eq!(fields[1].key, "user");
                assert!(matches!(fields[1].value.as_ref(), Expr::Var(name) if name == "User"));
            }
            _ => panic!("expected nominal keyed construction"),
        }
    }

    #[test]
    fn parses_eqeq_and_divrem_operators() {
        let source = r#"
module ops.

pub add(X: Int, Y: Int): Int ->
    X == Y + X div Y.
"#;

        let tokens = crate::terlan_syntax::lexer::lex(source).unwrap();
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
                assert!(matches!(
                    op,
                    crate::terlan_syntax::parse_tree::BinaryOp::GtEq
                ));
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

    /// Verifies implication arrows are reserved for compile-time evidence.
    ///
    /// Inputs:
    /// - A function body that attempts to use `=>` between runtime values.
    ///
    /// Output:
    /// - Test passes when parsing rejects the expression with a stable
    ///   implication-specific diagnostic.
    ///
    /// Transformation:
    /// - Locks the implication-arrow syntax contract at parser level so `=>`
    ///   cannot accidentally become a runtime binary operator while the
    ///   generic-parameter implication grammar is being implemented.
    #[test]
    fn rejects_implication_arrow_as_runtime_expression_operator() {
        let source = r#"
module implication_expression_rejected.

pub proven(left: Int, right: Int): Bool ->
    left => right.
"#;

        let error = parse_module(source).expect_err("runtime implication operator should fail");
        assert!(
            error.message.contains("compile-time implication arrow")
                && error.message.contains("not a runtime expression operator"),
            "unexpected diagnostic: {:?}",
            error
        );
    }

    /// Verifies implication arrows are rejected inside lambda bodies.
    ///
    /// Inputs:
    /// - A lambda body that attempts to use `=>` between runtime values.
    ///
    /// Output:
    /// - Test passes when parsing rejects the nested expression with the
    ///   implication-specific runtime diagnostic.
    ///
    /// Transformation:
    /// - Proves lambda bodies use the same implication reservation as ordinary
    ///   function bodies while positive implication constraints remain
    ///   restricted to owning compile-time forms.
    #[test]
    fn rejects_implication_arrow_in_lambda_body() {
        let error = parse_terlan_expr("(value) -> value => value")
            .expect_err("lambda runtime implication operator should fail");
        assert!(
            error.message.contains("compile-time implication arrow")
                && error.message.contains("not a runtime expression operator"),
            "unexpected diagnostic: {:?}",
            error
        );
    }

    /// Verifies implication arrows are rejected inside case branch bodies.
    ///
    /// Inputs:
    /// - A case branch body that attempts to use `=>` between runtime values.
    ///
    /// Output:
    /// - Test passes when parsing rejects the nested branch expression with the
    ///   implication-specific runtime diagnostic.
    ///
    /// Transformation:
    /// - Proves case branch bodies cannot become an accidental implication
    ///   surface while the formal generic-parameter implication grammar is being
    ///   implemented.
    #[test]
    fn rejects_implication_arrow_in_case_branch_body() {
        let error = parse_terlan_expr("case value { current -> current => value }")
            .expect_err("case branch runtime implication operator should fail");
        assert!(
            error.message.contains("compile-time implication arrow")
                && error.message.contains("not a runtime expression operator"),
            "unexpected diagnostic: {:?}",
            error
        );
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
                assert!(matches!(
                    op,
                    crate::terlan_syntax::parse_tree::BinaryOp::PipeForward
                ));
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
    ///   removed VM-shaped binary operator cannot produce an expression.
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

    /// Verifies removed callable dot-call syntax fails with a stable diagnostic.
    ///
    /// Inputs:
    /// - A function body that attempts to call a function value with
    ///   `callback.(1)`.
    ///
    /// Output:
    /// - Test passes when parsing rejects the old syntax and points to
    ///   `callback(1)`.
    ///
    /// Transformation:
    /// - Locks the 0.0.7 callable syntax cleanup rule at parser level so the
    ///   old dot-call suffix cannot remain as hidden syntax.
    #[test]
    fn rejects_function_value_dot_call_syntax() {
        let source = r#"
module callable_dot_call_removed.

pub apply(callback: (Int) -> Int): Int ->
    callback.(1).
"#;

        let error = parse_module(source).expect_err("dot-call syntax should fail");
        assert!(
            error
                .message
                .contains("function-value dot-call syntax was removed; use `callee(args)`"),
            "unexpected diagnostic: {:?}",
            error
        );
    }

    /// Verifies parenthesized function-value dot-call syntax is also removed.
    ///
    /// Inputs:
    /// - A function body that attempts to call a parenthesized function value
    ///   with `(callback).(1)`.
    ///
    /// Output:
    /// - Test passes when parsing rejects the old syntax and points to normal
    ///   call syntax.
    ///
    /// Transformation:
    /// - Locks the removed syntax rule for both identifier and expression
    ///   receivers so the old dot-call suffix cannot survive behind grouping.
    #[test]
    fn rejects_parenthesized_function_value_dot_call_syntax() {
        let source = r#"
module parenthesized_callable_dot_call_removed.

pub apply(callback: (Int) -> Int): Int ->
    (callback).(1).
"#;

        let error = parse_module(source).expect_err("parenthesized dot-call syntax should fail");
        assert!(
            error
                .message
                .contains("function-value dot-call syntax was removed; use `callee(args)`"),
            "unexpected diagnostic: {:?}",
            error
        );
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

    /// Verifies decimal and scientific Float literals share one finite domain.
    #[test]
    fn small_float_parity_parses_finite_scientific_literals() {
        for source in ["1e3", "2.5e-4", "5.0e-324"] {
            let Expr::Float(value) = parse_terlan_expr(source).expect("parse scientific Float")
            else {
                panic!("expected Float expression for {source}");
            };
            assert!(value.is_finite(), "{source} must remain finite");
        }
    }

    /// Verifies overflowing Float literals fail instead of entering CoreIR as infinity.
    #[test]
    fn small_float_parity_rejects_non_finite_scientific_literals() {
        let error = parse_terlan_expr("1e999").expect_err("overflowing Float must fail");
        assert_eq!(error.message, "float literal must be finite");
    }

    #[test]
    fn rejects_bodyless_let_expression() {
        let source = r#"
module let_requires_result.

pub subtotal(price: Int): Int ->
    let subtotal = price.
"#;

        let error = parse_module(source).expect_err("bodyless let should fail");
        assert_eq!(
            error.message,
            "let expression requires an explicit result expression"
        );
    }
