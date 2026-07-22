use super::*;
use crate::runtime::vm::process::VmProcessSource;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.TerminalAccounting", function, 0)
}

#[test]
fn scheduler_charges_terminal_reductions_only_after_process_exit() {
    let mut processes = VmProcessTable::default();
    let live = processes.spawn_root(source("live"));
    let exited = processes.spawn_root(source("exited"));
    let missing = VmProcessId::from_raw_for_test(404);
    let mut scheduler = VmScheduler::default();

    assert_eq!(
        scheduler
            .charge_terminal_reductions(&mut processes, live, 1)
            .expect_err("live process must reject terminal charge"),
        format!(
            "cannot charge terminal reductions for live process {}",
            live.as_u64()
        )
    );
    assert_eq!(scheduler.metrics().total_reductions, 0);

    processes
        .exit_process(exited, VmExitReason::Normal)
        .expect("exit process");
    assert_eq!(
        scheduler
            .charge_terminal_reductions(&mut processes, exited, 3)
            .expect("charge completed terminal work"),
        3
    );
    assert_eq!(processes.get(exited).expect("exited process").reductions, 3);
    assert_eq!(scheduler.metrics().total_reductions, 3);
    assert_eq!(
        scheduler.metrics().processes[&exited.as_u64()].reductions,
        3
    );

    assert_eq!(
        scheduler
            .charge_terminal_reductions(&mut processes, missing, 1)
            .expect_err("missing process must reject terminal charge"),
        "cannot charge terminal reductions for missing process 404"
    );
    assert_eq!(scheduler.metrics().total_reductions, 3);
}
