use super::{
    VmScheduler, VmSchedulerClass, VmSchedulerConfig, VmSchedulerDecision, VmSchedulerOutcome,
    VmSchedulerSlice,
};
use crate::runtime::vm::process::{
    VmExitReason, VmProcess, VmProcessId, VmProcessResumeState, VmProcessSource, VmProcessState,
    VmProcessTable,
};
use std::path::PathBuf;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

fn benign_scheduler_decision(
    _process: &mut VmProcess,
    _slice: VmSchedulerSlice,
) -> VmSchedulerDecision {
    VmSchedulerDecision::Yield { reductions: 1 }
}

#[test]
fn benign_scheduler_decision_yields_one_reduction() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let slice = VmSchedulerSlice {
        pid,
        tick: 1,
        reduction_budget: 10,
    };
    let process = table.get_mut(pid).expect("process should exist");

    assert_eq!(
        benign_scheduler_decision(process, slice),
        VmSchedulerDecision::Yield { reductions: 1 }
    );
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
    assert_eq!(scheduler.metrics().preemptions, 0);
    let process = table.get(pid).expect("process should remain");
    assert_eq!(process.heap_bytes, 128);
    assert_eq!(process.reductions, 7);
}

#[test]
fn scheduler_voluntary_yield_preserves_state_and_resumes_after_peer() {
    let mut table = VmProcessTable::default();
    let yielding = table.spawn_root(source("yielding"));
    let peer = table.spawn_root(source("peer"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 4));
    scheduler
        .enqueue_runnable(&table, yielding)
        .expect("yielding process should enqueue");
    scheduler
        .enqueue_runnable(&table, peer)
        .expect("peer process should enqueue");

    let yielded = scheduler
        .run_next(&mut table, |process, slice| {
            assert_eq!(slice.pid, yielding);
            process.heap_bytes = 73;
            VmSchedulerDecision::Yield { reductions: 1 }
        })
        .expect("voluntary yield should return control");
    assert_eq!(yielded.pid, Some(yielding));
    assert_eq!(yielded.outcome, VmSchedulerOutcome::Ran);
    assert_eq!(scheduler.queued_len(), 2);

    let peer_run = scheduler
        .run_next(&mut table, |_process, slice| {
            assert_eq!(slice.pid, peer);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("peer should run after voluntary yield");
    assert_eq!(peer_run.pid, Some(peer));

    let resumed = scheduler
        .run_next(&mut table, |process, slice| {
            assert_eq!(slice.pid, yielding);
            assert_eq!(process.heap_bytes, 73);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("yielding process should resume with retained state");
    assert_eq!(resumed.pid, Some(yielding));
    assert_eq!(resumed.outcome, VmSchedulerOutcome::Blocked);
    assert_eq!(scheduler.queued_len(), 0);
    assert_eq!(table.get(yielding).expect("yielding process").reductions, 2);
}

#[test]
fn scheduler_repeated_voluntary_yields_keep_one_queue_entry_without_preemption_charge() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("repeated-yield"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 4));
    scheduler
        .enqueue_runnable(&table, pid)
        .expect("process should enqueue");

    for _ in 0..64 {
        let run = scheduler
            .run_next(&mut table, |_process, _slice| VmSchedulerDecision::Yield {
                reductions: 0,
            })
            .expect("voluntary yield should remain runnable");
        assert_eq!(run.pid, Some(pid));
        assert_eq!(run.outcome, VmSchedulerOutcome::Ran);
        assert_eq!(scheduler.queued_len(), 1);
    }

    assert_eq!(scheduler.metrics().total_slices, 64);
    assert_eq!(scheduler.metrics().total_reductions, 0);
    assert_eq!(scheduler.metrics().preemptions, 0);
    assert_eq!(table.get(pid).expect("process").reductions, 0);
}

#[test]
fn scheduler_duplicate_enqueue_keeps_single_queue_entry() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 4));

    scheduler
        .enqueue_runnable(&table, pid)
        .expect("first runnable enqueue should succeed");
    scheduler
        .enqueue_runnable(&table, pid)
        .expect("duplicate runnable enqueue should be idempotent");

    assert_eq!(scheduler.queued_len(), 1);
    let run = scheduler
        .run_next(&mut table, benign_scheduler_decision)
        .expect("queued process should run once");
    assert_eq!(run.pid, Some(pid));
    assert_eq!(run.outcome, VmSchedulerOutcome::Ran);
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
fn scheduler_suspension_is_idempotent_and_rejects_suspended_enqueue() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("suspended"));
    let mut scheduler = VmScheduler::default();

    scheduler
        .suspend_process(&mut table, pid)
        .expect("unqueued process should suspend");
    assert_eq!(
        scheduler
            .enqueue_runnable(&table, pid)
            .expect_err("suspended process must not enqueue"),
        "cannot enqueue suspended process 1"
    );

    scheduler
        .suspend_process(&mut table, pid)
        .expect("repeated suspension should be idempotent");

    assert_eq!(
        table.get(pid).expect("process should exist").state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    assert_eq!(scheduler.queued_len(), 0);
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
fn scheduler_skips_stale_exited_queue_entries() {
    let mut table = VmProcessTable::default();
    let exited = table.spawn_root(source("exited"));
    let runnable = table.spawn_root(source("runnable"));
    table
        .exit_process(exited, VmExitReason::Normal)
        .expect("process should exit");
    let mut scheduler = VmScheduler::default();
    scheduler.enqueue_for_test(exited);
    scheduler.enqueue_for_test(runnable);

    let run = scheduler
        .run_next(&mut table, |_process, _slice| VmSchedulerDecision::Yield {
            reductions: 1,
        })
        .expect("runnable process should run after stale exited id");

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
        .run_next(&mut table, benign_scheduler_decision)
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
fn scheduler_reports_missing_and_exited_wake_or_cancel_requests() {
    let mut table = VmProcessTable::default();
    let exited = table.spawn_root(source("exited"));
    table
        .exit_process(exited, VmExitReason::Normal)
        .expect("process exit should succeed");
    let missing = VmProcessId::from_raw_for_test(99);
    let mut scheduler = VmScheduler::default();

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
fn scheduler_does_not_requeue_process_that_exits_during_yield_slice() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&table, pid)
        .expect("runnable process should enqueue");

    let run = scheduler
        .run_next(&mut table, |process, _slice| {
            process.exit(VmExitReason::Normal);
            VmSchedulerDecision::Yield { reductions: 4 }
        })
        .expect("slice should run");

    assert_eq!(run.outcome, VmSchedulerOutcome::Ran);
    assert_eq!(scheduler.queued_len(), 0);
    assert_eq!(
        table.get(pid).expect("process should remain").state,
        VmProcessState::Exited(VmExitReason::Normal)
    );
}

#[test]
fn scheduler_returns_idle_after_stale_queue_poll_budget_is_exhausted() {
    let mut table = VmProcessTable::default();
    let blocked = table.spawn_root(source("blocked"));
    table
        .get_mut(blocked)
        .expect("blocked process should exist")
        .block();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 1));
    scheduler.enqueue_for_test(blocked);

    let run = scheduler
        .run_next(&mut table, benign_scheduler_decision)
        .expect("stale blocked queue should idle");

    assert_eq!(run.outcome, VmSchedulerOutcome::Idle);
    assert_eq!(run.pid, None);
}

