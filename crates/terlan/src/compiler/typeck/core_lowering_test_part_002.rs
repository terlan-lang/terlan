
#[test]
fn syntax_output_lowering_to_core_pattern_coverage_includes_list_cons_payload() {
    let pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::ListCons,
        arity: 2,
        text: None,
        children: vec![
            SyntaxPatternOutput {
                kind: SyntaxPatternKind::Int,
                arity: 1,
                text: Some("1".to_string()),
                children: Vec::new(),
                fields: Vec::new(),
            },
            SyntaxPatternOutput {
                kind: SyntaxPatternKind::Var,
                arity: 1,
                text: Some("rest".to_string()),
                children: Vec::new(),
                fields: Vec::new(),
            },
        ],
        fields: Vec::new(),
    };
    let core_pattern = core_pattern_from_syntax(&pattern);

    assert_eq!(
        core_pattern,
        Some(CorePattern::ListCons {
            head: Box::new(CorePattern::Int(1)),
            tail: Box::new(CorePattern::Var("rest".to_string())),
        })
    );
    assert_eq!(
        core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
        CoreProofCoverage::ProofModelRequired
    );
}

/// Verifies structural patterns require Lean-modeled child patterns before
/// they are reported as Lean-covered.
///
/// Inputs:
/// - None; constructs a tuple pattern containing a float child pattern.
///
/// Output:
/// - Test passes when the tuple still carries a typed CorePattern payload
///   but reports proof-model-required coverage.
///
/// Transformation:
/// - Exercises recursive Lean-shape validation for structural CorePattern
///   payloads.
#[test]
fn syntax_output_lowering_to_core_pattern_coverage_requires_covered_tuple_children() {
    let pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::Tuple,
        arity: 1,
        text: None,
        children: vec![SyntaxPatternOutput {
            kind: SyntaxPatternKind::Float,
            arity: 1,
            text: Some("1.0".to_string()),
            children: Vec::new(),
            fields: Vec::new(),
        }],
        fields: Vec::new(),
    };
    let core_pattern = core_pattern_from_syntax(&pattern);

    assert_eq!(
        core_pattern,
        Some(CorePattern::Tuple(vec![CorePattern::Float(
            "1.0".to_string()
        )]))
    );
    assert_eq!(
        core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
        CoreProofCoverage::ProofModelRequired
    );
}

/// Verifies list patterns require Lean-modeled child patterns before they
/// are reported as Lean-covered.
///
/// Inputs:
/// - None; constructs a list pattern containing a float child pattern.
///
/// Output:
/// - Test passes when the list still carries a typed CorePattern payload
///   but reports proof-model-required coverage.
///
/// Transformation:
/// - Exercises recursive Lean-shape validation for list CorePattern
///   payloads.
#[test]
fn syntax_output_lowering_to_core_pattern_coverage_requires_covered_list_children() {
    let pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::List,
        arity: 1,
        text: None,
        children: vec![SyntaxPatternOutput {
            kind: SyntaxPatternKind::Float,
            arity: 1,
            text: Some("1.0".to_string()),
            children: Vec::new(),
            fields: Vec::new(),
        }],
        fields: Vec::new(),
    };
    let core_pattern = core_pattern_from_syntax(&pattern);

    assert_eq!(
        core_pattern,
        Some(CorePattern::List(vec![CorePattern::Float(
            "1.0".to_string()
        )]))
    );
    assert_eq!(
        core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
        CoreProofCoverage::ProofModelRequired
    );
}

