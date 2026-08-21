use super::super::*;

/// Verifies integral-valued Float nodes retain Float syntax across interfaces.
#[test]
pub(super) fn syntax_output_preserves_integral_float_identity() {
    let expression =
        parse_expr_as_syntax_output("[0.0, 1000.0]").expect("syntax output float list");

    assert_eq!(expression.kind, SyntaxExprKind::List);
    assert_eq!(expression.children[0].kind, SyntaxExprKind::Float);
    assert_eq!(expression.children[0].text.as_deref(), Some("0.0"));
    assert_eq!(expression.children[1].text.as_deref(), Some("1000.0"));
}

#[test]
pub(super) fn syntax_output_ignores_typed_sql_dollar_quoted_interpolation_text() {
    let output = parse_module_as_syntax_output(
        r#"
        module typed_sql_dollar_quote_interpolation_tree.

        query(user: User): Dynamic ->
            sql[UserRow] {
                select $body$${ignored}$body$ where id = ${user.id}
            }.
        "#,
    )
    .expect("syntax output typed SQL dollar-quoted interpolation expression");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
        panic!("expected function declaration");
    };

    let body = &clauses[0].body;
    assert_eq!(body.children.len(), 1);
    assert_eq!(body.children[0].kind, SyntaxExprKind::FieldAccess);
    assert_eq!(body.children[0].text.as_deref(), Some("id"));
}

