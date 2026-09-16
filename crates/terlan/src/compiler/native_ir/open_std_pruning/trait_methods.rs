//! Trait-instance graph nodes retain implementations until typed selection.

use super::*;

fn identities(cores: &[CoreModule]) -> impl Iterator<Item = (FunctionKey, FunctionKey)> + '_ {
    cores.iter().flat_map(|core| {
        core.functions.iter().flat_map(move |function| {
            function.trait_method.iter().flat_map(move |identity| {
                let mut owners = vec![identity.trait_name.clone()];
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
}

/// Includes dispatch nodes in exact qualified call resolution.
pub(super) fn providers(cores: &[CoreModule]) -> impl Iterator<Item = FunctionKey> + '_ {
    identities(cores).map(|(dispatch, _)| dispatch)
}

/// Retains all candidates for a reachable dispatch; selection still rejects
/// ambiguous, inaccessible or incompatible implementations before native code.
pub(super) fn add_edges(
    cores: &[CoreModule],
    edges: &mut HashMap<FunctionKey, HashSet<FunctionKey>>,
) {
    for (dispatch, implementation) in identities(cores) {
        edges.entry(dispatch).or_default().insert(implementation);
    }
}