/// Verifies constructor patterns require Lean-modeled argument patterns
/// before they are reported as Lean-covered.
///
/// Inputs:
/// - None; constructs a constructor pattern containing a float argument
///   pattern.
///
/// Output:
/// - Test passes when the constructor still carries a typed CorePattern
///   payload but reports proof-model-required coverage.
///
/// Transformation:
/// - Exercises recursive Lean-shape validation for constructor CorePattern
///   payloads.
#[test]
fn syntax_output_lowering_to_core_pattern_coverage_requires_covered_constructor_args() {
    let pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::Constructor,
        arity: 1,
        text: Some("Some".to_string()),
        children: vec![SyntaxPatternOutput {
            kind: SyntaxPatternKind::Float,
            arity: 1,
            text: Some("1.0".to_string()),
            children: Vec::new(),
            fields: Vec::new(),
        }],
        fields: Vec::new(),
    };
    let core_pattern = core_pattern_from_syntax(&pattern);

    assert_eq!(
        core_pattern,
        Some(CorePattern::Constructor {
            name: "Some".to_string(),
            constructor_identity: None,
            args: vec![CorePattern::Float("1.0".to_string())],
        })
    );
    assert_eq!(
        core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
        CoreProofCoverage::ProofModelRequired
    );
}

#[test]
fn syntax_output_lowering_to_core_pattern_coverage_requires_map_field_payload() {
    let pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::MapField,
        arity: 1,
        text: Some("a".to_string()),
        children: Vec::new(),
        fields: vec![SyntaxPatternFieldOutput {
            key: "a".to_string(),
            required: true,
            value: Box::new(SyntaxPatternOutput {
                kind: SyntaxPatternKind::Int,
                arity: 1,
                text: Some("1".to_string()),
                children: Vec::new(),
                fields: Vec::new(),
            }),
        }],
    };
    let core_pattern = core_pattern_from_syntax(&pattern);

    assert_eq!(core_pattern, None);
    assert_eq!(
        core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
        CoreProofCoverage::ProofModelRequired
    );
}

#[test]
fn syntax_output_lowering_to_core_pattern_coverage_includes_compat_wildcards() {
    for kind in [SyntaxPatternKind::Ignore, SyntaxPatternKind::Placeholder] {
        let pattern = SyntaxPatternOutput {
            kind,
            arity: 0,
            text: None,
            children: Vec::new(),
            fields: Vec::new(),
        };
        let core_pattern = core_pattern_from_syntax(&pattern);

        assert_eq!(core_pattern, Some(CorePattern::Wildcard));
        assert_eq!(
            core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
            CoreProofCoverage::LeanCovered
        );
    }
}

#[test]
fn syntax_output_lowering_to_core_records_local_call_core_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_call_boundary.\n\
\n\
identity(x: Int): Int ->\n\
    x.\n\
