use super::super::{VmActorRuntime, VmExitReason, VmProcessSource};
use crate::runtime::vm::local_trace::{VmLocalTraceConfig, VmLocalTraceEventKind};
use crate::runtime::vm::meta_trace::{VmMetaTraceConfig, VmMetaTraceEventKind, VmMetaTraceState};

fn source(function: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new("parity.TraceMeta", function, arity)
}

#[test]
fn trace_meta_suite_observer_scoped_call_return_and_cursor_contract() {
    let mut runtime = VmActorRuntime::default();
    let subject = runtime.spawn_root(source("slave", 1));
    let observer = runtime.spawn_root(source("observer", 0));
    let chain = [
        source("exported_wrap", 1),
        source("exported", 1),
        source("local", 1),
        source("local2", 1),
        source("local_tail", 1),
    ];
    let cursor = runtime.meta_trace_cursor();
    for traced in &chain {
        assert!(runtime
            .enable_meta_trace(
                traced.clone(),
                observer,
                VmMetaTraceConfig::calls_and_returns(),
            )
            .expect("enable observer-scoped trace"));
        assert!(!runtime
            .enable_meta_trace(
                traced.clone(),
                observer,
                VmMetaTraceConfig::calls_and_returns(),
            )
            .expect("idempotently update observer-scoped trace"));
        assert_eq!(
            runtime.meta_trace_state(traced),
            VmMetaTraceState::Enabled {
                observer: observer.as_u64(),
                config: VmMetaTraceConfig::calls_and_returns(),
            }
        );
    }

    let mut tokens = Vec::new();
    for (offset, traced) in chain.iter().enumerate() {
        tokens.push(
            runtime
                .record_meta_call(subject, traced.clone(), offset)
                .expect("record observer-scoped call")
                .expect("return-enabled call yields token"),
        );
    }
    for token in tokens.into_iter().rev() {
        assert!(runtime
            .record_meta_return(token, subject)
            .expect("record observer-scoped return"));
    }

    let snapshot = runtime
        .meta_trace_since(cursor, observer)
        .expect("inspect observer event suffix");
    assert_eq!(snapshot.events.len(), 10);
    assert!(snapshot
        .events
        .windows(2)
        .all(|events| events[0].sequence + 1 == events[1].sequence));
    for (event, traced) in snapshot.events[..5].iter().zip(&chain) {
        assert_eq!(event.observer, observer.as_u64());
        assert_eq!(event.subject, subject.as_u64());
        assert!(matches!(
            &event.kind,
            VmMetaTraceEventKind::Call { location } if location.source == *traced
        ));
    }
    let VmMetaTraceEventKind::Return {
        source: returned,
        caller,
    } = &snapshot.events[5].kind
    else {
        panic!("sixth observer event must be a return")
    };
    assert_eq!(returned, &source("local_tail", 1));
    assert_eq!(caller.source, source("slave", 1));
    assert_eq!(
        runtime
            .meta_trace_since(cursor, observer)
            .expect("observer cursor replay is immutable"),
        snapshot
    );
    assert!(runtime
        .meta_trace_since(snapshot.next_cursor, observer)
        .expect("delivered observer cursor excludes old events")
        .events
        .is_empty());

    assert!(runtime
        .record_meta_call(subject, source("exported_wrap", 2), 0)
        .expect("wrong arity is silently outside exact subscription")
        .is_none());
    assert!(runtime.disable_meta_trace(&chain[2]));
    assert_eq!(
        runtime.meta_trace_state(&chain[2]),
        VmMetaTraceState::Disabled
    );
    assert!(runtime
        .record_meta_call(subject, chain[2].clone(), 0)
        .expect("disabled meta subscription is silent")
        .is_none());

    let calls_only = source("calls_only", 0);
    runtime
        .enable_meta_trace(
            calls_only.clone(),
            observer,
            VmMetaTraceConfig::calls_only(),
        )
        .expect("enable calls-only observation");
    let calls_only_cursor = runtime.meta_trace_cursor();
    assert!(runtime
        .record_meta_call(subject, calls_only, 0)
        .expect("record calls-only observation")
        .is_none());
    assert_eq!(
        runtime
            .meta_trace_since(calls_only_cursor, observer)
            .expect("inspect calls-only observation")
            .events
            .len(),
        1
    );
}

