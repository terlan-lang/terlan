//! Terlan-owned machine-independent IR for direct native object emission.

/// Canonical local function identity used by NativeIR analysis and specialization.
type LocalFunctionIdentity = (String, usize);
type QualifiedFunctionIdentity = (String, String, usize);

mod aggregate_types;
#[cfg(test)]
#[path = "native_ir/aot3_conformance_test.rs"]
#[cfg(test)]
mod aot3_conformance_test;
mod application;
mod application_admission;
#[cfg(test)]
#[path = "native_ir/application_admission_test.rs"]
#[cfg(test)]
mod application_admission_test;
mod application_calls;
mod atom_alias_values;
mod atom_inventory;
#[cfg(test)]
#[path = "native_ir/atom_inventory_test.rs"]
#[cfg(test)]
mod atom_inventory_test;
#[cfg(test)]
#[path = "native_ir/binding_identity_source_test.rs"]
#[cfg(test)]
mod binding_identity_source_test;
mod call_composition;
#[cfg(test)]
#[path = "native_ir/call_composition_test.rs"]
#[cfg(test)]
mod call_composition_test;
mod callee_scalar_replacement;
#[cfg(test)]
#[path = "native_ir/callee_scalar_replacement_test.rs"]
#[cfg(test)]
mod callee_scalar_replacement_test;
mod case_lowering;
#[cfg(test)]
#[path = "native_ir/case_lowering_test.rs"]
#[cfg(test)]
mod case_lowering_test;
#[cfg(test)]
#[path = "native_ir/cast_lowering_test.rs"]
#[cfg(test)]
mod cast_lowering_test;
mod closure_conversion;
#[cfg(test)]
#[path = "native_ir/closure_conversion_test.rs"]
#[cfg(test)]
mod closure_conversion_test;
mod closure_invocation;
mod codegen_policy;
#[cfg(test)]
#[path = "native_ir/codegen_policy_test.rs"]
#[cfg(test)]
mod codegen_policy_test;
mod collection_intrinsic_specialization;
#[cfg(test)]
#[path = "native_ir/collection_intrinsic_specialization_test.rs"]
#[cfg(test)]
mod collection_intrinsic_specialization_test;
mod collection_values;
#[cfg(test)]
#[path = "native_ir/collection_values_test.rs"]
#[cfg(test)]
mod collection_values_test;
mod collections;
#[cfg(test)]
mod composed_continuation;
mod constructor_chain;
#[cfg(test)]
#[path = "native_ir/constructor_lowering_test.rs"]
#[cfg(test)]
mod constructor_lowering_test;
mod constructors;
mod continuation_sharing;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use continuation_sharing::is_materialized_continuation_module;
#[cfg(test)]
#[path = "native_ir/bool_intrinsic_native_test.rs"]
mod bool_intrinsic_native_test;
#[cfg(test)]
#[path = "native_ir/continuation_sharing_test.rs"]
#[cfg(test)]
mod continuation_sharing_test;
mod control;
mod control_completion;
#[cfg(test)]
#[path = "native_ir/control_test.rs"]
#[cfg(test)]
mod control_test;
#[cfg(feature = "native-codegen")]
mod cranelift;
mod dynamic_return;
#[cfg(test)]
#[path = "native_ir/dynamic_return_test.rs"]
#[cfg(test)]
mod dynamic_return_test;
mod escape;
#[cfg(test)]
#[path = "native_ir/escape_test.rs"]
#[cfg(test)]
mod escape_test;
mod expression;
#[cfg(test)]
#[path = "native_ir/field_projection_test.rs"]
#[cfg(test)]
mod field_projection_test;
mod fingerprint;
#[cfg(test)]
#[path = "native_ir/fingerprint_test.rs"]
#[cfg(test)]
mod fingerprint_test;
#[cfg(test)]
#[path = "native_ir/float_suite_native_parity_test.rs"]
#[cfg(test)]
mod float_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/fun_suite_native_parity_test.rs"]
#[cfg(test)]
mod fun_suite_native_parity_test;
mod function_lowering;
mod generic_specialization;
#[cfg(test)]
#[path = "native_ir/generic_specialization_test.rs"]
#[cfg(test)]
mod generic_specialization_test;
#[cfg(test)]
#[path = "native_ir/guard_no_opt_suite_native_parity_test.rs"]
#[cfg(test)]
mod guard_no_opt_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/guard_suite_native_parity_test.rs"]
#[cfg(test)]
mod guard_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/hello_suite_native_parity_test.rs"]
#[cfg(test)]
mod hello_suite_native_parity_test;
mod higher_order_context;
mod higher_order_specialization;
#[cfg(test)]
#[path = "native_ir/higher_order_specialization_test.rs"]
#[cfg(test)]
mod higher_order_specialization_test;
mod http_values;
#[cfg(test)]
#[path = "native_ir/http_values_test.rs"]
#[cfg(test)]
mod http_values_test;
mod identity;
#[cfg(test)]
#[path = "native_ir/list_bif_suite_native_parity_test.rs"]
#[cfg(test)]
mod list_bif_suite_native_parity_test;
mod list_comprehension;
#[cfg(test)]
#[path = "native_ir/list_comprehension_test.rs"]
mod list_comprehension_test;
mod lowering_coverage;
#[cfg(test)]
#[path = "native_ir/lowering_coverage_test.rs"]
#[cfg(test)]
mod lowering_coverage_test;
#[cfg(test)]
#[path = "native_ir/lowering_test_support.rs"]
#[cfg(test)]
mod lowering_test_support;
#[cfg(test)]
#[path = "native_ir/map_suite_native_parity_test.rs"]
#[cfg(test)]
mod map_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/match_spec_suite_native_parity_test.rs"]
#[cfg(test)]
mod match_spec_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/module_info_suite_native_parity_test.rs"]
#[cfg(test)]
mod module_info_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/multi_load_suite_native_parity_test.rs"]
#[cfg(test)]
mod multi_load_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/native_record_suite_native_parity_test.rs"]
#[cfg(test)]
mod native_record_suite_native_parity_test;
mod nested_closure_lifting;
mod nominal_identity;
mod open_std_pruning;
#[cfg(test)]
#[path = "native_ir/suspending_case_source_test.rs"]
#[cfg(test)]
mod suspending_case_source_test;
#[cfg(test)]
#[path = "native_ir/value_intrinsic_native_test.rs"]
mod value_intrinsic_native_test;
#[cfg(test)]
use lowering_test_support::lower_native_function;
#[cfg(test)]
#[path = "native_ir/capability_transition_test.rs"]
#[cfg(test)]
mod capability_transition_test;
mod model;
#[cfg(test)]
#[path = "native_ir/native_object_test_support.rs"]
#[cfg(test)]
mod native_object_test_support;
mod request_projection;
#[cfg(test)]
#[path = "native_ir/request_projection_test.rs"]
#[cfg(test)]
mod request_projection_test;
mod scalar_replacement;
#[cfg(test)]
#[path = "native_ir/scalar_replacement_index_test.rs"]
#[cfg(test)]
mod scalar_replacement_index_test;
#[cfg(test)]
#[path = "native_ir/scalar_replacement_test.rs"]
#[cfg(test)]
mod scalar_replacement_test;
#[cfg(test)]
#[path = "native_ir/set_suite_native_parity_test.rs"]
#[cfg(test)]
mod set_suite_native_parity_test;
mod short_circuit_normalization;
#[cfg(test)]
#[path = "native_ir/short_circuit_normalization_test.rs"]
#[cfg(test)]
mod short_circuit_normalization_test;
#[cfg(test)]
#[path = "native_ir/small_suite_native_parity_test.rs"]
#[cfg(test)]
mod small_suite_native_parity_test;
mod specialization_budget;
#[cfg(test)]
#[path = "native_ir/specialization_budget_test.rs"]
#[cfg(test)]
mod specialization_budget_test;
mod static_callable;
#[cfg(test)]
#[path = "native_ir/static_callable_test.rs"]
#[cfg(test)]
mod static_callable_test;
mod structured_case;
#[cfg(test)]
#[path = "native_ir/structured_case_test.rs"]
#[cfg(test)]
mod structured_case_test;
#[path = "native_ir/cranelift/suspension.rs"]
mod suspension;
mod symbol;
mod tail_position;
#[cfg(test)]
#[path = "native_ir/tail_position_source_test.rs"]
#[cfg(test)]
mod tail_position_source_test;
#[cfg(test)]
#[path = "native_ir/tail_position_test.rs"]
#[cfg(test)]
mod tail_position_test;
mod template_values;
#[cfg(test)]
#[path = "native_ir/template_values_test.rs"]
#[cfg(test)]
mod template_values_test;
mod transitions;
mod try_lowering;
#[cfg(test)]
#[path = "native_ir/try_lowering_test.rs"]
#[cfg(test)]
mod try_lowering_test;
mod typed_empty_lists;
#[cfg(test)]
#[path = "native_ir/typed_empty_lists_test.rs"]
mod typed_empty_lists_test;

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use crate::runtime::native_image::{
    TVM_DISPATCH_SYMBOL_V3 as DISPATCH_SYMBOL, TVM_IMAGE_ENTRY_SYMBOL_V1 as IMAGE_ENTRY_SYMBOL,
};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use application::resolve_typed_mutable_receiver_calls;
use application_calls::expr_calls_suspending;
#[cfg(test)]
use call_composition::rebase_callee_locals;
use call_composition::{
    composed_call_region, has_uncomposed_suspending_call, is_composable_suspending_body,
    CallRegion, ComposedCallProfile, RecursiveReductionMember,
};
use closure_conversion::NativeCallableShape;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use codegen_policy::NativeCodegenPolicy;
use constructors::NativeConstructorLayouts;
#[cfg(all(test, feature = "native-codegen"))]
pub(crate) use cranelift::emit_native_application_object;
#[cfg(feature = "native-codegen")]
pub(crate) use cranelift::{
    emit_native_application_dispatch_object_with_policy,
    emit_native_application_object_with_policy, emit_native_module_object_with_policy,
    native_application_abi_fingerprint,
};
#[cfg(test)]
use expression::infer_native_type;
use expression::{
    expr_is_scalar, free_variables, infer_native_type_with_constructors,
    lower_expr_with_constructors, native_type,
};
#[cfg(feature = "native-codegen")]
pub(crate) use function_lowering::status;
use function_lowering::{
    condition_yield_region, condition_yield_region_at_depth, contains_process_yield,
    expr_calls_are_supported, expr_is_native_control, is_scalar_candidate,
    lower_native_function_with_callables, lower_yield_region, native_return_type,
    native_return_type_with_constructors, native_type_with_constructors, yield_region,
    NativeFunctionLoweringEnvironment, NativeFunctionLoweringOutputs, YieldRegion,
    YieldRegionEnvironment, YieldRegionRequest,
};
pub(crate) use function_lowering::{
    NativeContinuation, NativeFunction, NativeModule, NATIVE_ABI_VERSION,
};
use identity::{stable_composed_completion_id, stable_continuation_id, stable_export_id};
pub(crate) use model::{
    NativeBinaryOperator, NativeCallResume, NativeDynamicCallResume, NativeExpr,
    NativeTransitionOperation, NativeType,
};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use open_std_pruning::{
    prune_application_to_function_roots, prune_module_to_function_roots,
};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use request_projection::install_native_request_projection_exports;
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
pub(crate) use request_projection::native_request_projections;
use transitions::is_process_transition;

/// Typed internal failure for NativeIR analysis and lowering passes.
#[derive(Debug)]
pub(crate) struct NativeIrError(terlan_runtime_abi::BoundaryError);

pub(crate) type NativeIrResult<T> = Result<T, NativeIrError>;

impl std::fmt::Display for NativeIrError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for NativeIrError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl std::ops::Deref for NativeIrError {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.context()
    }
}

impl From<String> for NativeIrError {
    fn from(error: String) -> Self {
        Self(native_ir_boundary_error("lower NativeIR", error))
    }
}

impl From<&str> for NativeIrError {
    fn from(error: &str) -> Self {
        error.to_string().into()
    }
}

impl From<NativeIrError> for String {
    fn from(error: NativeIrError) -> Self {
        error.to_string()
    }
}

fn native_ir_boundary_error(
    operation: &'static str,
    error: String,
) -> terlan_runtime_abi::BoundaryError {
    terlan_runtime_abi::BoundaryError::message(
        terlan_runtime_abi::ErrorDomain::NativeIrEmission,
        operation,
        error,
    )
}
