use super::{
    VmSchedulerClass, VmSchedulerMetrics, VmSchedulerQueueTransition, MAX_QUEUE_TRANSITIONS,
};

fn transition(tick: u64) -> VmSchedulerQueueTransition {
    VmSchedulerQueueTransition {
        tick,
        pid: 1,
        action: "enqueue",
        class: VmSchedulerClass::Normal,
        queue_len: 1,
    }
}

#[test]
fn scheduler_history_bounds_storage_and_rejects_incomplete_replay() {
    let mut metrics = VmSchedulerMetrics::default();
    for tick in 0..MAX_QUEUE_TRANSITIONS as u64 {
        metrics.record_queue_transition(transition(tick));
    }
    let complete = metrics.queue_transitions().to_vec();
    assert_eq!(complete.len(), MAX_QUEUE_TRANSITIONS);
    assert_eq!(complete.first().unwrap().tick, 0);
    assert_eq!(
        complete.last().unwrap().tick,
        MAX_QUEUE_TRANSITIONS as u64 - 1
    );
    let capacity = metrics.queue_transitions.capacity();
    for tick in 0..4 * MAX_QUEUE_TRANSITIONS as u64 {
        metrics.record_queue_transition(transition(tick));
    }
    assert_eq!(metrics.queue_transitions, complete);
    assert_eq!(metrics.queue_transitions.capacity(), capacity);
    assert_eq!(
        metrics.omitted_queue_transitions,
        4 * MAX_QUEUE_TRANSITIONS as u64
    );
    assert!(std::panic::catch_unwind(|| metrics.queue_transitions()).is_err());

    metrics.reap_queue_transitions(1);
    assert!(metrics.queue_transitions.is_empty());
    assert!(std::panic::catch_unwind(|| metrics.queue_transitions()).is_err());
}

#[test]
fn scheduler_history_reaps_only_the_exited_process() {
    let mut metrics = VmSchedulerMetrics::default();
    metrics.record_queue_transition(transition(1));
    let mut retained = transition(2);
    retained.pid = 2;
    metrics.record_queue_transition(retained.clone());
    metrics.reap_queue_transitions(1);
    assert_eq!(metrics.queue_transitions(), &[retained]);
    metrics.record_queue_transition(transition(3));
    assert_eq!(metrics.queue_transitions().len(), 2);
}
