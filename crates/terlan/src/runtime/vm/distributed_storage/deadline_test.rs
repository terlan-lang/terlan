use super::*;
use crate::runtime::vm::{
    distributed_state::{
        VmDistributedStateEntry, VmDistributedStatePolicy, VmDistributedStateScope,
        VmDistributedStateVersion,
    },
    distributed_storage::{
        VmDistributedStorageMode, VmDistributedStoragePolicy, VmDistributedStorageSnapshot,
    },
    process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessTable},
    scheduler::{VmScheduler, VmSchedulerConfig},
    timer::{VmTimerEvent, VmTimerKind, VmTimerTable},
    ReplValue,
};

fn opened_adapter() -> VmDistributedStorageAdapter {
    let mut adapter = VmDistributedStorageAdapter::new(VmDistributedStoragePolicy::force_local());
    assert_eq!(
        adapter.open(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );
    let snapshot = VmDistributedStorageSnapshot::new(
        "checkpoint-a",
        1,
        vec![VmDistributedStateEntry {
            scope: VmDistributedStateScope::new("state", "cart").expect("scope"),
            owner_node_id: "node-a".to_string(),
            value: ReplValue::Int(1),
            version: VmDistributedStateVersion::new(1, "node-a").expect("version"),
            policy: VmDistributedStatePolicy::WinnerTakesAll,
        }],
    )
    .expect("snapshot");
    assert!(matches!(
        adapter.append(snapshot),
        VmDistributedStorageOutcome::Appended { sequence: 1, .. }
    ));
    adapter
}

fn runtime() -> (VmProcessTable, VmScheduler, VmTimerTable, VmProcessId) {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("test.Checkpoint", "flush", 0));
    (
        processes,
        VmScheduler::new(VmSchedulerConfig::new(10, 100)),
        VmTimerTable::default(),
        owner,
    )
}

#[test]
fn checkpoint_flush_completion_cancels_deadline_before_advancing_durable_state() {
    let mut adapter = opened_adapter();
    let (processes, _scheduler, mut timers, owner) = runtime();
    let mut queue = VmCheckpointFlushDeadlineQueue::default();
    let scheduled = queue
        .start(&adapter, &mut timers, &processes, owner, 10, 5)
        .expect("schedule flush");

    assert_eq!(scheduled.sequence, 1);
    assert_eq!(scheduled.deadline_tick, 15);
    assert_eq!(
        queue
            .complete(&mut adapter, &mut timers, scheduled.timer_id)
            .expect("complete flush"),
        VmCheckpointFlushCompletion::Completed {
            timer_id: scheduled.timer_id,
            outcome: VmDistributedStorageOutcome::Flushed { sequence: 1 },
        }
    );
    assert_eq!(queue.pending_len(), 0);
    assert_eq!(timers.snapshots(), Vec::new());
    assert_eq!(
        adapter
            .durable_flush_proof()
            .expect("durable proof")
            .flushed_sequence(),
        1
    );
}

#[test]
fn checkpoint_flush_deadline_wins_race_without_advancing_state_and_allows_retry() {
    let mut adapter = opened_adapter();
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    let mut queue = VmCheckpointFlushDeadlineQueue::default();
    let first = queue
        .start(&adapter, &mut timers, &processes, owner, 0, 5)
        .expect("schedule flush");
    let events = timers.advance_clock(&mut processes, &mut scheduler, 5);

    assert_eq!(
        queue
            .complete(&mut adapter, &mut timers, first.timer_id)
            .expect_err("delivered deadline must win"),
        format!(
            "checkpoint flush timer {} no longer owns completion: missing timer {}",
            first.timer_id.as_u64(),
            first.timer_id.as_u64()
        )
    );
    assert_eq!(
        queue.handle_timer_event(&events[0]).expect("timeout event"),
        Some(VmCheckpointFlushCompletion::TimedOut {
            timer_id: first.timer_id,
            outcome: VmDistributedStorageOutcome::FlushTimedOut {
                operation: VmDistributedStorageOperation::Flush,
                sequence: 1,
            },
        })
    );
    assert_eq!(
        adapter
            .durable_flush_proof()
            .expect("durable proof")
            .flushed_sequence(),
        0
    );

    let retry = queue
        .start(&adapter, &mut timers, &processes, owner, 5, 5)
        .expect("retry flush");
    assert!(matches!(
        queue
            .complete(&mut adapter, &mut timers, retry.timer_id)
            .expect("complete retry"),
        VmCheckpointFlushCompletion::Completed {
            outcome: VmDistributedStorageOutcome::Flushed { sequence: 1 },
            ..
        }
    ));
}