#[test]
pub(super) fn syntax_output_includes_quoted_atom_literals() {
    let output = parse_module_as_syntax_output(
        r#"
        module quoted_atom_tree.

        module_atom(): Dynamic ->
            Atom["Elixir.Module"].

        classify(value: Dynamic): Dynamic ->
            case value {
                Atom["some atom"] -> Atom["ok"]
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

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload else {
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
pub(super) fn syntax_output_includes_canonical_atom_literal_expr_source() {
    let output = parse_module_as_syntax_output(
        r#"
        module atom_literal_expr_tree.

        ready(): Atom ->
            Atom["quote \" slash \\ newline \n carriage \r tab \t"].
        "#,
    )
    .expect("syntax output atom literal expression");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_normalizes_prefixed_integer_literals() {
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

/// Verifies Vm binary segment syntax is rejected before syntax-output
/// boundary.
///
/// Inputs:
/// - A module containing an Vm binary expression with size and segment
///   modifiers.
///
/// Output:
/// - Test passes when syntax-output construction rejects the source.
///
/// Transformation:
/// - Keeps backend Vm binary syntax from entering canonical Terlan
///   syntax output.

/// Verifies Vm binary segment syntax is rejected before syntax-output
/// boundary.
///
/// Inputs:
/// - A module containing an Vm binary expression with size and segment
///   modifiers.
///
/// Output:
/// - Test passes when syntax-output construction rejects the source.
///
/// Transformation:
/// - Keeps backend Vm binary syntax from entering canonical Terlan
///   syntax output.
#[test]
pub(super) fn syntax_output_rejects_erlang_binary_segment_text() {
    let error = parse_module_as_syntax_output(
        r#"
        module binary_segment_text.

        byte(value: Int): Binary ->
            <<value:8/integer-unsigned-big>>.
        "#,
    )
    .expect_err("Vm binary segment syntax should be rejected");

    let crate::terlan_syntax::ebnf::EbnfCompileError::Parse(message, _) = error else {
        panic!("expected parse error");
    };
    assert!(message.contains("Vm binary literal syntax"));
}

#[test]
pub(super) fn syntax_output_includes_constructor_chain_expr_trees() {
    let output = parse_module_as_syntax_output(
        r#"
        module constructor_chain_expr_tree.

        demo(id: Int, name: Binary): Dynamic ->
            User(id, name) with Admin { id: id, name: name }.
        "#,
    )
    .expect("syntax output constructor chain expr");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_allows_keyword_expressions_in_operator_chains() {
    let output = parse_module_as_syntax_output(
        r#"
        module keyword_expr_chain.

        demo(option: Dynamic): Dynamic ->
            case option {
                Atom["none"] -> 0;
                value -> value
            } |> inspect().
        "#,
    )
    .expect("syntax output keyword expression chain");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_includes_if_expression_trees() {
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

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_includes_try_expression_trees() {
    let output = parse_module_as_syntax_output(
        r#"
        module try_expr.

        wait(): Int ->
            try risky() {
                {Atom["ok"], value} -> value
            catch
                Atom["error"] -> 0
            after
                0 -> cleanup()
            }.
        "#,
    )
    .expect("syntax output try expression");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_keeps_constructor_call_candidates_as_named_calls() {
    let output = parse_module_as_syntax_output(
        r#"
        module constructor_calls.

        make(): Dynamic ->
            Ok(123).
        "#,
    )
    .expect("syntax output constructor call candidate");

    let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload else {
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
pub(super) fn syntax_output_includes_record_suffix_trees() {
    let output = parse_module_as_syntax_output(
        r#"
        module record_suffix_trees.

        field(user: Dynamic): Dynamic ->
            user#foo.bar.

        update(user: Dynamic): Dynamic ->
            user#foo{bar: 2}.
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
pub(super) fn syntax_output_includes_sequence_primary_expr_trees() {
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

/// Verifies descriptor-backed binary layout scaffold reaches syntax output.
#[test]
pub(super) fn syntax_output_includes_binary_layout_scaffold() {
    let output = parse_module_as_syntax_output(
        r#"
        module binary_layout_output.

        packet(): Dynamic ->
            Binary[big] { source_port: UInt[16], payload: Rest }.

        decode(Binary[little] { opcode: UInt[8], payload: Rest }): Int ->
            1.
        "#,
    )
    .expect("syntax output binary layout scaffold");

    let SyntaxDeclarationPayload::Function {
        clauses: packet_clauses,
        ..
    } = &output.declarations[0].payload
    else {
        panic!("expected packet function");
    };
    let layout = &packet_clauses[0].body;
    assert_eq!(layout.kind, SyntaxExprKind::BinaryLayout);
    assert_eq!(layout.text.as_deref(), Some("big"));
    assert_eq!(layout.fields.len(), 2);
    assert_eq!(layout.fields[0].key, "source_port");
    assert_eq!(layout.fields[0].value.text.as_deref(), Some("UInt[16]"));
    assert_eq!(layout.fields[1].key, "payload");
    assert_eq!(layout.fields[1].value.text.as_deref(), Some("Rest"));

    let SyntaxDeclarationPayload::Function {
        clauses: decode_clauses,
        ..
    } = &output.declarations[1].payload
    else {
        panic!("expected decode function");
    };
    let pattern = &decode_clauses[0].patterns[0];
    assert_eq!(pattern.kind, SyntaxPatternKind::BinaryLayout);
    assert_eq!(pattern.text.as_deref(), Some("little"));
    assert_eq!(pattern.fields.len(), 2);
    assert_eq!(pattern.fields[0].key, "opcode");
    assert_eq!(pattern.fields[0].value.text.as_deref(), Some("UInt[8]"));
    assert_eq!(pattern.fields[1].key, "payload");
    assert_eq!(pattern.fields[1].value.text.as_deref(), Some("Rest"));
}

#[test]
pub(super) fn syntax_output_includes_map_constructor_record_and_template_field_trees() {
    let output = parse_module_as_syntax_output(
        r#"
        module field_payload_trees.

        map(): Map ->
            {a: 1, b: 2}.

        chain(id: Int): Dynamic ->
            User(id) with Admin {name: "Ada"}.

        render_template(): Dynamic ->
            Page {title: "hello"}.
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
    assert!(map.fields[1].required);
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
    assert_eq!(template.kind, SyntaxExprKind::RecordConstruct);
    assert_eq!(template.text.as_deref(), Some("Page"));
    assert_eq!(template.fields.len(), 1);
    assert_eq!(template.fields[0].key, "title");
    assert!(template.fields[0].required);
    assert_eq!(template.fields[0].value.kind, SyntaxExprKind::Binary);
    assert_eq!(template.fields[0].value.text.as_deref(), Some("\"hello\""));
}
