use super::super::{VmActorReceive, VmActorRuntime, VmExitReason, VmProcessSource};
use crate::runtime::vm::local_trace::{VmLocalTraceConfig, VmLocalTraceEventKind};
use crate::runtime::vm::meta_trace::{VmMetaTraceConfig, VmMetaTraceEventKind, VmMetaTraceState};
use crate::runtime::vm::scheduler::VmSchedulerDecision;
use crate::runtime::vm::system_profile::VmSystemProfileActivity;
use crate::runtime::vm::ReplValue;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("parity.TracePort", function, 0)
}

fn large_payload() -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Atom("trace-payload".to_string()),
        ReplValue::List((0..4_096).map(ReplValue::Int).collect()),
    ])
}

fn receive_payload(runtime: &mut VmActorRuntime, pid: super::super::VmProcessId) -> ReplValue {
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(pid)
        .expect("receive traced payload")
    else {
        panic!("traced actor must have a queued payload")
    };
    message.payload
}

#[test]
fn trace_port_suite_typed_actor_and_scheduler_diagnostic_transport_contract() {
    let mut runtime = VmActorRuntime::default();
    let subject = runtime.spawn_root(source("subject"));
    let observer = runtime.spawn_root(source("observer"));
    let receiver = runtime.spawn_root(source("receiver"));
    let traced = source("traced");
    assert!(runtime.enable_local_trace(traced.clone(), VmLocalTraceConfig::calls_and_returns(),));
    runtime
        .enable_meta_trace(
            traced.clone(),
            observer,
            VmMetaTraceConfig::calls_and_returns(),
        )
        .expect("enable typed trace observer");
    let local_cursor = runtime.local_trace_cursor();
    let meta_cursor = runtime.meta_trace_cursor();
    let profile_cursor = runtime.system_profile_cursor();

    assert!(runtime
        .record_local_call(subject, traced.clone(), 17)
        .expect("record local call"));
    let meta_call = runtime
        .record_meta_call(subject, traced.clone(), 17)
        .expect("record observed call")
        .expect("return-enabled observation token");
    assert!(runtime
        .record_local_return(subject, traced.clone())
        .expect("record local return"));
    assert!(runtime
        .record_meta_return(meta_call, subject)
        .expect("record observed return"));

    let local = runtime
        .local_trace_since(local_cursor)
        .expect("capture typed local diagnostics");
    assert_eq!(local.events.len(), 2);
    assert!(matches!(
        &local.events[0].kind,
        VmLocalTraceEventKind::Call { location }
            if location.source == traced && location.instruction_offset == 17
    ));
    assert!(matches!(
        &local.events[1].kind,
        VmLocalTraceEventKind::Return { source: returned, caller }
            if returned == &traced && caller.source == source("subject")
    ));
    let meta = runtime
        .meta_trace_since(meta_cursor, observer)
        .expect("capture observer-scoped diagnostics");
    assert_eq!(meta.events.len(), 2);
    assert!(matches!(
        meta.events[0].kind,
        VmMetaTraceEventKind::Call { .. }
    ));
    assert!(matches!(
        meta.events[1].kind,
        VmMetaTraceEventKind::Return { .. }
    ));

    let payload = large_payload();
    runtime
        .send(subject, receiver, payload.clone())
        .expect("send large typed trace payload");
    assert!(
        runtime
            .memory_metrics(receiver)
            .expect("receiver logical memory")
            .current_bytes
            > 0
    );
    assert_eq!(receive_payload(&mut runtime, receiver), payload);
    assert_eq!(
        runtime
            .memory_metrics(receiver)
            .expect("drained receiver logical memory")
            .current_bytes,
        0
    );

    assert!(runtime
        .link_actors(subject, receiver)
        .expect("link traced actors"));
    assert_eq!(
        runtime
            .failure_snapshot(subject)
            .expect("linked process diagnostics")
            .links,
        vec![receiver]
    );
    assert!(runtime
        .unlink_actors(subject, receiver)
        .expect("unlink traced actors"));
    assert!(runtime
        .failure_snapshot(subject)
        .expect("unlinked process diagnostics")
        .links
        .is_empty());

    runtime
        .run_next(|_, _| VmSchedulerDecision::Yield { reductions: 3 })
        .expect("run one traced scheduler slice");
    let profile = runtime
        .system_profile_since(profile_cursor)
        .expect("capture scheduler diagnostic stream");
    assert!(profile.events.iter().any(|event| {
        event.pid == subject.as_u64()
            && event.activity == VmSystemProfileActivity::Inactive
            && event.transition == "dequeue"
    }));
    assert!(profile
        .events
        .windows(2)
        .all(|events| events[0].sequence + 1 == events[1].sequence));
    assert_eq!(
        profile,
        runtime
            .system_profile_since(profile_cursor)
            .expect("scheduler diagnostic replay is immutable")
    );
    assert!(runtime
        .system_profile_since(profile.next_cursor)
        .expect("delivered scheduler cursor")
        .events
        .is_empty());
}

#[test]
fn trace_port_suite_observer_failure_cleanup_and_tracee_isolation_contract() {
    let mut runtime = VmActorRuntime::default();
    let subject = runtime.spawn_root(source("subject"));
    let observer = runtime.spawn_root(source("observer"));
    let traced = source("traced");
    runtime.enable_local_trace(traced.clone(), VmLocalTraceConfig::calls_and_returns());
    runtime
        .enable_meta_trace(
            traced.clone(),
            observer,
            VmMetaTraceConfig::calls_and_returns(),
        )
        .expect("enable failing observer");
    let local_cursor = runtime.local_trace_cursor();
    let meta_cursor = runtime.meta_trace_cursor();

    runtime
        .record_local_call(subject, traced.clone(), 0)
        .expect("record in-flight local call");
    let in_flight = runtime
        .record_meta_call(subject, traced.clone(), 0)
        .expect("record in-flight observed call")
        .expect("return-enabled observation token");
    runtime
        .exit_actor(
            observer,
            VmExitReason::Error("diagnostic sink closed".to_string()),
        )
        .expect("close diagnostic observer");
    assert_eq!(
        runtime.meta_trace_state(&traced),
        VmMetaTraceState::Disabled
    );
    assert!(!runtime
        .record_meta_return(in_flight, subject)
        .expect("dead observer drops in-flight return"));
    runtime
        .record_local_return(subject, traced.clone())
        .expect("local return survives observer failure");
    assert!(runtime.is_alive(subject));

    for offset in 0..256 {
        runtime
            .record_local_call(subject, traced.clone(), offset)
            .expect("record local call after observer failure");
        runtime
            .record_local_return(subject, traced.clone())
            .expect("record local return after observer failure");
        assert!(runtime
            .record_meta_call(subject, traced.clone(), offset)
            .expect("disabled observer remains a no-op")
            .is_none());
    }
    let retained = runtime
        .meta_trace_since(meta_cursor, observer)
        .expect("retain events delivered before observer failure");
    assert_eq!(retained.events.len(), 1);
    assert!(matches!(
        retained.events[0].kind,
        VmMetaTraceEventKind::Call { .. }
    ));
    let local = runtime
        .local_trace_since(local_cursor)
        .expect("local diagnostics continue without observer");
    assert_eq!(local.events.len(), 514);
    assert!(local
        .events
        .windows(2)
        .all(|events| events[0].sequence + 1 == events[1].sequence));

    runtime
        .send(
            subject,
            subject,
            ReplValue::Atom("still-running".to_string()),
        )
        .expect("tracee continues after observer failure");
    assert_eq!(
        receive_payload(&mut runtime, subject),
        ReplValue::Atom("still-running".to_string())
    );
}
