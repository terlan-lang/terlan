use super::super::{VmActorRuntime, VmExitReason, VmProcessSource};
use crate::runtime::vm::local_trace::{
    VmLocalTraceConfig, VmLocalTraceEventKind, VmLocalTraceSnapshot,
};
use crate::runtime::vm::meta_trace::{VmMetaTraceConfig, VmMetaTraceEventKind};

fn source(function: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new("parity.TraceNif", function, arity)
}

fn assert_adjacent_sequences(snapshot: &VmLocalTraceSnapshot) {
    assert!(snapshot
        .events
        .windows(2)
        .all(|events| events[0].sequence + 1 == events[1].sequence));
}

#[test]
fn trace_nif_suite_exact_native_entry_return_and_arity_contract() {
    let mut runtime = VmActorRuntime::default();
    let subject = runtime.spawn_root(source("nif_process", 0));
    let observer = runtime.spawn_root(source("observer", 0));
    let nif_zero = source("nif", 0);
    let nif_one = source("nif", 1);
    for native in [&nif_zero, &nif_one] {
        assert!(
            runtime.enable_local_trace(native.clone(), VmLocalTraceConfig::calls_and_returns(),)
        );
        runtime
            .enable_meta_trace(
                native.clone(),
                observer,
                VmMetaTraceConfig::calls_and_returns(),
            )
            .expect("enable native observer");
    }
    let local_cursor = runtime.local_trace_cursor();
    let meta_cursor = runtime.meta_trace_cursor();

    for native in [&nif_zero, &nif_one, &nif_zero, &nif_one] {
        let call = runtime
            .begin_native_trace_call(subject, native.clone())
            .expect("publish native entry");
        runtime
            .complete_native_trace_call(subject, call)
            .expect("publish native return");
    }

    let local = runtime
        .local_trace_since(local_cursor)
        .expect("inspect native local diagnostics");
    let meta = runtime
        .meta_trace_since(meta_cursor, observer)
        .expect("inspect native observer diagnostics");
    assert_eq!(local.events.len(), 8);
    assert_eq!(meta.events.len(), 8);
    assert_adjacent_sequences(&local);
    assert!(meta
        .events
        .windows(2)
        .all(|events| events[0].sequence + 1 == events[1].sequence));
    for (index, expected) in [&nif_zero, &nif_one, &nif_zero, &nif_one]
        .into_iter()
        .enumerate()
    {
        assert!(matches!(
            &local.events[index * 2].kind,
            VmLocalTraceEventKind::Call { location } if location.source == *expected
        ));
        assert!(matches!(
            &local.events[index * 2 + 1].kind,
            VmLocalTraceEventKind::Return { source: returned, caller }
                if returned == expected && caller.source == source("nif_process", 0)
        ));
        assert!(matches!(
            &meta.events[index * 2].kind,
            VmMetaTraceEventKind::Call { location } if location.source == *expected
        ));
        assert!(matches!(
            &meta.events[index * 2 + 1].kind,
            VmMetaTraceEventKind::Return { source: returned, caller }
                if returned == expected && caller.source == source("nif_process", 0)
        ));
    }

    let silent_local = runtime.local_trace_cursor();
    let silent_meta = runtime.meta_trace_cursor();
    let wrong_arity = runtime
        .begin_native_trace_call(subject, source("nif", 2))
        .expect("unsubscribed arity still executes");
    runtime
        .complete_native_trace_call(subject, wrong_arity)
        .expect("unsubscribed arity still returns");
    assert!(runtime
        .local_trace_since(silent_local)
        .expect("inspect exact local arity filter")
        .events
        .is_empty());
    assert!(runtime
        .meta_trace_since(silent_meta, observer)
        .expect("inspect exact meta arity filter")
        .events
        .is_empty());
}

#[test]
fn trace_nif_suite_failure_toggle_and_observer_cleanup_contract() {
    let mut runtime = VmActorRuntime::default();
    let subject = runtime.spawn_root(source("nif_process", 0));
    let observer = runtime.spawn_root(source("observer", 0));
    let native = source("nif", 1);
    runtime.enable_local_trace(native.clone(), VmLocalTraceConfig::all());
    runtime
        .enable_meta_trace(
            native.clone(),
            observer,
            VmMetaTraceConfig::calls_and_returns(),
        )
        .expect("enable native observer");
    let local_cursor = runtime.local_trace_cursor();
    let meta_cursor = runtime.meta_trace_cursor();

    let failed = runtime
        .begin_native_trace_call(subject, native.clone())
        .expect("publish failing native entry");
    runtime
        .fail_native_trace_call(subject, failed, "worker rejected request")
        .expect("publish native failure");
    let local = runtime
        .local_trace_since(local_cursor)
        .expect("inspect native failure diagnostics");
    let meta = runtime
        .meta_trace_since(meta_cursor, observer)
        .expect("inspect failed native observer diagnostics");
    assert_eq!(local.events.len(), 2);
    assert!(matches!(
        local.events[0].kind,
        VmLocalTraceEventKind::Call { .. }
    ));
    assert!(matches!(
        &local.events[1].kind,
        VmLocalTraceEventKind::Exception { class, reason, .. }
            if class == "native" && reason == "worker rejected request"
    ));
    assert_eq!(meta.events.len(), 1);
    assert!(matches!(
        meta.events[0].kind,
        VmMetaTraceEventKind::Call { .. }
    ));

    assert!(runtime.disable_local_trace(&native));
    let local_disabled = runtime.local_trace_cursor();
    let meta_active = runtime.meta_trace_cursor();
    let completed = runtime
        .begin_native_trace_call(subject, native.clone())
        .expect("observer remains active while local stream is disabled");
    runtime
        .complete_native_trace_call(subject, completed)
        .expect("complete observer-only native call");
    assert!(runtime
        .local_trace_since(local_disabled)
        .expect("local disable is immediate")
        .events
        .is_empty());
    assert_eq!(
        runtime
            .meta_trace_since(meta_active, observer)
            .expect("meta observation remains independent")
            .events
            .len(),
        2
    );

    runtime
        .exit_actor(observer, VmExitReason::Normal)
        .expect("exit native observer");
    let cleaned_cursor = runtime.meta_trace_cursor();
    let unobserved = runtime
        .begin_native_trace_call(subject, native)
        .expect("native call continues after observer cleanup");
    runtime
        .complete_native_trace_call(subject, unobserved)
        .expect("native return continues after observer cleanup");
    assert!(runtime
        .meta_trace_since(cleaned_cursor, observer)
        .expect("dead observer receives no further events")
        .events
        .is_empty());
}
