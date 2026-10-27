//! Shared concrete trait dispatch for source backends.

use super::super::{
    generic_specialization, specialization_budget::SpecializationBudget, NativeIrResult,
};
use crate::terlan_typeck::CoreModule;

/// Resolves checked trait implementations after generic arguments become concrete.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) fn resolve_typed_generic_calls(cores: &mut [CoreModule]) -> NativeIrResult<()> {
    super::normalize_application_remote_calls(cores, false);
    let deferred = super::overloads::resolve_typed_overloads(cores)?;
    super::transparent_aliases::expand_transparent_aliases(cores);
    let exported_templates = cores
        .iter()
        .map(|core| {
            core.functions
                .iter()
                .filter(|function| {
                    function.public
                        && !generic_specialization::generic_parameters(function).is_empty()
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    generic_specialization::specialize_application_generics_with_budget(
        cores,
        &mut SpecializationBudget::default(),
    )?;
    // JavaScript exports can remain generic when their bodies need no static
    // trait choice. Keep those exports; unresolved trait bodies fail emission.
    for (core, templates) in cores.iter_mut().zip(exported_templates) {
        core.functions.extend(templates);
    }
    deferred.resolve(cores)
}