#[test]
fn scheduler_reports_missing_stale_queue_entry() {
    let mut table = VmProcessTable::default();
    let mut scheduler = VmScheduler::default();
    scheduler.enqueue_for_test(VmProcessId::from_raw_for_test(99));

    let error = scheduler
        .run_next(&mut table, benign_scheduler_decision)
        .expect_err("missing queued process should fail");

    assert_eq!(error, "scheduled process 99 is missing");
}

#[test]
fn scheduler_config_clamps_zero_values_and_reports_idle() {
    let mut table = VmProcessTable::default();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(0, 0));

    let idle = scheduler
        .run_next(&mut table, benign_scheduler_decision)
        .expect("empty scheduler should be idle");

    assert_eq!(idle.outcome, VmSchedulerOutcome::Idle);
    assert_eq!(idle.tick, 0);
}

fn run_adversarial_fairness_scenario() -> (VmScheduler, Vec<u64>) {
    let mut table = VmProcessTable::default();
    let pids = [
        table.spawn_root(source("cpu-a")),
        table.spawn_root(source("cpu-b")),
        table.spawn_root(source("cpu-c")),
    ];
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(4, 16));
    for pid in pids {
        scheduler
            .enqueue_runnable(&table, pid)
            .expect("CPU-bound process should enqueue");
    }
    let mut trace = Vec::new();
    for _ in 0..9 {
        let run = scheduler
            .run_next(&mut table, |_process, slice| {
                trace.push(slice.pid.as_u64());
                VmSchedulerDecision::Yield {
                    reductions: slice.reduction_budget,
                }
            })
            .expect("CPU-bound process should yield at its preemption point");
        assert_eq!(run.outcome, VmSchedulerOutcome::Ran);
    }
    (scheduler, trace)
}

