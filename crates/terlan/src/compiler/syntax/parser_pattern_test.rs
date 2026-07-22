#[cfg(test)]
mod tests {
    use crate::terlan_syntax::parse_module;
    use crate::terlan_syntax::parse_tree::{Decl, Expr};

    #[test]
    fn formal_atom_literal_patterns_are_literal_patterns() {
        let module = parse_module(
            r#"
            module atoms.

            value(Status: Status): Int ->
                case Status {
                    Atom["none"] -> 0;
                    Atom["empty"] -> 1
                }.
            "#,
        )
        .expect("parse canonical atom patterns");

        let Decl::Function(function) = &module.declarations[0] else {
            panic!("expected function");
        };
        let Expr::Case { clauses, .. } = &function.clauses[0].body else {
            panic!("expected case expression");
        };
        assert!(
            matches!(&clauses[0].pattern, crate::terlan_syntax::parse_tree::Pattern::AtomLiteral(name) if name == "none")
        );
        assert!(
            matches!(&clauses[1].pattern, crate::terlan_syntax::parse_tree::Pattern::AtomLiteral(name) if name == "empty")
        );
    }

    #[test]
    fn small_maps_parity_decodes_expression_and_pattern_string_keys_once() {
        let module = parse_module(
            r#"
            module small_maps_key_identity.

            value(): Int ->
                case {"line\nkey": 1} {
                    {"line\nkey": found} -> found
                }.
            "#,
        )
        .expect("parse matching escaped map keys");

        let Decl::Function(function) = &module.declarations[0] else {
            panic!("expected function");
        };
        let Expr::Case { scrutinee, clauses } = &function.clauses[0].body else {
            panic!("expected case expression");
        };
        let Expr::Map(expression_fields) = scrutinee.as_ref() else {
            panic!("expected map expression");
        };
        let crate::terlan_syntax::parse_tree::Pattern::Map(pattern_fields) = &clauses[0].pattern
        else {
            panic!("expected map pattern");
        };

        assert_eq!(expression_fields[0].key, "line\nkey");
        assert_eq!(pattern_fields[0].key, expression_fields[0].key);
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
                {kind: Atom["ok"], count: n} where n > 0 -> n;
                {} -> 0
              }.

            list_cons_pattern(values: List[Int]): Int ->
              case values {
                [head | tail] where head > 0 -> head;
                [] -> 0
              }.

            literal_patterns(value: Dynamic): Int ->
              case value {
                Atom["none"] -> 0;
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
            matches!(&clauses[0].pattern, crate::terlan_syntax::parse_tree::Pattern::Map(fields) if fields.len() == 2)
        );
        assert!(clauses[0].guard.is_some());
        assert!(
            matches!(&clauses[1].pattern, crate::terlan_syntax::parse_tree::Pattern::Map(fields) if fields.is_empty())
        );

        let Decl::Function(cons_function) = &module.declarations[1] else {
            panic!("expected cons pattern function");
        };
        let Expr::Case { clauses, .. } = &cons_function.clauses[0].body else {
            panic!("expected cons pattern case");
        };
        assert!(matches!(
            &clauses[0].pattern,
            crate::terlan_syntax::parse_tree::Pattern::ListCons(_, _)
        ));
        assert!(clauses[0].guard.is_some());

        let Decl::Function(literal_function) = &module.declarations[2] else {
            panic!("expected literal pattern function");
        };
        let Expr::Case { clauses, .. } = &literal_function.clauses[0].body else {
            panic!("expected literal pattern case");
        };
        assert!(
            matches!(&clauses[0].pattern, crate::terlan_syntax::parse_tree::Pattern::AtomLiteral(name) if name == "none")
        );
        assert!(
            matches!(&clauses[1].pattern, crate::terlan_syntax::parse_tree::Pattern::Float(value) if (*value - 1.5).abs() < f64::EPSILON)
        );
        assert!(
            matches!(&clauses[2].pattern, crate::terlan_syntax::parse_tree::Pattern::Tuple(items) if items.len() == 2)
        );

        let Decl::Function(constructor_function) = &module.declarations[3] else {
            panic!("expected constructor pattern function");
        };
        let Expr::Case { clauses, .. } = &constructor_function.clauses[0].body else {
            panic!("expected constructor pattern case");
        };
        assert!(matches!(
            &clauses[0].pattern,
            crate::terlan_syntax::parse_tree::Pattern::Tuple(items)
                if matches!(items.as_slice(), [crate::terlan_syntax::parse_tree::Pattern::Atom(name)] if name == "None")
        ));
        assert!(matches!(
            &clauses[1].pattern,
            crate::terlan_syntax::parse_tree::Pattern::Tuple(items)
                if matches!(
                    items.as_slice(),
                    [crate::terlan_syntax::parse_tree::Pattern::Atom(name), crate::terlan_syntax::parse_tree::Pattern::Var(var)]
            if name == "Ok" && var == "item"
                )
        ));
    }

    #[test]
    fn parses_nullary_constructor_pattern_call() {
        let module = parse_module(
            r#"
            module nullary_constructor_pattern.

            value(Option: Option): Int ->
                case Option {
                    None() -> 0
                }.
            "#,
        )
        .expect("parse nullary constructor pattern call");
        let Decl::Function(function) = &module.declarations[0] else {
            panic!("expected function declaration");
        };
        let Expr::Case { clauses, .. } = &function.clauses[0].body else {
            panic!("expected case expression");
        };
        assert!(matches!(
            &clauses[0].pattern,
            crate::terlan_syntax::parse_tree::Pattern::Tuple(items)
                if matches!(items.as_slice(), [crate::terlan_syntax::parse_tree::Pattern::Atom(name)] if name == "None")
        ));
    }

    #[test]
    fn rejects_hash_prefixed_struct_patterns() {
        let err = parse_module(
            r#"
            module bad_struct_pattern.

            read(#Point{x: x}): Int ->
                x.
            "#,
        )
        .expect_err("reject hash-prefixed struct pattern");

        assert_eq!(
            err.message,
            "struct patterns must use Type { field: pattern } syntax"
        );
    }

    #[test]
    fn parses_constructor_style_patterns() {
        let source = r#"
module syntax.

pub simplify(E: Expr): Expr ->
    case E {
        Call(Atom["atom"], [x, y]) ->
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
            crate::terlan_syntax::parse_tree::Pattern::Tuple(items) => {
                assert_eq!(items.len(), 3);
                match &items[0] {
                    crate::terlan_syntax::parse_tree::Pattern::Atom(name) => {
                        assert_eq!(name, "Call")
                    }
                    _ => panic!("expected constructor atom"),
                }
                match &items[1] {
                    crate::terlan_syntax::parse_tree::Pattern::AtomLiteral(name) => {
                        assert_eq!(name, "atom")
                    }
                    _ => panic!("expected canonical atom-literal argument"),
                }
            }
            _ => panic!("expected tuple pattern"),
        }
    }
}
