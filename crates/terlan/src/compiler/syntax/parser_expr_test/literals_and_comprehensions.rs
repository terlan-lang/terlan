use crate::terlan_syntax::parse_tree::{Decl, Expr, UnaryOp};
use crate::terlan_syntax::{parse_module, parse_terlan_expr};

/// Verifies that `rem` keeps a distinct parse tree operator instead of collapsing
/// into `div`.
///
/// Inputs:
/// - A source expression using the formal `rem` multiplicative operator.
///
/// Output:
/// - Test passes when the parsed expression carries `BinaryOp::Rem`.
///
/// Transformation:
/// - Parses one expression through the recursive-descent parser and
///   inspects the binary operator identity preserved for syntax-output and
///   backend lowering.

/// Verifies that `rem` keeps a distinct parse tree operator instead of collapsing
/// into `div`.
///
/// Inputs:
/// - A source expression using the formal `rem` multiplicative operator.
///
/// Output:
/// - Test passes when the parsed expression carries `BinaryOp::Rem`.
///
/// Transformation:
/// - Parses one expression through the recursive-descent parser and
///   inspects the binary operator identity preserved for syntax-output and
///   backend lowering.
#[test]
pub(super) fn formal_rem_operator_preserves_distinct_binary_op() {
    let expr = parse_terlan_expr("x rem y").expect("parse rem expression");
    let Expr::BinaryOp { op, .. } = expr else {
        panic!("expected rem binary expression");
    };

    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::Rem
    ));
}

#[test]
pub(super) fn formal_keyword_expr_participates_in_pipe_expression() {
    let expr = parse_terlan_expr(
        r#"
        case Option {
          None -> 0;
                Ok(value) -> value
        } |> inspect()
        "#,
    )
    .expect("parse keyword expression in pipe");

    let Expr::BinaryOp { op, left, .. } = expr else {
        panic!("expected pipe expression");
    };
    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::PipeForward
    ));
    let Expr::Case { clauses, .. } = left.as_ref() else {
        panic!("expected case expression as pipe left side");
    };
    assert!(matches!(
        &clauses[0].pattern,
        crate::terlan_syntax::parse_tree::Pattern::Tuple(items)
            if matches!(items.as_slice(), [crate::terlan_syntax::parse_tree::Pattern::Atom(name)] if name == "None")
    ));
    assert!(matches!(
        &clauses[1].pattern,
        crate::terlan_syntax::parse_tree::Pattern::Tuple(items)
            if matches!(items.as_slice(), [crate::terlan_syntax::parse_tree::Pattern::Atom(name), crate::terlan_syntax::parse_tree::Pattern::Var(var)] if name == "Ok" && var == "value")
    ));
}

#[test]
pub(super) fn formal_cons_list_expr_is_distinct_from_generator_expr() {
    let cons = parse_terlan_expr("[Head | Tail]").expect("parse cons list expression");
    assert!(matches!(cons, Expr::ListCons(_, _)));

    let generator = parse_terlan_expr("[Item | Item <- Items]").expect("parse generator");
    assert!(matches!(generator, Expr::ListComprehension { .. }));
}

