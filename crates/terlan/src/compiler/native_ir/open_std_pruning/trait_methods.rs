//! Trait-instance graph nodes retain implementations until typed selection.

use super::*;
use crate::terlan_typeck::CoreType;

fn identities(cores: &[&CoreModule]) -> Vec<(FunctionKey, FunctionKey)> {
    let mut identities = cores
        .iter()
        .flat_map(|core| {
            core.functions.iter().flat_map(move |function| {
                function.trait_method.iter().flat_map(move |identity| {
                    let mut owners = vec![identity.trait_name.clone()];
                    if let Some(short) = identity.trait_name.rsplit('.').next() {
                        if matches!(
                            (short, identity.method.as_str()),
                            ("IndexGet", "get_at") | ("IndexSet", "set_at")
                        ) {
                            owners.push(short.to_string());
                        }
                    }
                    // Receiver syntax retains implementation bodies until the
                    // checked receiver type selects one, even without a trait call.
                    if !function.params.is_empty() {
                        owners.push("__receiver__".to_string());
                    }
                    if !identity.type_args.is_empty() {
                        owners.push(identity.dispatch_owner());
                    }
                    owners.into_iter().map(move |owner| {
                        (
                            (owner, identity.method.clone(), function.arity),
                            (core.module.clone(), function.name.clone(), function.arity),
                        )
                    })
                })
            })
        })
        .collect::<Vec<_>>();
    identities.extend(declaration_site_identities(cores));
    identities
}

/// Recovers dispatch nodes for declaration-site `implements` methods.
///
/// Interface round-tripping intentionally records these as conformance facts,
/// not explicit method adapters. Reachability must therefore use the same
/// facts as overload selection or a qualified trait call can prune its only
/// executable receiver before selection runs.
fn declaration_site_identities(cores: &[&CoreModule]) -> Vec<(FunctionKey, FunctionKey)> {
    let mut identities = Vec::new();
    for core in cores {
        let local_types = core
            .types
            .iter()
            .map(|declaration| declaration.name.as_str())
            .collect::<HashSet<_>>();
        for conformance in &core.trait_conformances {
            let Some(mut trait_type) =
                crate::terlan_typeck::core_type_from_text(&conformance.trait_ref)
            else {
                continue;
            };
            let Some(mut owner_type) =
                crate::terlan_typeck::core_type_from_text(&conformance.for_type)
            else {
                continue;
            };
            qualify_type(&mut trait_type, core, &local_types);
            qualify_type(&mut owner_type, core, &local_types);
            let (trait_name, type_args) = match trait_type {
                CoreType::Apply { constructor, args } => (constructor, args),
                CoreType::Named(name) => (name, Vec::new()),
                _ => continue,
            };
            let dispatch_owner = crate::terlan_typeck::CoreTraitMethodIdentity {
                trait_name: trait_name.clone(),
                type_args,
                method: String::new(),
            }
            .dispatch_owner();
            for function in &core.functions {
                let receiver_type = function
                    .params
                    .first()
                    .and_then(|parameter| parameter.core_ty.as_ref())
                    .cloned()
                    .map(|mut ty| {
                        qualify_type(&mut ty, core, &local_types);
                        ty
                    });
                if !function.receiver_method
                    || function.trait_method.is_some()
                    || receiver_type.as_ref() != Some(&owner_type)
                {
                    continue;
                }
                let implementation = (core.module.clone(), function.name.clone(), function.arity);
                for owner in [&trait_name, &dispatch_owner] {
                    identities.push((
                        (owner.clone(), function.name.clone(), function.arity),
                        implementation.clone(),
                    ));
                }
            }
        }
    }
    identities
}

fn qualify_type(ty: &mut CoreType, core: &CoreModule, local_types: &HashSet<&str>) {
    ty.visit_names_mut(&mut |name| {
        if !name.contains('.') && local_types.contains(name.as_str()) {
            *name = format!("{}.{}", core.module, name);
        }
    });
}

/// Includes dispatch nodes in exact qualified call resolution.
pub(super) fn providers<'a>(cores: &'a [&'a CoreModule]) -> impl Iterator<Item = FunctionKey> + 'a {
    identities(cores).into_iter().map(|(dispatch, _)| dispatch)
}

/// Retains all candidates for a reachable dispatch; selection still rejects
/// ambiguous, inaccessible or incompatible implementations before native code.
pub(super) fn add_edges(
    cores: &[&CoreModule],
    edges: &mut HashMap<FunctionKey, HashSet<FunctionKey>>,
) {
    for (dispatch, implementation) in identities(cores) {
        edges.entry(dispatch).or_default().insert(implementation);
    }
}
