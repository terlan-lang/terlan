use super::{VmRuntimeEnvironmentProfile, VmRuntimeEnvironmentSnapshot};
use crate::runtime::vm::actor::VmActorRuntime;
use crate::runtime::vm::memory::{VmMemoryAccountant, VmMemoryLimits};
use crate::runtime::vm::process::{VmExitReason, VmProcessSource, VmProcessTable};
use crate::runtime::vm::scheduler::VmScheduler;
use crate::runtime::vm::timer::VmTimerTable;
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Environment", name, 0)
}

fn profile(process_limit: usize, scheduler_count: usize) -> VmRuntimeEnvironmentProfile {
    VmRuntimeEnvironmentProfile::new(process_limit, scheduler_count).expect("valid profile")
}

#[test]
fn runtime_environment_profile_rejects_zero_capacities() {
    assert_eq!(
        VmRuntimeEnvironmentProfile::new(0, 1).expect_err("zero process limit must fail"),
        "VM process limit must be non-zero"
    );
    assert_eq!(
        VmRuntimeEnvironmentProfile::new(1, 0).expect_err("zero schedulers must fail"),
        "VM scheduler count must be non-zero"
    );
}

#[test]
fn runtime_environment_snapshot_reports_empty_runtime() {
    let snapshot = VmRuntimeEnvironmentSnapshot::capture(
        profile(64, 1),
        &VmProcessTable::default(),
        &VmScheduler::default(),
        &VmTimerTable::default(),
    )
    .expect("empty snapshot");

    assert_eq!(snapshot.process_limit, 64);
    assert_eq!(snapshot.scheduler_count, 1);
    assert_eq!(snapshot.word_size_bytes, std::mem::size_of::<usize>());
    assert_eq!(snapshot.total_processes, 0);
    assert_eq!(snapshot.live_processes, 0);
    assert_eq!(snapshot.exited_processes, 0);
    assert_eq!(snapshot.run_queue, 0);
    assert_eq!(snapshot.mailbox_messages, 0);
    assert_eq!(snapshot.logical_heap_bytes, 0);
    assert_eq!(snapshot.resource_handles, 0);
    assert_eq!(snapshot.active_timers, 0);
    assert_eq!(snapshot.timers_started, 0);
    assert_eq!(snapshot.timers_fired, 0);
    assert_eq!(snapshot.timers_cancelled, 0);
    assert_eq!(snapshot.total_reductions, 0);
    assert_eq!(snapshot.memory_reductions, 0);
    assert_eq!(snapshot.scheduler_slices, 0);
    assert_eq!(snapshot.scheduler_preemptions, 0);
}

#[test]
fn runtime_environment_snapshot_composes_owned_subsystem_metrics() {
    let mut processes = VmProcessTable::default();
    let worker = processes.spawn_root(source("worker"));
    let receiver = processes.spawn_root(source("receiver"));
    let exited = processes.spawn_root(source("exited"));
    processes
        .send(worker, receiver, ReplValue::Int(7))
        .expect("message delivery");
    processes
        .get_mut(worker)
        .expect("worker")
        .add_resource_handle("resource:database");
    processes
        .exit_process(exited, VmExitReason::Normal)
        .expect("process exit");

    let mut memory =
        VmMemoryAccountant::new(VmMemoryLimits::new(4_096, 8_192).expect("memory limits"));
    memory
        .account_heap(&mut processes, worker, 2_048)
        .expect("heap accounting");

    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&processes, worker)
        .expect("worker enqueue");
    scheduler
        .enqueue_runnable(&processes, receiver)
        .expect("receiver enqueue");
    scheduler
        .charge_memory_reductions(&mut processes, worker, 2_048)
        .expect("memory reductions");

    let mut timers = VmTimerTable::default();
    timers
        .start_one_shot(&processes, worker, 10)
        .expect("one-shot timer");
    timers
        .start_interval(&processes, receiver, 20, 5)
        .expect("interval timer");

    let snapshot =
        VmRuntimeEnvironmentSnapshot::capture(profile(8, 2), &processes, &scheduler, &timers)
            .expect("populated snapshot");

    assert_eq!(snapshot.total_processes, 3);
    assert_eq!(snapshot.live_processes, 2);
    assert_eq!(snapshot.exited_processes, 1);
    assert_eq!(snapshot.run_queue, 2);
    assert_eq!(snapshot.mailbox_messages, 1);
    assert_eq!(snapshot.logical_heap_bytes, 2_048);
    assert_eq!(snapshot.resource_handles, 1);
    assert_eq!(snapshot.active_timers, 2);
    assert_eq!(snapshot.timers_started, 2);
    assert_eq!(snapshot.total_reductions, 3);
    assert_eq!(snapshot.memory_reductions, 3);
}

#[test]
fn runtime_environment_snapshot_is_stable_and_enforces_live_process_limit() {
    let mut processes = VmProcessTable::default();
    processes.spawn_root(source("first"));
    processes.spawn_root(source("second"));
    let scheduler = VmScheduler::default();
    let timers = VmTimerTable::default();
    let before = processes.metrics();

    assert_eq!(
        VmRuntimeEnvironmentSnapshot::capture(profile(1, 1), &processes, &scheduler, &timers,)
            .expect_err("capacity violation must fail"),
        "VM live process count 2 exceeds configured limit 1"
    );
    let first =
        VmRuntimeEnvironmentSnapshot::capture(profile(2, 1), &processes, &scheduler, &timers)
            .expect("first snapshot");
    let second =
        VmRuntimeEnvironmentSnapshot::capture(profile(2, 1), &processes, &scheduler, &timers)
            .expect("second snapshot");

    assert_eq!(first, second);
    assert_eq!(processes.metrics(), before);
}

#[test]
fn actor_runtime_exposes_its_owned_environment_snapshot() {
    let mut runtime = VmActorRuntime::default();
    runtime.spawn_root(source("root"));

    let snapshot = runtime
        .environment_snapshot(profile(4, 1))
        .expect("actor environment snapshot");

    assert_eq!(snapshot.total_processes, 1);
    assert_eq!(snapshot.live_processes, 1);
    assert_eq!(snapshot.run_queue, 1);
}
