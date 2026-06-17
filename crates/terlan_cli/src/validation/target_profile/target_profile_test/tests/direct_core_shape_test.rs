use super::*;

/// Builds a Lean-covered expression summary for direct target-profile tests.
///
/// Inputs:
/// - `expr`: typed Core expression shape under test.
///
/// Output:
/// - `CoreExprSummary` carrying the expression as a typed payload.
///
/// Transformation:
/// - Wraps the expression in minimal Lean-covered summary metadata without
///   adding child summaries or runtime-boundary annotations.
fn lean_expr_summary(expr: CoreExpr) -> CoreExprSummary {
    CoreExprSummary {
        kind: "direct-test".to_string(),
        core_expr: Some(expr),
        checked_preservation_evidence: Some(expr_evidence("direct-test")),
        proof_coverage: CoreProofCoverage::LeanCovered,
        text: None,
        remote: None,
        operator: None,
        arity: 0,
        children: Vec::new(),
    }
}

/// Builds structural checked-preservation evidence for direct expression
/// profile tests.
///
/// Inputs:
/// - `target`: stable evidence target label.
///
/// Output:
/// - `CoreCheckedPreservationEvidence` for a typed expression payload.
///
/// Transformation:
/// - Creates structural expression evidence with a conservative
///   runtime-bindings-required freshness marker.
fn expr_evidence(target: &str) -> CoreCheckedPreservationEvidence {
    CoreCheckedPreservationEvidence {
        kind: CoreCheckedPreservationEvidenceKind::StructuralCoreExpr,
        freshness: CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired,
        target: target.to_string(),
    }
}

/// Builds structural checked-preservation evidence for direct pattern
/// profile tests.
///
/// Inputs:
/// - `target`: stable evidence target label.
///
/// Output:
/// - `CoreCheckedPreservationEvidence` for a typed pattern payload.
///
/// Transformation:
/// - Creates structural pattern evidence with a conservative
///   runtime-bindings-required freshness marker.
fn pattern_evidence(target: &str) -> CoreCheckedPreservationEvidence {
    CoreCheckedPreservationEvidence {
        kind: CoreCheckedPreservationEvidenceKind::StructuralCorePattern,
        freshness: CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired,
        target: target.to_string(),
    }
}

/// Builds zeroed Lean-covered module metadata for direct profile tests.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - `CoreModuleMetadata` with no unresolved constructor candidates.
///
/// Transformation:
/// - Creates metadata sufficient for target-profile validation, where only
///   constructor-resolution counters are consumed by the validator.
fn lean_core_metadata() -> CoreModuleMetadata {
    CoreModuleMetadata {
        interface_function_count: 1,
        interface_type_count: 0,
        constructor_count: 0,
        proof_readiness: CoreProofReadiness::LeanCovered,
        lean_covered_expr_count: 1,
        partial_expr_count: 0,
        proof_model_required_expr_count: 0,
        runtime_boundary_expr_count: 0,
        artifact_only_expr_count: 0,
        lean_covered_pattern_count: 1,
        partial_pattern_count: 0,
        proof_model_required_pattern_count: 0,
        runtime_boundary_pattern_count: 0,
        artifact_only_pattern_count: 0,
        typed_core_expr_count: 1,
        summary_only_expr_count: 0,
        typed_core_pattern_count: 1,
        summary_only_pattern_count: 0,
        typed_core_type_count: 0,
        summary_only_type_count: 0,
        checked_preservation_expr_count: 0,
        checked_preservation_pattern_count: 0,
        checked_preservation_expr_structural_count: 0,
        checked_preservation_pattern_structural_count: 0,
        checked_preservation_expr_no_runtime_bindings_count: 0,
        checked_preservation_pattern_no_runtime_bindings_count: 0,
        checked_preservation_expr_runtime_bindings_required_count: 0,
        checked_preservation_pattern_runtime_bindings_required_count: 0,
        resolved_constructor_call_identity_count: 0,
        resolved_constructor_chain_identity_count: 0,
        resolved_constructor_pattern_identity_count: 0,
        unresolved_constructor_call_candidate_count: 0,
        unresolved_constructor_chain_candidate_count: 0,
        unresolved_constructor_pattern_candidate_count: 0,
    }
}

