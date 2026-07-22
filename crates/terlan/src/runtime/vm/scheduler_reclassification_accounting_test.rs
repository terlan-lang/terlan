use super::*;
use crate::runtime::vm::process::VmProcessSource;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.ReclassificationAccounting", function, 0)
}

fn process_reductions(processes: &VmProcessTable, pid: VmProcessId) -> u64 {
    processes.get(pid).expect("accounted process").reductions
}

#[test]
fn scheduler_charges_only_successful_explicit_reclassification() {
    let mut processes = VmProcessTable::default();
    let queued = processes.spawn_root(source("queued"));
    let blocked = processes.spawn_root(source("blocked"));
    let exited = processes.spawn_root(source("exited"));
    let missing = VmProcessId::from_raw_for_test(404);
    let mut scheduler = VmScheduler::default();

    scheduler
        .enqueue_runnable(&processes, queued)
        .expect("queue classified process");
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
        .set_process_class(&mut processes, queued, VmSchedulerClass::Priority)
        .expect("reclassify queued process");
    scheduler
        .set_process_class(&mut processes, queued, VmSchedulerClass::Priority)
        .expect("repeat explicit class request");
    scheduler
        .set_process_class(&mut processes, blocked, VmSchedulerClass::Background)
        .expect("reclassify blocked process");

    assert_eq!(process_reductions(&processes, queued), 2);
    assert_eq!(process_reductions(&processes, blocked), 1);
    assert_eq!(scheduler.metrics().total_reductions, 3);
    assert_eq!(scheduler.queued_len(), 1);
    assert_eq!(
        processes.get(blocked).expect("blocked process").state,
        VmProcessState::Blocked
    );

    let total_before_rejections = scheduler.metrics().total_reductions;
    assert_eq!(
        scheduler
            .set_process_class(&mut processes, missing, VmSchedulerClass::Priority)
            .expect_err("missing process must reject reclassification"),
        "cannot reclassify missing process 404"
    );
    assert_eq!(
        scheduler
            .set_process_class(&mut processes, exited, VmSchedulerClass::Priority)
            .expect_err("exited process must reject reclassification"),
        format!("cannot reclassify exited process {}", exited.as_u64())
    );
    assert_eq!(
        scheduler.metrics().total_reductions,
        total_before_rejections
    );
    assert_eq!(process_reductions(&processes, exited), 0);
}
