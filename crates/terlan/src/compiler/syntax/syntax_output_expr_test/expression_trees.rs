use super::super::*;

#[test]
pub(super) fn syntax_output_includes_recursive_expression_and_pattern_trees() {
    let output = parse_module_as_syntax_output(
        r#"
        module recursive.

        pick(Value: Int): Int ->
            case Value {
                {Atom["ok"], value} -> value;
                _ -> 0
            }.
        "#,
    )
    .expect("syntax output");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_preserves_cast_expression_shape() {
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
pub(super) fn syntax_output_includes_case_guard_trees() {
    let output = parse_module_as_syntax_output(
        r#"
        module guarded_case.

        pick(value: Int): Int ->
            case value {
                x where x > 0 -> x;
                _ -> 0
            }.
        "#,
    )
    .expect("syntax output guarded case");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_includes_function_clause_guard_trees() {
    let output = parse_module_as_syntax_output(
        r#"
        module guarded_function.

        pick(value) where value > 0 -> value;
        pick(_) -> 0.
        "#,
    )
    .expect("syntax output guarded function");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_accepts_function_clause_where_guard_trees() {
    let output = parse_module_as_syntax_output(
        r#"
        module guarded_function_where.

        pick(value) where value > 0 -> value;
        pick(_) -> 0.
        "#,
    )
    .expect("syntax output where-guarded function");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
        panic!("expected function declaration");
    };

    assert_eq!(clauses.len(), 2);
    assert!(clauses[0].has_guard);
    let guard = clauses[0]
        .guard
        .as_ref()
        .expect("function where guard tree");
    assert_eq!(guard.kind, SyntaxExprKind::BinaryOp);
    assert_eq!(guard.operator.as_deref(), Some(">"));
    assert_eq!(guard.children.len(), 2);
    assert_eq!(guard.children[0].kind, SyntaxExprKind::Var);
    assert_eq!(guard.children[0].text.as_deref(), Some("value"));
    assert_eq!(guard.children[1].kind, SyntaxExprKind::Int);
    assert_eq!(guard.children[1].text.as_deref(), Some("0"));
    assert!(!clauses[1].has_guard);
}

#[test]
pub(super) fn syntax_output_preserves_expression_precedence_tree() {
    let output = parse_module_as_syntax_output(
        r#"
        module precedence_tree.

        demo(a: Int, b: Int, c: Int): Dynamic ->
            a + b * c |> inspect().
        "#,
    )
    .expect("syntax output precedence tree");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_preserves_boolean_expression_precedence_tree() {
    let output = parse_module_as_syntax_output(
        r#"
        module boolean_precedence_tree.

        demo(a: Bool, b: Bool, c: Bool, d: Int, e: Int): Dynamic ->
            a |> inspect() or b and c == d + e.
        "#,
    )
    .expect("syntax output boolean precedence tree");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_preserves_let_expression_tree() {
    let output = parse_module_as_syntax_output(
        r#"
        module let_tree.

        with_body(x: Int): Int ->
            let y = x + 1; let z = y * 2; z + y.

        final_value(x: Int): Int ->
            let y = x + 1; let z = y * 2; z.
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
    assert_eq!(final_value.patterns[1].text.as_deref(), Some("z"));
    assert_eq!(final_value.children.len(), 3);
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
pub(super) fn syntax_output_parses_index_assignment_after_let_binding() {
    let output = parse_module_as_syntax_output(
        r#"
        module let_index_assignment.

        update(source: List[Int]): List[Int] ->
            let values = source; values[1] = 2; values.
        "#,
    )
    .expect("syntax output let index assignment");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_preserves_unary_expression_precedence_tree() {
    let output = parse_module_as_syntax_output(
        r#"
        module unary_precedence_tree.

        demo(ready: Bool, value: Int, scale: Int): Bool ->
            not ready == (-value * scale).
        "#,
    )
    .expect("syntax output unary precedence tree");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_rejects_remote_fun_ref_source_syntax() {
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
pub(super) fn syntax_output_includes_colon_remote_call_trees() {
    let output = parse_module_as_syntax_output(
        r#"
        module remote_call_tree.

        demo(): Dynamic ->
            io_lib:format("~p", []).
        "#,
    )
    .expect("syntax output colon remote call");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_includes_named_call_argument_metadata() {
    let output = parse_module_as_syntax_output(
        r#"
        module named_call_args.

        demo(): Dynamic ->
            create_user(1, "Alice", active = True).
        "#,
    )
    .expect("syntax output named call arguments");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
/// - A module containing `f(10, 20)` in function body position.
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
/// - A module containing `f(10, 20)` in function body position.
///
/// Output:
/// - Test passes when syntax output records a call whose callee child is the
///   value expression `f`, not a remote call or constructor candidate.
///
/// Transformation:
/// - Parses source through `parse_module_as_syntax_output` and inspects the
///   emitted `SyntaxExprKind::Call` children and remote marker.
#[test]
pub(super) fn syntax_output_includes_function_value_invocation_trees() {
    let output = parse_module_as_syntax_output(
        r#"
        module function_value_invocation.

        invoke(f: Dynamic): Dynamic ->
            (f)(10, 20).
        "#,
    )
    .expect("syntax output function-value invocation");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_includes_method_call_suffix_trees() {
    let output = parse_module_as_syntax_output(
        r#"
        module method_call_suffix.

        display(user: Dynamic): Dynamic ->
            user.display_name("short").
        "#,
    )
    .expect("syntax output method call suffix");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_includes_dotted_call_type_args() {
    let output = parse_module_as_syntax_output(
        r#"
        module generic_dotted_call.

        demo(): Dynamic ->
            Vector.new[String]().
        "#,
    )
    .expect("syntax output generic dotted call");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_includes_macro_expr_trees() {
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

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload else {
        panic!("expected function declaration");
    };
    assert_eq!(clauses[0].body.kind, SyntaxExprKind::Macro);
    assert_eq!(clauses[0].body.text.as_deref(), Some("assert_equal"));
    assert_eq!(clauses[0].body.children.len(), 2);
    assert_eq!(clauses[0].body.arity, 2);
}

#[test]
pub(super) fn syntax_output_includes_raw_macro_expr_trees() {
    let output = parse_module_as_syntax_output(
        r#"
        module raw_macro_expr_tree.

        query(): Dynamic ->
            sql{select * from users}.
        "#,
    )
    .expect("syntax output raw macro expr");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_includes_typed_sql_raw_macro_expr_trees() {
    let output = parse_module_as_syntax_output(
        r#"
        module typed_sql_raw_macro_expr_tree.

        query(): Dynamic ->
            sql[UserRow] {select * from users}.
        "#,
    )
    .expect("syntax output typed sql raw macro expr");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_includes_typed_sql_interpolation_children() {
    let output = parse_module_as_syntax_output(
        r#"
        module typed_sql_interpolation_tree.

        query(user: User): Dynamic ->
            sql[UserRow] {select * from users where id = ${user.id}}.
        "#,
    )
    .expect("syntax output typed sql interpolation expr");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
        panic!("expected function declaration");
    };

    let body = &clauses[0].body;
    assert_eq!(body.kind, SyntaxExprKind::RawMacro);
    assert_eq!(body.children.len(), 1);
    assert_eq!(body.children[0].kind, SyntaxExprKind::FieldAccess);
    assert_eq!(body.children[0].text.as_deref(), Some("id"));
}

#[test]
pub(super) fn syntax_output_ignores_typed_sql_comment_interpolation_text() {
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

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
        panic!("expected function declaration");
    };

    let body = &clauses[0].body;
    assert_eq!(body.kind, SyntaxExprKind::RawMacro);
    assert_eq!(body.children.len(), 1);
    assert_eq!(body.children[0].kind, SyntaxExprKind::FieldAccess);
    assert_eq!(body.children[0].text.as_deref(), Some("id"));
}
