use super::super::failure::is_monitor_down_message;
use super::super::process::{VmExitReason, VmProcessId, VmProcessSource};
use super::super::ReplValue;
use super::{VmActorDemonitorOptions, VmActorReceive, VmActorRuntime};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.MonitorParity", name, 0)
}

fn receive_message(runtime: &mut VmActorRuntime, observer: VmProcessId) -> ReplValue {
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(observer)
        .expect("monitor observer should receive a message")
    else {
        panic!("monitor observer must not block")
    };
    message.payload
}

#[test]
fn monitor_suite_dead_target_and_fanout_cleanup_contract() {
    let mut runtime = VmActorRuntime::default();
    let observer = runtime.spawn_root(source("observer"));
    let dead_target = runtime.spawn_root(source("dead-target"));
    runtime
        .exit_actor(dead_target, VmExitReason::Killed)
        .expect("target should exit before monitoring");

    let dead_ref = runtime
        .monitor_actor(observer, dead_target)
        .expect("known dead target should complete a monitor");
    assert_eq!(dead_ref.as_u64(), 1);
    assert!(runtime
        .failure_snapshot(observer)
        .expect("observer relationship snapshot")
        .monitoring
        .is_empty());
    assert_eq!(
        receive_message(&mut runtime, observer),
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(dead_ref.as_u64() as i64),
            ReplValue::Int(dead_target.as_u64() as i64),
            ReplValue::Atom("noproc".to_string()),
        ])
    );
    let dead_demonitor = runtime
        .demonitor_actor(
            observer,
            dead_ref,
            VmActorDemonitorOptions::default().flush_down(),
        )
        .expect("completed monitor should demonitor idempotently");
    assert!(!dead_demonitor.removed);
    assert!(!dead_demonitor.flushed_down);

    let fanout_targets = (0..128)
        .map(|index| runtime.spawn_root(source(&format!("fanout-{index}"))))
        .collect::<Vec<_>>();
    let monitor_refs = fanout_targets
        .iter()
        .map(|target| {
            runtime
                .monitor_actor(observer, *target)
                .expect("fanout monitor should register")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        runtime
            .failure_snapshot(observer)
            .expect("fanout relationship snapshot")
            .monitoring
            .len(),
        fanout_targets.len()
    );

    for target in &fanout_targets {
        runtime
            .exit_actor(*target, VmExitReason::Normal)
            .expect("monitored target should exit");
    }
    assert!(runtime
        .failure_snapshot(observer)
        .expect("cleaned fanout snapshot")
        .monitoring
        .is_empty());
    for monitor_ref in &monitor_refs {
        let payload = receive_message(&mut runtime, observer);
        assert!(is_monitor_down_message(&payload, monitor_ref));
        let ReplValue::Tuple(fields) = payload else {
            panic!("DOWN payload must remain a tuple")
        };
        assert_eq!(fields[3], ReplValue::Atom("normal".to_string()));
    }

    let shared_target = runtime.spawn_root(source("shared-target"));
    let observers = (0..128)
        .map(|index| runtime.spawn_root(source(&format!("observer-{index}"))))
        .collect::<Vec<_>>();
    let shared_refs = observers
        .iter()
        .map(|watcher| {
            runtime
                .monitor_actor(*watcher, shared_target)
                .expect("shared target monitor should register")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        runtime
            .failure_snapshot(shared_target)
            .expect("shared target relationship snapshot")
            .monitored_by
            .len(),
        observers.len()
    );
    runtime
        .exit_actor(
            shared_target,
            VmExitReason::Error("fanout-failure".to_string()),
        )
        .expect("shared target should exit");
    for (watcher, monitor_ref) in observers.into_iter().zip(shared_refs) {
        let payload = receive_message(&mut runtime, watcher);
        assert!(is_monitor_down_message(&payload, &monitor_ref));
        assert!(runtime
            .failure_snapshot(watcher)
            .expect("watcher cleanup snapshot")
            .monitoring
            .is_empty());
    }
}