/// Builds an empty interface for direct CoreIR profile tests.
///
/// Inputs:
/// - `module`: module name to attach to the interface.
///
/// Output:
/// - Empty `ModuleInterface` with no public declarations.
///
/// Transformation:
/// - Creates deterministic empty declaration maps and sets for tests that
///   do not inspect interface rendering.
fn empty_interface(module: &str) -> ModuleInterface {
    ModuleInterface {
        module: module.to_string(),
        docs: Vec::new(),
        public_types: HashSet::new(),
        private_types: HashSet::new(),
        opaque_types: HashSet::new(),
        type_params: HashMap::new(),
        type_bodies: HashMap::new(),
        struct_fields: HashMap::new(),
        type_docs: HashMap::new(),
        traits: HashMap::new(),
        trait_conformances: Vec::new(),
        constructors: HashMap::new(),
        functions: HashMap::new(),
    }
}

/// Builds a minimal Core module around one typed expression body.
///
/// Inputs:
/// - `body`: typed Core expression to validate as a function body.
///
/// Output:
/// - `CoreModule` containing one public unary function.
///
/// Transformation:
/// - Wraps the body in a Lean-covered clause with one variable pattern and
///   zero unresolved constructor metadata.
fn module_with_core_body(body: CoreExpr) -> CoreModule {
    module_with_core_body_and_evidence(
        body,
        Some(expr_evidence("direct-test")),
        vec![Some(pattern_evidence("input"))],
    )
}

/// Builds a direct Core module with caller-selected unresolved constructor
/// metadata counters.
///
/// Inputs:
/// - `call_candidates`: unresolved constructor-call candidate count.
/// - `chain_candidates`: unresolved constructor-chain candidate count.
/// - `pattern_candidates`: unresolved constructor-pattern candidate count.
///
/// Output:
/// - `CoreModule` with a Lean-covered integer body and the provided
///   unresolved constructor metadata counts.
///
/// Transformation:
/// - Starts from the standard direct CoreIR test fixture and mutates only
///   constructor-resolution counters, isolating target-profile validation
///   from parser and typechecker diagnostics.
fn module_with_unresolved_constructor_candidates(
    call_candidates: usize,
    chain_candidates: usize,
    pattern_candidates: usize,
) -> CoreModule {
    let mut module = module_with_core_body(CoreExpr::Int(0));
    module.metadata.unresolved_constructor_call_candidate_count = call_candidates;
    module.metadata.unresolved_constructor_chain_candidate_count = chain_candidates;
    module
        .metadata
        .unresolved_constructor_pattern_candidate_count = pattern_candidates;
    module
}

/// Asserts the unresolved-constructor target-profile diagnostic is present
/// with exact counter details.
///
/// Inputs:
/// - `violations`: validation output returned by `target_profile_checks`.
/// - `calls`: expected unresolved constructor-call candidate count.
/// - `chains`: expected unresolved constructor-chain candidate count.
/// - `patterns`: expected unresolved constructor-pattern candidate count.
///
/// Output:
/// - Test assertion only; no compiler artifacts are modified.
///
/// Transformation:
/// - Locates the shared unresolved-constructor diagnostic by code and
///   compares its formatted message against the expected profile/count
///   payload.
fn assert_unresolved_constructor_violation(
    violations: &[TargetProfileViolation],
    calls: usize,
    chains: usize,
    patterns: usize,
) {
    let violation = violations
        .iter()
        .find(|violation| violation.code == TARGET_PROFILE_UNRESOLVED_CONSTRUCTOR_CODE)
        .unwrap_or_else(|| {
            panic!(
                "Erlang profile should reject unresolved constructor candidates: {:?}",
                violations
            )
        });
    assert_eq!(
        violation.message,
        unresolved_constructor_message(TargetProfile::Erlang, calls, chains, patterns),
        "unexpected unresolved constructor diagnostic message"
    );
}

