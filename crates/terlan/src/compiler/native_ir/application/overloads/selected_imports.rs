//! Typed selection of explicitly imported function families before ABI admission.

use super::*;

/// Qualifies a selected import only when its checked argument types identify a
/// unique public provider. Whole-module imports keep the existing ambiguity
/// diagnostics; unrelated application functions are never added as candidates.
pub(in crate::compiler::native_ir::application) fn resolve(
    cores: &mut [CoreModule],
) -> NativeIrResult<()> {
    let mut groups = HashMap::<OverloadKey, Vec<OverloadCandidate>>::new();
    let mut intrinsic_signatures = HashMap::new();
    for caller in cores.iter() {
        for import in &caller.selected_function_imports {
            let Some(provider) = cores.iter().find(|core| core.module == import.module) else {
                continue;
            };
            let intrinsic_functions = intrinsic_signatures
                .entry(provider.module.clone())
                .or_insert_with(|| {
                    crate::terlan_typeck::core_interface::lower_core_functions(&provider.interface)
                        .into_iter()
                        .filter(|function| {
                            crate::terlan_typeck::core_intrinsic_lowering::core_primitive_intrinsic(
                                &provider.module,
                                &function.name,
                                function.arity,
                            )
                            .is_some()
                                && !provider.functions.iter().any(|body| {
                                    body.name == function.name && body.arity == function.arity
                                })
                        })
                        .collect::<Vec<_>>()
                });
            for function in provider
                .functions
                .iter()
                .chain(intrinsic_functions.iter())
                .filter(|function| function.public && function.name == import.function)
            {
                if caller
                    .functions
                    .iter()
                    .any(|local| local.name == import.local_name && local.arity == function.arity)
                {
                    continue;
                }
                let Some(parameters) = function
                    .params
                    .iter()
                    .map(|param| param.core_ty.clone())
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                let Some(result) = function.core_return_type.clone() else {
                    continue;
                };
                groups
                    .entry((
                        caller.module.clone(),
                        import.local_name.clone(),
                        function.arity,
                    ))
                    .or_default()
                    .push(OverloadCandidate {
                        module: provider.module.clone(),
                        private_trait_impl: false,
                        generic_trait_method: false,
                        arity: function.arity,
                        internal_name: function.name.clone(),
                        parameters,
                        result,
                    });
            }
        }
    }
    groups.retain(|_, candidates| candidates.len() > 1);
    rewrite_application(cores, &groups)
}
