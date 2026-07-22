use super::{VmActorRuntime, VmProcessSource};
use crate::runtime::vm::{
    code_server::{VmCodeServer, VmCodeServerEvent, VmModuleGenerationState},
    local_trace::{VmLocalTraceConfig, VmLocalTraceEventKind},
    meta_trace::{VmMetaTraceConfig, VmMetaTraceEventKind},
    process::VmProcessTable,
    scheduler::VmSchedulerDecision,
};

fn source(function: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new("parity.CallTrace", function, arity)
}

#[test]
fn call_trace_suite_exact_arity_process_and_toggle_contract() {
    let mut runtime = VmActorRuntime::default();
    let first = runtime.spawn_root(source("first_subject", 0));
    let second = runtime.spawn_root(source("second_subject", 0));
    let functions = [source("foo", 0), source("foo", 1), source("foo", 2)];
    for function in &functions {
        assert!(
            runtime.enable_local_trace(function.clone(), VmLocalTraceConfig::calls_and_returns(),)
        );
        assert!(
            !runtime.enable_local_trace(function.clone(), VmLocalTraceConfig::calls_and_returns(),)
        );
    }

    let cursor = runtime.local_trace_cursor();
    assert!(runtime
        .record_local_call(first, functions[0].clone(), 10)
        .expect("first zero-arity call"));
    assert!(runtime
        .record_local_call(first, functions[1].clone(), 11)
        .expect("first one-arity call"));
    assert!(runtime
        .record_local_call(second, functions[2].clone(), 12)
        .expect("second two-arity call"));
    assert!(!runtime
        .record_local_call(first, source("foo", 3), 13)
        .expect("wrong arity is silent"));
    for (actor, function) in [
        (first, functions[0].clone()),
        (first, functions[1].clone()),
        (second, functions[2].clone()),
    ] {
        assert!(runtime
            .record_local_return(actor, function)
            .expect("typed return"));
    }

    let snapshot = runtime
        .local_trace_since(cursor)
        .expect("exact trace snapshot");
    assert_eq!(snapshot.events.len(), 6);
    assert_eq!(
        snapshot
            .events
            .iter()
            .map(|event| event.pid)
            .collect::<Vec<_>>(),
        [
            first.as_u64(),
            first.as_u64(),
            second.as_u64(),
            first.as_u64(),
            first.as_u64(),
            second.as_u64(),
        ]
    );
    assert!(snapshot
        .events
        .windows(2)
        .all(|pair| pair[0].sequence + 1 == pair[1].sequence));
    assert!(snapshot.events[..3]
        .iter()
        .all(|event| matches!(event.kind, VmLocalTraceEventKind::Call { .. })));
    assert!(snapshot.events[3..]
        .iter()
        .all(|event| matches!(event.kind, VmLocalTraceEventKind::Return { .. })));
    assert_eq!(
        runtime.local_trace_since(cursor).expect("immutable replay"),
        snapshot
    );

    assert!(runtime.disable_local_trace(&functions[1]));
    let disabled_cursor = runtime.local_trace_cursor();
    assert!(!runtime
        .record_local_call(first, functions[1].clone(), 99)
        .expect("disabled exact function"));
    assert!(runtime
        .local_trace_since(disabled_cursor)
        .expect("disabled suffix")
        .events
        .is_empty());
}

#[test]
fn call_trace_suite_observer_replacement_pins_in_flight_returns() {
    let mut runtime = VmActorRuntime::default();
    let subject = runtime.spawn_root(source("subject", 0));
    let first_observer = runtime.spawn_root(source("first_observer", 0));
    let second_observer = runtime.spawn_root(source("second_observer", 0));
    let traced = source("pam_foo", 2);
    let cursor = runtime.meta_trace_cursor();

    assert!(runtime
        .enable_meta_trace(
            traced.clone(),
            first_observer,
            VmMetaTraceConfig::calls_and_returns(),
        )
        .expect("enable first observer"));
    let first_call = runtime
        .record_meta_call(subject, traced.clone(), 20)
        .expect("record first call")
        .expect("first return token");
    assert!(!runtime
        .enable_meta_trace(
            traced.clone(),
            second_observer,
            VmMetaTraceConfig::calls_and_returns(),
        )
        .expect("replace observer"));
    let second_call = runtime
        .record_meta_call(subject, traced.clone(), 21)
        .expect("record second call")
        .expect("second return token");
    assert!(runtime
        .record_meta_return(first_call, subject)
        .expect("first pinned return"));
    assert!(runtime
        .record_meta_return(second_call, subject)
        .expect("second pinned return"));
    assert!(runtime
        .record_meta_call(subject, source("pam_foo", 1), 22)
        .expect("wrong arity is silent")
        .is_none());

    let first = runtime
        .meta_trace_since(cursor, first_observer)
        .expect("first observer snapshot");
    let second = runtime
        .meta_trace_since(cursor, second_observer)
        .expect("second observer snapshot");
    assert_eq!(first.events.len(), 2);
    assert_eq!(second.events.len(), 2);
    assert!(matches!(
        first.events[0].kind,
        VmMetaTraceEventKind::Call { .. }
    ));
    assert!(matches!(
        first.events[1].kind,
        VmMetaTraceEventKind::Return { .. }
    ));
    assert!(matches!(
        second.events[0].kind,
        VmMetaTraceEventKind::Call { .. }
    ));
    assert!(matches!(
        second.events[1].kind,
        VmMetaTraceEventKind::Return { .. }
    ));
    assert_eq!(first.events[0].observer, first_observer.as_u64());
    assert_eq!(second.events[0].observer, second_observer.as_u64());
}