/// Builds a minimal Core module around one typed expression body and caller
/// supplied preservation evidence.
///
/// Inputs:
/// - `body`: typed Core expression to validate as a function body.
/// - `body_evidence`: checked-preservation evidence attached to the body
///   summary.
/// - `pattern_evidence`: checked-preservation evidence attached to the
///   single function-clause pattern.
///
/// Output:
/// - `CoreModule` containing one public unary function.
///
/// Transformation:
/// - Wraps the body in a Lean-covered clause with one variable pattern and
///   caller-controlled preservation evidence.
fn module_with_core_body_and_evidence(
    body: CoreExpr,
    body_evidence: Option<CoreCheckedPreservationEvidence>,
    pattern_evidence: Vec<Option<CoreCheckedPreservationEvidence>>,
) -> CoreModule {
    let module_name = "profile_test_core_v0_direct".to_string();
    CoreModule {
        schema: CORE_IR_SCHEMA.to_string(),
        module: module_name.clone(),
        source: CoreSourceIdentity {
            source_kind: "direct_profile_test".to_string(),
            syntax_contract_fingerprint: None,
        },
        imports: Vec::new(),
        exports: Vec::new(),
        types: Vec::new(),
        functions: vec![CoreFunction {
            name: "value".to_string(),
            arity: 1,
            public: true,
            params: vec![CoreParam {
                name: "input".to_string(),
                ty: "Dynamic".to_string(),
                core_ty: None,
            }],
            return_type: "Dynamic".to_string(),
            core_return_type: None,
            clauses: vec![CoreFunctionClause {
                patterns: vec!["input".to_string()],
                core_patterns: vec![Some(CorePattern::Var("input".to_string()))],
                pattern_proof_coverage: vec![CoreProofCoverage::LeanCovered],
                pattern_checked_preservation_evidence: pattern_evidence,
                guard: None,
                body: CoreExprSummary {
                    checked_preservation_evidence: body_evidence,
                    ..lean_expr_summary(body)
                },
            }],
        }],
        constructors: Vec::new(),
        trait_conformances: Vec::new(),
        metadata: lean_core_metadata(),
        interface: empty_interface(&module_name),
    }
}

/// Verifies CoreV0 accepts the documented portable expression and pattern
/// subset.
///
/// Inputs:
/// - A directly constructed typed Core expression using case, if, call,
///   lambda, field access, constructor call, tuple/list/list-cons, and
///   arithmetic/comparison operators.
///
/// Output:
/// - Test assertion only; no source fixtures or compiler artifacts are
///   written.
///
/// Transformation:
/// - Wraps accepted CoreIR shapes in a minimal `CoreModule` and validates
///   it under `TargetProfile::CoreV0`.
#[test]
fn target_profile_accepts_documented_core_v0_shape_matrix() {
    let body = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::Var("input".to_string())),
        clauses: vec![
            CoreCaseClause {
                pattern: CorePattern::Tuple(vec![
                    CorePattern::Int(0),
                    CorePattern::Atom("zero".to_string()),
                ]),
                guard: None,
                body: CoreExpr::Tuple(vec![
                    CoreExpr::Binary("zero".to_string()),
                    CoreExpr::List(vec![CoreExpr::Int(0), CoreExpr::Int(1)]),
                    CoreExpr::UnaryOp {
                        operator: "-".to_string(),
                        operand: Box::new(CoreExpr::Int(1)),
                    },
                ]),
            },
            CoreCaseClause {
                pattern: CorePattern::Constructor {
                    name: "Ok".to_string(),
                    constructor_identity: Some("Ok/1".to_string()),
                    args: vec![CorePattern::List(vec![CorePattern::Var(
                        "value".to_string(),
                    )])],
                },
                guard: None,
                body: CoreExpr::If {
                    clauses: vec![
                        CoreIfClause {
                            condition: CoreExpr::BinaryOp {
                                operator: "==".to_string(),
                                left: Box::new(CoreExpr::Var("value".to_string())),
                                right: Box::new(CoreExpr::Int(0)),
                            },
                            body: CoreExpr::Call {
                                function: "identity".to_string(),
                                args: vec![CoreExpr::ListCons {
                                    head: Box::new(CoreExpr::Int(1)),
                                    tail: Box::new(CoreExpr::List(Vec::new())),
                                }],
                            },
                        },
                        CoreIfClause {
                            condition: CoreExpr::Atom("true".to_string()),
                            body: CoreExpr::ConstructorCall {
                                constructor: "Ok".to_string(),
                                constructor_identity: Some("Ok/1".to_string()),
                                args: vec![CoreExpr::Lam {
                                    params: vec![CorePattern::Var("x".to_string())],
                                    body: Box::new(CoreExpr::FieldAccess {
                                        base: Box::new(CoreExpr::Var("x".to_string())),
                                        field: "name".to_string(),
                                    }),
                                }],
                            },
                        },
                    ],
                },
            },
        ],
    };
    let module = module_with_core_body(body);

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0.is_empty(),
        "CoreV0 profile should accept the documented portable shape matrix: {:?}",
        core_v0
    );
}