/// Verifies canonical atom literals are expression syntax.
///
/// Inputs:
/// - A standalone `Atom["ready"]` expression.
///
/// Output:
/// - Parsed `Expr::AtomLiteral` with the unescaped payload.
///
/// Transformation:
/// - Confirms the parser treats the language-neutral atom form as a value
///   expression instead of as a generic type-argument call head.
#[test]
pub(super) fn formal_atom_literal_expr_syntax_parses_canonical_atom_values() {
    let expr = parse_terlan_expr(r#"Atom["ready"]"#).expect("parse atom literal expression");
    assert!(matches!(expr, Expr::AtomLiteral(value) if value == "ready"));
}

#[test]
pub(super) fn formal_atom_literal_expr_syntax_preserves_atoms_inside_tuples() {
    let expr = parse_terlan_expr(r#"{Atom["ok"], 7}"#).expect("parse tuple atom literal");
    let Expr::Tuple(items) = expr else {
        panic!("expected tuple expression");
    };
    assert!(matches!(
        items.as_slice(),
        [Expr::AtomLiteral(value), Expr::Int(7)] if value == "ok"
    ));
}

/// Verifies legacy atom syntax remains a source-only compatibility alias.
#[test]
pub(super) fn formal_atom_literal_expr_syntax_accepts_legacy_aliases() {
    for (source, expected) in [(":ready", "ready"), (":'interop-ready'", "interop-ready")] {
        let expr = parse_terlan_expr(source).expect("parse legacy atom compatibility alias");
        assert!(matches!(expr, Expr::AtomLiteral(value) if value == expected));
    }
}

/// Verifies canonical atom literal expressions decode string escapes.
///
/// Inputs:
/// - A standalone `Atom["..."]` expression containing quote, backslash,
///   newline, carriage return, and tab escapes.
///
/// Output:
/// - Parsed `Expr::AtomLiteral` with the decoded payload.
///
/// Transformation:
/// - Exercises the same atom string payload decoder used by canonical atom
///   value expressions before type checking or backend lowering.
#[test]
pub(super) fn formal_atom_literal_expr_syntax_decodes_escaped_atom_values() {
    let expr = parse_terlan_expr(r#"Atom["quote \" slash \\ newline \n carriage \r tab \t"]"#)
        .expect("parse escaped atom literal expression");

    assert!(matches!(
        expr,
        Expr::AtomLiteral(value)
            if value == "quote \" slash \\ newline \n carriage \r tab \t"
    ));
}

/// Verifies canonical atom literal expressions reject empty payloads.
///
/// Inputs:
/// - A standalone `Atom[""]` expression.
///
/// Output:
/// - Stable parser diagnostic requiring a non-empty atom payload.
///
/// Transformation:
/// - Keeps the parser from constructing empty singleton atom values that
///   later phases cannot meaningfully type or emit.
#[test]
pub(super) fn formal_atom_literal_expr_syntax_rejects_empty_atom_values() {
    let error = parse_terlan_expr(r#"Atom[""]"#).expect_err("empty atom literal should fail");

    assert!(error
        .message
        .contains("expected non-empty atom string literal"));
}

/// Verifies canonical atom literal expressions reject dynamic payloads.
///
/// Inputs:
/// - A source expression attempting `Atom[name]`.
///
/// Output:
/// - Stable parser diagnostic requiring a string literal payload.
///
/// Transformation:
/// - Keeps atom construction finite and compiler-known by rejecting
///   variable, call, or computed payloads at parse time.
#[test]
pub(super) fn formal_atom_literal_expr_syntax_rejects_dynamic_atom_values() {
    let error = parse_terlan_expr("Atom[name]").expect_err("dynamic atom literal should fail");

    assert!(error.message.contains("expected String"));
}

#[test]
pub(super) fn formal_list_comprehension_preserves_stacked_filters_in_source_order() {
    let expr = parse_terlan_expr("[Item | Item <- Items, Item > 0, Item < 10]")
        .expect("stacked list comprehension filters should parse");

    let Expr::ListComprehension { guards, .. } = expr else {
        panic!("expected guarded list comprehension");
    };
    assert_eq!(guards.len(), 2);
    assert!(matches!(
        &guards[0],
        Expr::BinaryOp {
            op: crate::terlan_syntax::parse_tree::BinaryOp::Gt,
            ..
        }
    ));
    assert!(matches!(
        &guards[1],
        Expr::BinaryOp {
            op: crate::terlan_syntax::parse_tree::BinaryOp::Lt,
            ..
        }
    ));
}

/// Verifies list comprehensions reject `where` filters clearly.
///
/// Inputs:
/// - A list comprehension that attempts to introduce a filter with
///   `where` after the generator.
///
/// Output:
/// - Stable parser diagnostic explaining that comprehension filters use
///   comma-separated expressions.
///
/// Transformation:
/// - Keeps comprehension guard syntax distinct from pattern and clause
///   guard syntax, where `where` separates a pattern from a condition.
#[test]
pub(super) fn formal_list_comprehension_rejects_where_filter_spelling() {
    let err = parse_terlan_expr("[Item | Item <- Items where Item > 0]")
        .expect_err("where filters should be rejected in list comprehensions");

    assert!(
        err.message
            .contains("list comprehension filters use comma-separated boolean expressions"),
        "unexpected error: {}",
        err.message
    );
}

/// Verifies collection expressions accepted by the A0.24 syntax baseline.
///
/// Inputs:
/// - Source expressions for list, cons-list, generator, fixed-array, and
///   map forms.
///
/// Output:
/// - Test passes when each expression maps to its dedicated syntax-output
///   variant.
///
/// Transformation:
/// - Parses each expression through the recursive-descent parser and
///   inspects the collection-specific parse tree shape.

/// Verifies collection expressions accepted by the A0.24 syntax baseline.
///
/// Inputs:
/// - Source expressions for list, cons-list, generator, fixed-array, and
///   map forms.
///
/// Output:
/// - Test passes when each expression maps to its dedicated syntax-output
///   variant.
///
/// Transformation:
/// - Parses each expression through the recursive-descent parser and
///   inspects the collection-specific parse tree shape.
#[test]
pub(super) fn formal_collection_exprs_preserve_ast_shapes() {
    let list = parse_terlan_expr("[1, 2, 3]").expect("parse list expression");
    assert!(matches!(list, Expr::List(items) if items.len() == 3));

    let cons = parse_terlan_expr("[Head | Tail]").expect("parse cons list expression");
    assert!(matches!(cons, Expr::ListCons(_, _)));

    let generator = parse_terlan_expr("[Item * 2 | Item <- Items]").expect("parse list generator");
    assert!(matches!(
        generator,
        Expr::ListComprehension { guards, .. } if guards.is_empty()
    ));

    let fixed = parse_terlan_expr("#[255, 128, 0]").expect("parse fixed array");
    assert!(matches!(fixed, Expr::FixedArray(items) if items.len() == 3));

    let map = parse_terlan_expr("{name: \"Ada\", age: 42}").expect("parse map");
    let Expr::Map(fields) = map else {
        panic!("expected map expression");
    };
    assert_eq!(fields.len(), 2);
    assert!(fields[0].required);
    assert!(fields[1].required);
}

/// Verifies Vm binary segment syntax is rejected by the syntax parser.
///
/// Inputs:
/// - An Vm binary literal containing size and segment-type annotations.
///
/// Output:
/// - Test passes when the parser rejects the source-level Vm syntax.
///
/// Transformation:
/// - Keeps backend Vm binary syntax out of canonical Terlan source.

/// Verifies Vm binary segment syntax is rejected by the syntax parser.
///
/// Inputs:
/// - An Vm binary literal containing size and segment-type annotations.
///
/// Output:
/// - Test passes when the parser rejects the source-level Vm syntax.
///
/// Transformation:
/// - Keeps backend Vm binary syntax out of canonical Terlan source.
#[test]
pub(super) fn formal_binary_segments_are_rejected_as_erlang_source_syntax() {
    let error = parse_terlan_expr("<<head:16/big-unsigned-integer, tail/binary>>")
        .expect_err("Vm binary segment literal should be rejected");

    assert!(error.message.contains("Vm binary literal syntax"));
}

/// Verifies process-message receive syntax is not canonical Terlan source.
///
/// Inputs:
/// - A source expression using the removed `receive { ... }` shape.
///
/// Output:
/// - Test passes when expression parsing rejects the source.
///
/// Transformation:
/// - Parses the removed VM-shaped syntax through the normal expression
///   parser and confirms it does not produce a Terlan expression node.

/// Verifies process-message receive syntax is not canonical Terlan source.
///
/// Inputs:
/// - A source expression using the removed `receive { ... }` shape.
///
/// Output:
/// - Test passes when expression parsing rejects the source.
///
/// Transformation:
/// - Parses the removed VM-shaped syntax through the normal expression
///   parser and confirms it does not produce a Terlan expression node.
#[test]
pub(super) fn formal_receive_expr_is_not_canonical_source_syntax() {
    let err = parse_terlan_expr(
        r#"
        receive {
            value -> value
        }
        "#,
    )
    .expect_err("receive expression syntax must be rejected");

    assert!(err.message.contains("unexpected token") || err.message.contains("expected"));
}

#[test]
pub(super) fn formal_try_expr_parses_of_and_catch_clauses() {
    let expr = parse_terlan_expr(
        r#"
        try risky() {
            {Atom["ok"], value} -> value
        catch
            Atom["error"] -> 0
        } |> inspect()
        "#,
    )
    .expect("parse try expression in pipe");

    let Expr::BinaryOp { op, left, .. } = expr else {
        panic!("expected pipe expression");
    };
    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::PipeForward
    ));
    let Expr::Try {
        of_clauses,
        catch_clauses,
        ..
    } = left.as_ref()
    else {
        panic!("expected try expression as pipe left side");
    };
    assert_eq!(of_clauses.len(), 1);
    assert_eq!(catch_clauses.len(), 1);
}

#[test]
pub(super) fn formal_try_expr_parses_after_clause() {
    let expr = parse_terlan_expr(
        r#"
        try risky() {
            after
            0 -> cleanup()
        } |> inspect()
        "#,
    )
    .expect("parse try after expression in pipe");

    let Expr::BinaryOp { op, left, .. } = expr else {
        panic!("expected pipe expression");
    };
    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::PipeForward
    ));
    let Expr::Try { after_clause, .. } = left.as_ref() else {
        panic!("expected try expression as pipe left side");
    };
    let after_clause = after_clause.as_ref().expect("expected try after clause");
    assert!(matches!(after_clause.trigger.as_ref(), Expr::Int(0)));
    assert!(matches!(
        after_clause.body.as_ref(),
        Expr::Call { remote: None, .. }
    ));
}

