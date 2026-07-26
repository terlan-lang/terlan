//! Load-aware connection placement for the VM protocol owners.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use super::{VmProtocolShardIngress, MAX_TASKS_PER_SHARD};

/// Cross-owner reservations isolated from owner-loop parking writes.
#[repr(align(64))]
pub(super) struct VmProtocolShardLoad(AtomicUsize);

impl VmProtocolShardLoad {
    pub(super) const fn new(value: usize) -> Self {
        Self(AtomicUsize::new(value))
    }

    pub(super) fn load(&self, ordering: Ordering) -> usize {
        self.0.load(ordering)
    }

    #[cfg(test)]
    pub(super) fn store(&self, value: usize, ordering: Ordering) {
        self.0.store(value, ordering);
    }

    pub(super) fn fetch_add(&self, value: usize, ordering: Ordering) -> usize {
        self.0.fetch_add(value, ordering)
    }

    pub(super) fn fetch_sub(&self, value: usize, ordering: Ordering) -> usize {
        self.0.fetch_sub(value, ordering)
    }
}

/// Owner sleep state isolated from acceptor reservation traffic.
#[repr(align(64))]
pub(super) struct VmProtocolOwnerParked(AtomicBool);

impl VmProtocolOwnerParked {
    pub(super) const fn new(value: bool) -> Self {
        Self(AtomicBool::new(value))
    }

    pub(super) fn load(&self, ordering: Ordering) -> bool {
        self.0.load(ordering)
    }

    pub(super) fn store(&self, value: bool, ordering: Ordering) {
        self.0.store(value, ordering);
    }
}

/// Reserves one exact owner before the listener consumes a connection.
///
/// The acceptor is the only reservation producer. Combining placement with
/// `try_reserve` avoids first reading a remote owner's hot load cacheline and
/// then performing a second atomic operation to claim the same capacity.
pub(super) fn reserve_admission_target(
    ingresses: &[Arc<VmProtocolShardIngress>],
    local_index: usize,
    next_tie: usize,
) -> Option<usize> {
    if ingresses.len() == 1 {
        return ingresses[local_index].try_reserve().then_some(local_index);
    }
    if ingresses[local_index].load() == 0 && ingresses[local_index].try_reserve() {
        return Some(local_index);
    }
    let mut remote_candidate = None;
    for offset in 0..ingresses.len() {
        let index = (next_tie + offset) % ingresses.len();
        if index == local_index {
            continue;
        }
        let load = ingresses[index].load();
        if remote_candidate.is_none() && load < MAX_TASKS_PER_SHARD {
            remote_candidate = Some(index);
        }
        if load > 0 {
            break;
        }
    }
    if let Some(index) = remote_candidate {
        if ingresses[index].try_reserve() {
            return Some(index);
        }
    }
    reserve_remote_admission_target(ingresses, local_index, next_tie)
        .or_else(|| ingresses[local_index].try_reserve().then_some(local_index))
}

/// Reserves a non-acceptor execution owner in rotating order.
pub(super) fn reserve_remote_admission_target(
    ingresses: &[Arc<VmProtocolShardIngress>],
    local_index: usize,
    next_tie: usize,
) -> Option<usize> {
    for offset in 0..ingresses.len() {
        let index = (next_tie + offset) % ingresses.len();
        if index != local_index && ingresses[index].try_reserve() {
            return Some(index);
        }
    }
    None
}

/// Keeps isolated sockets local and transfers only material owner overload.
#[cfg(test)]
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
    if ingresses.len() > 1 {
        return rotating_remote_shard(ingresses, local_index, next_tie)
            .or_else(|| least_loaded_shard(ingresses, next_tie));
    }
    Some(local_index)
}

/// Selects the next capacity-bearing remote owner in round-robin order.
///
/// Short-lived protocol tasks complete on a similar timescale, so rotating
/// admission already distributes their load. Reading several independently
/// mutated owner counters on every accept adds cacheline traffic without
/// improving this common case; the complete scan remains the full-capacity
/// fallback.
#[cfg(test)]
fn rotating_remote_shard(
    ingresses: &[Arc<VmProtocolShardIngress>],
    local_index: usize,
    next_tie: usize,
) -> Option<usize> {
    let width = ingresses.len();
    (0..ingresses.len())
        .map(|offset| (next_tie + offset) % width)
        .find(|index| *index != local_index && ingresses[*index].load() < MAX_TASKS_PER_SHARD)
}

/// Samples two rotating owners for ordinary connection placement.
///
/// Power-of-two load choice retains responsive balancing without reading
/// every owner's independently mutated cacheline on each accept. The complete
/// scan remains the capacity fallback when both sampled owners are full.
#[cfg(test)]
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
    select_sampled(ingresses, first, second)
}

/// Selects the lower-load available owner from two already chosen samples.
#[cfg(test)]
fn select_sampled(
    ingresses: &[Arc<VmProtocolShardIngress>],
    first: usize,
    second: usize,
) -> Option<usize> {
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
#[cfg(test)]
pub(super) fn least_loaded_shard(
    ingresses: &[Arc<VmProtocolShardIngress>],
    next_tie: usize,
) -> Option<usize> {
    least_loaded_matching(ingresses, next_tie, |_| true)
}

/// Selects the least-loaded capacity-bearing owner accepted by one predicate.
#[cfg(test)]
fn least_loaded_matching(
    ingresses: &[Arc<VmProtocolShardIngress>],
    next_tie: usize,
    mut include: impl FnMut(usize) -> bool,
) -> Option<usize> {
    let mut selected = None;
    for offset in 0..ingresses.len() {
        let index = (next_tie + offset) % ingresses.len();
        if !include(index) {
            continue;
        }
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