/// Verifies CoreV0 rejects typed expression payloads without
/// checked-preservation evidence.
///
/// Inputs:
/// - A directly constructed Core module with a Lean-covered typed
///   expression payload and no expression evidence.
///
/// Output:
/// - Test assertion only; no source fixtures or compiler artifacts are
///   written.
///
/// Transformation:
/// - Runs target-profile validation over the direct CoreIR module and
///   checks for the missing-evidence diagnostic.
#[test]
fn target_profile_rejects_missing_expr_evidence_for_core_v0_profile() {
    let module = module_with_core_body_and_evidence(
        CoreExpr::Int(1),
        None,
        vec![Some(pattern_evidence("input"))],
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0.iter().any(
            |violation| violation.code == "target_profile_missing_evidence"
                && violation.message.contains("typed expression payload")
        ),
        "CoreV0 profile should reject missing expression evidence: {:?}",
        core_v0
    );
}

/// Verifies CoreV0 rejects typed pattern payloads without
/// checked-preservation evidence.
///
/// Inputs:
/// - A directly constructed Core module with a Lean-covered typed pattern
///   payload and no pattern evidence.
///
/// Output:
/// - Test assertion only; no source fixtures or compiler artifacts are
///   written.
///
/// Transformation:
/// - Runs target-profile validation over the direct CoreIR module and
///   checks for the missing-evidence diagnostic.
#[test]
fn target_profile_rejects_missing_pattern_evidence_for_core_v0_profile() {
    let module = module_with_core_body_and_evidence(
        CoreExpr::Int(1),
        Some(expr_evidence("body")),
        vec![None],
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0.iter().any(
            |violation| violation.code == "target_profile_missing_evidence"
                && violation.message.contains("typed pattern payload")
        ),
        "CoreV0 profile should reject missing pattern evidence: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_accepts_float_for_erlang_profile() {
    let module = lower(
        "\
module profile_test.\n\npub f(): Int ->\n    1.0.\n",
        "src/profile_test.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should currently accept permissive coverage"
    );
}

/// Verifies CoreV0 rejects float literals.
///
/// Inputs:
/// - Source containing a typed float literal expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_float_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_float_core_v0.\n\npub f(): Int ->\n    1.0.\n",
        "src/profile_test_float_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("Float")),
        "CoreV0 profile should reject float core terms: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_accepts_binary_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_binary.\n\npub f(): Binary ->\n    \"hello\".\n",
        "src/profile_test_binary.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should accept typed binary literal core terms"
    );
}

#[test]
fn target_profile_allows_lambda_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_lambda.\n\npub f(): Dynamic ->\n    (x) -> x.\n",
        "src/profile_test_lambda.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow lambda-shaped core terms"
    );
}

#[test]
fn target_profile_allows_map_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_map_expr.\n\npub f(): Map ->\n    #{a := 1, b => 2}.\n",
        "src/profile_test_map_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed map-expression core terms"
    );
}

#[test]
fn target_profile_allows_list_cons_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_list_cons_expr.\n\npub f(head: Int, tail: List[Int]): List[Int] ->\n    [head | tail].\n",
        "src/profile_test_list_cons_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed list-cons expression core terms"
    );
}