/// Verifies guarded clauses in keyword expressions.
///
/// Inputs:
/// - A module containing guarded `case` and `try` clauses.
///
/// Output:
/// - Test passes when each keyword expression preserves a guard expression
///   on its first clause.
///
/// Transformation:
/// - Parses a module through the recursive-descent parser, locates the
///   function bodies, and inspects the keyword-expression clause guards.

/// Verifies guarded clauses in keyword expressions.
///
/// Inputs:
/// - A module containing guarded `case` and `try` clauses.
///
/// Output:
/// - Test passes when each keyword expression preserves a guard expression
///   on its first clause.
///
/// Transformation:
/// - Parses a module through the recursive-descent parser, locates the
///   function bodies, and inspects the keyword-expression clause guards.
#[test]
pub(super) fn formal_keyword_exprs_preserve_clause_guards() {
    let module = parse_module(
        r#"
        module keyword_guards.

        guarded_case(value: Int): Int ->
          case value {
            n where n > 0 -> n;
            _ -> 0
          }.

        guarded_try(): Int ->
          try risky() {
            value where value > 0 -> value;
            _ -> 0
          catch
            reason where reason != Atom["fatal"] -> 0;
            _ -> -1
          }.
        "#,
    )
    .expect("parse guarded keyword expressions");

    let Decl::Function(case_function) = &module.declarations[0] else {
        panic!("expected case function");
    };
    let Expr::Case { clauses, .. } = &case_function.clauses[0].body else {
        panic!("expected case expression");
    };
    assert!(clauses[0].guard.is_some());

    let Decl::Function(try_function) = &module.declarations[1] else {
        panic!("expected try function");
    };
    let Expr::Try {
        of_clauses,
        catch_clauses,
        ..
    } = &try_function.clauses[0].body
    else {
        panic!("expected try expression");
    };
    assert!(of_clauses[0].guard.is_some());
    assert!(catch_clauses[0].guard.is_some());
}

