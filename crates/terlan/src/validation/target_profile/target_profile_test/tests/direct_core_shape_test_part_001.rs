use super::*;
use crate::terlan_typeck::CoreProofCoverage;

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
        shapes: HashMap::new(),
        traits: HashMap::new(),
        trait_conformances: Vec::new(),
        constructors: HashMap::new(),
        functions: HashMap::new(),
        function_overloads: HashMap::new(),
        constants: HashMap::new(),
        const_functions: HashMap::new(),
        expression_macros: HashMap::new(),
        valued_unions: HashMap::new(),
        associated_constants: HashMap::new(),
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
    assert_unresolved_constructor_violation_for_profile(
        violations,
        TargetProfile::Vm,
        calls,
        chains,
        patterns,
    );
}

/// Asserts the unresolved-constructor target-profile diagnostic is present
/// for a caller-selected target profile.
///
/// Inputs:
/// - `violations`: validation output returned by `target_profile_checks`.
/// - `profile`: target profile expected to own the diagnostic message.
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
fn assert_unresolved_constructor_violation_for_profile(
    violations: &[TargetProfileViolation],
    profile: TargetProfile,
    calls: usize,
    chains: usize,
    patterns: usize,
) {
    let violation = violations
        .iter()
        .find(|violation| violation.code == TARGET_PROFILE_UNRESOLVED_CONSTRUCTOR_CODE)
        .unwrap_or_else(|| {
            panic!(
                "{:?} profile should reject unresolved constructor candidates: {:?}",
                profile, violations
            )
        });
    assert_eq!(
        violation.message,
        unresolved_constructor_message(profile, calls, chains, patterns),
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
            source_path: None,
            syntax_contract_fingerprint: None,
        },
        imports: Vec::new(),
        exports: Vec::new(),
        types: Vec::new(),
        functions: vec![CoreFunction {
            name: "value".to_string(),
            arity: 1,
            public: true,
            generic_params: Vec::new(),
            native_operation: None,
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
        templates: Vec::new(),
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
fn target_profile_accepts_float_for_vm_profile() {
    let module = lower(
        "\
module profile_test.\n\npub f(): Int ->\n    1.0.\n",
        "src/profile_test.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should currently accept permissive coverage"
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
fn target_profile_accepts_binary_for_vm_profile() {
    let module = lower(
        "\
module profile_test_binary.\n\npub f(): Binary ->\n    \"hello\".\n",
        "src/profile_test_binary.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should accept typed binary literal core terms"
    );
}

#[test]
fn target_profile_allows_lambda_for_vm_profile() {
    let module = lower(
        "\
module profile_test_lambda.\n\npub f(): Dynamic ->\n    (x) -> x.\n",
        "src/profile_test_lambda.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow lambda-shaped core terms"
    );
}

#[test]
fn target_profile_allows_map_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_map_expr.\n\npub f(): Map ->\n    {a: 1, b: 2}.\n",
        "src/profile_test_map_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed map-expression core terms"
    );
}

#[test]
fn target_profile_allows_list_cons_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_list_cons_expr.\n\npub f(head: Int, tail: List[Int]): List[Int] ->\n    [head | tail].\n",
        "src/profile_test_list_cons_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed list-cons expression core terms"
    );
}

#[test]
fn target_profile_allows_index_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_index_expr.\n\npub f(values: List[Int]): Dynamic ->\n    values[0].\n",
        "src/profile_test_index_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed index-expression core terms"
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
fn target_profile_allows_fixed_array_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_fixed_array_expr.\n\npub f(): FixedArray[3, Int] ->\n    #[1, 2, 3].\n",
        "src/profile_test_fixed_array_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed fixed-array core terms"
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
fn target_profile_allows_list_comprehension_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_list_comprehension_expr.\n\npub f(values: List[Int]): List[Int] ->\n    [value | value <- values].\n",
        "src/profile_test_list_comprehension_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed list-comprehension core terms"
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
fn target_profile_allows_record_construct_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_record_construct_expr.\n\npub f(): Dynamic ->\n    Point { x: 1 }.\n",
        "src/profile_test_record_construct_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed record-construction core terms"
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
module profile_test_record_construct_expr_core_v0.\n\npub f(): Dynamic ->\n    Point { x: 1 }.\n",
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
