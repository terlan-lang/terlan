use super::super::process::{
    VmExitReason, VmProcessId, VmProcessResumeState, VmProcessSource, VmProcessState,
    VmProcessTable,
};
use super::super::scheduler::{
    VmScheduler, VmSchedulerConfig, VmSchedulerDecision, VmSchedulerOutcome,
};
use super::super::ReplValue;
use super::{
    timer_event_mailbox_value, timer_event_owner, VmTimer, VmTimerCancellationToken, VmTimerEvent,
    VmTimerId, VmTimerKind, VmTimerTable,
};
use std::path::PathBuf;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn timer_table_starts_one_shot_timer_and_exposes_snapshot() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut timers = VmTimerTable::default();

    let timer = timers
        .start_one_shot(&processes, owner, 42)
        .expect("timer should start");
    let snapshots = timers.snapshots();

    assert_eq!(timer.as_u64(), 1);
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].id, timer);
    assert_eq!(snapshots[0].owner, owner);
    assert_eq!(snapshots[0].deadline_tick, 42);
    assert_eq!(snapshots[0].kind, VmTimerKind::OneShot);
}

#[test]
fn timer_table_interval_timer_fires_and_reschedules() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("interval"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();

    let timer = timers
        .start_interval(&processes, owner, 5, 3)
        .expect("interval timer should start");

    let fired_events = timers.advance_clock(&mut processes, &mut scheduler, 5);
    assert_eq!(
        fired_events,
        vec![VmTimerEvent::Fired {
            timer_id: timer,
            owner,
            kind: VmTimerKind::Interval,
        }]
    );
    let snapshots = timers.snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].id, timer);
    assert_eq!(snapshots[0].deadline_tick, 8);
    assert_eq!(snapshots[0].kind, VmTimerKind::Interval);

    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 8),
        vec![VmTimerEvent::Fired {
            timer_id: timer,
            owner,
            kind: VmTimerKind::Interval,
        }]
    );
    assert_eq!(timers.snapshots()[0].deadline_tick, 11);
}

#[test]
fn timer_table_coalesces_late_interval_timer_and_reschedules_after_skipped_deadlines() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("interval"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();

    let timer = timers
        .start_interval(&processes, owner, 5, 3)
        .expect("interval timer should start");

    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 14),
        vec![VmTimerEvent::Coalesced {
            timer_id: timer,
            owner,
            kind: VmTimerKind::Interval,
            skipped_intervals: 3,
            next_deadline_tick: 17,
        }]
    );
    let snapshots = timers.snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].id, timer);
    assert_eq!(snapshots[0].deadline_tick, 17);
}

#[test]
fn timer_table_reports_overflow_when_interval_reschedule_exceeds_tick_range() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("interval"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();

    let timer = timers
        .start_interval(&processes, owner, u64::MAX, 1)
        .expect("interval timer should start");

    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, u64::MAX),
        vec![VmTimerEvent::Overflow {
            timer_id: timer,
            owner,
            kind: VmTimerKind::Interval,
        }]
    );
    assert!(timers.snapshots().is_empty());
}

#[test]
fn timer_table_reports_overflow_when_late_interval_coalescing_exceeds_tick_range() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("interval"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();

    let timer = timers
        .start_interval(&processes, owner, u64::MAX - 5, 4)
        .expect("interval timer should start");

    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, u64::MAX),
        vec![VmTimerEvent::Overflow {
            timer_id: timer,
            owner,
            kind: VmTimerKind::Interval,
        }]
    );
    assert!(timers.snapshots().is_empty());
}

#[test]
fn timer_table_reports_deadline_missed_for_late_interval_before_next_interval_boundary() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("interval"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();

    let timer = timers
        .start_interval(&processes, owner, 5, 3)
        .expect("interval timer should start");

    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 7),
        vec![VmTimerEvent::DeadlineMissed {
            timer_id: timer,
            owner,
            kind: VmTimerKind::Interval,
            late_by_ticks: 2,
        }]
    );
    assert_eq!(timers.snapshots()[0].deadline_tick, 8);
}

#[test]
fn timer_table_rejects_zero_interval_timer_without_installing_snapshot() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("interval"));
    let mut timers = VmTimerTable::default();

    let error = timers
        .start_interval(&processes, owner, 5, 0)
        .expect_err("zero interval should fail");

    assert_eq!(
        error,
        "interval timer for process 1 must have a positive interval"
    );
    assert!(timers.snapshots().is_empty());
}

