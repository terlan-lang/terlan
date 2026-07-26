//! Terlan-owned machine-independent IR for direct native object emission.

mod aggregate_types;
#[cfg(test)]
#[path = "native_ir/aot3_conformance_test.rs"]
mod aot3_conformance_test;
mod application;
mod application_admission;
#[cfg(test)]
#[path = "native_ir/application_admission_test.rs"]
mod application_admission_test;
mod application_calls;
mod atom_alias_values;
mod atom_inventory;
#[cfg(test)]
#[path = "native_ir/atom_inventory_test.rs"]
mod atom_inventory_test;
mod call_composition;
#[cfg(test)]
#[path = "native_ir/call_composition_test.rs"]
mod call_composition_test;
mod callee_scalar_replacement;
#[cfg(test)]
#[path = "native_ir/callee_scalar_replacement_test.rs"]
mod callee_scalar_replacement_test;
mod case_lowering;
#[cfg(test)]
#[path = "native_ir/case_lowering_test.rs"]
mod case_lowering_test;
#[cfg(test)]
#[path = "native_ir/cast_lowering_test.rs"]
mod cast_lowering_test;
mod closure_conversion;
#[cfg(test)]
#[path = "native_ir/closure_conversion_test.rs"]
mod closure_conversion_test;
mod closure_invocation;
mod codegen_policy;
#[cfg(test)]
#[path = "native_ir/codegen_policy_test.rs"]
mod codegen_policy_test;
mod collection_intrinsic_specialization;
#[cfg(test)]
#[path = "native_ir/collection_intrinsic_specialization_test.rs"]
mod collection_intrinsic_specialization_test;
mod collection_values;
#[cfg(test)]
#[path = "native_ir/collection_values_test.rs"]
mod collection_values_test;
mod collections;
mod composed_continuation;
mod constructor_chain;
#[cfg(test)]
#[path = "native_ir/constructor_lowering_test.rs"]
mod constructor_lowering_test;
mod constructors;
mod continuation_sharing;
#[cfg(test)]
#[path = "native_ir/continuation_sharing_test.rs"]
mod continuation_sharing_test;
mod control;
mod control_completion;
#[cfg(test)]
#[path = "native_ir/control_test.rs"]
mod control_test;
mod cranelift;
mod dynamic_return;
#[cfg(test)]
#[path = "native_ir/dynamic_return_test.rs"]
mod dynamic_return_test;
mod escape;
#[cfg(test)]
#[path = "native_ir/escape_test.rs"]
mod escape_test;
mod expression;
#[cfg(test)]
#[path = "native_ir/field_projection_test.rs"]
mod field_projection_test;
mod fingerprint;
#[cfg(test)]
#[path = "native_ir/fingerprint_test.rs"]
mod fingerprint_test;
#[cfg(test)]
#[path = "native_ir/float_suite_native_parity_test.rs"]
mod float_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/fun_suite_native_parity_test.rs"]
mod fun_suite_native_parity_test;
mod generic_specialization;
#[cfg(test)]
#[path = "native_ir/generic_specialization_test.rs"]
mod generic_specialization_test;
#[cfg(test)]
#[path = "native_ir/guard_no_opt_suite_native_parity_test.rs"]
mod guard_no_opt_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/guard_suite_native_parity_test.rs"]
mod guard_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/hello_suite_native_parity_test.rs"]
mod hello_suite_native_parity_test;
mod higher_order_specialization;
#[cfg(test)]
#[path = "native_ir/higher_order_specialization_test.rs"]
mod higher_order_specialization_test;
mod http_values;
#[cfg(test)]
#[path = "native_ir/http_values_test.rs"]
mod http_values_test;
mod identity;
#[cfg(test)]
#[path = "native_ir/list_bif_suite_native_parity_test.rs"]
mod list_bif_suite_native_parity_test;
mod list_comprehension;
mod lowering_coverage;
#[cfg(test)]
#[path = "native_ir/lowering_coverage_test.rs"]
mod lowering_coverage_test;
#[cfg(test)]
#[path = "native_ir/lowering_test_support.rs"]
mod lowering_test_support;
#[cfg(test)]
#[path = "native_ir/map_suite_native_parity_test.rs"]
mod map_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/match_spec_suite_native_parity_test.rs"]
mod match_spec_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/module_info_suite_native_parity_test.rs"]
mod module_info_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/multi_load_suite_native_parity_test.rs"]
mod multi_load_suite_native_parity_test;
#[cfg(test)]
#[path = "native_ir/native_record_suite_native_parity_test.rs"]
mod native_record_suite_native_parity_test;
mod nested_closure_lifting;
mod nominal_identity;
mod open_std_pruning;
#[cfg(test)]
use lowering_test_support::lower_native_function;
#[cfg(test)]
#[path = "native_ir/capability_transition_test.rs"]
mod capability_transition_test;
mod model;
#[cfg(test)]
#[path = "native_ir/native_object_test_support.rs"]
mod native_object_test_support;
mod request_projection;
#[cfg(test)]
#[path = "native_ir/request_projection_test.rs"]
mod request_projection_test;
mod scalar_replacement;
#[cfg(test)]
#[path = "native_ir/scalar_replacement_index_test.rs"]
mod scalar_replacement_index_test;
#[cfg(test)]
#[path = "native_ir/scalar_replacement_test.rs"]
mod scalar_replacement_test;
mod short_circuit_normalization;
#[cfg(test)]
#[path = "native_ir/short_circuit_normalization_test.rs"]
mod short_circuit_normalization_test;
#[cfg(test)]
#[path = "native_ir/small_suite_native_parity_test.rs"]
mod small_suite_native_parity_test;
mod specialization_budget;
#[cfg(test)]
#[path = "native_ir/specialization_budget_test.rs"]
mod specialization_budget_test;
mod static_callable;
#[cfg(test)]
#[path = "native_ir/static_callable_test.rs"]
mod static_callable_test;
mod structured_case;
#[cfg(test)]
#[path = "native_ir/structured_case_test.rs"]
mod structured_case_test;
mod symbol;
mod template_values;
#[cfg(test)]
#[path = "native_ir/template_values_test.rs"]
mod template_values_test;
mod transitions;
mod try_lowering;
#[cfg(test)]
#[path = "native_ir/try_lowering_test.rs"]
mod try_lowering_test;
mod typed_empty_lists;
include!("native_ir_part_001.rs");
