use super::{
    VmScheduler, VmSchedulerConfig, VmSchedulerDecision, VmSchedulerOutcome, VmSchedulerSlice,
};
use crate::runtime::vm::process::{
    VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable,
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn scheduler_runs_runnable_process_and_requeues_yielded_slice() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("main"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(25, 4));
    scheduler
        .enqueue_runnable(&table, pid)
        .expect("runnable process should enqueue");

    let run = scheduler
        .run_next(&mut table, |process, slice| {
            assert_eq!(
                slice,
                VmSchedulerSlice {
                    pid,
                    tick: 1,
                    reduction_budget: 25
                }
            );
            process.heap_bytes = 128;
            VmSchedulerDecision::Yield { reductions: 7 }
        })
        .expect("slice should run");

    assert_eq!(run.pid, Some(pid));
    assert_eq!(run.tick, 1);
    assert_eq!(run.reductions_charged, 7);
    assert_eq!(run.outcome, VmSchedulerOutcome::Ran);
    assert_eq!(scheduler.queued_len(), 1);
    let process = table.get(pid).expect("process should remain");
    assert_eq!(process.heap_bytes, 128);
    assert_eq!(process.reductions, 7);
}

#[test]
fn scheduler_yield_does_not_requeue_process_made_non_runnable_by_slice() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("main"));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&table, pid)
        .expect("runnable process should enqueue");

    let run = scheduler
        .run_next(&mut table, |process, _slice| {
            process.block();
            VmSchedulerDecision::Yield { reductions: 1 }
        })
        .expect("slice should run");

    assert_eq!(run.outcome, VmSchedulerOutcome::Ran);
    assert_eq!(scheduler.queued_len(), 0);
    assert_eq!(
        table.get(pid).expect("process should exist").state,
        VmProcessState::Blocked
    );
}

#[test]
fn scheduler_does_not_duplicate_runnable_queue_entries() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("main"));
    let mut scheduler = VmScheduler::default();

    scheduler
        .enqueue_runnable(&table, pid)
        .expect("first enqueue should succeed");
    scheduler
        .enqueue_runnable(&table, pid)
        .expect("second enqueue should be idempotent");

    assert_eq!(scheduler.queued_len(), 1);
}

#[test]
fn scheduler_blocks_and_wakes_processes() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&table, pid)
        .expect("runnable process should enqueue");

    let blocked = scheduler
        .run_next(&mut table, |_process, _slice| VmSchedulerDecision::Block {
            reductions: 3,
        })
        .expect("block slice should run");

    assert_eq!(blocked.outcome, VmSchedulerOutcome::Blocked);
    assert_eq!(
        table.get(pid).expect("process should exist").state,
        VmProcessState::Blocked
    );
    assert_eq!(scheduler.queued_len(), 0);

    scheduler
        .wake_process(&mut table, pid)
        .expect("blocked process should wake");

    assert_eq!(
        table.get(pid).expect("process should exist").state,
        VmProcessState::Runnable
    );
    assert_eq!(scheduler.queued_len(), 1);
}

#[test]
fn scheduler_block_decision_tolerates_process_already_exited_by_slice() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&table, pid)
        .expect("runnable process should enqueue");

    let blocked = scheduler
        .run_next(&mut table, |process, _slice| {
            process.exit(VmExitReason::Killed);
            VmSchedulerDecision::Block { reductions: 2 }
        })
        .expect("block slice should run");

    assert_eq!(blocked.outcome, VmSchedulerOutcome::Blocked);
    assert_eq!(
        table.get(pid).expect("process should exist").state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
}

#[test]
fn scheduler_rejects_missing_and_exited_wake() {
    let mut table = VmProcessTable::default();
    let exited = table.spawn_root(source("exited"));
    table
        .exit_process(exited, VmExitReason::Normal)
        .expect("exit should succeed");
    let mut scheduler = VmScheduler::default();
    let missing = VmProcessId::from_raw_for_test(99);

    assert_eq!(
        scheduler
            .wake_process(&mut table, missing)
            .expect_err("missing wake should fail"),
        "cannot wake missing process 99"
    );
    assert_eq!(
        scheduler
            .wake_process(&mut table, exited)
            .expect_err("exited wake should fail"),
        "cannot wake exited process 1"
    );
}

#[test]
fn scheduler_exits_processes_and_returns_cleanup_handles() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    table
        .get_mut(pid)
        .expect("process should exist")
        .add_resource_handle("postgres:1");
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&table, pid)
        .expect("runnable process should enqueue");

    let exited = scheduler
        .run_next(&mut table, |_process, _slice| VmSchedulerDecision::Exit {
            reductions: 9,
            reason: VmExitReason::Error("boom".to_string()),
        })
        .expect("exit slice should run");

    assert_eq!(
        exited.outcome,
        VmSchedulerOutcome::Exited(vec!["postgres:1".to_string()])
    );
    let process = table.get(pid).expect("process should remain inspectable");
    assert_eq!(process.reductions, 9);
    assert_eq!(
        process.state,
        VmProcessState::Exited(VmExitReason::Error("boom".to_string()))
    );
}

