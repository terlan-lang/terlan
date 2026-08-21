//! Independently cacheable Cranelift application object units.

use cranelift_module::{Linkage, Module};

use super::super::symbol::{native_continuation_symbol, native_symbol};
use super::super::{NativeCodegenPolicy, NativeModule};
use super::{
    application_functions, define_dispatch, define_image_entry, define_native_function,
    flattened_application, is_suspending, managed_tail_loop_slots, native_signature,
    normalize_tail_component_profiles, object_module, suspension_profile, suspension_value_count,
    validate_callable_shapes, ManagedLayouts, NativeFunctionCatalog, NativeFunctionDefinition,
};

/// Produces the application ABI identity that invalidates module object units.
///
/// The identity excludes function bodies while retaining every property that
/// changes a direct-call declaration. A body-only edit therefore rebuilds its
/// owning unit without poisoning otherwise compatible dependency objects.
pub(crate) fn native_application_abi_fingerprint(
    natives: &[NativeModule],
) -> Result<String, terlan_runtime_abi::BoundaryError> {
    native_application_abi_fingerprint_untyped(natives)
        .map_err(|error| super::super::native_ir_boundary_error("fingerprint NativeIR ABI", error))
}

fn native_application_abi_fingerprint_untyped(natives: &[NativeModule]) -> Result<String, String> {
    validate_callable_shapes(natives)?;
    let application_native = flattened_application("abi", natives);
    let (suspending, mut transition_counts) = suspension_profile(&application_native)?;
    let tail_components = super::super::tail_position::mutual_tail_components(natives);
    normalize_tail_component_profiles(&suspending, &mut transition_counts, &tail_components)?;
    let functions = application_functions(natives);
    let externally_resumable =
        super::super::continuation_sharing::externally_resumable_continuation_ids(natives);
    let mut fingerprint = String::new();
    for (index, (native, function)) in functions.iter().enumerate() {
        fingerprint.push_str(&format!(
            "{}\0{}\0{}\0{}\0{:?}\0{:?}\0{}\0{}\n",
            native.name,
            function.export_id,
            function.name,
            function.arity,
            function.params,
            function.return_type,
            suspending[index],
            transition_counts[index]
        ));
    }
    let mut resumable_ids = externally_resumable.into_iter().collect::<Vec<_>>();
    resumable_ids.sort_unstable();
    for continuation_id in resumable_ids {
        fingerprint.push_str(&format!("resume\0{continuation_id}\n"));
    }
    Ok(fingerprint)
}

/// Emits one independently cacheable native module object.
///
/// Every application function is declared in canonical order so NativeIR call
/// indexes remain application-wide. Functions owned by `module_index` are
/// defined as linker-visible symbols; all remaining functions are imports.
pub(crate) fn emit_native_module_object_with_policy(
    application: &str,
    natives: &[NativeModule],
    module_index: usize,
    policy: NativeCodegenPolicy,
) -> Result<Vec<u8>, terlan_runtime_abi::BoundaryError> {
    emit_native_module_object_with_policy_untyped(application, natives, module_index, policy)
        .map_err(|error| super::super::native_ir_boundary_error("emit native module object", error))
}

