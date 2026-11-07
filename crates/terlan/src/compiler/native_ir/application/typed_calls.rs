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
    let nominal_parameters = generic_specialization::nominal_parameter_catalog(cores);
    let source_templates = cores
        .iter()
        .map(|core| {
            core.functions
                .iter()
                .filter(|function| {
                    !generic_specialization::generic_parameters_with_nominals(
                        function,
                        &nominal_parameters,
                    )
                    .is_empty()
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    generic_specialization::specialize_application_generics_with_budget(
        cores,
        &mut SpecializationBudget::default(),
    )?;
    // JavaScript erases generic types. Keep templates, including private helpers
    // reachable from generic exports, until ordinary function-graph pruning.
    // Unresolved trait bodies still fail emission when reachable.
    for (core, templates) in cores.iter_mut().zip(source_templates) {
        core.functions.extend(templates);
    }
    deferred.resolve(cores)
}
