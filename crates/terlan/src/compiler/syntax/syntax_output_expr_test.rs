use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_output_includes_recursive_expression_and_pattern_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module recursive.

            pick(Value: Int): Int ->
                case Value {
                    {:ok, value} -> value;
                    _ -> 0
                }.
            "#,
        )
        .expect("syntax output");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Case);
        assert_eq!(body.children.len(), 1);
        assert_eq!(body.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(body.children[0].text.as_deref(), Some("Value"));
        assert_eq!(body.clauses.len(), 2);

        let first_pattern = &body.clauses[0].patterns[0];
        assert_eq!(first_pattern.kind, SyntaxPatternKind::Tuple);
        assert_eq!(first_pattern.children.len(), 2);
        assert_eq!(first_pattern.children[0].kind, SyntaxPatternKind::Atom);
        assert_eq!(first_pattern.children[0].text.as_deref(), Some("ok"));
        assert_eq!(first_pattern.children[1].kind, SyntaxPatternKind::Var);
        assert_eq!(first_pattern.children[1].text.as_deref(), Some("value"));
        assert_eq!(body.clauses[0].body.kind, SyntaxExprKind::Var);
        assert_eq!(body.clauses[0].body.text.as_deref(), Some("value"));

        assert_eq!(
            body.clauses[1].patterns[0].kind,
            SyntaxPatternKind::Wildcard
        );
        assert_eq!(body.clauses[1].body.kind, SyntaxExprKind::Int);
        assert_eq!(body.clauses[1].body.text.as_deref(), Some("0"));
    }

    /// Verifies syntax output preserves explicit cast expressions.
    ///
    /// Inputs:
    /// - A source expression using `value as Option[String]`.
    ///
    /// Output:
    /// - Test passes when syntax output exposes `kind: cast`, `operator: as`,
    ///   the target type text, and the casted child expression.
    ///
    /// Transformation:
    /// - Parses the expression through the public syntax-output entry point
    ///   and inspects the compiler-facing serialized expression shape.

    /// Verifies syntax output preserves explicit cast expressions.
    ///
    /// Inputs:
    /// - A source expression using `value as Option[String]`.
    ///
    /// Output:
    /// - Test passes when syntax output exposes `kind: cast`, `operator: as`,
    ///   the target type text, and the casted child expression.
    ///
    /// Transformation:
    /// - Parses the expression through the public syntax-output entry point
    ///   and inspects the compiler-facing serialized expression shape.
    #[test]
    fn syntax_output_preserves_cast_expression_shape() {
        let output =
            parse_expr_as_syntax_output("value as Option[String]").expect("cast syntax output");

        assert_eq!(output.kind, SyntaxExprKind::Cast);
        assert_eq!(output.operator.as_deref(), Some("as"));
        assert_eq!(output.text.as_deref(), Some("Option[String]"));
        assert_eq!(output.children.len(), 1);
        assert_eq!(output.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(output.children[0].text.as_deref(), Some("value"));
    }

    #[test]
    fn syntax_output_includes_case_guard_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module guarded_case.

            pick(value: Int): Int ->
                case value {
                    x when x > 0 -> x;
                    _ -> 0
                }.
            "#,
        )
        .expect("syntax output guarded case");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Case);
        assert_eq!(body.clauses.len(), 2);

        let first_clause = &body.clauses[0];
        let guard = first_clause.guard.as_ref().expect("case guard tree");
        assert_eq!(guard.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(guard.operator.as_deref(), Some(">"));
        assert_eq!(guard.children.len(), 2);
        assert_eq!(guard.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(guard.children[0].text.as_deref(), Some("x"));
        assert_eq!(guard.children[1].kind, SyntaxExprKind::Int);
        assert_eq!(guard.children[1].text.as_deref(), Some("0"));

        assert!(body.clauses[1].guard.is_none());
    }

    #[test]
    fn syntax_output_includes_function_clause_guard_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module guarded_function.

            pick(value) when value > 0 -> value;
            pick(_) -> 0.
            "#,
        )
        .expect("syntax output guarded function");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        assert_eq!(clauses.len(), 2);
        assert!(clauses[0].has_guard);
        let guard = clauses[0].guard.as_ref().expect("function guard tree");
        assert_eq!(guard.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(guard.operator.as_deref(), Some(">"));
        assert_eq!(guard.children.len(), 2);
        assert_eq!(guard.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(guard.children[0].text.as_deref(), Some("value"));
        assert_eq!(guard.children[1].kind, SyntaxExprKind::Int);
        assert_eq!(guard.children[1].text.as_deref(), Some("0"));

        assert!(!clauses[1].has_guard);
        assert!(clauses[1].guard.is_none());
    }

    #[test]
    fn syntax_output_preserves_expression_precedence_tree() {
        let output = parse_module_as_syntax_output(
            r#"
            module precedence_tree.

            demo(a: Int, b: Int, c: Int): Dynamic ->
                a + b * c |> inspect().
            "#,
        )
        .expect("syntax output precedence tree");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let pipe = &clauses[0].body;
        assert_eq!(pipe.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(pipe.operator.as_deref(), Some("|>"));
        assert_eq!(pipe.children.len(), 2);

        let add = &pipe.children[0];
        assert_eq!(add.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(add.operator.as_deref(), Some("+"));
        assert_eq!(add.children.len(), 2);
        assert_eq!(add.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(add.children[0].text.as_deref(), Some("a"));

        let mul = &add.children[1];
        assert_eq!(mul.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(mul.operator.as_deref(), Some("*"));
        assert_eq!(mul.children.len(), 2);
        assert_eq!(mul.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(mul.children[0].text.as_deref(), Some("b"));
        assert_eq!(mul.children[1].kind, SyntaxExprKind::Var);
        assert_eq!(mul.children[1].text.as_deref(), Some("c"));

        assert_eq!(pipe.children[1].kind, SyntaxExprKind::Call);
    }

    /// Verifies that boolean operators are preserved in formal syntax output.
    ///
    /// Inputs:
    /// - A module whose function body combines pipe, `or`, `and`, comparison,
    ///   and arithmetic operators.
    ///
    /// Output:
    /// - Test passes when syntax output carries `or` and `and` as binary
    ///   operator nodes in canonical precedence order.
    ///
    /// Transformation:
    /// - Parses source to `SyntaxModuleOutput` and inspects the nested
    ///   expression tree used by the formal compiler path.

    /// Verifies that boolean operators are preserved in formal syntax output.
    ///
    /// Inputs:
    /// - A module whose function body combines pipe, `or`, `and`, comparison,
    ///   and arithmetic operators.
    ///
    /// Output:
    /// - Test passes when syntax output carries `or` and `and` as binary
    ///   operator nodes in canonical precedence order.
    ///
    /// Transformation:
    /// - Parses source to `SyntaxModuleOutput` and inspects the nested
    ///   expression tree used by the formal compiler path.
    #[test]
    fn syntax_output_preserves_boolean_expression_precedence_tree() {
        let output = parse_module_as_syntax_output(
            r#"
            module boolean_precedence_tree.

            demo(a: Bool, b: Bool, c: Bool, d: Int, e: Int): Dynamic ->
                a |> inspect() or b and c == d + e.
            "#,
        )
        .expect("syntax output boolean precedence tree");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let pipe = &clauses[0].body;
        assert_eq!(pipe.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(pipe.operator.as_deref(), Some("|>"));

        let or_expr = &pipe.children[1];
        assert_eq!(or_expr.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(or_expr.operator.as_deref(), Some("or"));

        let and_expr = &or_expr.children[1];
        assert_eq!(and_expr.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(and_expr.operator.as_deref(), Some("and"));

        let cmp = &and_expr.children[1];
        assert_eq!(cmp.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(cmp.operator.as_deref(), Some("=="));

        let add = &cmp.children[1];
        assert_eq!(add.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(add.operator.as_deref(), Some("+"));
    }

    /// Verifies local `let` expressions preserve binding order and explicit
    /// body shape.
    ///
    /// Inputs:
    /// - A module with two explicit-body `let` expressions.
    ///
    /// Output:
    /// - Test passes when binding names are preserved in `patterns`, binding
    ///   values are preserved in leading `children`, and an explicit body is
    ///   represented as the final child.
    ///
    /// Transformation:
    /// - Parses source through syntax output and inspects the formal tree
    ///   shape used by typecheck/CoreIR lowering.

    /// Verifies local `let` expressions preserve binding order and explicit
    /// body shape.
    ///
    /// Inputs:
    /// - A module with two explicit-body `let` expressions.
    ///
    /// Output:
    /// - Test passes when binding names are preserved in `patterns`, binding
    ///   values are preserved in leading `children`, and an explicit body is
    ///   represented as the final child.
    ///
    /// Transformation:
    /// - Parses source through syntax output and inspects the formal tree
    ///   shape used by typecheck/CoreIR lowering.
    #[test]
    fn syntax_output_preserves_let_expression_tree() {
        let output = parse_module_as_syntax_output(
            r#"
            module let_tree.

            with_body(x: Int): Int ->
                let y = x + 1; z = y * 2; z + y.

            final_value(x: Int): Int ->
                let y = x + 1; z = y * 2; z.
            "#,
        )
        .expect("syntax output let tree");

        let SyntaxDeclarationPayload::Function {
            clauses: with_body_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };
        let with_body = &with_body_clauses[0].body;
        assert_eq!(with_body.kind, SyntaxExprKind::Let);
        assert_eq!(with_body.arity, 2);
        assert_eq!(with_body.patterns.len(), 2);
        assert_eq!(with_body.patterns[0].text.as_deref(), Some("y"));
        assert_eq!(with_body.patterns[1].text.as_deref(), Some("z"));
        assert_eq!(with_body.children.len(), 3);
        assert_eq!(with_body.children[2].kind, SyntaxExprKind::BinaryOp);
        assert_eq!(with_body.children[2].operator.as_deref(), Some("+"));

        let SyntaxDeclarationPayload::Function {
            clauses: final_value_clauses,
            ..
        } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let final_value = &final_value_clauses[0].body;
        assert_eq!(final_value.kind, SyntaxExprKind::Let);
        assert_eq!(final_value.arity, 2);
        assert_eq!(final_value.patterns.len(), 2);
        assert_eq!(final_value.children.len(), 3);
        assert_eq!(final_value.patterns[1].text.as_deref(), Some("z"));
        assert_eq!(final_value.children[2].kind, SyntaxExprKind::Var);
        assert_eq!(final_value.children[2].text.as_deref(), Some("z"));
    }

    /// Verifies indexed assignment after a `let` binding is parsed as body
    /// expression syntax, not as another binding pattern.
    ///
    /// Inputs:
    /// - A module with `let values = source; values[1] = 2; values`.
    ///
    /// Output:
    /// - Test passes when the let body is a sequence whose first expression is
    ///   `IndexAssign`.
    ///
    /// Transformation:
    /// - Exercises the `let` binding lookahead so bare-name indexed assignment
    ///   is classified as expression syntax after the first semicolon.
    #[test]
    fn syntax_output_parses_index_assignment_after_let_binding() {
        let output = parse_module_as_syntax_output(
            r#"
            module let_index_assignment.

            update(source: List[Int]): List[Int] ->
                let values = source; values[1] = 2; values.
            "#,
        )
        .expect("syntax output let index assignment");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };
        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Let);
        assert_eq!(body.patterns.len(), 1);
        assert_eq!(body.patterns[0].text.as_deref(), Some("values"));
        assert_eq!(body.children.len(), 2);
        let sequence = &body.children[1];
        assert_eq!(sequence.kind, SyntaxExprKind::Sequence);
        assert_eq!(sequence.children.len(), 2);
        assert_eq!(sequence.children[0].kind, SyntaxExprKind::IndexAssign);
        assert_eq!(
            sequence.children[0].children[0].text.as_deref(),
            Some("values")
        );
        assert_eq!(sequence.children[1].kind, SyntaxExprKind::Var);
        assert_eq!(sequence.children[1].text.as_deref(), Some("values"));
    }

    #[test]
    fn syntax_output_preserves_unary_expression_precedence_tree() {
        let output = parse_module_as_syntax_output(
            r#"
            module unary_precedence_tree.

            demo(ready: Bool, value: Int, scale: Int): Bool ->
                not ready == (-value * scale).
            "#,
        )
        .expect("syntax output unary precedence tree");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let cmp = &clauses[0].body;
        assert_eq!(cmp.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(cmp.operator.as_deref(), Some("=="));

        let not_expr = &cmp.children[0];
        assert_eq!(not_expr.kind, SyntaxExprKind::UnaryOp);
        assert_eq!(not_expr.operator.as_deref(), Some("not"));
        assert_eq!(not_expr.children[0].kind, SyntaxExprKind::Var);

        let mul = &cmp.children[1];
        assert_eq!(mul.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(mul.operator.as_deref(), Some("*"));
        assert_eq!(mul.children[0].kind, SyntaxExprKind::UnaryOp);
        assert_eq!(mul.children[0].operator.as_deref(), Some("-"));
    }

    #[test]
    fn syntax_output_rejects_remote_fun_ref_source_syntax() {
        let error = parse_module_as_syntax_output(
            r#"
            module remote_fun_ref_tree.

            demo(): Dynamic ->
                fun math:double/1.
            "#,
        )
        .expect_err("remote fun refs are not canonical source syntax");

        let message = format!("{error:?}");
        assert!(
            message.contains("unexpected tokens after expression") || message.contains("expected"),
            "unexpected diagnostic: {message}"
        );
    }

    #[test]
    fn syntax_output_includes_colon_remote_call_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module remote_call_tree.

            demo(): Dynamic ->
                io_lib:format("~p", []).
            "#,
        )
        .expect("syntax output colon remote call");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Call);
        assert_eq!(body.remote.as_deref(), Some("io_lib"));
        assert_eq!(body.children[0].kind, SyntaxExprKind::Atom);
        assert_eq!(body.children[0].text.as_deref(), Some("format"));
        assert_eq!(body.children.len(), 3);
    }

    /// Verifies syntax output preserves named call-site argument metadata.
    ///
    /// Inputs:
    /// - A module containing a call with positional arguments followed by a
    ///   named argument.
    ///
    /// Output:
    /// - Test passes when call arity, children, and parallel argument names are
    ///   emitted for downstream semantic resolution.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output` and validates
    ///   that the formal output records names without wrapping argument
    ///   expressions in parser-only nodes.
    #[test]
    fn syntax_output_includes_named_call_argument_metadata() {
        let output = parse_module_as_syntax_output(
            r#"
            module named_call_args.

            demo(): Dynamic ->
                create_user(1, "Alice", active = True).
            "#,
        )
        .expect("syntax output named call arguments");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Call);
        assert_eq!(body.arity, 3);
        assert_eq!(body.children.len(), 4);
        assert_eq!(body.arg_names, vec![None, None, Some("active".to_string())]);
    }

    /// Verifies function-value invocation uses expression-call syntax output.
    ///
    /// Inputs:
    /// - A module containing `f.(10, 20)` in function body position.
    ///
    /// Output:
    /// - Test passes when syntax output records a call whose callee child is the
    ///   value expression `f`, not a remote call or constructor candidate.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output` and inspects the
    ///   emitted `SyntaxExprKind::Call` children and remote marker.

    /// Verifies function-value invocation uses expression-call syntax output.
    ///
    /// Inputs:
    /// - A module containing `f.(10, 20)` in function body position.
    ///
    /// Output:
    /// - Test passes when syntax output records a call whose callee child is the
    ///   value expression `f`, not a remote call or constructor candidate.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output` and inspects the
    ///   emitted `SyntaxExprKind::Call` children and remote marker.
    #[test]
    fn syntax_output_includes_function_value_invocation_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module function_value_invocation.

            invoke(f: Dynamic): Dynamic ->
                f.(10, 20).
            "#,
        )
        .expect("syntax output function-value invocation");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::FunctionCall);
        assert_eq!(body.remote, None);
        assert_eq!(body.children.len(), 3);
        assert_eq!(body.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(body.children[0].text.as_deref(), Some("f"));
        assert_eq!(body.children[1].kind, SyntaxExprKind::Int);
        assert_eq!(body.children[1].text.as_deref(), Some("10"));
        assert_eq!(body.children[2].kind, SyntaxExprKind::Int);
        assert_eq!(body.children[2].text.as_deref(), Some("20"));
    }

    /// Verifies receiver method calls are syntax-output calls over field access.
    ///
    /// Inputs:
    /// - A module containing `user.display_name("short")` in function body
    ///   position.
    ///
    /// Output:
    /// - Test passes when syntax output records a normal call whose callee child
    ///   is a `FieldAccess` expression.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output` and inspects the
    ///   emitted call tree consumed by later method-resolution phases.

    /// Verifies receiver method calls are syntax-output calls over field access.
    ///
    /// Inputs:
    /// - A module containing `user.display_name("short")` in function body
    ///   position.
    ///
    /// Output:
    /// - Test passes when syntax output records a normal call whose callee child
    ///   is a `FieldAccess` expression.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output` and inspects the
    ///   emitted call tree consumed by later method-resolution phases.
    #[test]
    fn syntax_output_includes_method_call_suffix_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module method_call_suffix.

            display(user: Dynamic): Dynamic ->
                user.display_name("short").
            "#,
        )
        .expect("syntax output method call suffix");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Call);
        assert_eq!(body.remote, None);
        assert_eq!(body.children.len(), 2);
        assert_eq!(body.children[0].kind, SyntaxExprKind::FieldAccess);
        assert_eq!(body.children[0].text.as_deref(), Some("display_name"));
        assert_eq!(body.children[0].children[0].kind, SyntaxExprKind::Var);
        assert_eq!(body.children[0].children[0].text.as_deref(), Some("user"));
        assert_eq!(body.children[1].kind, SyntaxExprKind::Binary);
        assert_eq!(body.children[1].text.as_deref(), Some("\"short\""));
    }

    /// Verifies syntax output preserves explicit type args on dotted calls.
    ///
    /// Inputs:
    /// - A module containing `Vector.new[String]()` in function body position.
    ///
    /// Output:
    /// - Test passes when syntax output records a remote call with one
    ///   `String` type argument.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output` and validates
    ///   that generic call metadata is preserved structurally for later
    ///   semantic/typecheck phases.
    #[test]
    fn syntax_output_includes_dotted_call_type_args() {
        let output = parse_module_as_syntax_output(
            r#"
            module generic_dotted_call.

            demo(): Dynamic ->
                Vector.new[String]().
            "#,
        )
        .expect("syntax output generic dotted call");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Call);
        assert_eq!(body.remote.as_deref(), Some("Vector"));
        assert_eq!(body.children.len(), 1);
        assert_eq!(body.children[0].kind, SyntaxExprKind::Atom);
        assert_eq!(body.children[0].text.as_deref(), Some("new"));
        assert_eq!(body.type_args.len(), 1);
        assert_eq!(body.type_args[0].text, "String");
    }

    #[test]
    fn syntax_output_includes_macro_expr_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module macro_expr_tree.

            module_name(): Dynamic ->
                ?MODULE.

            compare(a: Int, b: Int): Dynamic ->
                ?assert_equal(a, b).
            "#,
        )
        .expect("syntax output macro expr");

        let SyntaxDeclarationPayload::Function {
            clauses: module_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };
        assert_eq!(module_clauses[0].body.kind, SyntaxExprKind::Macro);
        assert_eq!(module_clauses[0].body.text.as_deref(), Some("MODULE"));
        assert_eq!(module_clauses[0].body.arity, 0);

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        assert_eq!(clauses[0].body.kind, SyntaxExprKind::Macro);
        assert_eq!(clauses[0].body.text.as_deref(), Some("assert_equal"));
        assert_eq!(clauses[0].body.children.len(), 2);
        assert_eq!(clauses[0].body.arity, 2);
    }

    #[test]
    fn syntax_output_includes_raw_macro_expr_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module raw_macro_expr_tree.

            query(): Dynamic ->
                sql{select * from users}.
            "#,
        )
        .expect("syntax output raw macro expr");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::RawMacro);
        assert_eq!(body.text.as_deref(), Some("sql"));
        assert_eq!(body.raw.as_deref(), Some("select * from users"));
        assert!(body.type_args.is_empty());
        assert!(body.children.is_empty());
    }

    #[test]
    fn syntax_output_includes_typed_sql_raw_macro_expr_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module typed_sql_raw_macro_expr_tree.

            query(): Dynamic ->
                sql[UserRow] {select * from users}.
            "#,
        )
        .expect("syntax output typed sql raw macro expr");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::RawMacro);
        assert_eq!(body.text.as_deref(), Some("sql"));
        assert_eq!(body.raw.as_deref(), Some("select * from users"));
        assert_eq!(body.type_args.len(), 1);
        assert_eq!(body.type_args[0].text, "UserRow");
        assert!(body.children.is_empty());
    }

    #[test]
    fn syntax_output_includes_typed_sql_interpolation_children() {
        let output = parse_module_as_syntax_output(
            r#"
            module typed_sql_interpolation_tree.

            query(user: User): Dynamic ->
                sql[UserRow] {select * from users where id = ${user.id}}.
            "#,
        )
        .expect("syntax output typed sql interpolation expr");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::RawMacro);
        assert_eq!(body.children.len(), 1);
        assert_eq!(body.children[0].kind, SyntaxExprKind::FieldAccess);
        assert_eq!(body.children[0].text.as_deref(), Some("id"));
    }

    #[test]
    fn syntax_output_ignores_typed_sql_comment_interpolation_text() {
        let output = parse_module_as_syntax_output(
            r#"
            module typed_sql_comment_interpolation_tree.

            query(user: User): Dynamic ->
                sql[UserRow] {
                    /* ${ignored} */
                    select * from users where id = ${user.id}
                    /* ${also_ignored} */
                }.
            "#,
        )
        .expect("syntax output typed sql comment interpolation expr");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::RawMacro);
        assert_eq!(body.children.len(), 1);
        assert_eq!(body.children[0].kind, SyntaxExprKind::FieldAccess);
        assert_eq!(body.children[0].text.as_deref(), Some("id"));
    }

    #[test]
    fn syntax_output_includes_quoted_atom_literals() {
        let output = parse_module_as_syntax_output(
            r#"
            module quoted_atom_tree.

            module_atom(): Dynamic ->
                :'Elixir.Module'.

            classify(value: Dynamic): Dynamic ->
                case value {
                    :'some atom' -> :ok
                }.
            "#,
        )
        .expect("syntax output quoted atom literals");

        let SyntaxDeclarationPayload::Function {
            clauses: atom_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };
        assert_eq!(atom_clauses[0].body.kind, SyntaxExprKind::Atom);
        assert_eq!(atom_clauses[0].body.text.as_deref(), Some("Elixir.Module"));

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let case_expr = &clauses[0].body;
        assert_eq!(case_expr.kind, SyntaxExprKind::Case);
        assert_eq!(
            case_expr.clauses[0].patterns[0].text.as_deref(),
            Some("some atom")
        );
    }

    /// Verifies canonical atom literal expressions preserve their source form.
    ///
    /// Inputs:
    /// - A module function returning `Atom["..."]` with escaped quote,
    ///   backslash, newline, carriage return, and tab payloads.
    ///
    /// Output:
    /// - A syntax-output atom node with normalized text and canonical raw
    ///   source spelling.
    ///
    /// Transformation:
    /// - Crosses the parse-tree-to-syntax-output boundary while preserving enough
    ///   source context for later validation to distinguish explicit atom
    ///   values from bare identifiers.
    #[test]
    fn syntax_output_includes_canonical_atom_literal_expr_source() {
        let output = parse_module_as_syntax_output(
            r#"
            module atom_literal_expr_tree.

            ready(): Atom ->
                Atom["quote \" slash \\ newline \n carriage \r tab \t"].
            "#,
        )
        .expect("syntax output atom literal expression");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };
        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Atom);
        assert_eq!(
            body.text.as_deref(),
            Some("quote \" slash \\ newline \n carriage \r tab \t")
        );
        assert_eq!(
            body.raw.as_deref(),
            Some(r#"Atom["quote \" slash \\ newline \n carriage \r tab \t"]"#)
        );
    }

    /// Verifies prefixed integer literals cross the formal syntax-output
    /// boundary as normalized integer values.
    ///
    /// Inputs:
    /// - A module containing decimal, binary, hexadecimal, and octal integer
    ///   literal function bodies.
    ///
    /// Output:
    /// - Test passes when each function body is a `SyntaxExprKind::Int` and the
    ///   prefixed forms normalize to decimal value text.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output`, extracts each
    ///   function clause body, and compares the syntax-output value text.

    /// Verifies prefixed integer literals cross the formal syntax-output
    /// boundary as normalized integer values.
    ///
    /// Inputs:
    /// - A module containing decimal, binary, hexadecimal, and octal integer
    ///   literal function bodies.
    ///
    /// Output:
    /// - Test passes when each function body is a `SyntaxExprKind::Int` and the
    ///   prefixed forms normalize to decimal value text.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output`, extracts each
    ///   function clause body, and compares the syntax-output value text.
    #[test]
    fn syntax_output_normalizes_prefixed_integer_literals() {
        let output = parse_module_as_syntax_output(
            r#"
            module radix_literals.

            decimal_int(): Int -> 42.
            binary_int(): Int -> 0b101010.
            hex_int(): Int -> 0x2a.
            octal_int(): Int -> 0o52.
            "#,
        )
        .expect("syntax output radix literals");

        let literal_texts = output
            .declarations
            .iter()
            .map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Function { clauses, .. } => {
                    assert_eq!(clauses[0].body.kind, SyntaxExprKind::Int);
                    clauses[0].body.text.as_deref()
                }
                other => panic!("unexpected declaration payload: {other:?}"),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            literal_texts,
            vec![Some("42"), Some("42"), Some("42"), Some("42")]
        );
    }

    /// Verifies Erlang binary segment syntax is rejected before syntax-output
    /// boundary.
    ///
    /// Inputs:
    /// - A module containing an Erlang binary expression with size and segment
    ///   modifiers.
    ///
    /// Output:
    /// - Test passes when syntax-output construction rejects the source.
    ///
    /// Transformation:
    /// - Keeps backend Erlang binary syntax from entering canonical Terlan
    ///   syntax output.

    /// Verifies Erlang binary segment syntax is rejected before syntax-output
    /// boundary.
    ///
    /// Inputs:
    /// - A module containing an Erlang binary expression with size and segment
    ///   modifiers.
    ///
    /// Output:
    /// - Test passes when syntax-output construction rejects the source.
    ///
    /// Transformation:
    /// - Keeps backend Erlang binary syntax from entering canonical Terlan
    ///   syntax output.
    #[test]
    fn syntax_output_rejects_erlang_binary_segment_text() {
        let error = parse_module_as_syntax_output(
            r#"
            module binary_segment_text.

            byte(value: Int): Binary ->
                <<value:8/integer-unsigned-big>>.
            "#,
        )
        .expect_err("Erlang binary segment syntax should be rejected");

        let crate::terlan_syntax::ebnf::EbnfCompileError::Parse(message, _) = error else {
            panic!("expected parse error");
        };
        assert!(message.contains("Erlang binary literal syntax"));
    }

    #[test]
    fn syntax_output_includes_constructor_chain_expr_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module constructor_chain_expr_tree.

            demo(id: Int, name: Binary): Dynamic ->
                User(id, name) with Admin { id = id, name = name }.
            "#,
        )
        .expect("syntax output constructor chain expr");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::ConstructorChain);
        assert_eq!(body.children.len(), 2);
        assert_eq!(body.children[0].kind, SyntaxExprKind::Call);
        assert_eq!(body.children[1].kind, SyntaxExprKind::RecordConstruct);
        assert_eq!(body.children[1].text.as_deref(), Some("Admin"));
    }

    #[test]
    fn syntax_output_allows_keyword_expressions_in_operator_chains() {
        let output = parse_module_as_syntax_output(
            r#"
            module keyword_expr_chain.

            demo(option: Dynamic): Dynamic ->
                case option {
                    :none -> 0;
                    value -> value
                } |> inspect().
            "#,
        )
        .expect("syntax output keyword expression chain");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let pipe = &clauses[0].body;
        assert_eq!(pipe.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(pipe.operator.as_deref(), Some("|>"));
        assert_eq!(pipe.children.len(), 2);

        let case_expr = &pipe.children[0];
        assert_eq!(case_expr.kind, SyntaxExprKind::Case);
        assert_eq!(case_expr.clauses.len(), 2);
        assert_eq!(
            case_expr.clauses[0].patterns[0].kind,
            SyntaxPatternKind::Atom
        );
        assert_eq!(
            case_expr.clauses[0].patterns[0].text.as_deref(),
            Some("none")
        );

        assert_eq!(pipe.children[1].kind, SyntaxExprKind::Call);
    }

    #[test]
    fn syntax_output_includes_if_expression_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module if_expr.

            choose(flag: Bool): Int ->
                if {
                    flag -> 1;
                    _ -> 0
                }.
            "#,
        )
        .expect("syntax output if expression");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::If);
        assert_eq!(body.clauses.len(), 2);
        let condition = body.clauses[0].guard.as_ref().expect("if condition");
        assert_eq!(condition.kind, SyntaxExprKind::Var);
        assert_eq!(condition.text.as_deref(), Some("flag"));
        assert_eq!(body.clauses[0].body.kind, SyntaxExprKind::Int);
        assert_eq!(body.clauses[0].body.text.as_deref(), Some("1"));
        let fallback = body.clauses[1].guard.as_ref().expect("fallback condition");
        assert_eq!(fallback.kind, SyntaxExprKind::Var);
        assert_eq!(fallback.text.as_deref(), Some("true"));
    }

    #[test]
    fn syntax_output_includes_try_expression_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module try_expr.

            wait(): Int ->
                try risky() {
                    {:ok, value} -> value
                catch
                    :error -> 0
                after
                    0 -> cleanup()
                }.
            "#,
        )
        .expect("syntax output try expression");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Try);
        assert_eq!(body.children[0].kind, SyntaxExprKind::Call);
        assert_eq!(body.clauses.len(), 1);
        assert_eq!(body.catch_clauses.len(), 1);
        assert_eq!(body.clauses[0].patterns[0].kind, SyntaxPatternKind::Tuple);
        assert_eq!(
            body.catch_clauses[0].patterns[0].kind,
            SyntaxPatternKind::Atom
        );
        let after = body.try_after.as_ref().expect("expected try after output");
        assert_eq!(after.trigger.kind, SyntaxExprKind::Int);
        assert_eq!(after.trigger.text.as_deref(), Some("0"));
        assert_eq!(after.body.kind, SyntaxExprKind::Call);
    }

    #[test]
    fn syntax_output_keeps_constructor_call_candidates_as_named_calls() {
        let output = parse_module_as_syntax_output(
            r#"
            module constructor_calls.

            make(): Dynamic ->
                Ok(123).
            "#,
        )
        .expect("syntax output constructor call candidate");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Call);
        assert_eq!(body.children.len(), 2);
        assert_eq!(body.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(body.children[0].text.as_deref(), Some("Ok"));
        assert_eq!(body.children[1].kind, SyntaxExprKind::Int);
        assert_eq!(body.children[1].text.as_deref(), Some("123"));
    }

    #[test]
    fn syntax_output_includes_record_suffix_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module record_suffix_trees.

            field(user: Dynamic): Dynamic ->
                user#foo.bar.

            update(user: Dynamic): Dynamic ->
                user#foo{bar = 2}.
            "#,
        )
        .expect("syntax output record suffix trees");

        let SyntaxDeclarationPayload::Function {
            clauses: field_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected field function declaration");
        };
        let access = &field_clauses[0].body;
        assert_eq!(access.kind, SyntaxExprKind::RecordAccess);
        assert_eq!(access.text.as_deref(), Some("foo.bar"));
        assert_eq!(access.children.len(), 1);
        assert_eq!(access.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(access.children[0].text.as_deref(), Some("user"));

        let SyntaxDeclarationPayload::Function {
            clauses: update_clauses,
            ..
        } = &output.declarations[1].payload
        else {
            panic!("expected update function declaration");
        };
        let update = &update_clauses[0].body;
        assert_eq!(update.kind, SyntaxExprKind::RecordUpdate);
        assert_eq!(update.text.as_deref(), Some("foo"));
        assert_eq!(update.children.len(), 1);
        assert_eq!(update.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(update.children[0].text.as_deref(), Some("user"));
        assert_eq!(update.fields.len(), 1);
        assert_eq!(update.fields[0].key, "bar");
        assert_eq!(update.fields[0].value.kind, SyntaxExprKind::Int);
        assert_eq!(update.fields[0].value.text.as_deref(), Some("2"));
    }

    #[test]
    fn syntax_output_includes_sequence_primary_expr_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module sequence_primary_trees.

            binary(): Binary ->
                "hello".

            fixed(): FixedArray[3, Int] ->
                #[1, 2, 3].

            indexed(items: List[Int]): Int ->
                items[0].

            indexed_assign(items: List[Int]): Unit ->
                items[0] = 1.

            generated(items: List[Int]): List[Int] ->
                [item | item <- items].
            "#,
        )
        .expect("syntax output sequence primary trees");

        let SyntaxDeclarationPayload::Function {
            clauses: binary_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected binary function declaration");
        };
        let binary = &binary_clauses[0].body;
        assert_eq!(binary.kind, SyntaxExprKind::Binary);
        assert_eq!(binary.text.as_deref(), Some("\"hello\""));

        let SyntaxDeclarationPayload::Function {
            clauses: fixed_clauses,
            ..
        } = &output.declarations[1].payload
        else {
            panic!("expected fixed array function declaration");
        };
        let fixed = &fixed_clauses[0].body;
        assert_eq!(fixed.kind, SyntaxExprKind::FixedArray);
        assert_eq!(fixed.children.len(), 3);
        assert_eq!(fixed.children[0].text.as_deref(), Some("1"));
        assert_eq!(fixed.children[2].text.as_deref(), Some("3"));

        let SyntaxDeclarationPayload::Function {
            clauses: index_clauses,
            ..
        } = &output.declarations[2].payload
        else {
            panic!("expected indexed function declaration");
        };
        let indexed = &index_clauses[0].body;
        assert_eq!(indexed.kind, SyntaxExprKind::Index);
        assert_eq!(indexed.children.len(), 2);
        assert_eq!(indexed.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(indexed.children[0].text.as_deref(), Some("items"));
        assert_eq!(indexed.children[1].kind, SyntaxExprKind::Int);
        assert_eq!(indexed.children[1].text.as_deref(), Some("0"));

        let SyntaxDeclarationPayload::Function {
            clauses: assign_clauses,
            ..
        } = &output.declarations[3].payload
        else {
            panic!("expected indexed assignment function declaration");
        };
        let indexed_assign = &assign_clauses[0].body;
        assert_eq!(indexed_assign.kind, SyntaxExprKind::IndexAssign);
        assert_eq!(indexed_assign.children.len(), 3);
        assert_eq!(indexed_assign.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(indexed_assign.children[0].text.as_deref(), Some("items"));
        assert_eq!(indexed_assign.children[1].kind, SyntaxExprKind::Int);
        assert_eq!(indexed_assign.children[1].text.as_deref(), Some("0"));
        assert_eq!(indexed_assign.children[2].kind, SyntaxExprKind::Int);
        assert_eq!(indexed_assign.children[2].text.as_deref(), Some("1"));

        let SyntaxDeclarationPayload::Function {
            clauses: generated_clauses,
            ..
        } = &output.declarations[4].payload
        else {
            panic!("expected generated function declaration");
        };
        let generated = &generated_clauses[0].body;
        assert_eq!(generated.kind, SyntaxExprKind::ListComprehension);
        assert_eq!(generated.children.len(), 2);
        assert_eq!(generated.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(generated.children[0].text.as_deref(), Some("item"));
        assert_eq!(generated.children[1].kind, SyntaxExprKind::Var);
        assert_eq!(generated.children[1].text.as_deref(), Some("items"));
        assert_eq!(generated.patterns.len(), 1);
        assert_eq!(generated.patterns[0].kind, SyntaxPatternKind::Var);
        assert_eq!(generated.patterns[0].text.as_deref(), Some("item"));
    }

    #[test]
    fn syntax_output_includes_map_constructor_record_and_template_field_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module field_payload_trees.

            map(): Map ->
                #{a := 1, b => 2}.

            chain(id: Int): Dynamic ->
                User(id) with Admin{name = "Ada"}.

            render_template(): Dynamic ->
                Page{title = "hello"}.
            "#,
        )
        .expect("syntax output field payload trees");

        let SyntaxDeclarationPayload::Function {
            clauses: map_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected map function declaration");
        };
        let map = &map_clauses[0].body;
        assert_eq!(map.kind, SyntaxExprKind::Map);
        assert_eq!(map.fields.len(), 2);
        assert_eq!(map.fields[0].key, "a");
        assert!(map.fields[0].required);
        assert_eq!(map.fields[0].value.kind, SyntaxExprKind::Int);
        assert_eq!(map.fields[0].value.text.as_deref(), Some("1"));
        assert_eq!(map.fields[1].key, "b");
        assert!(!map.fields[1].required);
        assert_eq!(map.fields[1].value.kind, SyntaxExprKind::Int);
        assert_eq!(map.fields[1].value.text.as_deref(), Some("2"));

        let SyntaxDeclarationPayload::Function {
            clauses: chain_clauses,
            ..
        } = &output.declarations[1].payload
        else {
            panic!("expected chain function declaration");
        };
        let chain = &chain_clauses[0].body;
        assert_eq!(chain.kind, SyntaxExprKind::ConstructorChain);
        assert_eq!(chain.children.len(), 2);
        let record = &chain.children[1];
        assert_eq!(record.kind, SyntaxExprKind::RecordConstruct);
        assert_eq!(record.text.as_deref(), Some("Admin"));
        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].key, "name");
        assert!(record.fields[0].required);
        assert_eq!(record.fields[0].value.kind, SyntaxExprKind::Binary);
        assert_eq!(record.fields[0].value.text.as_deref(), Some("\"Ada\""));

        let SyntaxDeclarationPayload::Function {
            clauses: template_clauses,
            ..
        } = &output.declarations[2].payload
        else {
            panic!("expected template function declaration");
        };
        let template = &template_clauses[0].body;
        assert_eq!(template.kind, SyntaxExprKind::TemplateInstantiate);
        assert_eq!(template.text.as_deref(), Some("Page"));
        assert_eq!(template.fields.len(), 1);
        assert_eq!(template.fields[0].key, "title");
        assert!(template.fields[0].required);
        assert_eq!(template.fields[0].value.kind, SyntaxExprKind::Binary);
        assert_eq!(template.fields[0].value.text.as_deref(), Some("\"hello\""));
    }
}