#[test]
fn scheduler_fairness_telemetry_is_deterministic_under_cpu_bound_load() {
    let (first, first_trace) = run_adversarial_fairness_scenario();
    let (second, second_trace) = run_adversarial_fairness_scenario();

    assert_eq!(first_trace, vec![1, 2, 3, 1, 2, 3, 1, 2, 3]);
    assert_eq!(first_trace, second_trace);
    assert_eq!(first.metrics(), second.metrics());
    assert_eq!(first.metrics().total_reductions, 36);
    assert_eq!(first.metrics().total_slices, 9);
    assert_eq!(first.metrics().preemptions, 9);
    assert_eq!(first.metrics().max_queue_depth, 3);
    assert_eq!(first.metrics().processes.len(), 3);
    for metrics in first.metrics().processes.values() {
        assert_eq!(metrics.reductions, 12);
        assert_eq!(metrics.slices, 3);
        assert_eq!(metrics.preemptions, 3);
        assert_eq!(metrics.max_wait_ticks, 3);
    }
}

#[test]
fn scheduler_writes_fairness_report_with_starvation_evidence() {
    let (scheduler, _trace) = run_adversarial_fairness_scenario();
    let report_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/quality/vm-scheduler-fairness-report.json");

    scheduler
        .write_fairness_report(&report_path, 2, Some("scheduler-adversarial-cpu"))
        .expect("write scheduler fairness report");
    let report = std::fs::read_to_string(report_path).expect("read scheduler fairness report");
    let report: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    assert_eq!(report["schema"], "terlan-vm-scheduler-fairness-report-v1");
    assert_eq!(report["correlationId"], "scheduler-adversarial-cpu");
    assert_eq!(report["schedulerTick"], 9);
    assert_eq!(report["totalReductions"], 36);
    assert_eq!(report["totalMemoryReductions"], 0);
    assert_eq!(report["totalSlices"], 9);
    assert_eq!(report["preemptionCount"], 9);
    assert_eq!(report["maxQueueDepth"], 3);
    assert_eq!(report["processMetrics"].as_array().unwrap().len(), 3);
    assert!(report["processMetrics"]
        .as_array()
        .unwrap()
        .iter()
        .all(|metrics| metrics["memoryReductions"] == 0));
    assert_eq!(report["starvationWarnings"].as_array().unwrap().len(), 3);
    assert_eq!(report["queueTransitions"].as_array().unwrap().len(), 21);
}