/// Verifies legacy `when` guard spelling is rejected.
///
/// Inputs:
/// - A module containing a `case` clause guarded with legacy `when`.
///
/// Output:
/// - Test passes when parsing rejects `when` with a stable diagnostic.
///
/// Transformation:
/// - Locks the single Terlan guard spelling so `where` stays the only
///   source-level guard introducer.
#[test]
pub(super) fn formal_keyword_exprs_reject_when_clause_guards() {
    let err = parse_module(
        r#"
        module keyword_when_guard_rejected.

        guarded_case(value: Int): Int ->
          case value {
            n when n > 0 -> n;
            _ -> 0
          }.
        "#,
    )
    .expect_err("when guards should be rejected");
    assert!(
        err.message.contains("Terlan clause guards use `where`")
            && err.message.contains("`when` is not supported"),
        "unexpected diagnostic: {:?}",
        err
    );
}

/// Verifies quote and unquote participate in formal keyword-expression
/// coverage.
///
/// Inputs:
/// - A source expression using `quote unquote(value)`.
///
/// Output:
/// - Test passes when parsing preserves `Expr::Quote(Expr::Unquote(_))`.
///
/// Transformation:
/// - Parses one expression through the recursive-descent parser and checks
///   the exact nested keyword-expression parse tree shape.