#[test]
fn timer_table_cancels_timer_and_reports_missing_timer() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut timers = VmTimerTable::default();
    let timer = timers
        .start_one_shot(&processes, owner, 10)
        .expect("timer should start");

    let cancelled = timers.cancel(timer).expect("timer should cancel");
    let error = timers
        .cancel(timer)
        .expect_err("canceling missing timer should fail");

    assert_eq!(
        cancelled,
        VmTimerEvent::Cancelled {
            timer_id: timer,
            owner,
            kind: VmTimerKind::OneShot
        }
    );
    assert_eq!(error, "missing timer 1");
    assert!(timers.snapshots().is_empty());
}

#[test]
fn timer_table_reports_owner_exited_for_owner_timer_cleanup_in_stable_order() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let other = processes.spawn_root(source("other"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();
    let first = timers
        .start_one_shot(&processes, owner, 10)
        .expect("first timer should start");
    let skipped = timers
        .start_one_shot(&processes, other, 10)
        .expect("other timer should start");
    let second = timers
        .start_receive_timeout(&mut processes, &mut scheduler, owner, 0, 20)
        .expect("receive timeout should start");

    processes
        .exit_process(owner, VmExitReason::Killed)
        .expect("owner should exit");
    let owner_exited = timers.cancel_owner_timers(owner);

    assert_eq!(
        owner_exited,
        vec![
            VmTimerEvent::OwnerExited {
                timer_id: first,
                owner,
                kind: VmTimerKind::OneShot,
            },
            VmTimerEvent::OwnerExited {
                timer_id: second,
                owner,
                kind: VmTimerKind::ReceiveTimeout,
            },
        ]
    );
    assert_eq!(timers.snapshots().len(), 1);
    assert_eq!(timers.snapshots()[0].id, skipped);
    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 20),
        vec![VmTimerEvent::DeadlineMissed {
            timer_id: skipped,
            owner: other,
            kind: VmTimerKind::OneShot,
            late_by_ticks: 10,
        }]
    );
}

#[test]
fn timer_table_distinguishes_manual_cancel_from_owner_exit_cleanup() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut timers = VmTimerTable::default();
    let manual = timers
        .start_one_shot(&processes, owner, 10)
        .expect("manual timer should start");
    let owner_cleanup = timers
        .start_one_shot(&processes, owner, 20)
        .expect("cleanup timer should start");

    assert_eq!(
        timers.cancel(manual).expect("manual cancel should succeed"),
        VmTimerEvent::Cancelled {
            timer_id: manual,
            owner,
            kind: VmTimerKind::OneShot,
        }
    );
    processes
        .exit_process(owner, VmExitReason::Killed)
        .expect("owner should exit");

    assert_eq!(
        timers.cancel_owner_timers(owner),
        vec![VmTimerEvent::OwnerExited {
            timer_id: owner_cleanup,
            owner,
            kind: VmTimerKind::OneShot,
        }]
    );
    assert!(timers.snapshots().is_empty());
}

#[test]
fn timer_table_fires_due_timers_only_once() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();
    let first = timers
        .start_one_shot(&processes, owner, 5)
        .expect("first timer should start");
    let second = timers
        .start_one_shot(&processes, owner, 7)
        .expect("second timer should start");

    assert!(timers
        .advance_clock(&mut processes, &mut scheduler, 4)
        .is_empty());
    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 5),
        vec![VmTimerEvent::Fired {
            timer_id: first,
            owner,
            kind: VmTimerKind::OneShot
        }]
    );
    assert_eq!(timers.snapshots()[0].id, second);
}

#[test]
fn timer_table_reports_deadline_missed_for_late_one_shot_timer() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();
    let timer = timers
        .start_one_shot(&processes, owner, 5)
        .expect("timer should start");

    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 8),
        vec![VmTimerEvent::DeadlineMissed {
            timer_id: timer,
            owner,
            kind: VmTimerKind::OneShot,
            late_by_ticks: 3,
        }]
    );
    assert!(timers.snapshots().is_empty());
}

#[test]
fn timer_table_reports_owner_exited_if_due_timer_owner_exited_before_fire() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();
    let timer = timers
        .start_one_shot(&processes, owner, 5)
        .expect("timer should start");
    processes
        .exit_process(owner, VmExitReason::Killed)
        .expect("owner should exit");

    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 5),
        vec![VmTimerEvent::OwnerExited {
            timer_id: timer,
            owner,
            kind: VmTimerKind::OneShot,
        }]
    );
    assert!(timers.snapshots().is_empty());
}