#[test]
fn target_profile_allows_index_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_index_expr.\n\npub f(values: List[Int]): Dynamic ->\n    values[0].\n",
        "src/profile_test_index_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed index-expression core terms"
    );
}

/// Verifies CoreV0 rejects source index expressions after formal lowering.
///
/// Inputs:
/// - Source containing a typed index expression.
///
/// Output:
/// - Test passes when target-profile validation reports the proof-required
///   trait-backed expression as unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path. Bracket
///   syntax becomes `IndexGet.get_at(...)`, which currently carries
///   proof-required coverage and is outside the `core-v0` subset.
#[test]
fn target_profile_rejects_index_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_index_expr_core_v0.\n\npub f(values: List[Int]): Dynamic ->\n    values[0].\n",
        "src/profile_test_index_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("ProofModelRequired")),
        "CoreV0 profile should reject proof-required index lowering: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_allows_fixed_array_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_fixed_array_expr.\n\npub f(): FixedArray[3, Int] ->\n    #[1, 2, 3].\n",
        "src/profile_test_fixed_array_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed fixed-array core terms"
    );
}

/// Verifies CoreV0 rejects fixed-array literals.
///
/// Inputs:
/// - Source containing a typed fixed-array literal expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_fixed_array_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_fixed_array_expr_core_v0.\n\npub f(): FixedArray[3, Int] ->\n    #[1, 2, 3].\n",
        "src/profile_test_fixed_array_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("FixedArray")),
        "CoreV0 profile should reject fixed-array core terms: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_allows_list_comprehension_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_list_comprehension_expr.\n\npub f(values: List[Int]): List[Int] ->\n    [value | value <- values].\n",
        "src/profile_test_list_comprehension_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed list-comprehension core terms"
    );
}

/// Verifies CoreV0 rejects list-comprehension expressions.
///
/// Inputs:
/// - Source containing a typed list-comprehension expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_list_comprehension_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_list_comprehension_expr_core_v0.\n\npub f(values: List[Int]): List[Int] ->\n    [value | value <- values].\n",
        "src/profile_test_list_comprehension_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("ListComprehension")),
        "CoreV0 profile should reject list-comprehension core terms: {:?}",
        core_v0
    );
}

/// Verifies CoreV0 rejects list-comprehension expressions sourced from
/// generic `Iterable[C, T]` implementations.
///
/// Inputs:
/// - Source containing a generic iterable-comprehension that is accepted by
///   formal typechecking.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_iterable_list_comprehension_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_iterable_list_comprehension_expr_core_v0.\n\n\
pub type Iterator[T] = List[T].\n\
\n\
pub trait Iterable[C, T] {\n\
iterator(collection: C): Iterator[T].\n\
}.\n\
\n\
pub struct IntCollection implements Iterable[IntCollection, Int] {\n\
values: List[Int]\n\
}.\n\n\
pub values(items: IntCollection): List[Int] ->\n     [value | value <- items, value > 0].\n",
        "src/profile_test_iterable_list_comprehension_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("ListComprehension")),
        "CoreV0 profile should reject generic iterable list-comprehension core terms: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_allows_record_construct_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_record_construct_expr.\n\npub f(): Dynamic ->\n    #Point { x = 1 }.\n",
        "src/profile_test_record_construct_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed record-construction core terms"
    );
}

/// Verifies CoreV0 rejects record construction expressions.
///
/// Inputs:
/// - Source containing a typed record construction expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_record_construct_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_record_construct_expr_core_v0.\n\npub f(): Dynamic ->\n    #Point { x = 1 }.\n",
        "src/profile_test_record_construct_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("RecordConstruct")),
        "CoreV0 profile should reject record-construction core terms: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_allows_field_access_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_field_access_expr.\n\npub f(point: Point): Dynamic ->\n    point.x.\n",
        "src/profile_test_field_access_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed field-access core terms"
    );
}

#[test]
fn target_profile_allows_record_access_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_record_access_expr.\n\npub f(point: Point): Dynamic ->\n    point#Point.x.\n",
        "src/profile_test_record_access_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed record-access core terms"
    );
}

