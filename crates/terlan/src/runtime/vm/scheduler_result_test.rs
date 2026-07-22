use super::{
    VmScheduler, VmSchedulerConfig, VmSchedulerDecision, VmSchedulerOutcome, VmSchedulerRun,
};
use crate::runtime::vm::process::{
    VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable,
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

fn scheduled(name: &str) -> (VmProcessTable, VmScheduler, VmProcessId) {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source(name));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 4));
    scheduler
        .enqueue_runnable(&processes, pid)
        .expect("fresh process must schedule");
    (processes, scheduler, pid)
}

#[test]
fn scheduler_result_contract_reports_idle_without_fabricating_process_work() {
    let mut processes = VmProcessTable::default();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 4));

    let run = scheduler
        .run_next(&mut processes, |_, _| {
            panic!("idle poll must not run a slice")
        })
        .expect("idle poll");

    assert_eq!(
        run,
        VmSchedulerRun {
            pid: None,
            tick: 0,
            reductions_charged: 0,
            outcome: VmSchedulerOutcome::Idle,
        }
    );
    assert_eq!(scheduler.metrics().total_slices, 0);
    assert_eq!(scheduler.metrics().total_reductions, 0);
}

#[test]
fn scheduler_result_contract_preserves_exact_slice_counts_and_saturates_totals() {
    let (mut processes, mut scheduler, pid) = scheduled("saturating");

    let maximum = scheduler
        .run_next(&mut processes, |_, _| VmSchedulerDecision::Yield {
            reductions: u64::MAX,
        })
        .expect("maximum reduction slice");
    assert_eq!(
        maximum,
        VmSchedulerRun {
            pid: Some(pid),
            tick: 1,
            reductions_charged: u64::MAX,
            outcome: VmSchedulerOutcome::Ran,
        }
    );

    let saturated = scheduler
        .run_next(&mut processes, |_, _| VmSchedulerDecision::Block {
            reductions: 1,
        })
        .expect("saturating block slice");
    assert_eq!(
        saturated,
        VmSchedulerRun {
            pid: Some(pid),
            tick: 2,
            reductions_charged: 1,
            outcome: VmSchedulerOutcome::Blocked,
        }
    );
    assert_eq!(processes.get(pid).expect("process").reductions, u64::MAX);
    assert_eq!(scheduler.metrics().total_reductions, u64::MAX);
    assert_eq!(scheduler.metrics().total_slices, 2);
}

#[test]
fn scheduler_result_contract_returns_exit_reason_and_ordered_cleanup() {
    let (mut processes, mut scheduler, pid) = scheduled("exiting");
    let process = processes.get_mut(pid).expect("process");
    process.add_resource_handle("database:1");
    process.add_resource_handle("stream:2");

    let run = scheduler
        .run_next(&mut processes, |_, _| VmSchedulerDecision::Exit {
            reductions: 9,
            reason: VmExitReason::Error("boom".to_string()),
        })
        .expect("exit slice");

    assert_eq!(
        run,
        VmSchedulerRun {
            pid: Some(pid),
            tick: 1,
            reductions_charged: 9,
            outcome: VmSchedulerOutcome::Exited(vec![
                "database:1".to_string(),
                "stream:2".to_string(),
            ]),
        }
    );
    assert_eq!(
        processes.get(pid).expect("exited process").state,
        VmProcessState::Exited(VmExitReason::Error("boom".to_string()))
    );
}

#[test]
fn scheduler_result_contract_distinguishes_pre_slice_and_boundary_cancellation() {
    let (mut processes, mut scheduler, pid) = scheduled("pre-cancelled");
    processes
        .get_mut(pid)
        .expect("process")
        .add_resource_handle("pre:1");
    scheduler
        .request_cancellation(&mut processes, pid)
        .expect("request cancellation");

    let pre_slice = scheduler
        .run_next(&mut processes, |_, _| {
            panic!("pre-slice cancellation must not execute user work")
        })
        .expect("pre-slice cancellation");
    assert_eq!(
        pre_slice,
        VmSchedulerRun {
            pid: Some(pid),
            tick: 1,
            reductions_charged: 0,
            outcome: VmSchedulerOutcome::Cancelled(vec!["pre:1".to_string()]),
        }
    );

    let (mut processes, mut scheduler, pid) = scheduled("boundary-cancelled");
    processes
        .get_mut(pid)
        .expect("process")
        .add_resource_handle("boundary:1");
    let boundary = scheduler
        .run_next(&mut processes, |process, _| {
            process.request_cancellation();
            VmSchedulerDecision::Yield { reductions: 4 }
        })
        .expect("boundary cancellation");
    assert_eq!(
        boundary,
        VmSchedulerRun {
            pid: Some(pid),
            tick: 1,
            reductions_charged: 4,
            outcome: VmSchedulerOutcome::Cancelled(vec!["boundary:1".to_string()]),
        }
    );

    let (mut processes, mut scheduler, pid) = scheduled("exit-wins");
    let exit = scheduler
        .run_next(&mut processes, |process, _| {
            process.request_cancellation();
            VmSchedulerDecision::Exit {
                reductions: 2,
                reason: VmExitReason::Normal,
            }
        })
        .expect("explicit exit");
    assert_eq!(
        exit,
        VmSchedulerRun {
            pid: Some(pid),
            tick: 1,
            reductions_charged: 2,
            outcome: VmSchedulerOutcome::Exited(Vec::new()),
        }
    );
}