fn emit_native_module_object_with_policy_untyped(
    application: &str,
    natives: &[NativeModule],
    module_index: usize,
    policy: NativeCodegenPolicy,
) -> Result<Vec<u8>, String> {
    let selected = natives.get(module_index).ok_or_else(|| {
        format!("error[cranelift.module_index]: native module index {module_index} is out of range")
    })?;
    validate_callable_shapes(natives)?;
    super::super::tail_position::validate_recursive_tail_targets(natives)?;
    let mut module = object_module(&format!("{application}.{}", selected.name), policy)?;
    // A module object can inline every body in an application-wide mutual-tail
    // component.  Its constructor and atom table must therefore use the same
    // closed application inventory as the dispatcher and monolithic emitter,
    // not merely the selected module's declarations.
    let managed_layouts = ManagedLayouts::declare(&mut module, natives)?;
    let pointer = module.target_config().pointer_type();
    let application_native = flattened_application(application, natives);
    let (function_suspending, mut function_transition_counts) =
        suspension_profile(&application_native)?;
    let functions = application_functions(natives);
    let externally_resumable =
        super::super::continuation_sharing::externally_resumable_continuation_ids(natives);
    let function_managed_returns = functions
        .iter()
        .map(|(_, function)| function.return_type.is_managed_reference())
        .collect::<Vec<_>>();
    let function_parameter_types = functions
        .iter()
        .map(|(_, function)| function.params.clone())
        .collect::<Vec<_>>();
    let tail_components = super::super::tail_position::mutual_tail_components(natives);
    normalize_tail_component_profiles(
        &function_suspending,
        &mut function_transition_counts,
        &tail_components,
    )?;
    let signatures = functions
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
    let function_ids = functions
        .iter()
        .enumerate()
        .map(|(index, (native, function))| {
            let linkage = if std::ptr::eq(*native, selected) {
                Linkage::Export
            } else {
                Linkage::Import
            };
            module
                .declare_function(
                    &native_symbol(&native.name, &function.name, function.arity),
                    linkage,
                    &signatures[index],
                )
                .map_err(|error| format!("error[cranelift.declare]: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (index, (native, function)) in functions.iter().enumerate() {
        if std::ptr::eq(*native, selected) {
            let tail_component = tail_components
                .iter()
                .find(|component| component.binary_search(&index).is_ok());
            let managed_loop_slots = managed_tail_loop_slots(
                &function.params,
                tail_component.map(Vec::as_slice),
                &functions,
            );
            let tail_component_bodies = tail_component.map(|component| {
                component
                    .iter()
                    .map(|member| {
                        (
                            *member,
                            functions[*member].1.arity,
                            &functions[*member].1.body,
                        )
                    })
                    .collect::<Vec<_>>()
            });
            define_native_function(
                &mut module,
                NativeFunctionDefinition {
                    id: function_ids[index],
                    self_function: Some(index),
                    tail_component_bodies: tail_component_bodies.as_deref(),
                    signature: &signatures[index],
                    body: &function.body,
                    managed_loop_slots: &managed_loop_slots,
                },
                NativeFunctionCatalog {
                    ids: &function_ids,
                    parameter_types: &function_parameter_types,
                    suspending: &function_suspending,
                    transition_counts: &function_transition_counts,
                    managed_returns: &function_managed_returns,
                    managed_layouts: &managed_layouts,
                },
            )
            .map_err(|error| {
                format!(
                    "{error}; while defining unit `{}.{}` at application index {index}",
                    native.name, function.name
                )
            })?;
        }
    }
    for continuation in &selected.continuations {
        if !externally_resumable.contains(&continuation.id) {
            continue;
        }
        let transition_count =
            suspension_value_count(&continuation.body, &function_transition_counts);
        let suspending = is_suspending(&continuation.body, &function_suspending);
        let signature = native_signature(
            continuation.params.len(),
            suspending,
            transition_count,
            pointer,
        );
        let id = module
            .declare_function(
                &native_continuation_symbol(continuation.id),
                Linkage::Export,
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
                self_function: None,
                tail_component_bodies: None,
                signature: &signature,
                body: &continuation.body,
                managed_loop_slots: &managed_loop_slots,
            },
            NativeFunctionCatalog {
                ids: &function_ids,
                parameter_types: &function_parameter_types,
                suspending: &function_suspending,
                transition_counts: &function_transition_counts,
                managed_returns: &function_managed_returns,
                managed_layouts: &managed_layouts,
            },
        )
        .map_err(|error| {
            format!(
                "{error}; while defining continuation {} in unit `{}`",
                continuation.id, selected.name
            )
        })?;
    }
    module
        .finish()
        .emit()
        .map_err(|error| format!("error[cranelift.emit]: {error}"))
}

/// Emits the small application-owned dispatcher and native image entry object.
pub(crate) fn emit_native_application_dispatch_object_with_policy(
    application: &str,
    natives: &[NativeModule],
    policy: NativeCodegenPolicy,
) -> Result<Vec<u8>, terlan_runtime_abi::BoundaryError> {
    emit_native_application_dispatch_object_with_policy_untyped(application, natives, policy)
        .map_err(|error| {
            super::super::native_ir_boundary_error("emit native dispatch object", error)
        })
}

fn emit_native_application_dispatch_object_with_policy_untyped(
    application: &str,
    natives: &[NativeModule],
    policy: NativeCodegenPolicy,
) -> Result<Vec<u8>, String> {
    if natives.is_empty() {
        return Err("error[cranelift.application]: native application has no modules".to_string());
    }
    validate_callable_shapes(natives)?;
    let mut module = object_module(&format!("{application}.dispatch"), policy)?;
    let pointer = module.target_config().pointer_type();
    let application_native = flattened_application(application, natives);
    let (function_suspending, function_transition_counts) =
        suspension_profile(&application_native)?;
    let functions = application_functions(natives);
    let externally_resumable =
        super::super::continuation_sharing::externally_resumable_continuation_ids(natives);
    let mut dispatch_functions = Vec::new();
    for (index, (native, function)) in functions.iter().enumerate() {
        let signature = native_signature(
            function.arity,
            function_suspending[index],
            function_transition_counts[index],
            pointer,
        );
        let id = module
            .declare_function(
                &native_symbol(&native.name, &function.name, function.arity),
                Linkage::Import,
                &signature,
            )
            .map_err(|error| format!("error[cranelift.dispatch_import]: {error}"))?;
        if !super::super::is_materialized_continuation_module(native) {
            dispatch_functions.push((
                function.export_id,
                function.arity,
                id,
                function_transition_counts[index],
                function_suspending[index],
            ));
        }
    }
    for native in natives {
        for continuation in &native.continuations {
            if !externally_resumable.contains(&continuation.id) {
                continue;
            }
            let transition_count =
                suspension_value_count(&continuation.body, &function_transition_counts);
            let suspending = is_suspending(&continuation.body, &function_suspending);
            let signature = native_signature(
                continuation.params.len(),
                suspending,
                transition_count,
                pointer,
            );
            let id = module
                .declare_function(
                    &native_continuation_symbol(continuation.id),
                    Linkage::Import,
                    &signature,
                )
                .map_err(|error| format!("error[cranelift.dispatch_import]: {error}"))?;
            dispatch_functions.push((
                continuation.id,
                continuation.params.len(),
                id,
                transition_count,
                suspending,
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
