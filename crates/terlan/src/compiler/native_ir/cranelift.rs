mod call_then;
mod callables;
mod dispatch;
#[cfg(all(test, unix))]
#[path = "cranelift/dispatch_test.rs"]
mod dispatch_test;
mod error;
mod float;
mod function;
mod image_entry;
mod indirect;
mod managed;
#[cfg(all(test, unix))]
#[path = "cranelift/managed_callback_test.rs"]
mod managed_callback_test;
#[cfg(test)]
#[path = "cranelift/managed_stack_map_test.rs"]
mod managed_stack_map_test;
#[cfg(test)]
#[path = "managed_type_test.rs"]
mod managed_type_test;
mod setup;
#[cfg(all(test, target_arch = "aarch64"))]
#[path = "cranelift/setup_test.rs"]
mod setup_test;
mod signature;
mod tail_call;
#[cfg(test)]
mod test_support;
mod transition;
mod try_expr;
mod units;
mod wrapped_yield;

use cranelift_codegen::ir::{
    condcodes::IntCC, types, Block, BlockArg, InstBuilder, MemFlagsData, StackSlot, Value,
};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;

use super::suspension::{
    is_suspending, normalize_tail_component_profiles, suspension_profile, suspension_value_count,
};
use super::symbol::native_symbol;
use super::{status, NativeBinaryOperator, NativeExpr, NativeModule};
use crate::runtime::native_image::TVM_COMPLETION_TRANSITION_WORD_CAPACITY;
use callables::validate_callable_shapes;
use dispatch::define_dispatch;
use error::{branch_if_error, branch_on_flag, emit_integer_comparison};
use float::emit_float_binary;
use function::{
    define_native_function, managed_tail_loop_slots, NativeFunctionDefinition,
    RUNTIME_ARGUMENT_COUNT,
};
use image_entry::define_image_entry;
use managed::ManagedLayouts;
use setup::{
    application_functions, declare_image_func_in_func, flattened_application, object_module,
};
use signature::native_signature;
use transition::{transition_flags, transition_status};
use wrapped_yield::{emit_wrapped_call_yield, WrappedCallYield};

pub(super) fn application_suspending_functions(
    natives: &[NativeModule],
) -> super::NativeIrResult<std::collections::HashSet<usize>> {
    let application = flattened_application("suspension-validation", natives);
    let (suspending, _) = suspension_profile(&application)?;
    Ok(suspending
        .into_iter()
        .enumerate()
        .filter_map(|(index, suspending)| suspending.then_some(index))
        .collect())
}

/// Immutable application-wide function metadata consulted during one emission.
#[derive(Clone, Copy)]
struct NativeFunctionCatalog<'a> {
    ids: &'a [FuncId],
    coverage_ids: &'a [u64],
    parameter_types: &'a [Vec<super::NativeType>],
    suspending: &'a [bool],
    transition_counts: &'a [usize],
    managed_returns: &'a [bool],
    managed_layouts: &'a ManagedLayouts,
}

/// Control-flow blocks and ownership identity of one generated tail loop.
#[derive(Clone, Copy)]
struct NativeTailFrame<'a> {
    self_function: Option<usize>,
    component: Option<&'a [(usize, usize)]>,
    loop_header: Block,
    reduction_budget_slot: Option<StackSlot>,
    /// Whether recursive edges must also observe actor-heap pressure.
    managed_pressure: bool,
    error_block: Block,
}

/// Caller-owned storage through which generated code reports suspension state.
#[derive(Clone, Copy)]
struct NativeTransitionFrame {
    pointer: Option<Value>,
    len_pointer: Option<Value>,
}

#[cfg(test)]
pub(crate) use test_support::emit_native_application_object;
pub(crate) use units::{
    emit_native_application_dispatch_object_with_policy, emit_native_module_object_with_policy,
    native_application_abi_fingerprint,
};

/// Emits one complete application object under explicit optimization policy.
pub(crate) fn emit_native_application_object_with_policy(
    application: &str,
    natives: &[NativeModule],
    policy: super::NativeCodegenPolicy,
) -> Result<Vec<u8>, terlan_runtime_abi::BoundaryError> {
    emit_native_application_object_with_policy_untyped(application, natives, policy)
        .map_err(|error| super::native_ir_boundary_error("emit native application object", error))
}