\n\
pub call_it(): Int ->\n\
    identity(1).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "call_it")
        .expect("core call_it function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Call {
            function: "identity".to_string(),
            args: vec![CoreExpr::Int(1)],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::LeanCovered
    );
    assert!(
            core.contract_text()
                .contains("Call:core=Call(identity;Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Call(identity;Int(1))):proof=lean-covered"),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies dedicated function-value invocation remains distinct in CoreIR.
///
/// Inputs:
/// - A syntax-output module whose function body uses `f(value)`.
///
/// Output:
/// - Test passes when the formal CoreIR payload is `CoreExpr::FunctionCall`
///   with a variable callee and one argument.
///
/// Transformation:
/// - Parses, resolves, and lowers source through the syntax-output path,
///   then inspects the backend-neutral CoreIR expression.
#[test]
fn syntax_output_lowering_to_core_records_function_value_call_core_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_function_call_boundary.\n\
\n\
pub apply(value: Int, f: (Int) -> Int): Int ->\n\
    f(value).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "apply")
        .expect("core apply function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::FunctionCall {
            callee: Box::new(CoreExpr::Var("f".to_string())),
            args: vec![CoreExpr::Var("value".to_string())],
        })
    );
    assert!(
        core.contract_text()
            .contains("FunctionCall(Var(f);Var(value))"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies pipe-forward can target dedicated function-value invocation.
///
/// Inputs:
/// - A syntax-output module using `value |> f()`.
///
/// Output:
/// - Test passes when the function typechecks without diagnostics.
///
/// Transformation:
/// - Exercises the pipe rule that prepends the left operand to a
///   `FunctionCall` argument list before checking the callee function type.
#[test]
fn syntax_output_typechecks_pipe_into_function_value_call() {
    let diagnostics = check_syntax_output(
        "\
module pipe_to_function_value_call.\n\
\n\
pub apply(value: Int, f: (Int) -> Int): Int ->\n\
    value |> f().\n",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies compound type annotations lower to CoreType.
///
/// Inputs:
/// - None; exercises `core_type_from_text` with nested type text.
///
/// Output:
/// - Test passes when supported atom literal, list, tuple, parameterized
///   named, function, and union annotations produce typed CoreType
///   payloads.
///
/// Transformation:
/// - Parses type text directly without constructing a full module.
#[test]
fn syntax_output_lowering_to_core_records_compound_core_type_payloads() {
    assert_eq!(
        core_type_from_text("List[Int]"),
        Some(CoreType::List(Box::new(CoreType::Int)))
    );
    assert_eq!(core_type_from_text("String"), Some(CoreType::String));
    assert_eq!(core_type_from_text("Text"), Some(CoreType::Binary));
    assert_eq!(
        core_type_from_text("Atom[\"none\"]"),
        Some(CoreType::AtomLiteral("none".to_string()))
    );
    assert_eq!(
        core_type_from_text("Atom[\"Elixir.Module\"]"),
        Some(CoreType::AtomLiteral("Elixir.Module".to_string()))
    );
    assert_eq!(
        core_type_from_text(r#"Atom["quote \" slash \\ newline \n carriage \r tab \t"]"#),
        Some(CoreType::AtomLiteral(
            "quote \" slash \\ newline \n carriage \r tab \t".to_string()
        ))
    );
    assert_eq!(core_type_from_text("Atom[\"\"]"), None);
    assert_eq!(
        core_type_from_text(": none"),
        Some(CoreType::AtomLiteral("none".to_string()))
    );
    assert_eq!(
        core_type_from_text("Atom[\"Elixir.Module\"]"),
        Some(CoreType::AtomLiteral("Elixir.Module".to_string()))
    );
    assert_eq!(
        core_type_from_text("{Int, Bool}"),
        Some(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::Int),
            CoreTupleTypeElem::Type(CoreType::Bool),
        ]))
    );
    assert_eq!(
        core_type_from_text("List[{Int, Bool}]"),
        Some(CoreType::List(Box::new(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::Int),
            CoreTupleTypeElem::Type(CoreType::Bool),
        ]))))
    );
    assert_eq!(
        core_type_from_text("{Atom[\"ok\"], value: T, _: Int}"),
        Some(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::AtomLiteral("ok".to_string())),
            CoreTupleTypeElem::Field {
                name: "value".to_string(),
                ty: CoreType::Named("T".to_string()),
            },
            CoreTupleTypeElem::Field {
                name: "_".to_string(),
                ty: CoreType::Int,
            },
        ]))
    );
    assert_eq!(
        core_type_from_text("{name: Binary}"),
        Some(CoreType::Map(vec![CoreMapTypeField {
            key: "name".to_string(),
            operator: ":".to_string(),
            value: CoreType::Binary,
        }]))
    );
    assert_eq!(core_type_from_text("#{name: Binary}"), None,);
    assert_eq!(
        core_type_from_text("{ok: {Atom[\"ok\"], value: T}}"),
        Some(CoreType::Map(vec![CoreMapTypeField {
            key: "ok".to_string(),
            operator: ":".to_string(),
            value: CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::AtomLiteral("ok".to_string())),
                CoreTupleTypeElem::Field {
                    name: "value".to_string(),
                    ty: CoreType::Named("T".to_string()),
                },
            ]),
        }]))
    );
    assert_eq!(
        core_type_from_text("Result[Int]"),
        Some(CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![CoreType::Int],
        })
    );
    assert_eq!(
        core_type_from_text("List[Result[{Int, Bool}]]"),
        Some(CoreType::List(Box::new(CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::Int),
                CoreTupleTypeElem::Type(CoreType::Bool),
            ])],
        })))
    );
    assert_eq!(
        core_type_from_text("(Int) -> Bool"),
        Some(CoreType::Arrow {
            params: vec![CoreType::Int],
            return_type: Box::new(CoreType::Bool),
        })
    );
    assert_eq!(
        core_type_from_text("((Int) -> Bool)"),
        Some(CoreType::Arrow {
            params: vec![CoreType::Int],
            return_type: Box::new(CoreType::Bool),
        })
    );
    assert_eq!(
        core_type_from_text("(Int, Result[Bool]) -> List[Int]"),
        Some(CoreType::Arrow {
            params: vec![
                CoreType::Int,
                CoreType::Apply {
                    constructor: "Result".to_string(),
                    args: vec![CoreType::Bool],
                },
            ],
            return_type: Box::new(CoreType::List(Box::new(CoreType::Int))),
        })
    );
    assert_eq!(
        core_type_from_text("Result[(Int) -> Bool]"),
        Some(CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![CoreType::Arrow {
                params: vec![CoreType::Int],
                return_type: Box::new(CoreType::Bool),
            }],
        })
    );
    assert_eq!(
        core_type_from_text("Int | Bool"),
        Some(CoreType::Union(vec![CoreType::Int, CoreType::Bool]))
    );
    assert_eq!(
        core_type_from_text("List[Int | Bool]"),
        Some(CoreType::List(Box::new(CoreType::Union(vec![
            CoreType::Int,
            CoreType::Bool,
        ]))))
    );
    assert_eq!(
        core_type_from_text("(Int) -> Bool | Never"),
        Some(CoreType::Union(vec![
            CoreType::Arrow {
                params: vec![CoreType::Int],
                return_type: Box::new(CoreType::Bool),
            },
            CoreType::Never,
        ]))
    );
    assert_eq!(
        core_type_from_text("Atom[\"none\"] | Atom[\"empty\"]"),
        Some(CoreType::Union(vec![
            CoreType::AtomLiteral("none".to_string()),
            CoreType::AtomLiteral("empty".to_string()),
        ]))
    );
    assert_eq!(core_type_from_text("Int | "), None);
    assert_eq!(core_type_from_text("none"), None);
    assert_eq!(core_type_from_text("result[Int]"), None);
}

