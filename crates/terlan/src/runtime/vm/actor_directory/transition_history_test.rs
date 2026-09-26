use super::{TransitionHistory, VmActorHandle, VmActorLifecycle, MAX_TRANSITION_EVENTS};
use crate::runtime::vm::process::VmProcessId;
use std::sync::Arc;

fn handle() -> VmActorHandle {
    VmActorHandle {
        pid: VmProcessId::from_raw_for_test(1),
        slot: 0,
        actor_generation: 1,
    }
}

fn record(history: &TransitionHistory, owner: u64) {
    history.record(
        handle(),
        VmActorLifecycle::Queued,
        VmActorLifecycle::Executing,
        owner,
        1,
    );
}

#[test]
fn transition_history_bounds_storage_and_rejects_incomplete_replay() {
    let history = TransitionHistory::default();
    for _ in 0..MAX_TRANSITION_EVENTS {
        record(&history, 1);
    }
    let complete = history.snapshot();
    assert_eq!(complete.len(), MAX_TRANSITION_EVENTS);
    assert_eq!(complete.first().unwrap().sequence, 1);
    assert_eq!(
        complete.last().unwrap().sequence,
        MAX_TRANSITION_EVENTS as u64
    );
    let capacity = history.state.lock().unwrap().events.capacity();

    for _ in 0..MAX_TRANSITION_EVENTS * 4 {
        record(&history, 2);
    }
    {
        let state = history.state.lock().unwrap();
        assert_eq!(state.events, complete);
        assert_eq!(state.events.capacity(), capacity);
        assert_eq!(state.omitted, (MAX_TRANSITION_EVENTS * 4) as u64);
    }
    assert!(std::panic::catch_unwind(|| history.snapshot()).is_err());
}

#[test]
fn transition_history_serializes_concurrent_sequence_assignment() {
    let history = Arc::new(TransitionHistory::default());
    std::thread::scope(|scope| {
        for owner in 1..=4 {
            let history = Arc::clone(&history);
            scope.spawn(move || {
                for _ in 0..256 {
                    record(&history, owner);
                }
            });
        }
    });
    let events = history.snapshot();
    assert_eq!(events.len(), 1024);
    for (index, event) in events.iter().enumerate() {
        assert_eq!(event.sequence, index as u64 + 1);
        assert_eq!(event.handle, handle());
        assert_eq!(event.from, VmActorLifecycle::Queued);
        assert_eq!(event.to, VmActorLifecycle::Executing);
        assert_eq!(event.owner_generation, 1);
    }
    for owner in 1..=4 {
        assert_eq!(
            events.iter().filter(|event| event.owner == owner).count(),
            256
        );
    }
}