fn emit_native_application_object_with_policy_untyped(
    application: &str,
    natives: &[NativeModule],
    policy: super::NativeCodegenPolicy,
) -> Result<Vec<u8>, String> {
    if natives.is_empty() {
        return Err("error[cranelift.application]: native application has no modules".to_string());
    }
    validate_callable_shapes(natives)?;
    super::tail_position::validate_recursive_tail_targets(natives)?;
    let mut module = object_module(application, policy)?;
    let managed_layouts = ManagedLayouts::declare(&mut module, natives)?;

    let pointer = module.target_config().pointer_type();
    let application_native = flattened_application(application, natives);
    let (function_suspending, mut function_transition_counts) =
        suspension_profile(&application_native)?;
    let application_functions = application_functions(natives);
    let externally_resumable =
        super::continuation_sharing::externally_resumable_continuation_ids(natives);
    let function_managed_returns = application_functions
        .iter()
        .map(|(_, function)| function.return_type.is_managed_reference())
        .collect::<Vec<_>>();
    let function_parameter_types = application_functions
        .iter()
        .map(|(_, function)| function.params.clone())
        .collect::<Vec<_>>();
    let tail_components = super::tail_position::mutual_tail_components(natives);
    normalize_tail_component_profiles(
        &function_suspending,
        &mut function_transition_counts,
        &tail_components,
    )?;
    let signatures = application_functions
        .iter()
        .enumerate()
        .map(|(index, (_, function))| {
            native_signature(
                function.arity,
                function_suspending[index],
                function_transition_counts[index],
                pointer,
            )
        })
        .collect::<Vec<_>>();
    let function_ids = application_functions
        .iter()
        .enumerate()
        .map(|(index, (native, function))| {
            module
                .declare_function(
                    &native_symbol(&native.name, &function.name, function.arity),
                    Linkage::Local,
                    &signatures[index],
                )
                .map_err(|error| format!("error[cranelift.declare]: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let function_coverage_ids = application_functions
        .iter()
        .map(|(_, function)| {
            super::coverage_callable_id(
                &function.source_module,
                &function.source_function,
                function.source_arity,
            )
        })
        .collect::<Vec<_>>();
    let mut dispatch_functions = Vec::new();
    for (index, (_, function)) in application_functions.iter().enumerate() {
        let tail_component = tail_components
            .iter()
            .find(|component| component.binary_search(&index).is_ok());
        let managed_loop_slots = managed_tail_loop_slots(
            &function.params,
            tail_component.map(Vec::as_slice),
            &application_functions,
        );
        let tail_component_bodies = tail_component.map(|component| {
            component
                .iter()
                .map(|member| {
                    (
                        *member,
                        application_functions[*member].1.arity,
                        &application_functions[*member].1.body,
                    )
                })
                .collect::<Vec<_>>()
        });
        define_native_function(
            &mut module,
            NativeFunctionDefinition {
                id: function_ids[index],
                coverage_id: function_coverage_ids[index],
                self_function: Some(index),
                tail_component_bodies: tail_component_bodies.as_deref(),
                signature: &signatures[index],
                body: &function.body,
                managed_loop_slots: &managed_loop_slots,
            },
            NativeFunctionCatalog {
                ids: &function_ids,
                coverage_ids: &function_coverage_ids,
                parameter_types: &function_parameter_types,
                suspending: &function_suspending,
                transition_counts: &function_transition_counts,
                managed_returns: &function_managed_returns,
                managed_layouts: &managed_layouts,
            },
        )
        .map_err(|error| {
            format!(
                "{error}; while defining `{}.{}` at application index {index}",
                application_functions[index].0.name, function.name
            )
        })?;
        if !super::is_materialized_continuation_module(application_functions[index].0) {
            dispatch_functions.push((
                function.export_id,
                function.arity,
                function_ids[index],
                function_transition_counts[index],
                function_suspending[index],
            ));
        }
    }
    for native in natives {
        for (index, continuation) in native.continuations.iter().enumerate() {
            if !externally_resumable.contains(&continuation.id) {
                continue;
            }
            let transition_value_count =
                suspension_value_count(&continuation.body, &function_transition_counts);
            let continuation_suspending = is_suspending(&continuation.body, &function_suspending);
            let signature = native_signature(
                continuation.params.len(),
                continuation_suspending,
                transition_value_count,
                pointer,
            );
            let id = module
                .declare_function(
                    &format!("terlan_continuation_{}_{}", continuation.id, index),
                    Linkage::Local,
                    &signature,
                )
                .map_err(|error| format!("error[cranelift.continuation_declare]: {error}"))?;
            let managed_loop_slots = continuation
                .params
                .iter()
                .map(|parameter| parameter.is_managed_reference())
                .collect::<Vec<_>>();
            define_native_function(
                &mut module,
                NativeFunctionDefinition {
                    id,
                    coverage_id: super::coverage_callable_id(
                        &continuation.source_module,
                        &continuation.source_function,
                        continuation.source_arity,
                    ),
                    self_function: None,
                    tail_component_bodies: None,
                    signature: &signature,
                    body: &continuation.body,
                    managed_loop_slots: &managed_loop_slots,
                },
                NativeFunctionCatalog {
                    ids: &function_ids,
                    coverage_ids: &function_coverage_ids,
                    parameter_types: &function_parameter_types,
                    suspending: &function_suspending,
                    transition_counts: &function_transition_counts,
                    managed_returns: &function_managed_returns,
                    managed_layouts: &managed_layouts,
                },
            )?;
            dispatch_functions.push((
                continuation.id,
                continuation.params.len(),
                id,
                transition_value_count,
                continuation_suspending,
            ));
        }
    }
    define_dispatch(&mut module, &dispatch_functions)?;
    define_image_entry(&mut module)?;

    module
        .finish()
        .emit()
        .map_err(|error| format!("error[cranelift.emit]: {error}"))
}
mod suspending_body;
use suspending_body::emit_suspending_body;

#[path = "cranelift/expression.rs"]
mod expression;
use expression::*;
