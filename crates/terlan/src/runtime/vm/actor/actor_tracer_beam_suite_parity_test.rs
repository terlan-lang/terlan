use super::super::{
    VmActorReceive, VmActorRuntime, VmActorSpawnOptions, VmExitReason, VmProcessSource,
};
use crate::runtime::vm::failure::is_monitor_down_message;
use crate::runtime::vm::local_trace::{VmLocalTraceConfig, VmLocalTraceEventKind};
use crate::runtime::vm::meta_trace::{VmMetaTraceConfig, VmMetaTraceEventKind, VmMetaTraceState};
use crate::runtime::vm::scheduler::VmSchedulerDecision;
use crate::runtime::vm::system_profile::{VmSystemProfileActivity, VmSystemProfileCursor};
use crate::runtime::vm::ReplValue;

fn source(function: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new("parity.Tracer", function, arity)
}

fn receive_payload(runtime: &mut VmActorRuntime, pid: super::super::VmProcessId) -> ReplValue {
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(pid)
        .expect("receive tracer payload")
    else {
        panic!("tracer actor must have a queued payload")
    };
    message.payload
}

#[test]
fn tracer_suite_callback_filter_discard_and_reload_contract() {
    let mut runtime = VmActorRuntime::default();
    let subject = runtime.spawn_root(source("subject", 0));
    let traced = source("traced", 1);
    let wrong_arity = source("traced", 2);

    for generation in 0..15 {
        let observer = runtime.spawn_root(source("observer", generation));
        assert!(
            runtime.enable_local_trace(traced.clone(), VmLocalTraceConfig::calls_and_returns(),)
        );
        assert!(runtime
            .enable_meta_trace(
                traced.clone(),
                observer,
                VmMetaTraceConfig::calls_and_returns(),
            )
            .expect("load typed tracer generation"));
        let local_cursor = runtime.local_trace_cursor();
        let meta_cursor = runtime.meta_trace_cursor();

        assert!(runtime
            .record_local_call(subject, traced.clone(), generation)
            .expect("publish enabled local call"));
        let meta_call = runtime
            .record_meta_call(subject, traced.clone(), generation)
            .expect("publish enabled observed call")
            .expect("return-enabled tracer token");
        assert!(runtime
            .record_local_return(subject, traced.clone())
            .expect("publish enabled local return"));
        assert!(runtime
            .record_meta_return(meta_call, subject)
            .expect("publish enabled observed return"));

        let local = runtime
            .local_trace_since(local_cursor)
            .expect("inspect local tracer generation");
        assert_eq!(local.events.len(), 2);
        assert!(matches!(
            &local.events[0].kind,
            VmLocalTraceEventKind::Call { location }
                if location.source == traced && location.instruction_offset == generation
        ));
        assert!(matches!(
            local.events[1].kind,
            VmLocalTraceEventKind::Return { .. }
        ));
        assert_eq!(local.events[0].sequence + 1, local.events[1].sequence);

        let meta = runtime
            .meta_trace_since(meta_cursor, observer)
            .expect("inspect observer tracer generation");
        assert_eq!(meta.events.len(), 2);
        assert!(matches!(
            meta.events[0].kind,
            VmMetaTraceEventKind::Call { .. }
        ));
        assert!(matches!(
            meta.events[1].kind,
            VmMetaTraceEventKind::Return { .. }
        ));
        assert_eq!(meta.events[0].sequence + 1, meta.events[1].sequence);

        assert!(runtime.disable_local_trace(&traced));
        assert!(runtime.disable_meta_trace(&traced));
        let discarded_local = runtime.local_trace_cursor();
        let discarded_meta = runtime.meta_trace_cursor();
        assert!(!runtime
            .record_local_call(subject, traced.clone(), generation + 100)
            .expect("disabled local callback discards"));
        assert!(runtime
            .record_meta_call(subject, traced.clone(), generation + 100)
            .expect("disabled observed callback discards")
            .is_none());
        assert!(runtime
            .local_trace_since(discarded_local)
            .expect("discarded local suffix")
            .events
            .is_empty());
        assert!(runtime
            .meta_trace_since(discarded_meta, observer)
            .expect("discarded observer suffix")
            .events
            .is_empty());
        runtime
            .exit_actor(observer, VmExitReason::Normal)
            .expect("unload typed tracer generation");
    }

    runtime.enable_local_trace(traced.clone(), VmLocalTraceConfig::calls_only());
    let exact_cursor = runtime.local_trace_cursor();
    assert!(!runtime
        .record_local_call(subject, wrong_arity, 0)
        .expect("wrong arity is not traced"));
    assert!(runtime
        .record_local_call(subject, traced.clone(), 0)
        .expect("exact arity remains traced"));
    assert!(!runtime
        .record_local_return(subject, traced)
        .expect("calls-only callback discards returns"));
    assert_eq!(
        runtime
            .local_trace_since(exact_cursor)
            .expect("exact callback filter")
            .events
            .len(),
        1
    );
}