/// Verifies CoreV0 rejects record access expressions.
///
/// Inputs:
/// - Source containing a typed record access expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_record_access_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_record_access_expr_core_v0.\n\npub f(point: Point): Dynamic ->\n    point#Point.x.\n",
        "src/profile_test_record_access_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("RecordAccess")),
        "CoreV0 profile should reject record-access core terms: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_allows_record_update_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_record_update_expr.\n\npub f(point: Point): Dynamic ->\n    point#Point { x = 1 }.\n",
        "src/profile_test_record_update_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed record-update core terms"
    );
}

/// Verifies CoreV0 rejects record update expressions.
///
/// Inputs:
/// - Source containing a typed record update expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_record_update_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_record_update_expr_core_v0.\n\npub f(point: Point): Dynamic ->\n    point#Point { x = 1 }.\n",
        "src/profile_test_record_update_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("RecordUpdate")),
        "CoreV0 profile should reject record-update core terms: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_allows_template_instantiate_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_template_instantiate_expr.\n\npub f(): Dynamic ->\n    UserCard{ name = \"Ada\" }.\n",
        "src/profile_test_template_instantiate_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed template-instantiation core terms"
    );
}

/// Verifies CoreV0 rejects template instantiation expressions.
///
/// Inputs:
/// - Source containing a typed template instantiation expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_template_instantiate_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_template_instantiate_expr_core_v0.\n\npub f(): Dynamic ->\n    UserCard{ name = \"Ada\" }.\n",
        "src/profile_test_template_instantiate_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("TemplateInstantiate")),
        "CoreV0 profile should reject template-instantiation core terms: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_allows_constructor_chain_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_constructor_chain_expr.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub f(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
        "src/profile_test_constructor_chain_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed constructor-chain core terms"
    );
}

/// Verifies CoreV0 rejects partial constructor-chain expressions.
///
/// Inputs:
/// - Source containing a declared constructor-chain expression whose base
///   constructor identity resolves but whose proof coverage remains partial.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_constructor_chain_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_constructor_chain_expr_core_v0.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub f(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
        "src/profile_test_constructor_chain_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("constructor chain")),
        "CoreV0 profile should reject partial constructor-chain core terms: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_allows_resolved_constructor_call_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_constructor_call_candidate.\n\npub constructor Ok {\n    (value: Int): Dynamic -> value\n}.\n\npub f(value: Int): Dynamic ->\n    Ok(value).\n",
        "src/profile_test_constructor_call_candidate.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow resolved constructor-call core terms"
    );
}

/// Verifies unresolved constructor-call metadata blocks backend validation.
///
/// Inputs:
/// - A directly constructed Lean-covered Core module whose metadata reports
///   one unresolved constructor-call candidate.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unresolved_constructor`.
///
/// Transformation:
/// - Uses the unresolved-constructor fixture helper to isolate the call
///   metadata counter from parser and typechecker diagnostics.
#[test]
fn target_profile_rejects_unresolved_constructor_call_candidate() {
    let module = module_with_unresolved_constructor_candidates(1, 0, 0);

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert_unresolved_constructor_violation(&erlang, 1, 0, 0);
}

/// Verifies unresolved constructor-pattern metadata blocks backend validation.
///
/// Inputs:
/// - A directly constructed Lean-covered Core module whose metadata reports
///   one unresolved constructor-pattern candidate.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unresolved_constructor`.
///
/// Transformation:
/// - Uses the unresolved-constructor fixture helper to isolate the pattern
///   metadata counter from parser and typechecker diagnostics.
#[test]
fn target_profile_rejects_unresolved_constructor_pattern_candidate() {
    let module = module_with_unresolved_constructor_candidates(0, 0, 1);

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert_unresolved_constructor_violation(&erlang, 0, 0, 1);
}

