//! Concrete trait methods share ordinary typed-overload selection.

use super::*;

/// Adds implementation bodies by canonical trait identity, including singleton
/// candidate groups. Generated bodies keep their owning module and visibility;
/// ordinary application admission still validates the selected call boundary.
pub(super) fn collect(
    cores: &[CoreModule],
    groups: &mut HashMap<OverloadKey, Vec<OverloadCandidate>>,
) {
    for core in cores {
        for function in &core.functions {
            let Some(identity) = &function.trait_method else {
                continue;
            };
            let Some(parameters) = function
                .params
                .iter()
                .map(|parameter| parameter.core_ty.clone())
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let Some(result) = function.core_return_type.clone() else {
                continue;
            };
            let candidate = OverloadCandidate {
                selected_import: false,
                module: core.module.clone(),
                private_trait_impl: !function.public,
                generic_trait_method:
                    !crate::compiler::native_ir::generic_specialization::generic_parameters(
                        function,
                    )
                    .is_empty(),
                arity: function.arity,
                internal_name: function.name.clone(),
                parameters,
                result,
            };
            // Bracket syntax carries an implicit trait name without an import.
            // Match the same IndexGet/IndexSet contracts accepted by typechecking.
            if let Some(short) = identity.trait_name.rsplit('.').next() {
                if matches!(
                    (short, identity.method.as_str()),
                    ("IndexGet", "get_at") | ("IndexSet", "set_at")
                ) {
                    groups
                        .entry((short.to_string(), identity.method.clone(), function.arity))
                        .or_default()
                        .push(candidate.clone());
                }
            }
            if !identity.type_args.is_empty() {
                groups
                    .entry((
                        identity.dispatch_owner(),
                        identity.method.clone(),
                        function.arity,
                    ))
                    .or_default()
                    .push(candidate.clone());
            }
            groups
                .entry((
                    identity.trait_name.clone(),
                    identity.method.clone(),
                    function.arity,
                ))
                .or_default()
                .push(candidate);
        }
    }
}