#[test]
fn tracer_suite_message_process_and_scheduler_event_contract() {
    let mut runtime = VmActorRuntime::default();
    let parent = runtime.spawn_root(source("parent", 0));
    let cursor = runtime.system_profile_cursor();
    let child_spawn = runtime
        .spawn_child_with_options(
            parent,
            source("child", 0),
            VmActorSpawnOptions::default().linked().monitored(),
        )
        .expect("spawn linked monitored traced child");
    let child = child_spawn.pid;
    let monitor_ref = child_spawn.monitor_ref.expect("child monitor reference");
    runtime
        .register_name("tracer.child", child)
        .expect("register traced child");

    assert_eq!(
        runtime
            .failure_snapshot(parent)
            .expect("parent relationship event state")
            .links,
        vec![child]
    );
    assert_eq!(
        runtime
            .failure_snapshot(child)
            .expect("child relationship event state")
            .links,
        vec![parent]
    );
    assert_eq!(
        runtime
            .processes()
            .snapshot(child)
            .expect("spawn event state")
            .parent,
        Some(parent)
    );

    let payload = ReplValue::Record {
        name: "TracerPayload".to_string(),
        fields: vec![
            ("tag".to_string(), ReplValue::Atom("send".to_string())),
            (
                "values".to_string(),
                ReplValue::List((0..2_048).map(ReplValue::Int).collect()),
            ),
        ],
    };
    runtime
        .send(parent, child, payload.clone())
        .expect("send typed tracer payload");
    assert!(
        runtime
            .memory_metrics(child)
            .expect("queued payload memory")
            .current_bytes
            > 0
    );
    assert_eq!(receive_payload(&mut runtime, child), payload);
    assert_eq!(
        runtime
            .memory_metrics(child)
            .expect("received payload memory")
            .current_bytes,
        0
    );

    runtime
        .run_next(|_, _| VmSchedulerDecision::Yield { reductions: 2 })
        .expect("publish scheduler out and in activity");
    assert!(runtime
        .unlink_actors(parent, child)
        .expect("publish symmetric unlink state"));
    assert!(runtime
        .failure_snapshot(parent)
        .expect("parent unlinked state")
        .links
        .is_empty());
    assert!(runtime
        .failure_snapshot(child)
        .expect("child unlinked state")
        .links
        .is_empty());
    runtime
        .unregister_name("tracer.child")
        .expect("unregister traced child");
    runtime
        .exit_actor(child, VmExitReason::Error("traced-exit".to_string()))
        .expect("publish traced child exit");
    assert!(is_monitor_down_message(
        &receive_payload(&mut runtime, parent),
        &monitor_ref,
    ));
    assert!(runtime.is_alive(parent));
    assert!(runtime.registered_names().is_empty());

    let profile = runtime
        .system_profile_since(cursor)
        .expect("capture immutable scheduler trace");
    assert!(profile.events.iter().any(|event| {
        event.transition == "dequeue" && event.activity == VmSystemProfileActivity::Inactive
    }));
    assert!(profile.events.iter().any(|event| {
        event.transition == "enqueue" && event.activity == VmSystemProfileActivity::Runnable
    }));
    assert!(profile
        .events
        .iter()
        .any(|event| event.pid == child.as_u64() && event.transition == "exit"));
    assert!(profile
        .events
        .windows(2)
        .all(|events| events[0].sequence + 1 == events[1].sequence));
    assert_eq!(
        profile,
        runtime
            .system_profile_since(cursor)
            .expect("scheduler event replay is stable")
    );
    assert!(runtime
        .system_profile_since(profile.next_cursor)
        .expect("delivered scheduler cursor")
        .events
        .is_empty());
}