/// Verifies unresolved constructor-chain metadata blocks backend validation.
///
/// Inputs:
/// - A directly constructed Lean-covered Core module whose metadata reports
///   one unresolved constructor-chain candidate.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unresolved_constructor`.
///
/// Transformation:
/// - Uses the unresolved-constructor fixture helper to isolate the chain
///   metadata counter from parser and typechecker diagnostics.
#[test]
fn target_profile_rejects_unresolved_constructor_chain_candidate() {
    let module = module_with_unresolved_constructor_candidates(0, 1, 0);

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert_unresolved_constructor_violation(&erlang, 0, 1, 0);
}

#[test]
fn target_profile_rejects_remote_fun_ref_source_syntax_before_profile_validation() {
    let parsed = parse_module_as_syntax_output(
        "\
module profile_test_remote_fun_ref_expr.\n\npub f(): Dynamic ->\n    fun erlang:abs/1.\n",
    );

    assert!(
        parsed.is_err(),
        "remote fun references are backend output syntax, not canonical Terlan source"
    );
}

/// Verifies CoreV0 rejects remote function references.
///
/// Inputs:
/// - Source containing a typed remote function reference expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_remote_fun_ref_expr_for_core_v0_profile() {
    let parsed = parse_module_as_syntax_output(
        "\
module profile_test_remote_fun_ref_expr_core_v0.\n\npub f(): Dynamic ->\n    fun erlang:abs/1.\n",
    );

    assert!(
        parsed.is_err(),
        "remote fun references are backend output syntax, not canonical Terlan source"
    );
}

#[test]
fn target_profile_allows_remote_call_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_remote_call_expr.\n\npub f(): Int ->\n    erlang.Math.abs(1).\n",
        "src/profile_test_remote_call_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed remote-call core terms"
    );
}

/// Verifies CoreV0 rejects proof-model-required remote calls.
///
/// Inputs:
/// - Source containing a typed remote-call expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_remote_call_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_remote_call_expr_core_v0.\n\npub f(): Int ->\n    erlang.Math.abs(1).\n",
        "src/profile_test_remote_call_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("remote call")),
        "CoreV0 profile should reject proof-model-required remote-call core terms: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_allows_if_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_if_expr.\n\npub f(flag: Bool): Int ->\n    if { flag -> 1; true -> 0 }.\n",
        "src/profile_test_if_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed if-expression core terms"
    );
}

#[test]
fn target_profile_allows_try_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_try_expr.\n\npub f(): Dynamic ->\n    try 1 {\n        value -> value\n    catch\n        reason -> reason\n    after\n        0 -> :done\n    }.\n",
        "src/profile_test_try_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed try-expression core terms"
    );
}

/// Verifies CoreV0 rejects try expressions.
///
/// Inputs:
/// - Source containing a typed try expression with `of`, `catch`, and
///   `after` branches.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_try_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_try_expr_core_v0.\n\npub f(): Dynamic ->\n    try 1 {\n        value -> value\n    catch\n        reason -> reason\n    after\n        0 -> :done\n    }.\n",
        "src/profile_test_try_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("Try")),
        "CoreV0 profile should reject try core terms: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_allows_unary_op_expr_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_unary_op_expr.\n\npub f(value: Int): Int ->\n    -value.\n",
        "src/profile_test_unary_op_expr.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed unary-op core terms"
    );
}

#[test]
fn target_profile_allows_map_pattern_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_map_pattern.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        #{a = x} -> x;\n        _ -> value\n    }.\n",
        "src/profile_test_map_pattern.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed map-pattern core terms"
    );
}

/// Verifies Erlang accepts float patterns.
///
/// Inputs:
/// - Source containing a typed case expression with a float pattern.
///
/// Output:
/// - Test passes when target-profile validation reports no Erlang-profile
///   violations for the lowered module.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   permissive Erlang-profile validation without mutating compiler
///   artifacts.
#[test]
fn target_profile_allows_float_pattern_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_float_pattern.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        1.0 -> :float;\n        _ -> :other\n    }.\n",
        "src/profile_test_float_pattern.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed float-pattern core terms"
    );
}