#[test]
fn call_trace_suite_deep_recursive_exception_preserves_complete_stack() {
    const DEPTH: usize = 512;

    let mut runtime = VmActorRuntime::default();
    let actor = runtime.spawn_root(source("deep_worker", 0));
    let recursive = source("bs_sum", 2);
    runtime.enable_local_trace(recursive.clone(), VmLocalTraceConfig::all());
    let cursor = runtime.local_trace_cursor();

    runtime
        .run_next(|process, _| {
            for offset in 0..DEPTH {
                process
                    .enter_execution_frame(recursive.clone(), offset, offset + 1)
                    .expect("grow traced recursion stack");
            }
            VmSchedulerDecision::Yield {
                reductions: DEPTH as u64,
            }
        })
        .expect("run recursive trace subject");
    for offset in 0..DEPTH {
        assert!(runtime
            .record_local_call(actor, recursive.clone(), offset)
            .expect("record recursive call"));
    }
    assert!(runtime
        .record_local_exception(actor, recursive.clone(), "error", "badmatch")
        .expect("record recursive exception"));

    let snapshot = runtime
        .local_trace_since(cursor)
        .expect("deep exception snapshot");
    assert_eq!(snapshot.events.len(), DEPTH + 1);
    assert!(snapshot
        .events
        .windows(2)
        .all(|pair| pair[0].sequence + 1 == pair[1].sequence));
    let VmLocalTraceEventKind::Exception {
        source: event_source,
        class,
        reason,
        stack,
    } = &snapshot.events[DEPTH].kind
    else {
        panic!("final deep event must be an exception")
    };
    assert_eq!(event_source, &recursive);
    assert_eq!(class, "error");
    assert_eq!(reason, "badmatch");
    assert_eq!(stack.len(), DEPTH + 1);
    assert_eq!(stack.first().expect("top frame").source, recursive);
    assert_eq!(
        stack.last().expect("root frame").source,
        source("deep_worker", 0)
    );
}

#[test]
fn call_trace_suite_reload_keeps_generation_events_ordered_and_purge_safe() {
    const MODULE: &str = "call_trace_upgrade";
    let version_one = concat!(
        "module call_trace_upgrade.\n\n",
        "pub version(): Int -> 1.\n\n",
        "pub local_version(): Int -> 1.\n",
    );
    let version_two = concat!(
        "module call_trace_upgrade.\n\n",
        "pub version(): Int -> 2.\n\n",
        "pub local_version(): Int -> 2.\n",
    );
    let mut processes = VmProcessTable::default();
    let process = processes.spawn_root(source("upgrade_worker", 0));
    let mut server = VmCodeServer::default();

    server
        .publish_source("call_trace_upgrade.terl", version_one)
        .expect("publish first generation");
    let old = server
        .enter_process_function(&mut processes, process, MODULE, "version", 0, 1, 7)
        .expect("enter first generation");
    server
        .publish_source("call_trace_upgrade.terl", version_two)
        .expect("publish replacement generation");
    let during_reload = server.snapshots_for_module(MODULE);
    assert_eq!(during_reload.len(), 2);
    assert_eq!(during_reload[0].generation, old.generation);
    assert_eq!(during_reload[0].state, VmModuleGenerationState::Retiring);
    assert_eq!(during_reload[0].active_processes, 1);
    assert_eq!(during_reload[1].state, VmModuleGenerationState::Active);

    let (_, retired) = server
        .return_process_function(&mut processes, process)
        .expect("return first generation");
    assert_eq!(
        retired,
        Some(VmCodeServerEvent::GenerationRetired {
            module: MODULE.to_string(),
            generation: old.generation,
        })
    );
    let current = server
        .enter_process_function(&mut processes, process, MODULE, "version", 0, 2, 8)
        .expect("enter replacement generation");
    assert_ne!(current.generation, old.generation);
    server
        .return_process_function(&mut processes, process)
        .expect("return replacement generation");

    assert_eq!(
        server
            .purge_retired_generations(MODULE)
            .expect("purge drained first generation"),
        [VmCodeServerEvent::GenerationPurged {
            module: MODULE.to_string(),
            generation: old.generation,
        }]
    );
    assert_eq!(server.snapshots_for_module(MODULE).len(), 1);
    assert!(server.function_exported(MODULE, "version", 0));
    server
        .unload_active_generation(MODULE)
        .expect("retire current generation");
    server
        .purge_retired_generations(MODULE)
        .expect("purge current generation");
    assert!(!server.module_loaded(MODULE));
    assert_eq!(
        server
            .event_snapshots_for_module(MODULE)
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );
}