#[test]
fn checkpoint_flush_start_rejects_invalid_and_duplicate_requests_atomically() {
    let adapter = opened_adapter();
    let (processes, _scheduler, mut timers, owner) = runtime();
    let mut queue = VmCheckpointFlushDeadlineQueue::default();

    assert_eq!(
        queue
            .start(&adapter, &mut timers, &processes, owner, 0, 0)
            .expect_err("zero timeout"),
        "checkpoint flush timeout must be positive"
    );
    assert_eq!(
        queue
            .start(&adapter, &mut timers, &processes, owner, u64::MAX, 1)
            .expect_err("overflow"),
        "checkpoint flush deadline overflow"
    );
    let scheduled = queue
        .start(&adapter, &mut timers, &processes, owner, 0, 1)
        .expect("schedule flush");
    assert_eq!(
        queue
            .start(&adapter, &mut timers, &processes, owner, 0, 2)
            .expect_err("duplicate"),
        format!(
            "checkpoint flush for process {} is already pending on timer {}",
            owner.as_u64(),
            scheduled.timer_id.as_u64()
        )
    );
    assert_eq!(queue.pending_len(), 1);
    assert_eq!(timers.snapshots().len(), 1);

    let closed = VmDistributedStorageAdapter::new(VmDistributedStoragePolicy::force_local());
    let mut other_queue = VmCheckpointFlushDeadlineQueue::default();
    assert_eq!(
        other_queue
            .start(&closed, &mut timers, &processes, owner, 0, 2)
            .expect_err("closed adapter"),
        "checkpoint flush unavailable: storage_unavailable"
    );
    assert_eq!(other_queue.pending_len(), 0);
    assert_eq!(timers.snapshots().len(), 1);
}

#[test]
fn checkpoint_flush_owner_exit_and_manual_cancel_are_terminal_without_flush() {
    let adapter = opened_adapter();
    let (mut processes, _scheduler, mut timers, owner) = runtime();
    let mut queue = VmCheckpointFlushDeadlineQueue::default();
    let cancelled = queue
        .start(&adapter, &mut timers, &processes, owner, 0, 10)
        .expect("schedule cancelled flush");
    assert_eq!(
        queue
            .cancel(&mut timers, cancelled.timer_id)
            .expect("cancel flush"),
        VmCheckpointFlushCompletion::Cancelled {
            timer_id: cancelled.timer_id,
            sequence: 1,
        }
    );

    let exited = queue
        .start(&adapter, &mut timers, &processes, owner, 0, 10)
        .expect("schedule owner exit");
    processes
        .exit_process(owner, VmExitReason::Normal)
        .expect("exit owner");
    let event = timers
        .cancel_owner_timers(owner)
        .into_iter()
        .next()
        .expect("owner exit event");
    assert_eq!(
        queue.handle_timer_event(&event).expect("handle owner exit"),
        Some(VmCheckpointFlushCompletion::OwnerExited {
            timer_id: exited.timer_id,
            sequence: 1,
        })
    );
    assert_eq!(queue.pending_len(), 0);
    assert_eq!(
        adapter
            .durable_flush_proof()
            .expect("durable proof")
            .flushed_sequence(),
        0
    );
}

