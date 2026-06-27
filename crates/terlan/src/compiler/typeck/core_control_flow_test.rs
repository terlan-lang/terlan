use super::*;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn syntax_output_lowering_to_core_records_case_core_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_case_boundary.\n\
\n\
pub choose(x: Int): Int ->\n\
    case x {\n\
        0 -> 1;\n\
        _ -> x\n\
    }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "choose")
        .expect("core choose function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Case {
            scrutinee: Box::new(CoreExpr::Var("x".to_string())),
            clauses: vec![
                CoreCaseClause {
                    pattern: CorePattern::Int(0),
                    guard: None,
                    body: CoreExpr::Int(1),
                },
                CoreCaseClause {
                    pattern: CorePattern::Wildcard,
                    guard: None,
                    body: CoreExpr::Var("x".to_string()),
                },
            ],
        })
    );
    assert!(
            core.contract_text().contains(
                "Case:core=Case(Var(x);Int(0)=>Int(1)|Wildcard=>Var(x)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Case(Var(x);Int(0)=>Int(1)|Wildcard=>Var(x))):proof=lean-covered"
            ),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies case expressions with typed-but-unmodeled patterns are not
/// reported as Lean-covered.
///
/// Inputs:
/// - None; parses a function whose case branch uses a record pattern.
///
/// Output:
/// - Test passes when the case expression carries a typed Core payload but
///   reports proof-model-required coverage.
///
/// Transformation:
/// - Exercises the case coverage gate that requires every branch pattern to
///   map to the current Lean pattern subset.
#[test]
fn syntax_output_lowering_to_core_case_with_record_pattern_requires_proof_model() {
    let module = parse_module_as_syntax_output(
        "\
module core_case_record_pattern_boundary.\n\
\n\
pub read(value: Dynamic): Int ->\n\
    case value {\n\
        #Point { x = 1 } -> 1\n\
    }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "read")
        .expect("core read function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Case {
            scrutinee: Box::new(CoreExpr::Var("value".to_string())),
            clauses: vec![CoreCaseClause {
                pattern: CorePattern::Record {
                    name: "Point".to_string(),
                    fields: vec![CoreRecordPatternField {
                        key: "x".to_string(),
                        required: true,
                        value: CorePattern::Int(1),
                    }],
                },
                guard: None,
                body: CoreExpr::Int(1),
            }],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert!(
            core.contract_text().contains(
                "Case:core=Case(Var(value);Record(Point;x=Int(1))=>Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Case(Var(value);Record(Point;x=Int(1))=>Int(1))):proof=proof-model-required"
            ),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies case expressions with typed-but-unmodeled branch bodies are not
/// reported as Lean-covered.
///
/// Inputs:
/// - None; parses a case expression whose branch body is a binary
///   operation.
///
/// Output:
/// - Test passes when the case expression carries a typed Core payload but
///   reports proof-model-required coverage.
///
/// Transformation:
/// - Exercises the case coverage gate that requires branch bodies to map to
///   the current Lean expression subset.
#[test]
fn syntax_output_lowering_to_core_case_with_binary_body_is_lean_covered() {
    let module = parse_module_as_syntax_output(
        "\
module core_case_binary_body_boundary.\n\
\n\
pub choose(x: Int): Int ->\n\
    case x {\n\
        0 -> 1 + 2\n\
    }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "choose")
        .expect("core choose function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Case {
            scrutinee: Box::new(CoreExpr::Var("x".to_string())),
            clauses: vec![CoreCaseClause {
                pattern: CorePattern::Int(0),
                guard: None,
                body: CoreExpr::BinaryOp {
                    operator: "+".to_string(),
                    left: Box::new(CoreExpr::Int(1)),
                    right: Box::new(CoreExpr::Int(2)),
                },
            }],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::LeanCovered
    );
    assert!(
            core.contract_text().contains(
                "Case:core=Case(Var(x);Int(0)=>BinaryOp(+;Int(1), Int(2))):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Case(Var(x);Int(0)=>BinaryOp(+;Int(1), Int(2)))):proof=lean-covered"
            ),
            "contract text: {}",
            core.contract_text()
        );
}

#[test]
fn syntax_output_lowering_to_core_records_case_core_expr_with_guard() {
    let module = parse_module_as_syntax_output(
        "\
module core_case_guard_boundary.\n\
\
pub choose(value: Int): Int ->\n\
    case value {\n\
        value when is_type(value, Int) -> 1;\n\
        _ -> 0\n\
    }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "choose")
        .expect("core choose function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Case {
            scrutinee: Box::new(CoreExpr::Var("value".to_string())),
            clauses: vec![
                CoreCaseClause {
                    pattern: CorePattern::Var("value".to_string()),
                    guard: Some(CoreExpr::Intrinsic(CoreIntrinsicCall {
                        id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IsType),
                        args: vec![
                            CoreExpr::Var("value".to_string()),
                            CoreExpr::Var("Int".to_string()),
                        ],
                        return_type: CoreType::Bool,
                        effects: core_pure_effect_set(),
                        span: Span::new(37, 123),
                    })),
                    body: CoreExpr::Int(1),
                },
                CoreCaseClause {
                    pattern: CorePattern::Wildcard,
                    guard: None,
                    body: CoreExpr::Int(0),
                },
            ],
        })
    );
}

#[test]
fn syntax_output_lowering_to_core_records_if_core_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_if_boundary.\n\
\n\
pub choose(flag: Bool): Int ->\n\
    if { flag -> 1 }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "choose")
        .expect("core choose function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::If {
            clauses: vec![CoreIfClause {
                condition: CoreExpr::Var("flag".to_string()),
                body: CoreExpr::Int(1),
            }],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::LeanCovered
    );
    assert!(
            core.contract_text()
                .contains("If:core=If(Var(flag)=>Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=If(Var(flag)=>Int(1))):proof=lean-covered"),
            "contract text: {}",
            core.contract_text()
        );
}

#[test]
fn syntax_output_lowering_to_core_records_try_core_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_try_boundary.\n\
\n\
pub run(): Dynamic ->\n\
    try 1 {\n\
        value -> value\n\
    catch\n\
        reason -> reason\n\
    after\n\
        0 -> :done\n\
    }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "run")
        .expect("core run function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Try {
            body: Box::new(CoreExpr::Int(1)),
            of_clauses: vec![CoreCaseClause {
                pattern: CorePattern::Var("value".to_string()),
                guard: None,
                body: CoreExpr::Var("value".to_string()),
            }],
            catch_clauses: vec![CoreCaseClause {
                pattern: CorePattern::Var("reason".to_string()),
                guard: None,
                body: CoreExpr::Var("reason".to_string()),
            }],
            after_clause: Some(CoreTryAfter {
                trigger: Box::new(CoreExpr::Int(0)),
                body: Box::new(CoreExpr::Atom("done".to_string())),
            }),
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert!(
            core.contract_text().contains(
                "Try(Int(1);of=Var(value)=>Var(value);catch=Var(reason)=>Var(reason);after=Int(0)=>Atom(done))"
            ),
            "contract text: {}",
            core.contract_text()
        );
}

#[test]
fn syntax_output_lowering_to_core_records_case_core_expr_unsupported_branch_body() {
    let module = parse_module_as_syntax_output(
        "\
module core_case_branch_gap.\n\
\n\
pub choose(x: Int): Int ->\n\
    case x {\n\
        0 -> quote x;\n\
        0 -> x\n\
    }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "choose")
        .expect("core choose function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(function.clauses[0].body.core_expr, None);
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert_eq!(
        core.metadata.proof_readiness,
        CoreProofReadiness::RuntimeBoundary
    );
    assert_eq!(core.metadata.proof_model_required_expr_count, 1);
    assert_eq!(core.metadata.runtime_boundary_expr_count, 1);
    assert_eq!(core.metadata.lean_covered_expr_count, 3);
    assert_eq!(core.metadata.typed_core_expr_count, 3);
    assert_eq!(core.metadata.summary_only_expr_count, 2);
    assert_eq!(core.metadata.checked_preservation_expr_count, 3);
    assert_eq!(
        core.contract_text()
            .contains("Case:proof=proof-model-required"),
        true
    );
    assert_eq!(core.contract_text().contains("function=choose/1"), true);
}

#[test]
fn syntax_output_lowering_to_core_records_fun_core_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_fun_boundary.\n\
\n\
pub id_fun(): Term ->\n\
    (x) -> x.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "id_fun")
        .expect("core id_fun function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Lam {
            params: vec![CorePattern::Var("x".to_string())],
            body: Box::new(CoreExpr::Var("x".to_string())),
        })
    );
    assert_eq!(core.metadata.checked_preservation_expr_count, 2);
    assert_eq!(core.metadata.checked_preservation_pattern_count, 0);
    assert_eq!(
        core.metadata
            .checked_preservation_expr_no_runtime_bindings_count,
        1
    );
    assert_eq!(
        core.metadata
            .checked_preservation_expr_runtime_bindings_required_count,
        1
    );
    assert!(
            core.contract_text()
                .contains("Fun:core=Lam(Var(x);Var(x)):preservation=structural-core-expr(freshness=runtime-bindings-required;target=Lam(Var(x);Var(x))):proof=lean-covered"),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies lambdas with typed-but-unmodeled bodies are not reported as
/// Lean-covered.
///
/// Inputs:
/// - None; parses an anonymous function whose body is a binary operation.
///
/// Output:
/// - Test passes when the lambda carries a typed Core payload but reports
///   proof-model-required coverage.
///
/// Transformation:
/// - Exercises recursive Lean-shape validation for anonymous function
///   bodies.
#[test]
fn syntax_output_lowering_to_core_fun_with_binary_body_is_lean_covered() {
    let module = parse_module_as_syntax_output(
        "\
module core_fun_binary_body_boundary.\n\
\n\
pub add_fun(): Term ->\n\
    (x) -> x + 1.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "add_fun")
        .expect("core add_fun function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Lam {
            params: vec![CorePattern::Var("x".to_string())],
            body: Box::new(CoreExpr::BinaryOp {
                operator: "+".to_string(),
                left: Box::new(CoreExpr::Var("x".to_string())),
                right: Box::new(CoreExpr::Int(1)),
            }),
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::LeanCovered
    );
    assert!(
            core.contract_text().contains(
                "Fun:core=Lam(Var(x);BinaryOp(+;Var(x), Int(1))):preservation=structural-core-expr(freshness=runtime-bindings-required;target=Lam(Var(x);BinaryOp(+;Var(x), Int(1)))):proof=lean-covered"
            ),
            "contract text: {}",
            core.contract_text()
        );
}
