use super::super::{VmActorReceive, VmActorRuntime, VmExitReason, VmProcessSource};
use crate::runtime::vm::failure::is_monitor_down_message;
use crate::runtime::vm::process_environment::VmRuntimeEnvironmentProfile;
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("parity.SystemInformation", name, 0)
}

fn profile(process_limit: usize, scheduler_count: usize) -> VmRuntimeEnvironmentProfile {
    VmRuntimeEnvironmentProfile::new(process_limit, scheduler_count).expect("system info profile")
}

#[test]
fn system_info_suite_identity_capacity_and_process_churn_contract() {
    let mut runtime = VmActorRuntime::default();
    let processes = (0..1_024)
        .map(|index| runtime.spawn_root(source(&format!("worker-{index}"))))
        .collect::<Vec<_>>();

    let populated = runtime
        .system_information_snapshot(profile(2_048, 4))
        .expect("populated system information");
    assert_eq!(populated.schema, "terlan-vm-system-information-v1");
    assert_eq!(populated.runtime_name, "terlan-vm");
    assert_eq!(populated.runtime_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(populated.target_architecture, std::env::consts::ARCH);
    assert_eq!(populated.process_limit, 2_048);
    assert_eq!(populated.scheduler_count, 4);
    assert_eq!(populated.word_size_bytes, std::mem::size_of::<usize>());
    assert_eq!(populated.process_count, 1_024);
    assert_eq!(populated.exited_process_count, 0);
    assert_eq!(populated.run_queue_length, 1_024);
    assert_eq!(runtime.live_process_ids().len(), populated.process_count);

    for pid in processes.iter().step_by(2) {
        runtime
            .exit_actor(*pid, VmExitReason::Normal)
            .expect("exit alternating worker");
    }
    let reduced = runtime
        .system_information_snapshot(profile(2_048, 4))
        .expect("reduced system information");
    assert_eq!(reduced.process_count, 512);
    assert_eq!(reduced.exited_process_count, 512);
    assert_eq!(reduced.run_queue_length, 512);
    assert_eq!(runtime.live_process_ids().len(), reduced.process_count);
    assert_eq!(reduced.mailbox_message_count, 0);
    assert_eq!(reduced.logical_heap_bytes, 0);
    assert_eq!(reduced.resource_handle_count, 0);
    assert_eq!(reduced.active_timer_count, 0);
    assert_eq!(
        runtime
            .system_information_snapshot(profile(511, 4))
            .expect_err("live count must enforce configured capacity"),
        "VM live process count 512 exceeds configured limit 511"
    );
}

#[test]
fn system_info_suite_inspection_with_pending_monitor_signals_contract() {
    let mut runtime = VmActorRuntime::default();
    let target = runtime.spawn_root(source("target"));
    let observers = (0..10)
        .map(|index| runtime.spawn_root(source(&format!("observer-{index}"))))
        .collect::<Vec<_>>();
    let monitor_refs = observers
        .iter()
        .map(|observer| {
            runtime
                .monitor_actor(*observer, target)
                .expect("monitor target")
        })
        .collect::<Vec<_>>();

    runtime
        .exit_actor(target, VmExitReason::Normal)
        .expect("complete monitored target");
    let pending = runtime
        .system_information_snapshot(profile(32, 1))
        .expect("inspect pending monitor signals");
    assert_eq!(pending.process_count, observers.len());
    assert_eq!(pending.exited_process_count, 1);
    assert_eq!(pending.run_queue_length, observers.len());
    assert_eq!(pending.mailbox_message_count, observers.len());
    assert_eq!(
        pending,
        runtime
            .system_information_snapshot(profile(32, 1))
            .expect("repeated inspection must not consume signals")
    );

    for (observer, monitor_ref) in observers.into_iter().zip(monitor_refs) {
        let VmActorReceive::Message(message) = runtime
            .receive_next_or_block(observer)
            .expect("receive monitor completion")
        else {
            panic!("observer must retain its pending monitor completion")
        };
        assert!(is_monitor_down_message(&message.payload, &monitor_ref));
        let ReplValue::Tuple(fields) = message.payload else {
            panic!("monitor completion must remain structured")
        };
        assert_eq!(fields[3], ReplValue::Atom("normal".to_string()));
    }
    let drained = runtime
        .system_information_snapshot(profile(32, 1))
        .expect("drained system information");
    assert_eq!(drained.process_count, 10);
    assert_eq!(drained.mailbox_message_count, 0);
    assert_eq!(drained.run_queue_length, 10);
}
