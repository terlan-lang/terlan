use terlan_hir::syntax_module_output_to_interface;
use terlan_syntax::{span::Span, SyntaxModuleOutput};
use terlan_typeck::{
    CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CoreModule, CoreModuleMetadata,
    CorePrimitiveIntrinsic, CoreRuntimeCapability, CoreSourceIdentity, CoreType, CORE_IR_SCHEMA,
};

/// Builds a minimal syntax-aware CoreIR module for backend gate tests.
///
/// Inputs:
/// - `module`: parsed syntax-output fixture.
///
/// Output:
/// - CoreIR module with matching schema, module name, source identity, and
///   interface payload.
///
/// Transformation:
/// - Copies syntax-output identity into CoreIR and derives the public
///   interface through the existing HIR adapter; declaration vectors are
///   left empty because these tests exercise backend identity gating only.
pub(super) fn test_core_module_for_syntax(module: &SyntaxModuleOutput) -> CoreModule {
    CoreModule {
        schema: CORE_IR_SCHEMA.to_string(),
        module: module.module_name.clone(),
        source: CoreSourceIdentity {
            source_kind: format!("{:?}", module.source_kind),
            syntax_contract_fingerprint: Some(module.syntax_contract.fingerprint.clone()),
        },
        imports: Vec::new(),
        exports: Vec::new(),
        types: Vec::new(),
        functions: Vec::new(),
        constructors: Vec::new(),
        trait_conformances: Vec::new(),
        metadata: CoreModuleMetadata {
            interface_function_count: 0,
            interface_type_count: 0,
            constructor_count: 0,
            proof_readiness: terlan_typeck::CoreProofReadiness::NoExpressions,
            lean_covered_expr_count: 0,
            partial_expr_count: 0,
            proof_model_required_expr_count: 0,
            runtime_boundary_expr_count: 0,
            artifact_only_expr_count: 0,
            lean_covered_pattern_count: 0,
            partial_pattern_count: 0,
            proof_model_required_pattern_count: 0,
            runtime_boundary_pattern_count: 0,
            artifact_only_pattern_count: 0,
            typed_core_expr_count: 0,
            summary_only_expr_count: 0,
            typed_core_pattern_count: 0,
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
        },
        interface: syntax_module_output_to_interface(module),
    }
}

/// Builds a pure CoreIR string intrinsic call for Erlang backend tests.
///
/// Inputs:
/// - `intrinsic`: string primitive intrinsic identity under test.
/// - `args`: CoreIR argument expressions supplied to the intrinsic.
/// - `return_type`: typed CoreIR result contract for the intrinsic.
///
/// Output:
/// - CoreIR intrinsic call with a pure effect set and empty source span.
///
/// Transformation:
/// - Wraps the primitive identity and arguments in the production CoreIR
///   intrinsic-call shape used by source lowering.
pub(super) fn test_string_intrinsic_call(
    intrinsic: CorePrimitiveIntrinsic,
    args: Vec<CoreExpr>,
    return_type: CoreType,
) -> CoreIntrinsicCall {
    test_primitive_intrinsic_call(intrinsic, args, return_type)
}

/// Builds a pure CoreIR primitive intrinsic call for Erlang backend tests.
///
/// Inputs:
/// - `intrinsic`: primitive intrinsic identity under test.
/// - `args`: CoreIR argument expressions supplied to the intrinsic.
/// - `return_type`: typed CoreIR result contract for the intrinsic.
///
/// Output:
/// - CoreIR intrinsic call with a pure effect set and empty source span.
///
/// Transformation:
/// - Wraps the primitive identity and arguments in the production CoreIR
///   intrinsic-call shape used by source lowering.
pub(super) fn test_primitive_intrinsic_call(
    intrinsic: CorePrimitiveIntrinsic,
    args: Vec<CoreExpr>,
    return_type: CoreType,
) -> CoreIntrinsicCall {
    CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(intrinsic),
        args,
        return_type,
        effects: CoreEffectSet {
            effects: vec!["pure".to_string()],
        },
        span: Span::new(0, 0),
    }
}

/// Builds an effectful CoreIR runtime capability call for Erlang backend tests.
///
/// Inputs:
/// - `capability`: runtime capability identity under test.
/// - `args`: CoreIR argument expressions supplied to the capability.
/// - `return_type`: typed CoreIR result contract for the capability.
///
/// Output:
/// - CoreIR intrinsic call with an `io` effect set and empty source span.
///
/// Transformation:
/// - Wraps the runtime capability identity and arguments in the production
///   CoreIR intrinsic-call shape used by source lowering.
pub(super) fn test_runtime_capability_call(
    capability: CoreRuntimeCapability,
    args: Vec<CoreExpr>,
    return_type: CoreType,
) -> CoreIntrinsicCall {
    CoreIntrinsicCall {
        id: CoreIntrinsicId::Runtime(capability),
        args,
        return_type,
        effects: CoreEffectSet {
            effects: vec!["io".to_string()],
        },
        span: Span::new(0, 0),
    }
}
