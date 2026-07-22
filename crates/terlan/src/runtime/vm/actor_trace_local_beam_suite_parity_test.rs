use super::{VmActorRuntime, VmExitReason, VmProcessSource};
use crate::runtime::vm::code_server::VmCodeServer;
use crate::runtime::vm::local_trace::{VmLocalTraceConfig, VmLocalTraceEventKind};
use crate::runtime::vm::process::VmProcessTable;
use crate::runtime::vm::scheduler::VmSchedulerDecision;

fn source(function: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new("parity.TraceLocal", function, arity)
}

#[test]
fn trace_local_suite_call_return_filter_and_cursor_contract() {
    let mut runtime = VmActorRuntime::default();
    let actor = runtime.spawn_root(source("slave", 2));
    let chain = [
        source("exported_wrap", 1),
        source("exported", 1),
        source("local", 1),
        source("local2", 1),
        source("local_tail", 1),
    ];
    let cursor = runtime.local_trace_cursor();
    for called in &chain {
        assert!(runtime.enable_local_trace(called.clone(), VmLocalTraceConfig::calls_and_returns()));
        assert!(
            !runtime.enable_local_trace(called.clone(), VmLocalTraceConfig::calls_and_returns())
        );
        assert!(runtime.local_trace_enabled(called));
    }

    for (offset, called) in chain.iter().enumerate() {
        assert!(runtime
            .record_local_call(actor, called.clone(), offset)
            .expect("record exact local call"));
    }
    runtime
        .run_next(|process, _| {
            for (index, called) in chain.iter().enumerate() {
                process
                    .enter_execution_frame(called.clone(), index, index + 10)
                    .expect("enter traced local frame");
            }
            VmSchedulerDecision::Yield { reductions: 5 }
        })
        .expect("run local call chain");

    for returned in chain.iter().rev() {
        runtime
            .run_next(|process, _| {
                assert_eq!(
                    process
                        .pop_execution_frame()
                        .expect("return traced local frame")
                        .source,
                    *returned
                );
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("run local return");
        assert!(runtime
            .record_local_return(actor, returned.clone())
            .expect("record exact local return"));
    }

    let snapshot = runtime
        .local_trace_since(cursor)
        .expect("inspect local trace suffix");
    assert_eq!(snapshot.events.len(), 10);
    assert!(snapshot
        .events
        .windows(2)
        .all(|events| events[0].sequence + 1 == events[1].sequence));
    for (event, called) in snapshot.events[..5].iter().zip(&chain) {
        assert_eq!(event.pid, actor.as_u64());
        let VmLocalTraceEventKind::Call { location } = &event.kind else {
            panic!("first local events must be calls")
        };
        assert_eq!(&location.source, called);
    }
    let VmLocalTraceEventKind::Return {
        source: event_source,
        caller,
    } = &snapshot.events[5].kind
    else {
        panic!("first unwind event must be a return")
    };
    assert_eq!(event_source, &source("local_tail", 1));
    assert_eq!(caller.source, source("local2", 1));
    let VmLocalTraceEventKind::Return {
        source: event_source,
        caller,
    } = &snapshot.events[9].kind
    else {
        panic!("last unwind event must be a return")
    };
    assert_eq!(event_source, &source("exported_wrap", 1));
    assert_eq!(caller.source, source("slave", 2));
    assert_eq!(
        runtime
            .local_trace_since(cursor)
            .expect("cursor replay is immutable"),
        snapshot
    );
    assert!(runtime
        .local_trace_since(snapshot.next_cursor)
        .expect("delivered cursor excludes old local events")
        .events
        .is_empty());

    assert!(!runtime
        .record_local_call(actor, source("exported_wrap", 2), 0)
        .expect("wrong arity is not traced"));
    assert!(runtime.disable_local_trace(&chain[2]));
    assert!(!runtime.local_trace_enabled(&chain[2]));
    assert!(!runtime
        .record_local_call(actor, chain[2].clone(), 0)
        .expect("disabled local function is not traced"));
    assert!(!runtime.disable_local_trace(&chain[2]));

    let recursive = source("bs_sum", 2);
    runtime.enable_local_trace(recursive.clone(), VmLocalTraceConfig::calls_only());
    let recursion_cursor = runtime.local_trace_cursor();
    for offset in 0..5 {
        runtime
            .record_local_call(actor, recursive.clone(), offset)
            .expect("record recursive bit-syntax-shaped call");
    }
    let recursion = runtime
        .local_trace_since(recursion_cursor)
        .expect("inspect recursive local calls");
    assert_eq!(recursion.events.len(), 5);
    assert!(recursion.events.iter().all(|event| matches!(
        &event.kind,
        VmLocalTraceEventKind::Call { location } if location.source == recursive
    )));
}

#[test]
fn trace_local_suite_exception_unload_stack_growth_and_toggle_churn_contract() {
    let mut runtime = VmActorRuntime::default();
    let actor = runtime.spawn_root(source("exception_worker", 0));
    let top = source("exc_top", 3);
    let crash = source("exc", 2);
    runtime.enable_local_trace(top.clone(), VmLocalTraceConfig::all());
    runtime.enable_local_trace(crash.clone(), VmLocalTraceConfig::all());
    let exception_cursor = runtime.local_trace_cursor();
    runtime
        .run_next(|process, _| {
            process
                .enter_execution_frame(top.clone(), 1, 7)
                .expect("enter exception wrapper");
            process
                .enter_execution_frame(crash.clone(), 4, 11)
                .expect("enter crashing local function");
            VmSchedulerDecision::Yield { reductions: 2 }
        })
        .expect("run exception frames");
    runtime
        .record_local_call(actor, crash.clone(), 4)
        .expect("record crashing call");
    runtime
        .record_local_exception(actor, crash.clone(), "error", "badmatch")
        .expect("record typed local exception");
    runtime
        .exit_actor(actor, VmExitReason::Error("badmatch".to_string()))
        .expect("exit crashing actor");

    let exception = runtime
        .local_trace_since(exception_cursor)
        .expect("inspect exception events");
    assert_eq!(exception.events.len(), 2);
    let VmLocalTraceEventKind::Exception {
        source: event_source,
        class,
        reason,
        stack,
    } = &exception.events[1].kind
    else {
        panic!("second event must be the typed exception")
    };
    assert_eq!(event_source, &crash);
    assert_eq!(class, "error");
    assert_eq!(reason, "badmatch");
    assert_eq!(stack.len(), 3);
    assert_eq!(stack[0].source, crash);
    assert_eq!(stack[1].source, top);
    assert_eq!(stack[2].source, source("exception_worker", 0));
    assert_eq!(
        runtime
            .local_trace_since(exception_cursor)
            .expect("post-exit exception history is immutable"),
        exception
    );

    let mut processes = VmProcessTable::default();
    let loader = processes.spawn_root(source("loader", 0));
    let mut code_server = VmCodeServer::default();
    code_server
        .publish_source(
            "trace_local_dummy.terl",
            "module trace_local_dummy.\n\npub dummy(): Int -> 1.\n",
        )
        .expect("publish local trace dummy");
    assert!(code_server.function_exported("trace_local_dummy", "dummy", 0));
    code_server
        .unload_active_generation("trace_local_dummy")
        .expect("retire unbound trace dummy");
    code_server
        .purge_retired_generations("trace_local_dummy")
        .expect("purge retired trace dummy");
    assert!(!code_server.module_loaded("trace_local_dummy"));
    assert!(!code_server.function_exported("trace_local_dummy", "dummy", 0));
    assert_eq!(
        code_server
            .enter_process_function(
                &mut processes,
                loader,
                "trace_local_dummy",
                "dummy",
                0,
                0,
                1,
            )
            .expect_err("purged local function cannot be entered"),
        "module `trace_local_dummy` has no active generation"
    );

    let mut churn = VmActorRuntime::default();
    let worker = churn.spawn_root(source("churn_worker", 0));
    let stable = source("stable", 0);
    let toggled = source("infinite_loop", 0);
    churn.enable_local_trace(stable.clone(), VmLocalTraceConfig::calls_only());
    let churn_cursor = churn.local_trace_cursor();
    for offset in 0..4_096 {
        assert!(churn.enable_local_trace(toggled.clone(), VmLocalTraceConfig::calls_only()));
        churn
            .record_local_call(worker, toggled.clone(), offset)
            .expect("record enabled churn function");
        assert!(churn.disable_local_trace(&toggled));
        assert!(!churn
            .record_local_call(worker, toggled.clone(), offset)
            .expect("disabled churn function is silent"));
        churn
            .record_local_call(worker, stable.clone(), offset)
            .expect("stable trace survives toggle churn");
    }
    let events = churn
        .local_trace_since(churn_cursor)
        .expect("inspect toggle churn");
    assert_eq!(events.events.len(), 8_192);
    assert!(events
        .events
        .windows(2)
        .all(|pair| pair[0].sequence + 1 == pair[1].sequence));
    assert_eq!(
        events
            .events
            .iter()
            .filter(|event| matches!(
                &event.kind,
                VmLocalTraceEventKind::Call { location } if location.source == stable
            ))
            .count(),
        4_096
    );
}
