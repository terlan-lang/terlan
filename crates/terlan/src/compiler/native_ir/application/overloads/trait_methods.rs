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

        collect_declaration_site_methods(core, groups);
    }
}

/// Adds receiver bodies that satisfy declaration-site `implements` facts.
///
/// These bodies are ordinary methods rather than explicit-impl adapters, so
/// they do not carry `CoreFunction::trait_method`. Their checked conformance
/// metadata supplies the canonical trait group while their qualified receiver
/// type selects the concrete implementation.
fn collect_declaration_site_methods(
    core: &CoreModule,
    groups: &mut HashMap<OverloadKey, Vec<OverloadCandidate>>,
) {
    for conformance in &core.trait_conformances {
        if conformance.source != crate::terlan_typeck::CoreTraitConformanceSource::Implements {
            continue;
        }
        let Some(trait_type) = crate::terlan_typeck::core_type_from_text(&conformance.trait_ref)
        else {
            continue;
        };
        let (mut trait_name, mut type_args) = match trait_type {
            CoreType::Apply { constructor, args } => (constructor, args),
            CoreType::Named(name) => (name, Vec::new()),
            _ => continue,
        };
        if !trait_name.contains('.') {
            trait_name = format!("{}.{}", core.module, trait_name);
        }
        type_args
            .iter_mut()
            .for_each(|ty| qualify_conformance_type(ty, core));
        let Some(mut owner_type) = crate::terlan_typeck::core_type_from_text(&conformance.for_type)
        else {
            continue;
        };
        qualify_conformance_type(&mut owner_type, core);
        let identity = crate::terlan_typeck::CoreTraitMethodIdentity {
            trait_name,
            type_args,
            method: String::new(),
        };
        let dispatch_owner = identity.dispatch_owner();

        for function in &core.functions {
            if !function.receiver_method
                || function.trait_method.is_some()
                || function
                    .params
                    .first()
                    .and_then(|param| param.core_ty.as_ref())
                    != Some(&owner_type)
            {
                continue;
            }
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
                private_trait_impl: !(conformance.public && function.public),
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
            if !identity.type_args.is_empty() {
                groups
                    .entry((
                        dispatch_owner.clone(),
                        function.name.clone(),
                        function.arity,
                    ))
                    .or_default()
                    .push(candidate.clone());
            }
            groups
                .entry((
                    identity.trait_name.clone(),
                    function.name.clone(),
                    function.arity,
                ))
                .or_default()
                .push(candidate);
        }
    }
}

/// Qualifies provider-local nominal names retained in conformance text.
fn qualify_conformance_type(ty: &mut CoreType, core: &CoreModule) {
    let local_types = core
        .types
        .iter()
        .map(|declaration| declaration.name.as_str())
        .collect::<HashSet<_>>();
    ty.visit_names_mut(&mut |name| {
        if !name.contains('.') && local_types.contains(name.as_str()) {
            *name = format!("{}.{}", core.module, name);
        }
    });
}