#[test]
fn timer_table_fires_equal_deadlines_in_timer_id_order() {
    let mut processes = VmProcessTable::default();
    let first_owner = processes.spawn_root(source("first"));
    let second_owner = processes.spawn_root(source("second"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();
    let first = timers
        .start_one_shot(&processes, first_owner, 10)
        .expect("first timer should start");
    let second = timers
        .start_one_shot(&processes, second_owner, 10)
        .expect("second timer should start");

    let events = timers.advance_clock(&mut processes, &mut scheduler, 10);

    assert_eq!(
        events,
        vec![
            VmTimerEvent::Fired {
                timer_id: first,
                owner: first_owner,
                kind: VmTimerKind::OneShot
            },
            VmTimerEvent::Fired {
                timer_id: second,
                owner: second_owner,
                kind: VmTimerKind::OneShot
            }
        ]
    );
    assert!(timers.snapshots().is_empty());
}

#[test]
fn timer_table_receive_timeout_wakes_blocked_process() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("receiver"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(3, 100));
    let mut timers = VmTimerTable::default();

    let timer = timers
        .start_receive_timeout(&mut processes, &mut scheduler, owner, 10, 5)
        .expect("receive timeout should start");
    assert_eq!(
        processes.get(owner).expect("owner should exist").state,
        VmProcessState::Blocked
    );

    let events = timers.advance_clock(&mut processes, &mut scheduler, 15);

    assert_eq!(
        events,
        vec![VmTimerEvent::Fired {
            timer_id: timer,
            owner,
            kind: VmTimerKind::ReceiveTimeout
        }]
    );
    assert_eq!(
        scheduler
            .run_next(&mut processes, |process, slice| {
                assert_eq!(slice.pid, owner);
                assert_eq!(slice.reduction_budget, 3);
                assert_eq!(process.state, VmProcessState::Runnable);
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("woken process should run")
            .outcome,
        VmSchedulerOutcome::Ran
    );
}

#[test]
fn receive_timeout_survives_unmatched_message_and_scheduler_preemption() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("receiver"));
    let sender = processes.spawn_root(source("sender"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(3, 100));
    let mut timers = VmTimerTable::default();

    let timer = timers
        .start_receive_timeout(&mut processes, &mut scheduler, owner, 10, 5)
        .expect("receive timeout should start");
    processes
        .send(
            sender,
            owner,
            ReplValue::Tuple(vec![
                ReplValue::Atom("wont".to_string()),
                ReplValue::Atom("match".to_string()),
            ]),
        )
        .expect("unmatched message should be delivered");
    scheduler
        .enqueue_runnable(&processes, owner)
        .expect("message delivery should make receiver runnable");

    let preempted = scheduler
        .run_next(&mut processes, |process, slice| {
            assert_eq!(slice.pid, owner);
            assert!(process
                .selective_receive(|message| {
                    matches!(
                        &message.payload,
                        ReplValue::Atom(value) if value == "expected"
                    )
                })
                .is_none());
            assert_eq!(process.mailbox_len(), 1);
            VmSchedulerDecision::Yield {
                reductions: slice.reduction_budget,
            }
        })
        .expect("receiver should yield at its reduction boundary");

    assert_eq!(preempted.outcome, VmSchedulerOutcome::Ran);
    assert_eq!(preempted.reductions_charged, 3);
    assert_eq!(scheduler.metrics().preemptions, 1);
    assert_eq!(timers.remaining_ticks(timer, 12), Ok(3));
    assert_eq!(timers.snapshots().len(), 1);

    assert_eq!(
        scheduler
            .run_next(&mut processes, |process, slice| {
                assert_eq!(slice.pid, owner);
                assert!(process
                    .selective_receive(|message| {
                        matches!(
                            &message.payload,
                            ReplValue::Atom(value) if value == "expected"
                        )
                    })
                    .is_none());
                VmSchedulerDecision::Block { reductions: 1 }
            })
            .expect("unmatched receive should block again")
            .outcome,
        VmSchedulerOutcome::Blocked
    );

    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 15),
        vec![VmTimerEvent::Fired {
            timer_id: timer,
            owner,
            kind: VmTimerKind::ReceiveTimeout,
        }]
    );
    assert_eq!(scheduler.queued_len(), 1);
    assert_eq!(
        scheduler
            .run_next(&mut processes, |process, slice| {
                assert_eq!(slice.pid, owner);
                assert_eq!(process.mailbox_len(), 1);
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("receive timeout should wake the receiver")
            .outcome,
        VmSchedulerOutcome::Ran
    );
}

#[test]
fn receive_timeout_zero_and_maximum_u32_deadlines_are_exact() {
    let mut processes = VmProcessTable::default();
    let zero_owner = processes.spawn_root(source("zero_timeout"));
    let large_owner = processes.spawn_root(source("large_timeout"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(3, 100));
    let mut timers = VmTimerTable::default();
    let now = 10;
    let large_timeout = u64::from(u32::MAX);

    let zero_timer = timers
        .start_receive_timeout(&mut processes, &mut scheduler, zero_owner, now, 0)
        .expect("zero receive timeout should start");
    let large_timer = timers
        .start_receive_timeout(
            &mut processes,
            &mut scheduler,
            large_owner,
            now,
            large_timeout,
        )
        .expect("maximum u32 receive timeout should start");

    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, now),
        vec![VmTimerEvent::Fired {
            timer_id: zero_timer,
            owner: zero_owner,
            kind: VmTimerKind::ReceiveTimeout,
        }]
    );
    assert_eq!(
        processes
            .get(large_owner)
            .expect("large timeout owner should exist")
            .state,
        VmProcessState::Blocked
    );
    let large_deadline = now + large_timeout;
    assert!(timers
        .advance_clock(&mut processes, &mut scheduler, large_deadline - 1)
        .is_empty());
    assert_eq!(
        timers.remaining_ticks(large_timer, large_deadline - 1),
        Ok(1)
    );
    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, large_deadline),
        vec![VmTimerEvent::Fired {
            timer_id: large_timer,
            owner: large_owner,
            kind: VmTimerKind::ReceiveTimeout,
        }]
    );
    assert_eq!(scheduler.queued_len(), 2);
}

