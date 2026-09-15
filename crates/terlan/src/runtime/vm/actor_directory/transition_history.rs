//! Bounded ownership traces for tests, with no production event retention.
//!
//! Ownership is enforced by the actor state and mutator token, not this history.
//! All trace readers are test-only; production must not accumulate an event for
//! every scheduler reduction or acquire a diagnostic-history mutex.

use super::{VmActorHandle, VmActorLifecycle};
#[cfg(test)]
use std::sync::Mutex;

/// Test instrumentation that occupies no storage in a production directory.
#[derive(Debug, Default)]
pub(super) struct TransitionHistory {
    #[cfg(test)]
    state: Mutex<HistoryState>,
}

#[cfg(not(test))]
const _: () = assert!(std::mem::size_of::<TransitionHistory>() == 0);

#[cfg(not(test))]
impl TransitionHistory {
    /// Production ownership transitions require no diagnostic allocation or lock.
    #[inline]
    pub(super) fn record(
        &self,
        _: VmActorHandle,
        _: VmActorLifecycle,
        _: VmActorLifecycle,
        _: u64,
        _: u64,
    ) {
    }
}

/// Stable ownership identity observed by focused replay and diagnostics tests.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmActorTransitionEvent {
    /// Monotonic event sequence within one directory.
    pub(crate) sequence: u64,
    /// Actor affected by the transition.
    pub(crate) handle: VmActorHandle,
    /// Lifecycle before the transition.
    pub(crate) from: VmActorLifecycle,
    /// Lifecycle after the transition.
    pub(crate) to: VmActorLifecycle,
    /// Scheduler owner, or zero for unowned transitions.
    pub(crate) owner: u64,
    /// Ownership generation visible after the transition.
    pub(crate) owner_generation: u64,
}

#[cfg(test)]
const MAX_TRANSITION_EVENTS: usize = 4096;

#[cfg(test)]
#[derive(Debug, Default)]
struct HistoryState {
    events: Vec<VmActorTransitionEvent>,
    omitted: u64,
}

#[cfg(test)]
impl TransitionHistory {
    /// Records a bounded prefix; overflow must never masquerade as a full trace.
    pub(super) fn record(
        &self,
        handle: VmActorHandle,
        from: VmActorLifecycle,
        to: VmActorLifecycle,
        owner: u64,
        owner_generation: u64,
    ) {
        let mut state = self
            .state
            .lock()
            .expect("actor transition log mutex must not be poisoned");
        if state.events.len() == MAX_TRANSITION_EVENTS {
            state.omitted = state.omitted.saturating_add(1);
            return;
        }
        // Assign sequence and append under the same lock, so concurrent readers
        // never observe a higher sequence before a lower one.
        let sequence = state.events.len() as u64 + 1;
        state.events.push(VmActorTransitionEvent {
            sequence,
            handle,
            from,
            to,
            owner,
            owner_generation,
        });
    }

    /// Rejects incomplete evidence instead of silently weakening replay checks.
    pub(super) fn snapshot(&self) -> Vec<VmActorTransitionEvent> {
        let state = self
            .state
            .lock()
            .expect("actor transition log mutex must not be poisoned");
        assert_eq!(
            state.omitted, 0,
            "actor transition test history exceeded its event budget; trace is incomplete"
        );
        state.events.clone()
    }
}

#[cfg(test)]
#[path = "transition_history_test.rs"]
mod transition_history_test;
