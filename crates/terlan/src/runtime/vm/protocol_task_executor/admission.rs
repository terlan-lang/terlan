//! Load-aware connection placement for the VM protocol owners.

use std::sync::Arc;

use super::{VmProtocolShardIngress, MAX_TASKS_PER_SHARD};

/// Keeps isolated sockets local and transfers only material owner overload.
pub(super) fn admission_target(
    ingresses: &[Arc<VmProtocolShardIngress>],
    local_index: usize,
    next_tie: usize,
) -> Option<usize> {
    let local_load = ingresses[local_index].load();
    if local_load >= MAX_TASKS_PER_SHARD {
        return least_loaded_shard(ingresses, next_tie);
    }
    if local_load == 0 {
        return Some(local_index);
    }
    sampled_loaded_shard(ingresses, next_tie).or_else(|| least_loaded_shard(ingresses, next_tie))
}

/// Samples two rotating owners for ordinary connection placement.
///
/// Power-of-two load choice retains responsive balancing without reading
/// every owner's independently mutated cacheline on each accept. The complete
/// scan remains the capacity fallback when both sampled owners are full.
pub(super) fn sampled_loaded_shard(
    ingresses: &[Arc<VmProtocolShardIngress>],
    next_tie: usize,
) -> Option<usize> {
    let width = ingresses.len();
    if width == 0 {
        return None;
    }
    let first = next_tie % width;
    let second = (first + (width / 2).max(1)) % width;
    let first_load = ingresses[first].load();
    let second_load = ingresses[second].load();
    match (
        first_load < MAX_TASKS_PER_SHARD,
        second_load < MAX_TASKS_PER_SHARD,
    ) {
        (true, true) if second_load < first_load => Some(second),
        (true, _) => Some(first),
        (_, true) => Some(second),
        (false, false) => None,
    }
}

/// Selects the least-loaded shard and rotates equal-load admission fairly.
pub(super) fn least_loaded_shard(
    ingresses: &[Arc<VmProtocolShardIngress>],
    next_tie: usize,
) -> Option<usize> {
    let mut selected = None;
    for offset in 0..ingresses.len() {
        let index = (next_tie + offset) % ingresses.len();
        let load = ingresses[index].load();
        if load >= MAX_TASKS_PER_SHARD {
            continue;
        }
        match selected {
            None => selected = Some((index, load)),
            Some((_, selected_load)) if load < selected_load => selected = Some((index, load)),
            Some(_) => {}
        }
    }
    selected.map(|(index, _)| index)
}