#[test]
fn checkpoint_flush_ignores_unrelated_timer_events() {
    let adapter = opened_adapter();
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    let mut queue = VmCheckpointFlushDeadlineQueue::default();
    let scheduled = queue
        .start(&adapter, &mut timers, &processes, owner, 0, 10)
        .expect("schedule flush");
    let unrelated = timers
        .start_one_shot(&processes, owner, 1)
        .expect("unrelated timer");
    let events = timers.advance_clock(&mut processes, &mut scheduler, 1);

    assert_eq!(
        events,
        vec![VmTimerEvent::Fired {
            timer_id: unrelated,
            owner,
            kind: VmTimerKind::OneShot,
        }]
    );
    assert_eq!(queue.handle_timer_event(&events[0]).expect("ignore"), None);
    assert_eq!(queue.pending_len(), 1);
    assert_eq!(timers.snapshots()[0].id, scheduled.timer_id);
}

#[test]
fn checkpoint_flush_completion_preserves_typed_adapter_failure_and_cancels_deadline() {
    let mut adapter = opened_adapter();
    let (processes, _scheduler, mut timers, owner) = runtime();
    let mut queue = VmCheckpointFlushDeadlineQueue::default();
    let scheduled = queue
        .start(&adapter, &mut timers, &processes, owner, 0, 10)
        .expect("schedule flush");
    adapter.fail_next_flush_for_test();

    assert_eq!(
        queue
            .complete(&mut adapter, &mut timers, scheduled.timer_id)
            .expect("complete failed adapter flush"),
        VmCheckpointFlushCompletion::Completed {
            timer_id: scheduled.timer_id,
            outcome: VmDistributedStorageOutcome::FinalizeFailed {
                operation: VmDistributedStorageOperation::Flush,
                sequence: 1,
            },
        }
    );
    assert_eq!(queue.pending_len(), 0);
    assert!(timers.snapshots().is_empty());
    assert_eq!(
        adapter
            .durable_flush_proof()
            .expect("durable proof")
            .flushed_sequence(),
        0
    );
}

#[test]
fn checkpoint_flush_rejects_foreign_owner_and_non_one_shot_events_without_consuming_intent() {
    let mut adapter = opened_adapter();
    let (mut processes, _scheduler, mut timers, owner) = runtime();
    let foreign = processes.spawn_root(VmProcessSource::new("test.Foreign", "wait", 0));
    let mut queue = VmCheckpointFlushDeadlineQueue::default();
    let scheduled = queue
        .start(&adapter, &mut timers, &processes, owner, 0, 10)
        .expect("schedule flush");

    let foreign_event = VmTimerEvent::Fired {
        timer_id: scheduled.timer_id,
        owner: foreign,
        kind: VmTimerKind::OneShot,
    };
    assert_eq!(
        queue
            .handle_timer_event(&foreign_event)
            .expect_err("foreign owner"),
        format!(
            "checkpoint flush timer {} owner mismatch: expected {}, observed {}",
            scheduled.timer_id.as_u64(),
            owner.as_u64(),
            foreign.as_u64()
        )
    );
    let interval_event = VmTimerEvent::Fired {
        timer_id: scheduled.timer_id,
        owner,
        kind: VmTimerKind::Interval,
    };
    assert_eq!(
        queue
            .handle_timer_event(&interval_event)
            .expect_err("non-one-shot kind"),
        format!(
            "checkpoint flush timer {} emitted non-one-shot outcome",
            scheduled.timer_id.as_u64()
        )
    );
    assert_eq!(queue.pending_len(), 1);
    assert!(matches!(
        queue
            .complete(&mut adapter, &mut timers, scheduled.timer_id)
            .expect("real completion remains possible"),
        VmCheckpointFlushCompletion::Completed {
            outcome: VmDistributedStorageOutcome::Flushed { sequence: 1 },
            ..
        }
    ));
}
