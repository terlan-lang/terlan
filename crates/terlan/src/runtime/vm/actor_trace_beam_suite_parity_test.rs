use super::{VmActorReceive, VmActorRuntime, VmExitReason, VmProcessSource};
use crate::runtime::vm::failure::is_monitor_down_message;
use crate::runtime::vm::process::VmProcessState;
use crate::runtime::vm::process_environment::VmRuntimeEnvironmentProfile;
use crate::runtime::vm::system_profile::VmSystemProfileActivity;
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("parity.Trace", name, 0)
}

fn receive_payload(runtime: &mut VmActorRuntime, pid: super::VmProcessId) -> ReplValue {
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(pid)
        .expect("traced actor receive")
    else {
        panic!("traced actor must have a queued message")
    };
    message.payload
}

#[test]
fn trace_suite_message_relationship_and_exit_correlation_contract() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let receiver = runtime.spawn_root(source("receiver"));
    runtime
        .register_name("trace.receiver", receiver)
        .expect("register trace receiver");
    let alias = runtime.create_alias(receiver).expect("create trace alias");
    let monitor_ref = runtime
        .monitor_actor(sender, receiver)
        .expect("monitor traced receiver");
    let cursor = runtime.system_profile_cursor();

    runtime
        .send(sender, receiver, ReplValue::Int(1))
        .expect("ordinary send");
    runtime
        .send_priority(sender, receiver, ReplValue::Int(0))
        .expect("priority send");
    runtime
        .send_alias(sender, alias, ReplValue::Int(2))
        .expect("alias send");
    runtime
        .send(sender, sender, ReplValue::Atom("self".to_string()))
        .expect("self send");

    assert_eq!(receive_payload(&mut runtime, receiver), ReplValue::Int(0));
    assert_eq!(receive_payload(&mut runtime, receiver), ReplValue::Int(1));
    assert_eq!(receive_payload(&mut runtime, receiver), ReplValue::Int(2));
    assert_eq!(
        receive_payload(&mut runtime, sender),
        ReplValue::Atom("self".to_string())
    );
    assert_eq!(
        runtime
            .receive_with_timeout(receiver, 0)
            .expect("immediate traced timeout"),
        VmActorReceive::Timeout
    );

    runtime
        .send(receiver, sender, ReplValue::Int(7))
        .expect("message before exit");
    runtime
        .exit_actor(receiver, VmExitReason::Error("trace-exit".to_string()))
        .expect("exit traced receiver");
    assert_eq!(receive_payload(&mut runtime, sender), ReplValue::Int(7));
    let down = receive_payload(&mut runtime, sender);
    assert!(is_monitor_down_message(&down, &monitor_ref));
    assert_eq!(
        runtime
            .send(sender, receiver, ReplValue::Int(8))
            .expect_err("post-exit send must fail"),
        format!("recipient process {} has exited", receiver.as_u64())
    );

    let receiver_snapshot = runtime
        .processes()
        .snapshot(receiver)
        .expect("postmortem trace location");
    assert_eq!(
        receiver_snapshot.state,
        VmProcessState::Exited(VmExitReason::Error("trace-exit".to_string()))
    );
    assert_eq!(
        receiver_snapshot.current_location.source.module,
        "parity.Trace"
    );
    assert!(runtime
        .failure_snapshot(sender)
        .expect("cleaned monitor state")
        .monitoring
        .is_empty());

    let profile = runtime
        .system_profile_since(cursor)
        .expect("correlated scheduler trace");
    assert!(profile.events.iter().any(|event| {
        event.pid == receiver.as_u64()
            && event.transition == "exit"
            && event.activity == VmSystemProfileActivity::Inactive
            && event.location.source.function == "receiver"
    }));
    assert_eq!(
        profile,
        runtime
            .system_profile_since(cursor)
            .expect("trace replay is immutable")
    );
    assert!(runtime
        .system_profile_since(profile.next_cursor)
        .expect("delivered trace cursor")
        .events
        .is_empty());
}

#[test]
fn trace_suite_suspend_exit_race_and_mailbox_pressure_contract() {
    let mut runtime = VmActorRuntime::default();
    let cursor = runtime.system_profile_cursor();
    for index in 0..100 {
        let worker = runtime.spawn_root(source(&format!("suspended-{index}")));
        runtime.suspend(worker).expect("suspend traced worker");
        assert!(matches!(
            runtime
                .processes()
                .snapshot(worker)
                .expect("suspended snapshot")
                .state,
            VmProcessState::Suspended(_)
        ));
        runtime
            .exit_actor(worker, VmExitReason::Killed)
            .expect("exit suspended worker");
        assert_eq!(
            runtime
                .resume(worker)
                .expect_err("exited worker cannot resume"),
            format!("cannot resume exited process {}", worker.as_u64())
        );
    }
    assert_eq!(runtime.scheduled_len(), 0);

    let sender = runtime.spawn_root(source("pressure-sender"));
    let receiver = runtime.spawn_root(source("pressure-receiver"));
    for value in 0..512 {
        runtime
            .send(sender, receiver, ReplValue::Int(value))
            .expect("pressure send");
    }
    let environment = runtime
        .environment_snapshot(
            VmRuntimeEnvironmentProfile::new(1_024, 1).expect("trace environment profile"),
        )
        .expect("mailbox pressure snapshot");
    assert_eq!(environment.live_processes, 2);
    assert_eq!(environment.exited_processes, 100);
    assert_eq!(environment.mailbox_messages, 512);
    for value in 0..512 {
        assert_eq!(
            receive_payload(&mut runtime, receiver),
            ReplValue::Int(value)
        );
    }
    assert_eq!(
        runtime
            .environment_snapshot(
                VmRuntimeEnvironmentProfile::new(1_024, 1)
                    .expect("drained trace environment profile"),
            )
            .expect("drained mailbox snapshot")
            .mailbox_messages,
        0
    );

    let profile = runtime
        .system_profile_since(cursor)
        .expect("suspension trace profile");
    let suspended_events = profile
        .events
        .iter()
        .filter(|event| event.transition == "suspend")
        .collect::<Vec<_>>();
    assert_eq!(suspended_events.len(), 100);
    assert!(suspended_events.iter().all(|event| {
        event.activity == VmSystemProfileActivity::Inactive
            && event.location.source.function.starts_with("suspended-")
    }));
    assert!(profile
        .events
        .windows(2)
        .all(|events| events[0].sequence + 1 == events[1].sequence));
}