#[test]
fn scheduler_weighted_classes_preserve_order_and_bound_background_wait() {
    let mut table = VmProcessTable::default();
    let priority = table.spawn_root(source("priority"));
    let normal = table.spawn_root(source("normal"));
    let background = table.spawn_root(source("background"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(1, 16));
    scheduler
        .enqueue_runnable_with_class(&table, priority, VmSchedulerClass::Priority)
        .expect("priority process");
    scheduler
        .enqueue_runnable_with_class(&table, normal, VmSchedulerClass::Normal)
        .expect("normal process");
    scheduler
        .enqueue_runnable_with_class(&table, background, VmSchedulerClass::Background)
        .expect("background process");

    let mut trace = Vec::new();
    for _ in 0..12 {
        scheduler
            .run_next(&mut table, |_process, slice| {
                trace.push(slice.pid.as_u64());
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("classified process should run");
    }

    assert_eq!(trace, vec![1, 1, 2, 1, 2, 3, 1, 1, 2, 1, 2, 3]);
    assert_eq!(trace.iter().filter(|pid| **pid == 1).count(), 6);
    assert_eq!(trace.iter().filter(|pid| **pid == 2).count(), 4);
    assert_eq!(trace.iter().filter(|pid| **pid == 3).count(), 2);
    assert!(
        scheduler.metrics().processes[&3].max_wait_ticks <= 6,
        "background work exceeded one weighted scheduling cycle"
    );
}

#[test]
fn scheduler_rejects_silent_reclassification_of_queued_process() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("classified"));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable_with_class(&table, pid, VmSchedulerClass::Background)
        .expect("background process");
    scheduler
        .enqueue_runnable_with_class(&table, pid, VmSchedulerClass::Background)
        .expect("same-class duplicate should be idempotent");

    assert_eq!(scheduler.queued_len(), 1);
    assert_eq!(
        scheduler
            .enqueue_runnable_with_class(&table, pid, VmSchedulerClass::Priority)
            .expect_err("queued process must not change class implicitly"),
        "cannot reclassify queued process 1"
    );
    assert_eq!(scheduler.queued_len(), 1);
}

#[test]
fn scheduler_explicit_reclassification_moves_one_queued_entry() {
    let mut table = VmProcessTable::default();
    let normal = table.spawn_root(source("normal"));
    let promoted = table.spawn_root(source("promoted"));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&table, normal)
        .expect("normal process");
    scheduler
        .enqueue_runnable(&table, promoted)
        .expect("process before promotion");

    scheduler
        .set_process_class(&mut table, promoted, VmSchedulerClass::Priority)
        .expect("queued process should be reclassified explicitly");
    scheduler
        .set_process_class(&mut table, promoted, VmSchedulerClass::Priority)
        .expect("same-class reclassification should be idempotent");

    assert_eq!(scheduler.queued_len(), 2);
    let first = scheduler
        .run_next(&mut table, |_process, _slice| VmSchedulerDecision::Block {
            reductions: 1,
        })
        .expect("promoted process should run first");
    let second = scheduler
        .run_next(&mut table, |_process, _slice| VmSchedulerDecision::Block {
            reductions: 1,
        })
        .expect("normal process should run second");
    assert_eq!(first.pid, Some(promoted));
    assert_eq!(second.pid, Some(normal));
    assert_eq!(scheduler.queued_len(), 0);
}

#[test]
fn scheduler_reclassifies_blocked_process_without_waking_it() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("blocked-priority"));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&table, pid)
        .expect("runnable process");
    scheduler
        .run_next(&mut table, |_process, _slice| VmSchedulerDecision::Block {
            reductions: 1,
        })
        .expect("process should block");

    scheduler
        .set_process_class(&mut table, pid, VmSchedulerClass::Background)
        .expect("blocked process should retain a new class");

    assert_eq!(scheduler.queued_len(), 0);
    assert_eq!(
        table.get(pid).expect("process should exist").state,
        VmProcessState::Blocked
    );
    scheduler
        .wake_process(&mut table, pid)
        .expect("blocked process should wake explicitly");
    assert_eq!(scheduler.queued_len(), 1);
    let transition = scheduler
        .metrics()
        .queue_transitions
        .last()
        .expect("wake should record an enqueue transition");
    assert_eq!(transition.action, "enqueue");
    assert_eq!(transition.class, VmSchedulerClass::Background);
}

