#[cfg(test)]
mod tests {
    use crate::terlan_syntax::{
        parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxPatternKind,
    };

    #[test]
    fn expands_local_shape_calls_in_case_and_function_head_patterns() {
        let output = parse_module_as_syntax_output(
            "module local_shapes.\n\
             shape Pair(value) = {Atom[\"pair\"], value}.\n\
             shape Wrapped(value) = {Atom[\"wrapped\"], Pair(value)}.\n\
             pub unwrap(Wrapped(value): Dynamic): Int -> value.\n\
             pub read(value: Dynamic): Int -> case value { Pair(number) -> number; _ -> 0 }.\n",
        )
        .expect("local shapes should expand");

        let patterns = output
            .declarations
            .iter()
            .filter_map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Function { clauses, .. } => Some(
                    clauses
                        .iter()
                        .flat_map(|clause| clause.patterns.iter())
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .flatten()
            .collect::<Vec<_>>();
        assert!(patterns.iter().all(|pattern| {
            pattern.kind != SyntaxPatternKind::Constructor
                || !matches!(pattern.text.as_deref(), Some("Pair" | "Wrapped"))
        }));
    }

    #[test]
    fn preserves_formal_atom_literals_in_case_scrutinees() {
        let output = parse_module_as_syntax_output(
            "module atom_shape_scrutinee.\n\
             shape Tagged(value) = {Atom[\"ok\"], value}.\n\
             pub read(): Int ->\n\
                 case {Atom[\"ok\"], 7} { Tagged(value) -> value; _ -> 0 }.\n",
        )
        .expect("formal atom literal case scrutinee should parse");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let scrutinee = &clauses[0].body.children[0];
        assert_eq!(scrutinee.kind, crate::terlan_syntax::SyntaxExprKind::Tuple);
        assert_eq!(
            scrutinee.children[0].kind,
            crate::terlan_syntax::SyntaxExprKind::Atom
        );
        assert_eq!(scrutinee.children[0].text.as_deref(), Some("ok"));
        assert_eq!(scrutinee.children[0].raw.as_deref(), Some("Atom[\"ok\"]"));
        let pattern = &clauses[0].body.clauses[0].patterns[0];
        assert_eq!(pattern.kind, SyntaxPatternKind::Tuple);
        assert_eq!(pattern.children[0].kind, SyntaxPatternKind::Atom);
        assert_eq!(pattern.children[0].text.as_deref(), Some("ok"));
    }

    #[test]
    fn expands_binary_layout_shape_captures_without_rewriting_descriptors() {
        let output = parse_module_as_syntax_output(
            "module binary_shapes.\n\
             shape Packet(port, body) = Binary[big] { port: UInt[16], body: Rest }.\n\
             pub read(value: Dynamic): Int ->\n\
                 case value { Packet(decoded_port, _) -> decoded_port; _ -> 0 }.\n",
        )
        .expect("binary layout shape should expand");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let pattern = &clauses[0].body.clauses[0].patterns[0];
        assert_eq!(pattern.kind, SyntaxPatternKind::BinaryLayout);
        assert_eq!(pattern.fields[0].key, "decoded_port");
        assert_eq!(pattern.fields[0].value.text.as_deref(), Some("UInt[16]"));
        assert_eq!(pattern.fields[1].key, "_");
        assert_eq!(pattern.fields[1].value.text.as_deref(), Some("Rest"));
    }

    #[test]
    fn rejects_structural_arguments_for_binary_layout_shape_captures() {
        let error = parse_module_as_syntax_output(
            "module binary_shape_argument.\n\
             shape Packet(port) = Binary[big] { port: UInt[16] }.\n\
             pub read(value: Dynamic): Int ->\n\
                 case value { Packet({left, right}) -> left + right; _ -> 0 }.\n",
        )
        .expect_err("binary shape captures require binding arguments");

        assert!(format!("{error:?}").contains(
            "shape `Packet` binary capture parameter `port` requires a variable or wildcard pattern argument"
        ));
    }

    #[test]
    fn rejects_local_shape_arity_mismatch() {
        let error = parse_module_as_syntax_output(
            "module shape_arity.\n\
             shape Pair(left, right) = {left, right}.\n\
             pub read(value: Dynamic): Dynamic -> case value { Pair(only) -> only; _ -> Atom[\"none\"] }.\n",
        )
        .expect_err("shape arity mismatch must fail");
        assert!(
            format!("{error:?}").contains("shape `Pair` expects 2 pattern argument(s), found 1")
        );
    }

    #[test]
    fn rejects_local_shape_called_as_runtime_value() {
        let error = parse_module_as_syntax_output(
            "module runtime_shape_call.\n\
             shape Pair(value) = {Atom[\"pair\"], value}.\n\
             pub build(value: Int): Dynamic -> Pair(value).\n",
        )
        .expect_err("shape aliases must not construct runtime values");
        assert!(format!("{error:?}").contains(
            "shape `Pair` is compile-time pattern-only and cannot be called as a runtime value"
        ));
    }

    #[test]
    fn rejects_recursive_local_shape_expansion() {
        let error = parse_module_as_syntax_output(
            "module recursive_shapes.\n\
             shape Left(value) = Right(value).\n\
             shape Right(value) = Left(value).\n\
             pub read(value: Dynamic): Dynamic -> case value { Left(found) -> found; _ -> Atom[\"none\"] }.\n",
        )
        .expect_err("recursive shape expansion must fail");
        assert!(format!("{error:?}").contains("recursive shape expansion: Left -> Right -> Left"));
    }

    #[test]
    fn rejects_duplicate_local_shape_parameters_and_names() {
        let duplicate_param = parse_module_as_syntax_output(
            "module duplicate_shape_param.\n\
             shape Pair(value, value) = {value, value}.\n",
        )
        .expect_err("duplicate shape parameters must fail");
        assert!(
            format!("{duplicate_param:?}").contains("shape `Pair` has duplicate parameter `value`")
        );

        let duplicate_name = parse_module_as_syntax_output(
            "module duplicate_shape_name.\n\
             shape Pair(value) = {Atom[\"left\"], value}.\n\
             shape Pair(value) = {Atom[\"right\"], value}.\n",
        )
        .expect_err("duplicate shape names must fail");
        assert!(format!("{duplicate_name:?}").contains("duplicate shape declaration `Pair`"));
    }

    #[test]
    fn rejects_duplicate_bindings_in_shape_bodies() {
        let duplicate_variable = parse_module_as_syntax_output(
            "module duplicate_shape_binding.\n\
             shape Same(value) = {value, value}.\n",
        )
        .expect_err("duplicate shape variable bindings must fail");
        assert!(format!("{duplicate_variable:?}").contains(
            "shape `Same` binds `value` more than once; duplicate shape bindings are ambiguous"
        ));

        let duplicate_capture = parse_module_as_syntax_output(
            "module duplicate_shape_capture.\n\
             shape Route(id) = \"users/${id}/posts/${id}\".\n",
        )
        .expect_err("duplicate shape string captures must fail");
        assert!(format!("{duplicate_capture:?}").contains(
            "shape `Route` binds `id` more than once; duplicate shape bindings are ambiguous"
        ));
    }

    #[test]
    fn rejects_duplicate_bindings_created_by_shape_expansion() {
        let duplicate_argument = parse_module_as_syntax_output(
            "module duplicate_shape_argument.\n\
             shape Pair(left, right) = {left, right}.\n\
             pub read(input: Dynamic): Bool ->\n\
                 case input { Pair(value, value) -> true; _ -> false }.\n",
        )
        .expect_err("overlapping shape arguments must fail after expansion");
        assert!(format!("{duplicate_argument:?}").contains(
            "shape `Pair` expansion binds `value` more than once; overlapping shape arguments are ambiguous"
        ));

        let structural_overlap = parse_module_as_syntax_output(
            "module overlapping_shape_argument.\n\
             shape Pair(left, right) = {left, right}.\n\
             pub read(input: Dynamic): Bool ->\n\
                 case input { Pair({left, right}, right) -> true; _ -> false }.\n",
        )
        .expect_err("structurally overlapping shape arguments must fail after expansion");
        assert!(format!("{structural_overlap:?}").contains(
            "shape `Pair` expansion binds `right` more than once; overlapping shape arguments are ambiguous"
        ));
    }

    #[test]
    fn composes_guarded_shape_with_explicit_clause_guard() {
        let output = parse_module_as_syntax_output(
            "module guarded_shape.\n\
             shape Success(body) = {status, body} where status >= 200 and status < 300.\n\
             pub read(value: Dynamic): Dynamic ->\n\
                 case value { Success(body) where body > 0 -> body; _ -> Atom[\"none\"] }.\n",
        )
        .expect("guarded shape should compose into its clause guard");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let case_clause = &clauses[0].body.clauses[0];
        let guard = case_clause.guard.as_ref().expect("composed clause guard");
        assert_eq!(guard.kind, crate::terlan_syntax::SyntaxExprKind::BinaryOp);
        assert_eq!(guard.operator.as_deref(), Some("and"));
        assert_eq!(guard.children.len(), 2);
    }

    #[test]
    fn expands_nested_shape_guards_and_substitutes_parameters() {
        let output = parse_module_as_syntax_output(
            "module nested_guarded_shape.\n\
             shape Positive(value) = value where value > 0.\n\
             shape TaggedPositive(value) = {Atom[\"positive\"], Positive(value)}.\n\
             pub read(TaggedPositive(value): Dynamic): Int -> value.\n",
        )
        .expect("nested guarded shape should expand");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[2].payload
        else {
            panic!("expected function declaration");
        };
        let guard = clauses[0].guard.as_ref().expect("nested shape guard");
        assert_eq!(guard.kind, crate::terlan_syntax::SyntaxExprKind::BinaryOp);
        assert_eq!(guard.operator.as_deref(), Some(">"));
        assert_eq!(guard.children[0].text.as_deref(), Some("value"));
    }

    #[test]
    fn rejects_guard_parameter_substitution_from_non_value_pattern() {
        let error = parse_module_as_syntax_output(
            "module guarded_shape_wildcard.\n\
             shape Positive(value) = value where value > 0.\n\
             pub read(input: Dynamic): Int ->\n\
                 case input { Positive(_) -> 1; _ -> 0 }.\n",
        )
        .expect_err("guard parameters cannot read wildcard pattern arguments");
        assert!(format!("{error:?}").contains(
            "shape `Positive` guard references parameter `value` with a non-value pattern argument"
        ));
    }

    #[test]
    fn composes_guarded_shape_with_comprehension_filter() {
        let output = parse_module_as_syntax_output(
            "module guarded_shape_comprehension.\n\
             shape Positive(value) = value where value > 0.\n\
             pub collect(values: List[Int]): List[Int] ->\n\
                 [value | Positive(value) <- values, value < 10].\n",
        )
        .expect("guarded shape should compose with comprehension filter");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let comprehension = &clauses[0].body;
        assert_eq!(
            comprehension.kind,
            crate::terlan_syntax::SyntaxExprKind::ListComprehension
        );
        assert_eq!(comprehension.children.len(), 3);
        let guard = &comprehension.children[2];
        assert_eq!(guard.kind, crate::terlan_syntax::SyntaxExprKind::BinaryOp);
        assert_eq!(guard.operator.as_deref(), Some("and"));
    }

    #[test]
    fn expands_guarded_shape_in_let_pattern_as_case_assertion() {
        let output = parse_module_as_syntax_output(
            "module guarded_shape_let.\n\
             shape Positive(value) = value where value > 0.\n\
             pub read(input: Int): Int ->\n\
                 let Positive(value) = input;\n\
                 value.\n",
        )
        .expect("guarded shape let should become a match assertion");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let assertion = &clauses[0].body;
        assert_eq!(assertion.kind, crate::terlan_syntax::SyntaxExprKind::Case);
        assert_eq!(assertion.clauses.len(), 1);
        assert!(assertion.clauses[0].guard.is_some());
        assert_eq!(
            assertion.clauses[0].patterns[0].kind,
            crate::terlan_syntax::SyntaxPatternKind::Var
        );
    }

    #[test]
    fn carries_guarded_shapes_into_grouped_let_success_guards() {
        let output = parse_module_as_syntax_output(
            "module guarded_shape_grouped_let.\n\
             shape Positive(value) = value where value > 0.\n\
             pub add(left: Int, right: Int): Int ->\n\
                 let {\n\
                     Positive(first) <- left;\n\
                     Positive(second) <- right\n\
                 } else {\n\
                     _ -> 0\n\
                 };\n\
                 first + second.\n",
        )
        .expect("guarded shapes should expand in grouped let bindings");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let grouped_let = &clauses[0].body;
        assert_eq!(grouped_let.kind, crate::terlan_syntax::SyntaxExprKind::Let);
        assert_eq!(grouped_let.let_guards.len(), 2);
        assert!(grouped_let.let_guards.iter().all(Option::is_some));
    }

    #[test]
    fn gives_each_private_shape_binding_a_distinct_compiler_name() {
        let output = parse_module_as_syntax_output(
            "module hygienic_shapes.\n\
             shape Success(body) = {status, body} where status >= 200 and status < 300.\n\
             pub read(left: Dynamic, right: Dynamic): Int ->\n\
                 case {left, right} {\n\
                     {Success(first), Success(second)} -> first + second;\n\
                     _ -> 0\n\
                 }.\n",
        )
        .expect("private shape bindings should be hygienic");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let clause = &clauses[0].body.clauses[0];
        let expanded = format!("{:?} {:?}", clause.patterns, clause.guard);
        assert!(expanded.contains("#shape_0_status"), "expanded: {expanded}");
        assert!(expanded.contains("#shape_1_status"), "expanded: {expanded}");
    }

    #[test]
    fn substitutes_string_capture_parameters_in_text_and_binding_metadata() {
        let output = parse_module_as_syntax_output(
            "module string_capture_shape.\n\
             shape UserAsset(id) = \"users/${id: Int}.json\".\n\
             pub read(value: String): Int ->\n\
                 case value { UserAsset(found) -> found; _ -> 0 }.\n",
        )
        .expect("string capture shape parameters should expand");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let pattern = &clauses[0].body.clauses[0].patterns[0];
        assert_eq!(pattern.kind, SyntaxPatternKind::StringPattern);
        assert_eq!(pattern.text.as_deref(), Some("users/${found: Int}.json"));
        assert_eq!(pattern.children[0].text.as_deref(), Some("found: Int"));
    }

    #[test]
    fn gives_private_string_captures_compiler_names() {
        let output = parse_module_as_syntax_output(
            "module private_string_capture_shape.\n\
             shape PositiveAsset(_marker) = \"users/${id: Int}.json\" where id > 0.\n\
             pub read(value: String, id: Int): Bool ->\n\
                 case value { PositiveAsset(_) where id == 500 -> true; _ -> false }.\n",
        )
        .expect("private string captures should be hygienic");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let clause = &clauses[0].body.clauses[0];
        let expanded = format!("{:?} {:?}", clause.patterns, clause.guard);
        assert!(expanded.contains("#shape_0_id"), "expanded: {expanded}");
        assert!(
            expanded.contains("${#shape_0_id: Int}"),
            "expanded: {expanded}"
        );
    }

    #[test]
    fn rejects_non_binding_string_capture_arguments() {
        let error = parse_module_as_syntax_output(
            "module invalid_string_capture_shape.\n\
             shape UserAsset(id) = \"users/${id: Int}.json\".\n\
             pub read(value: String): Bool ->\n\
                 case value { UserAsset(42) -> true; _ -> false }.\n",
        )
        .expect_err("string capture arguments must introduce bindings");
        assert!(format!("{error:?}").contains(
            "shape `UserAsset` string capture parameter `id` requires a variable, alias, or wildcard pattern argument"
        ));
    }
}
