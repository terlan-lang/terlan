use super::{VmTimerEvent, VmTimerKind, VmTimerTable};
use crate::runtime::vm::process::{
    VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable,
};
use crate::runtime::vm::scheduler::{
    VmScheduler, VmSchedulerConfig, VmSchedulerDecision, VmSchedulerOutcome,
};

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