#[test]
fn receive_timeout_wakes_large_equal_deadline_batch_once_in_identity_order() {
    const RECEIVER_COUNT: usize = 10_000;
    const DEADLINE: u64 = 5_000;

    let mut processes = VmProcessTable::default();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(3, RECEIVER_COUNT + 1));
    let mut timers = VmTimerTable::default();
    let mut receivers = Vec::with_capacity(RECEIVER_COUNT);

    for _ in 0..RECEIVER_COUNT {
        let owner = processes.spawn_root(source("blast_receiver"));
        let timer = timers
            .start_receive_timeout(&mut processes, &mut scheduler, owner, 0, DEADLINE)
            .expect("blast receive timeout should start");
        receivers.push((timer, owner));
    }

    assert!(timers
        .advance_clock(&mut processes, &mut scheduler, DEADLINE - 1)
        .is_empty());
    assert_eq!(scheduler.queued_len(), 0);
    assert_eq!(timers.snapshots().len(), RECEIVER_COUNT);

    let events = timers.advance_clock(&mut processes, &mut scheduler, DEADLINE);
    assert_eq!(events.len(), RECEIVER_COUNT);
    for (event, (expected_timer, expected_owner)) in events.iter().zip(&receivers) {
        assert_eq!(
            event,
            &VmTimerEvent::Fired {
                timer_id: *expected_timer,
                owner: *expected_owner,
                kind: VmTimerKind::ReceiveTimeout,
            }
        );
        assert_eq!(
            processes
                .get(*expected_owner)
                .expect("blast receiver should exist")
                .state,
            VmProcessState::Runnable
        );
    }
    assert_eq!(scheduler.queued_len(), RECEIVER_COUNT);
    assert!(timers.snapshots().is_empty());
    assert!(timers
        .advance_clock(&mut processes, &mut scheduler, DEADLINE)
        .is_empty());
}

#[test]
fn timer_wakeup_keeps_exactly_one_scheduler_entry() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("receiver"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(3, 100));
    let mut timers = VmTimerTable::default();

    timers
        .start_receive_timeout(&mut processes, &mut scheduler, owner, 0, 1)
        .expect("first receive timeout should start");
    timers.advance_clock(&mut processes, &mut scheduler, 1);
    assert_eq!(scheduler.queued_len(), 1);

    timers
        .start_receive_timeout(&mut processes, &mut scheduler, owner, 1, 1)
        .expect("second receive timeout should start");
    timers.advance_clock(&mut processes, &mut scheduler, 2);
    assert_eq!(scheduler.queued_len(), 1);

    assert_eq!(
        scheduler
            .run_next(&mut processes, |_, _| VmSchedulerDecision::Block {
                reductions: 1,
            })
            .expect("woken process should run once")
            .outcome,
        VmSchedulerOutcome::Blocked
    );
    assert_eq!(
        scheduler
            .run_next(&mut processes, |_, _| panic!(
                "duplicate wakeup must not run"
            ))
            .expect("empty scheduler should remain valid")
            .outcome,
        VmSchedulerOutcome::Idle
    );
}