/// Verifies quote and unquote participate in formal keyword-expression
/// coverage.
///
/// Inputs:
/// - A source expression using `quote unquote(value)`.
///
/// Output:
/// - Test passes when parsing preserves `Expr::Quote(Expr::Unquote(_))`.
///
/// Transformation:
/// - Parses one expression through the recursive-descent parser and checks
///   the exact nested keyword-expression parse tree shape.
#[test]
pub(super) fn formal_quote_unquote_exprs_parse_as_keyword_expressions() {
    let expr = parse_terlan_expr("quote unquote(value)").expect("parse quote/unquote");

    let Expr::Quote(inner) = expr else {
        panic!("expected quote expression");
    };
    assert!(matches!(inner.as_ref(), Expr::Unquote(_)));
}

/// Verifies receiver method-call suffixes parse before field suffixes.
///
/// Inputs:
/// - Expression source using `user.display_name("short")`.
///
/// Output:
/// - Test passes when the expression is a call whose callee is a field-access
///   expression.
///
/// Transformation:
/// - Parses the canonical method-call postfix syntax and validates the parse tree
///   shape used by later receiver-method resolution.

/// Verifies receiver method-call suffixes parse before field suffixes.
///
/// Inputs:
/// - Expression source using `user.display_name("short")`.
///
/// Output:
/// - Test passes when the expression is a call whose callee is a field-access
///   expression.
///
/// Transformation:
/// - Parses the canonical method-call postfix syntax and validates the parse tree
///   shape used by later receiver-method resolution.
#[test]
pub(super) fn formal_method_call_suffix_parses_before_field_access() {
    let expr = parse_terlan_expr(r#"user.display_name("short")"#)
        .expect("parse receiver method call suffix");
    let Expr::Call {
        callee,
        args,
        is_fun_value,
        ..
    } = expr
    else {
        panic!("expected method call expression");
    };
    assert!(!is_fun_value);
    assert_eq!(args.len(), 1);
    let Expr::FieldAccess { value, field } = callee.as_ref() else {
        panic!("expected field-access callee");
    };
    assert_eq!(field, "display_name");
    assert!(matches!(value.as_ref(), Expr::Var(name) if name == "user"));
}

/// Verifies method-call suffixes can follow call results.
///
/// Inputs:
/// - Expression source using `Router.new().get("/", home)`.
///
/// Output:
/// - Test passes when the outer expression is a receiver-method call whose
///   receiver is the inner `Router.new()` call.
///
/// Transformation:
/// - Exercises the postfix suffix loop after uppercase dotted calls so
///   router-builder chains remain normal expression syntax.
#[test]
pub(super) fn formal_method_call_suffix_parses_after_call_result() {
    let expr = parse_terlan_expr(r#"Router.new().get("/", home)"#)
        .expect("parse call-result method call suffix");
    let Expr::Call {
        callee,
        args,
        is_fun_value,
        ..
    } = expr
    else {
        panic!("expected outer method call expression");
    };
    assert!(!is_fun_value);
    assert_eq!(args.len(), 2);
    let Expr::FieldAccess { value, field } = callee.as_ref() else {
        panic!("expected outer field-access callee");
    };
    assert_eq!(field, "get");
    let Expr::Call {
        callee: inner_callee,
        args: inner_args,
        remote: inner_remote,
        ..
    } = value.as_ref()
    else {
        panic!("expected call-result receiver");
    };
    assert!(inner_args.is_empty());
    assert_eq!(inner_remote.as_deref(), Some("Router"));
    assert!(matches!(inner_callee.as_ref(), Expr::Atom(name) if name == "new"));
}

/// Verifies grouped callable expressions use ordinary call syntax.
///
/// Inputs:
/// - Expression source using `(make_reducer())(acc, value)`.
///
/// Output:
/// - Test passes when the outer expression is a function-value call whose
///   callee is the grouped call result.
///
/// Transformation:
/// - Locks the 0.0.7 callable syntax cleanup rule that returned callables
///   are invoked by grouping the callee expression and applying normal
///   call syntax instead of using the removed dot-call operator.
#[test]
pub(super) fn formal_grouped_callable_expression_uses_normal_call_suffix() {
    let expr = parse_terlan_expr("(make_reducer())(acc, value)")
        .expect("parse grouped callable expression call");
    let Expr::Call {
        callee,
        args,
        is_fun_value,
        ..
    } = expr
    else {
        panic!("expected function-value call expression");
    };

    assert!(is_fun_value);
    assert_eq!(args.len(), 2);
    let Expr::Call {
        callee: inner_callee,
        args: inner_args,
        is_fun_value: inner_is_fun_value,
        ..
    } = callee.as_ref()
    else {
        panic!("expected grouped call result callee");
    };
    assert!(!inner_is_fun_value);
    assert!(inner_args.is_empty());
    assert!(matches!(inner_callee.as_ref(), Expr::Var(name) if name == "make_reducer"));
}

/// Verifies generic type arguments on dotted calls parse before call args.
///
/// Inputs:
/// - Expression source using `Vector.new[String]()`.
///
/// Output:
/// - Test passes when the call is remote `Vector.new` with one explicit
///   call type argument.
///
/// Transformation:
/// - Exercises the dotted-call parser path that must consume `[String]`
///   as call metadata rather than mistaking it for an index expression.
#[test]
pub(super) fn formal_dotted_call_preserves_explicit_type_args() {
    let expr = parse_terlan_expr("Vector.new[String]()").expect("parse generic dotted call");
    let Expr::Call {
        callee,
        type_args,
        args,
        remote,
        is_fun_value,
        ..
    } = expr
    else {
        panic!("expected generic dotted call");
    };

    assert!(!is_fun_value);
    assert!(args.is_empty());
    assert_eq!(remote.as_deref(), Some("Vector"));
    assert_eq!(type_args.len(), 1);
    assert_eq!(type_args[0].text, "String");
    assert!(matches!(callee.as_ref(), Expr::Atom(name) if name == "new"));
}

/// Verifies generic type arguments on bare calls parse before call args.
///
/// Inputs:
/// - Expression source using `identity[Option, Int](value)`.
///
/// Output:
/// - Test passes when the call is local and carries both explicit call type
///   arguments.
///
/// Transformation:
/// - Exercises the bare-call parser path so local HKT helpers can accept
///   constructor and value type arguments without being parsed as index
///   syntax.
#[test]
pub(super) fn formal_bare_call_preserves_multiple_explicit_type_args() {
    let expr = parse_terlan_expr("identity[Option, Int](value)").expect("parse generic bare call");
    let Expr::Call {
        callee,
        type_args,
        args,
        remote,
        is_fun_value,
        ..
    } = expr
    else {
        panic!("expected generic bare call");
    };

    assert!(!is_fun_value);
    assert_eq!(args.len(), 1);
    assert!(remote.is_none());
    assert_eq!(type_args.len(), 2);
    assert_eq!(type_args[0].text, "Option");
    assert_eq!(type_args[1].text, "Int");
    assert!(matches!(callee.as_ref(), Expr::Var(name) if name == "identity"));
}

/// Verifies named call-site arguments are preserved in source order.
///
/// Inputs:
/// - A call with two positional arguments followed by one named argument.
///
/// Output:
/// - Test passes when argument expressions and their optional names remain
///   parallel in the parsed call expression.
///
/// Transformation:
/// - Parses `name = expr` only in call-argument position without changing
///   the positional expression list consumed by downstream phases.
#[test]
pub(super) fn formal_named_call_arguments_preserve_parallel_names() {
    let expr = parse_terlan_expr(r#"create_user(1, "Alice", active = True)"#)
        .expect("parse named call arguments");
    let Expr::Call {
        args, arg_names, ..
    } = expr
    else {
        panic!("expected call expression");
    };

    assert_eq!(args.len(), 3);
    assert_eq!(arg_names, vec![None, None, Some("active".to_string())]);
}

/// Verifies named arguments close the positional call-argument segment.
///
/// Inputs:
/// - A call with a positional argument after a named argument.
///
/// Output:
/// - Test passes when parsing fails with a stable ordering diagnostic.
///
/// Transformation:
/// - Rejects ambiguous call-site ordering before semantic name resolution
///   or default-argument lowering runs.
#[test]
pub(super) fn formal_named_call_arguments_reject_positional_after_named() {
    let err = parse_terlan_expr(r#"create_user(active = True, "Alice")"#)
        .expect_err("reject positional argument after named argument");

    assert!(err
        .message
        .contains("positional arguments must come before named arguments"));
}

#[test]
pub(super) fn formal_unary_expr_preserves_precedence() {
    let expr = parse_terlan_expr("not Ready == false").expect("parse unary not precedence");
    let Expr::BinaryOp { op, left, .. } = expr else {
        panic!("expected comparison expression");
    };
    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::EqEq
    ));
    assert!(matches!(
        left.as_ref(),
        Expr::UnaryOp {
            op: UnaryOp::Not,
            ..
        }
    ));

    let expr = parse_terlan_expr("-A * B").expect("parse unary neg precedence");
    let Expr::BinaryOp { op, left, .. } = expr else {
        panic!("expected multiply expression");
    };
    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::Mul
    ));
    assert!(matches!(
        left.as_ref(),
        Expr::UnaryOp {
            op: UnaryOp::Neg,
            ..
        }
    ));
}

#[test]
pub(super) fn formal_remote_call_expr_parses_colon_syntax() {
    let expr = parse_terlan_expr("io_lib:format(\"~p\", []) |> inspect()")
        .expect("parse colon remote call in pipe");

    let Expr::BinaryOp { op, left, .. } = expr else {
        panic!("expected pipe expression");
    };
    assert!(matches!(
        op,
        crate::terlan_syntax::parse_tree::BinaryOp::PipeForward
    ));
    let Expr::Call {
        callee,
        remote,
        args,
        is_fun_value: _,
        ..
    } = left.as_ref()
    else {
        panic!("expected remote call expression as pipe left side");
    };
    assert_eq!(remote.as_deref(), Some("io_lib"));
    assert!(matches!(callee.as_ref(), Expr::Atom(name) if name == "format"));
    assert_eq!(args.len(), 2);
}