#[test]
fn trace_meta_suite_replacement_cleanup_local_combo_and_trigger_contract() {
    let mut runtime = VmActorRuntime::default();
    let subject = runtime.spawn_root(source("receiver", 1));
    let first_observer = runtime.spawn_root(source("observer_one", 0));
    let second_observer = runtime.spawn_root(source("observer_two", 0));
    let receiver = source("receiver", 1);
    let cursor = runtime.meta_trace_cursor();
    runtime
        .enable_meta_trace(
            receiver.clone(),
            first_observer,
            VmMetaTraceConfig::calls_and_returns(),
        )
        .expect("enable first observer");
    let first_token = runtime
        .record_meta_call(subject, receiver.clone(), 0)
        .expect("record first-observer call")
        .expect("return-enabled first call token");
    assert!(!runtime
        .enable_meta_trace(
            receiver.clone(),
            second_observer,
            VmMetaTraceConfig::calls_and_returns(),
        )
        .expect("replace observer without creating another subscription"));
    assert!(runtime
        .record_meta_return(first_token, subject)
        .expect("in-flight return stays pinned to first observer"));

    let second_token = runtime
        .record_meta_call(subject, receiver.clone(), 1)
        .expect("record replacement-observer call")
        .expect("return-enabled replacement call token");
    assert!(runtime
        .record_meta_return(second_token, subject)
        .expect("new return routes to replacement observer"));
    let first_events = runtime
        .meta_trace_since(cursor, first_observer)
        .expect("inspect first observer events");
    let second_events = runtime
        .meta_trace_since(cursor, second_observer)
        .expect("inspect second observer events");
    assert_eq!(first_events.events.len(), 2);
    assert_eq!(second_events.events.len(), 2);
    assert!(matches!(
        first_events.events[1].kind,
        VmMetaTraceEventKind::Return { .. }
    ));
    assert!(matches!(
        second_events.events[0].kind,
        VmMetaTraceEventKind::Call { .. }
    ));

    runtime
        .exit_actor(second_observer, VmExitReason::Normal)
        .expect("exit replacement observer");
    assert_eq!(
        runtime.meta_trace_state(&receiver),
        VmMetaTraceState::Disabled,
        "observer exit removes its subscriptions"
    );
    assert!(runtime
        .record_meta_call(subject, receiver.clone(), 2)
        .expect("dead observer subscription is silent")
        .is_none());

    let third_observer = runtime.spawn_root(source("observer_three", 0));
    runtime
        .enable_meta_trace(
            receiver.clone(),
            third_observer,
            VmMetaTraceConfig::calls_only(),
        )
        .expect("enable combined meta observer");
    runtime.enable_local_trace(receiver.clone(), VmLocalTraceConfig::calls_only());
    let meta_combo_cursor = runtime.meta_trace_cursor();
    let local_combo_cursor = runtime.local_trace_cursor();
    runtime
        .record_meta_call(subject, receiver.clone(), 3)
        .expect("record combined meta event");
    runtime
        .record_local_call(subject, receiver.clone(), 3)
        .expect("record independent local event");
    assert_eq!(
        runtime
            .meta_trace_since(meta_combo_cursor, third_observer)
            .expect("inspect combined meta stream")
            .events
            .len(),
        1
    );
    assert!(matches!(
        runtime
            .local_trace_since(local_combo_cursor)
            .expect("inspect combined local stream")
            .events[0]
            .kind,
        VmLocalTraceEventKind::Call { .. }
    ));

    let trigger = source("id", 1);
    let locally_silent = source("exported_wrap", 1);
    runtime
        .enable_meta_trace(
            trigger.clone(),
            third_observer,
            VmMetaTraceConfig::calls_and_returns(),
        )
        .expect("enable explicit diagnostic trigger");
    let trigger_cursor = runtime.meta_trace_cursor();
    let silent_cursor = runtime.local_trace_cursor();
    let start_token = runtime
        .record_meta_call(subject, trigger.clone(), 0)
        .expect("meta start trigger is never silenced")
        .expect("start trigger return token");
    runtime.enable_local_trace(locally_silent.clone(), VmLocalTraceConfig::calls_only());
    runtime
        .record_local_call(subject, locally_silent.clone(), 0)
        .expect("explicit start enables local observation");
    let stop_token = runtime
        .record_meta_call(subject, trigger, 1)
        .expect("meta stop trigger is never silenced")
        .expect("stop trigger return token");
    assert!(runtime.disable_local_trace(&locally_silent));
    assert!(!runtime
        .record_local_call(subject, locally_silent, 1)
        .expect("explicit stop silences local observation"));
    runtime
        .record_meta_return(start_token, subject)
        .expect("publish start trigger return");
    runtime
        .record_meta_return(stop_token, subject)
        .expect("publish stop trigger return");
    assert_eq!(
        runtime
            .local_trace_since(silent_cursor)
            .expect("inspect explicitly gated local stream")
            .events
            .len(),
        1
    );
    assert_eq!(
        runtime
            .meta_trace_since(trigger_cursor, third_observer)
            .expect("meta trigger remains independent from local silence")
            .events
            .len(),
        4
    );

    let loop_source = source("loop", 4);
    runtime
        .enable_meta_trace(
            loop_source.clone(),
            third_observer,
            VmMetaTraceConfig::calls_only(),
        )
        .expect("enable recursive observer stream");
    let recursion_cursor = runtime.meta_trace_cursor();
    for offset in 0..4_096 {
        runtime
            .record_meta_call(subject, loop_source.clone(), offset)
            .expect("record deep recursive observation");
    }
    let recursion = runtime
        .meta_trace_since(recursion_cursor, third_observer)
        .expect("inspect deep observer stream");
    assert_eq!(recursion.events.len(), 4_096);
    assert!(recursion
        .events
        .windows(2)
        .all(|events| events[0].sequence + 1 == events[1].sequence));
}
