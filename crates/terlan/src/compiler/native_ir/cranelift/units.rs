//! Independently cacheable Cranelift application object units.

use cranelift_module::{Linkage, Module};

use super::super::symbol::{native_continuation_symbol, native_symbol};
use super::super::{NativeCodegenPolicy, NativeModule};
use super::{
    application_functions, define_dispatch, define_image_entry, define_native_function,
    flattened_application, is_suspending, native_signature, object_module, suspension_profile,
    suspension_value_count, validate_callable_shapes, ManagedLayouts,
};

/// Produces the application ABI identity that invalidates module object units.
///
/// The identity excludes function bodies while retaining every property that
/// changes a direct-call declaration. A body-only edit therefore rebuilds its
/// owning unit without poisoning otherwise compatible dependency objects.
pub(crate) fn native_application_abi_fingerprint(
    natives: &[NativeModule],
) -> Result<String, String> {
    validate_callable_shapes(natives)?;
    let application_native = flattened_application("abi", natives);
    let (suspending, transition_counts) = suspension_profile(&application_native);
    let functions = application_functions(natives);
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
) -> Result<Vec<u8>, String> {
    let selected = natives.get(module_index).ok_or_else(|| {
        format!("error[cranelift.module_index]: native module index {module_index} is out of range")
    })?;
    validate_callable_shapes(natives)?;
    let mut module = object_module(&format!("{application}.{}", selected.name), policy)?;
    let managed_layouts = ManagedLayouts::declare(&mut module, std::slice::from_ref(selected))?;
    let pointer = module.target_config().pointer_type();
    let application_native = flattened_application(application, natives);
    let (function_suspending, function_transition_counts) = suspension_profile(&application_native);
    let functions = application_functions(natives);
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
            define_native_function(
                &mut module,
                function_ids[index],
                &signatures[index],
                &function.body,
                &function_ids,
                &function_suspending,
                &function_transition_counts,
                &managed_layouts,
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
        define_native_function(
            &mut module,
            id,
            &signature,
            &continuation.body,
            &function_ids,
            &function_suspending,
            &function_transition_counts,
            &managed_layouts,
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
) -> Result<Vec<u8>, String> {
    if natives.is_empty() {
        return Err("error[cranelift.application]: native application has no modules".to_string());
    }
    validate_callable_shapes(natives)?;
    let mut module = object_module(&format!("{application}.dispatch"), policy)?;
    let pointer = module.target_config().pointer_type();
    let application_native = flattened_application(application, natives);
    let (function_suspending, function_transition_counts) = suspension_profile(&application_native);
    let functions = application_functions(natives);
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
        dispatch_functions.push((
            function.export_id,
            function.arity,
            id,
            function_transition_counts[index],
            function_suspending[index],
        ));
    }
    for native in natives {
        for continuation in &native.continuations {
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