#[test]
fn scheduler_rejects_missing_and_exited_cancellation() {
    let mut table = VmProcessTable::default();
    let exited = table.spawn_root(source("exited"));
    table
        .exit_process(exited, VmExitReason::Normal)
        .expect("exit should succeed");
    let mut scheduler = VmScheduler::default();
    let missing = VmProcessId::from_raw_for_test(99);

    assert_eq!(
        scheduler
            .request_cancellation(&mut table, missing)
            .expect_err("missing cancellation should fail"),
        "cannot cancel missing process 99"
    );
    assert_eq!(
        scheduler
            .request_cancellation(&mut table, exited)
            .expect_err("exited cancellation should fail"),
        "cannot cancel exited process 1"
    );
}

#[test]
fn scheduler_rejects_missing_blocked_and_exited_enqueue() {
    let mut table = VmProcessTable::default();
    let blocked = table.spawn_root(source("blocked"));
    let exited = table.spawn_root(source("exited"));
    table
        .get_mut(blocked)
        .expect("blocked should exist")
        .block();
    table
        .exit_process(exited, VmExitReason::Normal)
        .expect("exit should succeed");
    let mut scheduler = VmScheduler::default();
    let missing = VmProcessId::from_raw_for_test(99);

    assert_eq!(
        scheduler
            .enqueue_runnable(&table, missing)
            .expect_err("missing enqueue should fail"),
        "cannot enqueue missing process 99"
    );
    assert_eq!(
        scheduler
            .enqueue_runnable(&table, blocked)
            .expect_err("blocked enqueue should fail"),
        "cannot enqueue blocked process 1"
    );
    assert_eq!(
        scheduler
            .enqueue_runnable(&table, exited)
            .expect_err("exited enqueue should fail"),
        "cannot enqueue exited process 2"
    );
}

#[test]
fn scheduler_skips_stale_non_runnable_queue_entries() {
    let mut table = VmProcessTable::default();
    let blocked = table.spawn_root(source("blocked"));
    let runnable = table.spawn_root(source("runnable"));
    table
        .get_mut(blocked)
        .expect("blocked should exist")
        .block();
    let mut scheduler = VmScheduler::default();
    scheduler.enqueue_for_test(blocked);
    scheduler.enqueue_for_test(runnable);

    let run = scheduler
        .run_next(&mut table, |_process, _slice| VmSchedulerDecision::Yield {
            reductions: 1,
        })
        .expect("runnable process should run after stale blocked id");

    assert_eq!(run.pid, Some(runnable));
    assert_eq!(run.outcome, VmSchedulerOutcome::Ran);
}

#[test]
fn scheduler_cancels_process_before_running_slice() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    table
        .get_mut(pid)
        .expect("process should exist")
        .add_resource_handle("vector:1");
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&table, pid)
        .expect("runnable process should enqueue");
    scheduler
        .request_cancellation(&mut table, pid)
        .expect("cancellation should be recorded");

    let cancelled = scheduler
        .run_next(&mut table, |_process, _slice| {
            panic!("cancelled process must not run")
        })
        .expect("cancelled process should exit");

    assert_eq!(
        cancelled.outcome,
        VmSchedulerOutcome::Cancelled(vec!["vector:1".to_string()])
    );
    assert_eq!(
        table.get(pid).expect("process should remain").state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
}

#[test]
fn scheduler_reports_missing_stale_queue_entry() {
    let mut table = VmProcessTable::default();
    let mut scheduler = VmScheduler::default();
    scheduler.enqueue_for_test(VmProcessId::from_raw_for_test(99));

    let error = scheduler
        .run_next(&mut table, |_process, _slice| VmSchedulerDecision::Yield {
            reductions: 1,
        })
        .expect_err("missing queued process should fail");

    assert_eq!(error, "scheduled process 99 is missing");
}

#[test]
fn scheduler_reports_idle_after_stale_entries_exhaust_empty_poll_budget() {
    let mut table = VmProcessTable::default();
    let blocked = table.spawn_root(source("blocked"));
    table
        .get_mut(blocked)
        .expect("blocked should exist")
        .block();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 1));
    scheduler.enqueue_for_test(blocked);

    let idle = scheduler
        .run_next(&mut table, |_process, _slice| {
            panic!("blocked stale entry must not run")
        })
        .expect("stale queue exhaustion should be idle");

    assert_eq!(idle.outcome, VmSchedulerOutcome::Idle);
    assert_eq!(idle.tick, 0);
}

#[test]
fn scheduler_config_clamps_zero_values_and_reports_idle() {
    let mut table = VmProcessTable::default();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(0, 0));

    let idle = scheduler
        .run_next(&mut table, |_process, _slice| VmSchedulerDecision::Yield {
            reductions: 1,
        })
        .expect("empty scheduler should be idle");

    assert_eq!(idle.outcome, VmSchedulerOutcome::Idle);
    assert_eq!(idle.tick, 0);
}
