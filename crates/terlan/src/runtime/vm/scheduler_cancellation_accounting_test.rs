use super::*;
use crate::runtime::vm::process::VmProcessSource;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.CancellationAccounting", function, 0)
}

fn process_reductions(processes: &VmProcessTable, pid: VmProcessId) -> u64 {
    processes.get(pid).expect("accounted process").reductions
}

#[test]
fn scheduler_charges_only_successful_cancellation_requests() {
    let mut processes = VmProcessTable::default();
    let queued = processes.spawn_root(source("queued"));
    let blocked = processes.spawn_root(source("blocked"));
    let exited = processes.spawn_root(source("exited"));
    let missing = VmProcessId::from_raw_for_test(404);
    let mut scheduler = VmScheduler::default();

    scheduler
        .enqueue_runnable(&processes, queued)
        .expect("queue cancellable process");
    scheduler
        .enqueue_runnable(&processes, blocked)
        .expect("queue process before blocking");
    scheduler
        .run_next(&mut processes, |_process, _slice| {
            VmSchedulerDecision::Yield { reductions: 0 }
        })
        .expect("first queued process yields");
    scheduler
        .run_next(&mut processes, |_process, _slice| {
            VmSchedulerDecision::Block { reductions: 0 }
        })
        .expect("second process blocks");
    processes
        .exit_process(exited, VmExitReason::Normal)
        .expect("exit rejected process");

    scheduler
        .request_cancellation(&mut processes, queued)
        .expect("cancel queued process");
    scheduler
        .request_cancellation(&mut processes, queued)
        .expect("repeat explicit cancellation request");
    scheduler
        .request_cancellation(&mut processes, blocked)
        .expect("cancel blocked process");

    assert_eq!(process_reductions(&processes, queued), 2);
    assert_eq!(process_reductions(&processes, blocked), 1);
    assert_eq!(scheduler.metrics().total_reductions, 3);
    assert!(
        processes
            .get(queued)
            .expect("queued process")
            .cancellation_requested
    );
    assert!(
        processes
            .get(blocked)
            .expect("blocked process")
            .cancellation_requested
    );

    let total_before_rejections = scheduler.metrics().total_reductions;
    assert_eq!(
        scheduler
            .request_cancellation(&mut processes, missing)
            .expect_err("missing process must reject cancellation"),
        "cannot cancel missing process 404"
    );
    assert_eq!(
        scheduler
            .request_cancellation(&mut processes, exited)
            .expect_err("exited process must reject cancellation"),
        format!("cannot cancel exited process {}", exited.as_u64())
    );
    assert_eq!(
        scheduler.metrics().total_reductions,
        total_before_rejections
    );
    assert_eq!(process_reductions(&processes, exited), 0);

    let cancelled = scheduler
        .run_next(&mut processes, |_process, _slice| {
            panic!("pre-slice cancellation must not execute process work")
        })
        .expect("queued process cancels at scheduler boundary");
    assert_eq!(cancelled.pid, Some(queued));
    assert_eq!(cancelled.reductions_charged, 0);
    assert_eq!(cancelled.outcome, VmSchedulerOutcome::Cancelled(Vec::new()));
    assert_eq!(process_reductions(&processes, queued), 2);
    assert_eq!(scheduler.metrics().total_reductions, 3);
}
