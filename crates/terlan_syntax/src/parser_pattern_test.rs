#[cfg(test)]
mod tests {
    use crate::parse_module;
    use crate::parse_tree::{Decl, Expr};

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
}
