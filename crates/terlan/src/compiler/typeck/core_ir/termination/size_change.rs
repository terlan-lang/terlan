//! Call-graph closure and lexicographic size-change classification.

use super::*;

/// Computes transitive reachability for exact local function identities.
pub(super) fn call_reachability(
    keys: &[(String, usize)],
    facts: &[FunctionFacts],
) -> Vec<Vec<bool>> {
    let indices = keys
        .iter()
        .enumerate()
        .map(|(index, key)| (key.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut reachable = vec![vec![false; keys.len()]; keys.len()];
    for (caller, fact) in facts.iter().enumerate() {
        for edge in &fact.calls {
            if let Some(callee) = indices.get(&(edge.callee.clone(), edge.callee_arity)) {
                reachable[caller][*callee] = true;
            }
        }
    }
    for via in 0..keys.len() {
        for from in 0..keys.len() {
            for to in 0..keys.len() {
                reachable[from][to] |= reachable[from][via] && reachable[via][to];
            }
        }
    }
    reachable
}

/// Classifies a recursive component from its witnessed decrease and yield edges.
pub(super) fn classify_component(
    recursive: bool,
    actor_cycle: bool,
    component_len: usize,
    arity: usize,
    edges: &[CoreRecursiveCallEvidence],
) -> (CoreTerminationState, CoreTerminationReason, Vec<usize>) {
    if !recursive {
        return (
            CoreTerminationState::Proven,
            CoreTerminationReason::NonRecursive,
            Vec::new(),
        );
    }
    if actor_cycle {
        if !edges.is_empty()
            && edges
                .iter()
                .all(|edge| !edge.productivity_boundaries.is_empty())
        {
            return (
                CoreTerminationState::ProductivePersistent,
                CoreTerminationReason::ActorCycleProductive,
                Vec::new(),
            );
        }
        return (
            CoreTerminationState::IntentionalPersistent,
            CoreTerminationReason::ActorCycleMissingProductivityBoundary,
            Vec::new(),
        );
    }
    let Some(measure) = find_lexicographic_measure(arity, edges) else {
        let reason = if edges
            .iter()
            .any(|edge| edge.argument_relations.iter().any(|item| item.is_strict()))
        {
            CoreTerminationReason::RecursiveEdgeNotDecreasing
        } else {
            CoreTerminationReason::UnsupportedRecursiveShape
        };
        return (CoreTerminationState::Unproven, reason, Vec::new());
    };
    let strict_kinds = edges
        .iter()
        .flat_map(|edge| edge.argument_relations.iter().copied())
        .filter(|relation| relation.is_strict())
        .collect::<BTreeSet<_>>();
    let reason = if component_len > 1 {
        CoreTerminationReason::MutualSizeChange
    } else if measure.len() > 1 {
        CoreTerminationReason::LexicographicDescent
    } else if strict_kinds == BTreeSet::from([CoreDecreaseKind::Structural]) {
        CoreTerminationReason::StructuralDescent
    } else {
        CoreTerminationReason::GuardedIntegerDescent
    };
    (CoreTerminationState::Proven, reason, measure)
}

fn find_lexicographic_measure(
    arity: usize,
    edges: &[CoreRecursiveCallEvidence],
) -> Option<Vec<usize>> {
    if edges.is_empty()
        || edges
            .iter()
            .any(|edge| edge.argument_relations.len() != arity)
    {
        return None;
    }
    for index in 0..arity {
        let singleton = [index];
        if lexicographically_decreases(&singleton, edges) {
            return Some(singleton.to_vec());
        }
    }
    let mut order = (0..arity).collect::<Vec<_>>();
    if lexicographically_decreases(&order, edges) {
        return Some(order);
    }
    if arity <= 7 && permute_measure(0, &mut order, edges) {
        return Some(order);
    }
    None
}

fn permute_measure(start: usize, order: &mut [usize], edges: &[CoreRecursiveCallEvidence]) -> bool {
    if start == order.len() {
        return lexicographically_decreases(order, edges);
    }
    for index in start..order.len() {
        order.swap(start, index);
        if permute_measure(start + 1, order, edges) {
            return true;
        }
        order.swap(start, index);
    }
    false
}

fn lexicographically_decreases(order: &[usize], edges: &[CoreRecursiveCallEvidence]) -> bool {
    edges.iter().all(|edge| {
        order.iter().any(|index| {
            let relation = edge.argument_relations[*index];
            relation.is_strict()
                && order
                    .iter()
                    .take_while(|candidate| *candidate != index)
                    .all(|previous| {
                        edge.argument_relations[*previous] == CoreDecreaseKind::NonIncreasing
                    })
        })
    })
}

/// Orders recursive-edge evidence deterministically for reproducible reports.
pub(super) fn edge_order(
    left: &CoreRecursiveCallEvidence,
    right: &CoreRecursiveCallEvidence,
) -> std::cmp::Ordering {
    left.caller
        .cmp(&right.caller)
        .then_with(|| left.caller_arity.cmp(&right.caller_arity))
        .then_with(|| left.callee.cmp(&right.callee))
        .then_with(|| left.callee_arity.cmp(&right.callee_arity))
        .then_with(|| left.argument_relations.cmp(&right.argument_relations))
}
