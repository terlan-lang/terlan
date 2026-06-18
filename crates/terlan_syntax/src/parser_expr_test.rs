#[cfg(test)]
mod tests {
    use crate::parse_tree::{Decl, Expr, UnaryOp};
    use crate::{parse_module, parse_terlan_expr};

    #[test]
    fn formal_expr_precedence_keeps_pipe_below_boolean_chain() {
        let expr = parse_terlan_expr("A |> B + C * D or Ready").expect("parse formal precedence");
        let Expr::BinaryOp { op, right, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::parse_tree::BinaryOp::PipeForward));

        let Expr::BinaryOp { op, left, .. } = right.as_ref() else {
            panic!("expected or expression on pipe right side");
        };
        assert!(matches!(op, crate::parse_tree::BinaryOp::Or));

        let Expr::BinaryOp { op, right, .. } = left.as_ref() else {
            panic!("expected additive expression on or left side");
        };
        assert!(matches!(op, crate::parse_tree::BinaryOp::Add));
        assert!(matches!(
            right.as_ref(),
            Expr::BinaryOp {
                op: crate::parse_tree::BinaryOp::Mul,
                ..
            }
        ));
    }

    /// Verifies the boolean precedence chain introduced by the canonical EBNF.
    ///
    /// Inputs:
    /// - A source expression containing pipe, `or`, `and`, comparison, and
    ///   arithmetic operators.
    ///
    /// Output:
    /// - Test passes when parsing preserves `pipe < or < and < cmp`.
    ///
    /// Transformation:
    /// - Parses one expression through the recursive-descent parser and
    ///   inspects the nested binary operator tree.

    /// Verifies the boolean precedence chain introduced by the canonical EBNF.
    ///
    /// Inputs:
    /// - A source expression containing pipe, `or`, `and`, comparison, and
    ///   arithmetic operators.
    ///
    /// Output:
    /// - Test passes when parsing preserves `pipe < or < and < cmp`.
    ///
    /// Transformation:
    /// - Parses one expression through the recursive-descent parser and
    ///   inspects the nested binary operator tree.
    #[test]
    fn formal_boolean_operators_preserve_ebnf_precedence() {
        let expr =
            parse_terlan_expr("A |> C or D and E == F + G").expect("parse boolean precedence");
        let Expr::BinaryOp { op, right, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::parse_tree::BinaryOp::PipeForward));

        let Expr::BinaryOp { op, right, .. } = right.as_ref() else {
            panic!("expected or expression on pipe right side");
        };
        assert!(matches!(op, crate::parse_tree::BinaryOp::Or));

        let Expr::BinaryOp { op, right, .. } = right.as_ref() else {
            panic!("expected and expression on or right side");
        };
        assert!(matches!(op, crate::parse_tree::BinaryOp::And));

        let Expr::BinaryOp { op, right, .. } = right.as_ref() else {
            panic!("expected comparison expression on and right side");
        };
        assert!(matches!(op, crate::parse_tree::BinaryOp::EqEq));
        assert!(matches!(
            right.as_ref(),
            Expr::BinaryOp {
                op: crate::parse_tree::BinaryOp::Add,
                ..
            }
        ));
    }

    /// Verifies explicit cast syntax follows the canonical precedence chain.
    ///
    /// Inputs:
    /// - Expressions containing `as`, multiplication, pipe, and keyword forms.
    ///
    /// Output:
    /// - Test passes when `Cast` binds above multiplication and below postfix
    ///   primary parsing, including keyword expressions.
    ///
    /// Transformation:
    /// - Parses representative expressions and inspects the preserved syntax
    ///   tree instead of resolving the conversion semantically.

    /// Verifies explicit cast syntax follows the canonical precedence chain.
    ///
    /// Inputs:
    /// - Expressions containing `as`, multiplication, pipe, and keyword forms.
    ///
    /// Output:
    /// - Test passes when `Cast` binds above multiplication and below postfix
    ///   primary parsing, including keyword expressions.
    ///
    /// Transformation:
    /// - Parses representative expressions and inspects the preserved syntax
    ///   tree instead of resolving the conversion semantically.
    #[test]
    fn formal_cast_expr_preserves_ebnf_precedence() {
        let expr = parse_terlan_expr("Value as Int * Count").expect("parse cast before multiply");
        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected multiplication expression");
        };
        assert!(matches!(op, crate::parse_tree::BinaryOp::Mul));
        assert!(matches!(
            left.as_ref(),
            Expr::Cast {
                target_type,
                ..
            } if target_type.text == "Int"
        ));

        let expr =
            parse_terlan_expr("case Option { :none -> 0; value -> value } as Int |> inspect()")
                .expect("parse casted keyword expression before pipe");
        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::parse_tree::BinaryOp::PipeForward));
        let Expr::Cast { expr, target_type } = left.as_ref() else {
            panic!("expected cast expression on pipe left side");
        };
        assert_eq!(target_type.text, "Int");
        assert!(matches!(expr.as_ref(), Expr::Case { .. }));
    }

    /// Verifies that canonical Terlan source rejects backend-style equality
    /// spellings.
    ///
    /// Inputs:
    /// - Three source expressions using deprecated equality spellings.
    ///
    /// Output:
    /// - Test passes when all deprecated spellings fail parsing.
    ///
    /// Transformation:
    /// - Parses each expression through the recursive-descent parser and
    ///   asserts the comparison operator guard fires before syntax output is
    ///   accepted.

    /// Verifies that canonical Terlan source rejects backend-style equality
    /// spellings.
    ///
    /// Inputs:
    /// - Three source expressions using deprecated equality spellings.
    ///
    /// Output:
    /// - Test passes when all deprecated spellings fail parsing.
    ///
    /// Transformation:
    /// - Parses each expression through the recursive-descent parser and
    ///   asserts the comparison operator guard fires before syntax output is
    ///   accepted.
    #[test]
    fn formal_deprecated_equality_operators_are_rejected() {
        for operator in ["=:=", "/=", "=/="] {
            let source = format!("left {operator} right");
            let error = parse_terlan_expr(&source)
                .err()
                .expect("deprecated equality spelling should fail");

            assert!(
                error.message.contains("deprecated"),
                "unexpected diagnostic for {operator}: {}",
                error.message
            );
        }
    }

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
    fn formal_rem_operator_preserves_distinct_binary_op() {
        let expr = parse_terlan_expr("x rem y").expect("parse rem expression");
        let Expr::BinaryOp { op, .. } = expr else {
            panic!("expected rem binary expression");
        };

        assert!(matches!(op, crate::parse_tree::BinaryOp::Rem));
    }

    #[test]
    fn formal_keyword_expr_participates_in_pipe_expression() {
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
        assert!(matches!(op, crate::parse_tree::BinaryOp::PipeForward));
        let Expr::Case { clauses, .. } = left.as_ref() else {
            panic!("expected case expression as pipe left side");
        };
        assert!(matches!(
            &clauses[0].pattern,
            crate::parse_tree::Pattern::Tuple(items)
                if matches!(items.as_slice(), [crate::parse_tree::Pattern::Atom(name)] if name == "None")
        ));
        assert!(matches!(
            &clauses[1].pattern,
            crate::parse_tree::Pattern::Tuple(items)
                if matches!(items.as_slice(), [crate::parse_tree::Pattern::Atom(name), crate::parse_tree::Pattern::Var(var)] if name == "Ok" && var == "value")
        ));
    }

    #[test]
    fn formal_cons_list_expr_is_distinct_from_generator_expr() {
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
    fn formal_atom_literal_expr_syntax_parses_canonical_atom_values() {
        let expr = parse_terlan_expr(r#"Atom["ready"]"#).expect("parse atom literal expression");
        assert!(matches!(expr, Expr::AtomLiteral(value) if value == "ready"));
    }

    #[test]
    fn formal_list_comprehension_rejects_unrepresented_extra_generators() {
        let err = parse_terlan_expr("[Item | Item <- Items, Other <- Others]")
            .err()
            .expect("multiple generators should be rejected");

        assert!(
            err.message
                .contains("multiple list comprehension generators are not supported"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn formal_list_comprehension_accepts_stacked_filters_as_guard() {
        let expr = parse_terlan_expr("[Item | Item <- Items, Item > 0, Item < 10]")
            .expect("stacked list comprehension filters should parse");

        let Expr::ListComprehension {
            guard: Some(guard), ..
        } = expr
        else {
            panic!("expected guarded list comprehension");
        };
        let Expr::BinaryOp {
            op, left, right, ..
        } = guard.as_ref()
        else {
            panic!("expected combined filter guard");
        };
        assert!(matches!(op, crate::parse_tree::BinaryOp::And));
        assert!(matches!(
            left.as_ref(),
            Expr::BinaryOp {
                op: crate::parse_tree::BinaryOp::Gt,
                ..
            }
        ));
        assert!(matches!(
            right.as_ref(),
            Expr::BinaryOp {
                op: crate::parse_tree::BinaryOp::Lt,
                ..
            }
        ));
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
    fn formal_collection_exprs_preserve_ast_shapes() {
        let list = parse_terlan_expr("[1, 2, 3]").expect("parse list expression");
        assert!(matches!(list, Expr::List(items) if items.len() == 3));

        let cons = parse_terlan_expr("[Head | Tail]").expect("parse cons list expression");
        assert!(matches!(cons, Expr::ListCons(_, _)));

        let generator =
            parse_terlan_expr("[Item * 2 | Item <- Items]").expect("parse list generator");
        assert!(matches!(
            generator,
            Expr::ListComprehension { guard: None, .. }
        ));

        let fixed = parse_terlan_expr("#[255, 128, 0]").expect("parse fixed array");
        assert!(matches!(fixed, Expr::FixedArray(items) if items.len() == 3));

        let map = parse_terlan_expr("#{name := \"Ada\", age => 42}").expect("parse map");
        let Expr::Map(fields) = map else {
            panic!("expected map expression");
        };
        assert_eq!(fields.len(), 2);
        assert!(fields[0].required);
        assert!(!fields[1].required);
    }

    /// Verifies binary segment syntax is preserved by the syntax parser.
    ///
    /// Inputs:
    /// - A binary literal containing size and segment-type annotations.
    ///
    /// Output:
    /// - Test passes when the parser preserves the full binary literal text.
    ///
    /// Transformation:
    /// - Parses the binary literal as an expression and checks that semantic
    ///   segment lowering remains deferred beyond the syntax phase.

    /// Verifies binary segment syntax is preserved by the syntax parser.
    ///
    /// Inputs:
    /// - A binary literal containing size and segment-type annotations.
    ///
    /// Output:
    /// - Test passes when the parser preserves the full binary literal text.
    ///
    /// Transformation:
    /// - Parses the binary literal as an expression and checks that semantic
    ///   segment lowering remains deferred beyond the syntax phase.
    #[test]
    fn formal_binary_segments_are_preserved_as_binary_literal_text() {
        let expr = parse_terlan_expr("<<head:16/big-unsigned-integer, tail/binary>>")
            .expect("parse binary segment literal");

        let Expr::Binary(text) = expr else {
            panic!("expected binary literal");
        };
        assert!(text.contains("head:16/big-unsigned-integer"));
        assert!(text.contains("tail/binary"));
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
    /// - Parses the removed BEAM-shaped syntax through the normal expression
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
    /// - Parses the removed BEAM-shaped syntax through the normal expression
    ///   parser and confirms it does not produce a Terlan expression node.
    #[test]
    fn formal_receive_expr_is_not_canonical_source_syntax() {
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
    fn formal_try_expr_parses_of_and_catch_clauses() {
        let expr = parse_terlan_expr(
            r#"
            try risky() {
                {:ok, value} -> value
            catch
                :error -> 0
            } |> inspect()
            "#,
        )
        .expect("parse try expression in pipe");

        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::parse_tree::BinaryOp::PipeForward));
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
    fn formal_try_expr_parses_after_clause() {
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
        assert!(matches!(op, crate::parse_tree::BinaryOp::PipeForward));
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
    fn formal_keyword_exprs_preserve_clause_guards() {
        let module = parse_module(
            r#"
            module keyword_guards.

            guarded_case(value: Int): Int ->
              case value {
                n when n > 0 -> n;
                _ -> 0
              }.

            guarded_try(): Int ->
              try risky() {
                value when value > 0 -> value;
                _ -> 0
              catch
                reason when reason != :fatal -> 0;
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
    fn formal_quote_unquote_exprs_parse_as_keyword_expressions() {
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
    fn formal_method_call_suffix_parses_before_field_access() {
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

    #[test]
    fn formal_unary_expr_preserves_precedence() {
        let expr = parse_terlan_expr("not Ready == false").expect("parse unary not precedence");
        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected comparison expression");
        };
        assert!(matches!(op, crate::parse_tree::BinaryOp::EqEq));
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
        assert!(matches!(op, crate::parse_tree::BinaryOp::Mul));
        assert!(matches!(
            left.as_ref(),
            Expr::UnaryOp {
                op: UnaryOp::Neg,
                ..
            }
        ));
    }

    #[test]
    fn formal_remote_call_expr_parses_colon_syntax() {
        let expr = parse_terlan_expr("io_lib:format(\"~p\", []) |> inspect()")
            .expect("parse colon remote call in pipe");

        let Expr::BinaryOp { op, left, .. } = expr else {
            panic!("expected pipe expression");
        };
        assert!(matches!(op, crate::parse_tree::BinaryOp::PipeForward));
        let Expr::Call {
            callee,
            remote,
            args,
            is_fun_value: _,
        } = left.as_ref()
        else {
            panic!("expected remote call expression as pipe left side");
        };
        assert_eq!(remote.as_deref(), Some("io_lib"));
        assert!(matches!(callee.as_ref(), Expr::Atom(name) if name == "format"));
        assert_eq!(args.len(), 2);
    }

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
        assert!(matches!(op, crate::parse_tree::BinaryOp::PipeForward));
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
        assert!(matches!(op, crate::parse_tree::BinaryOp::PipeForward));
        assert!(matches!(
            left.as_ref(),
            Expr::RawMacro { name, raw } if name == "sql" && raw == "select * from users"
        ));

        let spaced = parse_terlan_expr("sql {select * from users}");
        assert!(
            spaced.is_err(),
            "spaced raw macro should not parse as expression"
        );
    }

    #[test]
    fn formal_constructor_chain_expr_parses_with_record_expr() {
        let expr = parse_terlan_expr("User(id, name) with Admin { id = id, name = name }")
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
