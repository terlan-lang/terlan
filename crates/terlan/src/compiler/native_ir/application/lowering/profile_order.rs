//! Dependency ordering for native suspension profile inference.

use super::*;

pub(super) fn profile_dependency_order(
    candidates: &[Candidate<'_>],
    selected: &[bool],
    resolvers: &HashMap<String, HashMap<CallIdentity, usize>>,
) -> Vec<usize> {
    fn visit(
        candidate_index: usize,
        candidates: &[Candidate<'_>],
        selected: &[bool],
        resolvers: &HashMap<String, HashMap<CallIdentity, usize>>,
        states: &mut [u8],
        order: &mut Vec<usize>,
    ) {
        if !selected[candidate_index] || states[candidate_index] != 0 {
            return;
        }
        states[candidate_index] = 1;
        let candidate = &candidates[candidate_index];
        if let Some(body) = candidate
            .function
            .clauses
            .first()
            .and_then(|clause| clause.body.core_expr.as_ref())
        {
            let resolver = &resolvers[&candidate.core.module];
            let mut dependencies = Vec::new();
            dynamic_targets::walk_calls(body, &mut |function, args| {
                if let Some(target) = resolver.get(&(function.to_string(), args.len())) {
                    dependencies.push(*target);
                }
            });
            dependencies.sort_unstable();
            dependencies.dedup();
            for dependency in dependencies {
                visit(dependency, candidates, selected, resolvers, states, order);
            }
        }
        states[candidate_index] = 2;
        order.push(candidate_index);
    }

    let mut states = vec![0; candidates.len()];
    let mut order = Vec::with_capacity(candidates.len());
    for candidate_index in 0..candidates.len() {
        visit(
            candidate_index,
            candidates,
            selected,
            resolvers,
            &mut states,
            &mut order,
        );
    }
    order
}