/// Verifies type declaration bodies carry optional typed CoreType payloads.
///
/// Inputs:
/// - None; constructs a syntax-output module with supported and
///   unsupported type declaration bodies.
///
/// Output:
/// - Test passes when supported aliases, including atom-literal aliases,
///   have typed `core_body` payloads.
///
/// Transformation:
/// - Lowers resolved module interface type declarations into CoreIR type
///   declarations without emitting backend-specific type syntax.
#[test]
fn syntax_output_lowering_to_core_records_type_decl_core_body_payloads() {
    let module = parse_module_as_syntax_output(
        "\
module core_type_decl_boundary.\n\
\n\
pub type Text = Binary.\n\
pub type MaybeInt = Int | Never.\n\
pub type Items[T] = List[T].\n\
pub type None = Atom[\"none\"].\n\
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
pub type Props = {name: Binary}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_resolved_module_to_core(&resolved);

    let text = core
        .types
        .iter()
        .find(|decl| decl.name == "Text")
        .expect("Text core type declaration");
    assert_eq!(text.core_body, Some(CoreType::Binary));

    let maybe_int = core
        .types
        .iter()
        .find(|decl| decl.name == "MaybeInt")
        .expect("MaybeInt core type declaration");
    assert_eq!(
        maybe_int.core_body,
        Some(CoreType::Union(vec![CoreType::Int, CoreType::Never]))
    );

    let items = core
        .types
        .iter()
        .find(|decl| decl.name == "Items")
        .expect("Items core type declaration");
    assert_eq!(
        items.core_body,
        Some(CoreType::List(Box::new(CoreType::Named("T".to_string()))))
    );

    let none = core
        .types
        .iter()
        .find(|decl| decl.name == "None")
        .expect("None core type declaration");
    assert_eq!(
        none.core_body,
        Some(CoreType::AtomLiteral("none".to_string()))
    );

    let ok = core
        .types
        .iter()
        .find(|decl| decl.name == "Ok")
        .expect("Ok core type declaration");
    assert_eq!(
        ok.core_body,
        Some(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::AtomLiteral("ok".to_string())),
            CoreTupleTypeElem::Field {
                name: "value".to_string(),
                ty: CoreType::Named("T".to_string()),
            },
        ]))
    );

    let props = core
        .types
        .iter()
        .find(|decl| decl.name == "Props")
        .expect("Props core type declaration");
    assert_eq!(
        props.core_body,
        Some(CoreType::Map(vec![CoreMapTypeField {
            key: "name".to_string(),
            operator: ":".to_string(),
            value: CoreType::Binary,
        }]))
    );
    assert_eq!(core.metadata.typed_core_type_count, 6);
    assert_eq!(core.metadata.summary_only_type_count, 0);
}

