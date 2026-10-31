use super::*;

/// Finds a local or already-qualified overload group.
pub(super) fn local_overload_candidates<'a>(
    current_module: &str,
    function: &str,
    arity: usize,
    groups: &'a HashMap<OverloadKey, Vec<OverloadCandidate>>,
) -> Option<&'a [OverloadCandidate]> {
    if let Some(candidates) = groups.get(&(current_module.to_string(), function.to_string(), arity))
    {
        return Some(candidates);
    }
    groups
        .iter()
        .find_map(|((module, name, candidate_arity), candidates)| {
            (*candidate_arity == arity
                && canonical_callable_path(&format!("{module}.{name}"))
                    == canonical_callable_path(function))
            .then_some(candidates.as_slice())
        })
}

/// Looks up a non-overloaded local or qualified call result.
pub(super) fn lookup_call_return(
    current_module: &str,
    function: &str,
    arity: usize,
    returns: &HashMap<OverloadKey, CoreType>,
) -> Option<CoreType> {
    returns
        .get(&(current_module.to_string(), function.to_string(), arity))
        .cloned()
        .or_else(|| {
            returns
                .iter()
                .find_map(|((module, name, candidate_arity), result)| {
                    (*candidate_arity == arity
                        && canonical_callable_path(&format!("{module}.{name}"))
                            == canonical_callable_path(function))
                    .then(|| result.clone())
                })
        })
}

/// Collapses only a repeated default nominal segment in a qualified callable
/// owner (`pkg.Handler.Handler.handle` -> `pkg.Handler.handle`).
fn canonical_callable_path(path: &str) -> String {
    let Some((owner, method)) = path.rsplit_once('.') else {
        return path.to_string();
    };
    let Some((parent, leaf)) = owner.rsplit_once('.') else {
        return path.to_string();
    };
    if parent.rsplit('.').next() == Some(leaf) {
        format!("{parent}.{method}")
    } else {
        path.to_string()
    }
}