/// Verifies CoreV0 rejects float patterns.
///
/// Inputs:
/// - Source containing a typed case expression with a float pattern.
///
/// Output:
/// - Test passes when target-profile validation reports the pattern as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_float_pattern_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_float_pattern_core_v0.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        1.0 -> :float;\n        _ -> :other\n    }.\n",
        "src/profile_test_float_pattern_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("Float")),
        "CoreV0 profile should reject float-pattern core terms: {:?}",
        core_v0
    );
}

/// Verifies CoreV0 rejects map patterns.
///
/// Inputs:
/// - Source containing a typed case expression with a map pattern.
///
/// Output:
/// - Test passes when target-profile validation reports the pattern as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_map_pattern_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_map_pattern_core_v0.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        #{a = x} -> x;\n        _ -> value\n    }.\n",
        "src/profile_test_map_pattern_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("Map")),
        "CoreV0 profile should reject map-pattern core terms: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_allows_list_cons_pattern_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_list_cons_pattern.\n\npub f(value: List[Int]): Dynamic ->\n    case value {\n        [head | tail] -> head;\n        _ -> value\n    }.\n",
        "src/profile_test_list_cons_pattern.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed list-cons pattern core terms"
    );
}

/// Verifies CoreV0 rejects list-cons patterns.
///
/// Inputs:
/// - Source containing a typed case expression with a list-cons pattern.
///
/// Output:
/// - Test passes when target-profile validation reports the pattern as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_list_cons_pattern_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_list_cons_pattern_core_v0.\n\npub f(value: List[Int]): Dynamic ->\n    case value {\n        [head | tail] -> head;\n        _ -> value\n    }.\n",
        "src/profile_test_list_cons_pattern_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("ListCons")),
        "CoreV0 profile should reject list-cons pattern core terms: {:?}",
        core_v0
    );
}

#[test]
fn target_profile_allows_record_pattern_for_erlang_profile() {
    let module = lower(
        "\
module profile_test_record_pattern.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        #Point { x = x } -> x;\n        _ -> value\n    }.\n",
        "src/profile_test_record_pattern.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        erlang.is_empty(),
        "Erlang profile should allow typed record-pattern core terms"
    );
}

/// Verifies CoreV0 rejects record patterns.
///
/// Inputs:
/// - Source containing a typed case expression with a record pattern.
///
/// Output:
/// - Test passes when target-profile validation reports the pattern as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
fn target_profile_rejects_record_pattern_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_record_pattern_core_v0.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        #Point { x = x } -> x;\n        _ -> value\n    }.\n",
        "src/profile_test_record_pattern_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("Record")),
        "CoreV0 profile should reject record-pattern core terms: {:?}",
        core_v0
    );
}

/// Verifies the portable CoreIR v0 profile accepts a Lean-covered arithmetic
/// expression.
///
/// Inputs:
/// - A source module whose function body lowers to typed `BinaryOp(-)`.
///
/// Output:
/// - Test assertion only; no compiler artifacts are written.
///
/// Transformation:
/// - Lowers source through syntax output to CoreIR, then validates it under
///   the `core-v0` target profile.
#[test]
fn target_profile_accepts_subtraction_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_core_v0_sub.\n\npub f(x: Int, y: Int): Int ->\n    x - y.\n",
        "src/profile_test_core_v0_sub.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0.is_empty(),
        "Core v0 profile should accept Lean-covered subtraction: {:?}",
        core_v0
    );
}

/// Verifies the portable CoreIR v0 profile rejects a broad backend-specific
/// expression form while the Erlang profile remains permissive.
///
/// Inputs:
/// - A source module whose function body lowers to typed map CoreIR.
///
/// Output:
/// - Test assertion only; no compiler artifacts are written.
///
/// Transformation:
/// - Lowers source through syntax output to CoreIR, checks that Erlang still
///   accepts the shape, then checks that `core-v0` reports unsupported
///   expression coverage or shape.
#[test]
fn target_profile_rejects_map_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_core_v0_map.\n\npub f(): Map ->\n    #{a := 1}.\n",
        "src/profile_test_core_v0_map.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);
    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        erlang.is_empty(),
        "Erlang profile should remain permissive for map core terms"
    );
    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"),
        "Core v0 profile should reject map core terms: {:?}",
        core_v0
    );
}