#[test]
fn timer_wakeup_preserves_suspension_until_explicit_resume() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("suspended_receiver"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(3, 100));
    let mut timers = VmTimerTable::default();

    timers
        .start_receive_timeout(&mut processes, &mut scheduler, owner, 0, 1)
        .expect("receive timeout should start");
    scheduler
        .suspend_process(&mut processes, owner)
        .expect("blocked owner should suspend");

    timers.advance_clock(&mut processes, &mut scheduler, 1);

    assert_eq!(
        processes.get(owner).expect("owner should exist").state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    assert_eq!(scheduler.queued_len(), 0);

    scheduler
        .resume_process(&mut processes, owner)
        .expect("explicit resume should succeed");
    assert_eq!(scheduler.queued_len(), 1);
}

#[test]
fn timer_table_deadline_missed_receive_timeout_still_wakes_blocked_process() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("receiver"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(3, 100));
    let mut timers = VmTimerTable::default();

    let timer = timers
        .start_receive_timeout(&mut processes, &mut scheduler, owner, 10, 5)
        .expect("receive timeout should start");

    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 17),
        vec![VmTimerEvent::DeadlineMissed {
            timer_id: timer,
            owner,
            kind: VmTimerKind::ReceiveTimeout,
            late_by_ticks: 2,
        }]
    );
    assert_eq!(
        scheduler
            .run_next(&mut processes, |process, slice| {
                assert_eq!(slice.pid, owner);
                assert_eq!(process.state, VmProcessState::Runnable);
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("late timeout should still wake")
            .outcome,
        VmSchedulerOutcome::Ran
    );
}

#[test]
fn timer_table_rejects_exited_process_owner() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    processes
        .exit_process(owner, VmExitReason::Normal)
        .expect("owner should exit");
    let mut timers = VmTimerTable::default();

    let error = timers
        .start_one_shot(&processes, owner, 1)
        .expect_err("exited owner should fail");

    assert_eq!(error, "process 1 has exited");
}

#[test]
fn timer_table_rejects_receive_timeout_deadline_overflow_without_blocking_owner() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("receiver"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();

    let error = timers
        .start_receive_timeout(&mut processes, &mut scheduler, owner, u64::MAX, 1)
        .expect_err("overflowing deadline should fail");

    assert_eq!(error, "timer deadline overflow for process 1");
    assert_eq!(
        processes.get(owner).expect("owner should exist").state,
        VmProcessState::Runnable
    );
    assert!(timers.snapshots().is_empty());
}

#[test]
fn timer_table_rejects_missing_process_owner() {
    let mut processes = VmProcessTable::default();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let missing = VmProcessId::from_raw_for_test(99);
    let mut timers = VmTimerTable::default();

    assert_eq!(
        timers
            .start_one_shot(&processes, missing, 1)
            .expect_err("missing one-shot owner should fail"),
        "missing process 99"
    );
    assert_eq!(
        timers
            .start_receive_timeout(&mut processes, &mut scheduler, missing, 0, 1)
            .expect_err("missing receive-timeout owner should fail"),
        "missing process 99"
    );
}

#[test]
fn timer_table_cancellation_token_wins_or_loses_delivery_race_deterministically() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("cancel-race"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();

    let cancelled = timers
        .start_one_shot(&processes, owner, 5)
        .expect("cancelled timer");
    let cancelled_token = timers.cancellation_token(cancelled).expect("cancel token");
    assert_eq!(
        timers
            .cancel_with_token(cancelled_token)
            .expect("cancel before delivery"),
        VmTimerEvent::Cancelled {
            timer_id: cancelled,
            owner,
            kind: VmTimerKind::OneShot,
        }
    );
    assert!(timers
        .advance_clock(&mut processes, &mut scheduler, 5)
        .is_empty());

    let delivered = timers
        .start_one_shot(&processes, owner, 6)
        .expect("delivered timer");
    let delivered_token = timers
        .cancellation_token(delivered)
        .expect("delivery token");
    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 6),
        vec![VmTimerEvent::Fired {
            timer_id: delivered,
            owner,
            kind: VmTimerKind::OneShot,
        }]
    );
    assert_eq!(
        timers
            .cancel_with_token(delivered_token)
            .expect_err("delivery must invalidate cancellation token"),
        "missing timer 2"
    );
}