#[test]
fn tracer_suite_invalid_observer_and_failure_isolation_contract() {
    let mut runtime = VmActorRuntime::default();
    let subject = runtime.spawn_root(source("subject", 0));
    let dead_observer = runtime.spawn_root(source("dead-observer", 0));
    let traced = source("traced", 1);
    runtime
        .exit_actor(dead_observer, VmExitReason::Normal)
        .expect("prepare invalid observer identity");

    let error = runtime
        .enable_meta_trace(
            traced.clone(),
            dead_observer,
            VmMetaTraceConfig::calls_and_returns(),
        )
        .expect_err("exited observer must be rejected");
    assert!(error.contains("register meta trace observer"));
    assert_eq!(
        runtime.meta_trace_state(&traced),
        VmMetaTraceState::Disabled
    );

    let observer = runtime.spawn_root(source("observer", 0));
    runtime
        .enable_meta_trace(
            traced.clone(),
            observer,
            VmMetaTraceConfig::calls_and_returns(),
        )
        .expect("enable live observer");
    let retained_state = runtime.meta_trace_state(&traced);
    assert!(runtime
        .enable_meta_trace(
            traced.clone(),
            dead_observer,
            VmMetaTraceConfig::calls_only(),
        )
        .is_err());
    assert_eq!(runtime.meta_trace_state(&traced), retained_state);

    runtime.enable_local_trace(traced.clone(), VmLocalTraceConfig::all());
    let local_cursor = runtime.local_trace_cursor();
    let meta_cursor = runtime.meta_trace_cursor();
    assert!(runtime
        .record_local_call(subject, traced.clone(), 9)
        .expect("record isolated local call"));
    let in_flight = runtime
        .record_meta_call(subject, traced.clone(), 9)
        .expect("record isolated observed call")
        .expect("return-enabled observation token");
    runtime
        .exit_actor(
            observer,
            VmExitReason::Error("tracer-callback-failed".to_string()),
        )
        .expect("fail tracer observer");
    assert_eq!(
        runtime.meta_trace_state(&traced),
        VmMetaTraceState::Disabled
    );
    assert!(!runtime
        .record_meta_return(in_flight, subject)
        .expect("dead observer drops in-flight return"));
    assert!(runtime
        .record_local_exception(subject, traced, "error", "subject-failure")
        .expect("local failure diagnostics survive observer failure"));
    assert!(runtime.is_alive(subject));

    let local = runtime
        .local_trace_since(local_cursor)
        .expect("capture isolated local diagnostics");
    assert_eq!(local.events.len(), 2);
    assert!(matches!(
        local.events[0].kind,
        VmLocalTraceEventKind::Call { .. }
    ));
    assert!(matches!(
        local.events[1].kind,
        VmLocalTraceEventKind::Exception { .. }
    ));
    let retained = runtime
        .meta_trace_since(meta_cursor, observer)
        .expect("retain event delivered before observer failure");
    assert_eq!(retained.events.len(), 1);
    assert!(matches!(
        retained.events[0].kind,
        VmMetaTraceEventKind::Call { .. }
    ));

    let current = runtime.system_profile_cursor();
    let future = VmSystemProfileCursor::from_position(current.position() + 1);
    assert!(runtime.system_profile_since(future).is_err());
}