#[test]
fn scheduler_reclassification_rejects_missing_and_exited_processes() {
    let mut table = VmProcessTable::default();
    let exited = table.spawn_root(source("exited-priority"));
    table
        .exit_process(exited, VmExitReason::Normal)
        .expect("process should exit");
    let missing = VmProcessId::from_raw_for_test(99);
    let mut scheduler = VmScheduler::default();

    assert_eq!(
        scheduler
            .set_process_class(&mut table, missing, VmSchedulerClass::Priority)
            .expect_err("missing process must be rejected"),
        "cannot reclassify missing process 99"
    );
    assert_eq!(
        scheduler
            .set_process_class(&mut table, exited, VmSchedulerClass::Priority)
            .expect_err("exited process must be rejected"),
        "cannot reclassify exited process 1"
    );
    assert_eq!(scheduler.queued_len(), 0);
}

#[test]
fn scheduler_cancels_at_preemption_boundary_without_requeueing() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("cancel-at-boundary"));
    table
        .get_mut(pid)
        .expect("process")
        .add_resource_handle("stream:1");
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(5, 16));
    scheduler
        .enqueue_runnable_with_class(&table, pid, VmSchedulerClass::Priority)
        .expect("priority process");
    let mut executions = 0;

    let cancelled = scheduler
        .run_next(&mut table, |process, slice| {
            executions += 1;
            process.request_cancellation();
            VmSchedulerDecision::Yield {
                reductions: slice.reduction_budget,
            }
        })
        .expect("cancellation should complete at boundary");

    assert_eq!(cancelled.tick, 1);
    assert_eq!(cancelled.reductions_charged, 5);
    assert_eq!(
        cancelled.outcome,
        VmSchedulerOutcome::Cancelled(vec!["stream:1".to_string()])
    );
    assert_eq!(executions, 1);
    assert_eq!(scheduler.queued_len(), 0);
    assert_eq!(scheduler.metrics().preemptions, 1);
    assert_eq!(scheduler.metrics().total_reductions, 5);
    let process = table
        .get(pid)
        .expect("cancelled process remains inspectable");
    assert_eq!(process.reductions, 5);
    assert_eq!(process.state, VmProcessState::Exited(VmExitReason::Killed));

    let idle = scheduler
        .run_next(&mut table, |_process, _slice| {
            executions += 1;
            VmSchedulerDecision::Yield { reductions: 1 }
        })
        .expect("cancelled process must not run again");
    assert_eq!(idle.outcome, VmSchedulerOutcome::Idle);
    assert_eq!(executions, 1);
}

#[test]
fn scheduler_attributes_external_memory_reductions_without_slices() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("memory"));
    let missing = VmProcessId::from_raw_for_test(99);
    let mut scheduler = VmScheduler::default();

    assert_eq!(
        scheduler
            .charge_memory_reductions(&mut table, missing, 1)
            .expect_err("missing process cannot be charged"),
        "cannot charge memory for missing process 99"
    );

    assert_eq!(
        scheduler
            .charge_memory_reductions(&mut table, pid, 0)
            .expect("base memory charge"),
        1
    );
    assert_eq!(
        scheduler
            .charge_memory_reductions(&mut table, pid, 1025)
            .expect("two memory units"),
        3
    );
    assert_eq!(scheduler.total_memory_reductions(), 4);
    assert_eq!(scheduler.memory_reductions(pid), 4);
    assert_eq!(scheduler.metrics().total_reductions, 4);
    assert_eq!(scheduler.metrics().total_slices, 0);
    assert_eq!(table.get(pid).expect("process").reductions, 4);

    table
        .exit_process(pid, VmExitReason::Normal)
        .expect("exit process");
    assert_eq!(
        scheduler
            .charge_memory_reductions(&mut table, pid, 1)
            .expect_err("exited process cannot be charged"),
        "cannot charge memory for exited process 1"
    );
    assert_eq!(scheduler.total_memory_reductions(), 4);
}
