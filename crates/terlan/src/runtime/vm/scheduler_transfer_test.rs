use super::*;
use crate::runtime::vm::process::VmProcessSource;

fn transfer_processes() -> (VmProcessTable, VmProcessId) {
    let mut processes = VmProcessTable::default();
    let process = processes.spawn_root(VmProcessSource::new("app.Transfer", "run", 0));
    (processes, process)
}

#[test]
fn scheduler_placement_moves_class_queue_and_process_metrics() {
    let (mut processes, process) = transfer_processes();
    let mut source = VmScheduler::default();
    source
        .enqueue_runnable(&processes, process)
        .expect("queue process");
    source
        .set_process_class(&mut processes, process, VmSchedulerClass::Priority)
        .expect("set priority class");
    source
        .charge_memory_reductions(&mut processes, process, 2_048)
        .expect("record process metrics");

    let transfer = source.detach_process_placement(process);
    assert_eq!(transfer.process_id(), process);
    assert_eq!(transfer.class(), VmSchedulerClass::Priority);
    assert!(transfer.was_queued());
    assert!(source.diagnostic_queued_processes().is_empty());
    assert_eq!(source.memory_reductions(process), 0);

    let mut destination = VmScheduler::default();
    destination
        .import_process_placement(&processes, transfer)
        .expect("import placement");
    assert_eq!(destination.diagnostic_queued_processes(), vec![process]);
    assert!(destination.memory_reductions(process) > 0);
}

#[test]
fn scheduler_import_collision_returns_exact_placement_for_rollback() {
    let (processes, process) = transfer_processes();
    let mut source = VmScheduler::default();
    source
        .enqueue_runnable(&processes, process)
        .expect("queue source");
    let transfer = source.detach_process_placement(process);
    let mut destination = VmScheduler::default();
    destination
        .enqueue_runnable(&processes, process)
        .expect("queue collision");

    let failure = destination
        .import_process_placement(&processes, transfer)
        .expect_err("destination placement collision");
    assert!(failure.reason().contains("already has"));
    source
        .import_process_placement(&processes, failure.into_transfer())
        .expect("restore source placement");
    assert_eq!(source.diagnostic_queued_processes(), vec![process]);
}

#[test]
fn scheduler_transfer_is_send_and_rejects_queued_suspended_process() {
    fn assert_send<T: Send>() {}
    assert_send::<VmSchedulerPlacementTransfer>();

    let (mut processes, process) = transfer_processes();
    let mut source = VmScheduler::default();
    source
        .enqueue_runnable(&processes, process)
        .expect("queue source");
    let transfer = source.detach_process_placement(process);
    processes
        .with_process_control_mutator(process, VmProcess::suspend)
        .expect("suspend process")
        .expect("live process suspends");
    let destination = VmScheduler::default();
    assert!(destination
        .validate_process_placement_import(&processes, &transfer)
        .expect_err("queued suspended process must fail")
        .contains("not runnable"));
}