/// Verifies unsupported type declaration bodies count as summary-only
/// CoreType payloads.
///
/// Inputs:
/// - None; constructs a syntax-output module with a public struct
///   declaration whose structural body is not yet represented as CoreType.
///
/// Output:
/// - Test passes when the type declaration has no `core_body`, and metadata
///   records one summary-only type payload.
///
/// Transformation:
/// - Lowers a resolved struct declaration through CoreIR metadata
///   construction without backend-specific type encoding.
#[test]
fn syntax_output_lowering_to_core_counts_summary_only_type_decl_payloads() {
    let module = parse_module_as_syntax_output(
        "\
module core_summary_type_decl_boundary.\n\
\n\
pub struct Point {\n\
    x: Int,\n\
    y: Int\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_resolved_module_to_core(&resolved);

    let point = core
        .types
        .iter()
        .find(|decl| decl.name == "Point")
        .expect("Point core type declaration");
    assert_eq!(point.core_body, None);
    assert_eq!(
        core.metadata.proof_readiness,
        CoreProofReadiness::ProofModelRequired
    );
    assert_eq!(core.metadata.typed_core_type_count, 0);
    assert_eq!(core.metadata.summary_only_type_count, 1);
}

/// Verifies uppercase constructor-like calls lower as CoreIR candidates.
///
/// Inputs:
/// - None; constructs a syntax-output call expression for `Ok(1)`.
///
/// Output:
/// - Test passes when the expression has a typed `CoreExpr::ConstructorCall`
///   payload and is classified as partial.
///
/// Transformation:
/// - Exercises the named-call lowering rule without invoking resolver
///   behavior for constructor aliases.
#[test]
fn syntax_output_lowering_to_core_records_constructor_call_candidate() {
    let expr = SyntaxExprOutput {
        kind: SyntaxExprKind::Call,
        arity: 1,
        text: None,
        span: Default::default(),
        raw: None,
        type_args: Vec::new(),
        operator: None,
        remote: None,
        arg_names: Vec::new(),
        comprehension_lift: None,
        children: vec![
            SyntaxExprOutput {
                kind: SyntaxExprKind::Var,
                arity: 0,
                text: Some("Ok".to_string()),
                span: Default::default(),
                raw: None,
                type_args: Vec::new(),
                operator: None,
                remote: None,
                arg_names: Vec::new(),
                comprehension_lift: None,
                children: Vec::new(),
                patterns: Vec::new(),
                let_guards: Vec::new(),
                fields: Vec::new(),
                clauses: Vec::new(),
                catch_clauses: Vec::new(),
                try_after: None,
                html_nodes: Vec::new(),
            },
            SyntaxExprOutput {
                kind: SyntaxExprKind::Int,
                arity: 0,
                text: Some("1".to_string()),
                span: Default::default(),
                raw: None,
                type_args: Vec::new(),
                operator: None,
                remote: None,
                arg_names: Vec::new(),
                comprehension_lift: None,
                children: Vec::new(),
                patterns: Vec::new(),
                let_guards: Vec::new(),
                fields: Vec::new(),
                clauses: Vec::new(),
                catch_clauses: Vec::new(),
                try_after: None,
                html_nodes: Vec::new(),
            },
        ],
        patterns: Vec::new(),
        let_guards: Vec::new(),
        fields: Vec::new(),
        clauses: Vec::new(),
        catch_clauses: Vec::new(),
        try_after: None,
        html_nodes: Vec::new(),
    };
    let core_expr = core_expr_from_syntax(&expr);

    assert_eq!(
        core_expr,
        Some(CoreExpr::ConstructorCall {
            constructor: "Ok".to_string(),
            constructor_identity: None,
            args: vec![CoreExpr::Int(1)],
        })
    );
    assert_eq!(
        core_expr_proof_coverage(&expr, core_expr.as_ref()),
        CoreProofCoverage::Partial
    );
}

/// Verifies the remote-call proof policy switch remains conservative.
///
/// Inputs:
/// - A typed `CoreExpr::RemoteCall` value matching the formal remote-call
///   payload shape.
/// - The summary-only `None` path used when coverage is requested without a
///   typed Core payload.
///
/// Output:
/// - The test passes when both paths report `ProofModelRequired`, and the
///   promotion helper still prevents remote calls from counting as
///   Lean-modeled.
///
/// Transformation:
/// - Exercises the named compiler-side promotion policy without lowering a
///   source fixture, so future remote-dispatch promotion must update this
///   explicit policy guard.
#[test]
fn syntax_output_lowering_to_core_remote_call_policy_switch_stays_proof_model_required() {
    let remote_call = CoreExpr::RemoteCall {
        module: "Eq".to_string(),
        function: "equal".to_string(),
        args: vec![
            CoreExpr::Var("Left".to_string()),
            CoreExpr::Var("Right".to_string()),
        ],
    };

    assert_eq!(
        remote_call_proof_coverage_policy(Some(&remote_call)),
        CoreProofCoverage::ProofModelRequired
    );
    assert_eq!(
        remote_call_proof_coverage_policy(None),
        CoreProofCoverage::ProofModelRequired
    );
    assert!(!remote_call_is_promoted_to_lean_covered());
    assert!(!core_expr_is_lean_modeled(&remote_call));
}

/// Verifies a local structural generic impl is erased into one private CoreIR
/// function and its statically checked trait call targets that function.
#[test]
fn syntax_output_lowering_rewrites_structural_impl_call_to_private_function() {
    let module = parse_module_as_syntax_output(
        r#"
module structural_impl_core_dispatch.

pub struct Profile { title: String }.

pub trait Render[T] {
    render(value: T): String.
}.

pub impl Render[T => {title: String}] for T {
    render(value: T): String -> value.title.
}.

pub display(profile: Profile): String ->
    Render.render(profile).
"#,
    )
    .expect("structural impl fixture parses");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let implementation = core
        .functions
        .iter()
        .find(|function| function.name == "__terlan_structural_impl_Render_render_1")
        .expect("private structural impl function");
    assert!(!implementation.public);
    let display = core
        .functions
        .iter()
        .find(|function| function.name == "display")
        .expect("display function");
    assert!(matches!(
        display.clauses[0].body.core_expr.as_ref(),
        Some(CoreExpr::Call { function, .. })
            if function == "__terlan_structural_impl_Render_render_1"
    ));
}
