use super::{VmActorReceive, VmActorRuntime, VmActorSpawnOptions, VmExitReason, VmProcessSource};
use crate::runtime::vm::call_count::VmCallCountState;
use crate::runtime::vm::call_memory::VmCallMemoryState;
use crate::runtime::vm::call_time::VmCallTimeState;
use crate::runtime::vm::failure::is_monitor_down_message;
use crate::runtime::vm::local_trace::{VmLocalTraceConfig, VmLocalTraceEventKind};
use crate::runtime::vm::meta_trace::{VmMetaTraceConfig, VmMetaTraceEventKind, VmMetaTraceState};
use crate::runtime::vm::process::VmProcessState;
use crate::runtime::vm::process_environment::VmRuntimeEnvironmentProfile;
use crate::runtime::vm::scheduler::VmSchedulerDecision;
use crate::runtime::vm::system_profile::VmSystemProfileCursor;
use crate::runtime::vm::ReplValue;

fn source(function: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new("parity.TraceSession", function, arity)
}

fn profile() -> VmRuntimeEnvironmentProfile {
    VmRuntimeEnvironmentProfile::new(1_024, 1).expect("valid trace-session profile")
}

fn receive_payload(runtime: &mut VmActorRuntime, pid: super::VmProcessId) -> ReplValue {
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(pid)
        .expect("receive session payload")
    else {
        panic!("session actor must have a queued payload")
    };
    message.payload
}

fn publish_call_and_return(
    runtime: &mut VmActorRuntime,
    subject: super::VmProcessId,
    traced: &VmProcessSource,
    offset: usize,
) {
    runtime
        .record_local_call(subject, traced.clone(), offset)
        .expect("record scoped local call");
    let meta = runtime
        .record_meta_call(subject, traced.clone(), offset)
        .expect("record scoped meta call");
    runtime
        .record_local_return(subject, traced.clone())
        .expect("record scoped local return");
    if let Some(meta) = meta {
        runtime
            .record_meta_return(meta, subject)
            .expect("record scoped meta return");
    }
}

#[test]
fn trace_session_suite_independent_consumers_toggle_and_teardown_contract() {
    let mut runtime = VmActorRuntime::default();
    let subject = runtime.spawn_root(source("subject", 0));
    let observer = runtime.spawn_root(source("observer", 0));
    let traced = source("traced", 1);
    runtime.enable_local_trace(traced.clone(), VmLocalTraceConfig::calls_and_returns());
    runtime
        .enable_meta_trace(
            traced.clone(),
            observer,
            VmMetaTraceConfig::calls_and_returns(),
        )
        .expect("enable independent observer stream");
    let local_early = runtime.local_trace_cursor();
    let meta_early = runtime.meta_trace_cursor();
    let profile_first = runtime.system_profile_cursor();
    let profile_second = runtime.system_profile_cursor();

    publish_call_and_return(&mut runtime, subject, &traced, 3);
    let local_late = runtime.local_trace_cursor();
    let meta_late = runtime.meta_trace_cursor();
    publish_call_and_return(&mut runtime, subject, &traced, 7);

    let local_all = runtime
        .local_trace_since(local_early)
        .expect("first local consumer replay");
    let local_suffix = runtime
        .local_trace_since(local_late)
        .expect("second local consumer suffix");
    assert_eq!(local_all.events.len(), 4);
    assert_eq!(local_suffix.events, local_all.events[2..]);
    assert!(matches!(
        &local_all.events[0].kind,
        VmLocalTraceEventKind::Call { location } if location.instruction_offset == 3
    ));
    assert!(matches!(
        local_all.events[3].kind,
        VmLocalTraceEventKind::Return { .. }
    ));

    let meta_all = runtime
        .meta_trace_since(meta_early, observer)
        .expect("first meta consumer replay");
    let meta_suffix = runtime
        .meta_trace_since(meta_late, observer)
        .expect("second meta consumer suffix");
    assert_eq!(meta_all.events.len(), 4);
    assert_eq!(meta_suffix.events, meta_all.events[2..]);
    assert!(matches!(
        meta_all.events[0].kind,
        VmMetaTraceEventKind::Call { .. }
    ));
    assert!(matches!(
        meta_all.events[3].kind,
        VmMetaTraceEventKind::Return { .. }
    ));

    assert!(runtime.disable_local_trace(&traced));
    let local_disabled = runtime.local_trace_cursor();
    let meta_active = runtime.meta_trace_cursor();
    publish_call_and_return(&mut runtime, subject, &traced, 11);
    assert!(runtime
        .local_trace_since(local_disabled)
        .expect("disabled local consumer")
        .events
        .is_empty());
    assert_eq!(
        runtime
            .meta_trace_since(meta_active, observer)
            .expect("independent meta consumer remains active")
            .events
            .len(),
        2
    );

    runtime
        .exit_actor(observer, VmExitReason::Normal)
        .expect("tear down observer ownership");
    assert_eq!(
        runtime.meta_trace_state(&traced),
        VmMetaTraceState::Disabled
    );
    runtime.enable_local_trace(traced.clone(), VmLocalTraceConfig::calls_only());
    let local_after_teardown = runtime.local_trace_cursor();
    publish_call_and_return(&mut runtime, subject, &traced, 13);
    let after_teardown = runtime
        .local_trace_since(local_after_teardown)
        .expect("local diagnostics survive observer teardown");
    assert_eq!(after_teardown.events.len(), 1);
    assert!(matches!(
        after_teardown.events[0].kind,
        VmLocalTraceEventKind::Call { .. }
    ));

    runtime
        .run_next(|_, _| VmSchedulerDecision::Yield { reductions: 2 })
        .expect("publish scheduler activity");
    let first_profile = runtime
        .system_profile_since(profile_first)
        .expect("first profile consumer");
    let second_profile = runtime
        .system_profile_since(profile_second)
        .expect("second profile consumer");
    assert_eq!(first_profile, second_profile);
    assert!(!first_profile.events.is_empty());
    assert!(runtime
        .system_profile_since(first_profile.next_cursor)
        .expect("advanced consumer is empty")
        .events
        .is_empty());
    assert_eq!(
        runtime
            .system_profile_since(profile_second)
            .expect("unadvanced consumer remains replayable"),
        second_profile
    );
    let future = VmSystemProfileCursor::from_position(first_profile.next_cursor.position() + 1);
    assert!(runtime.system_profile_since(future).is_err());
}

#[test]
fn trace_session_suite_spawn_link_registry_and_mailbox_pressure_contract() {
    let mut runtime = VmActorRuntime::default();
    let parent = runtime.spawn_root(source("parent", 0));
    runtime
        .register_name("trace.session.parent", parent)
        .expect("register traced parent");
    let cursor = runtime.system_profile_cursor();
    let child_spawn = runtime
        .spawn_child_with_options(
            parent,
            source("child", 0),
            VmActorSpawnOptions::default().linked().monitored(),
        )
        .expect("spawn linked monitored child");
    let child = child_spawn.pid;
    let monitor_ref = child_spawn.monitor_ref.expect("child monitor reference");
    let grandchild = runtime
        .spawn_child(child, source("grandchild", 0))
        .expect("spawn unlinked grandchild");

    assert_eq!(
        runtime
            .failure_snapshot(parent)
            .expect("parent relationship snapshot")
            .links,
        vec![child]
    );
    assert_eq!(
        runtime
            .processes()
            .snapshot(child)
            .expect("child topology snapshot")
            .parent,
        Some(parent)
    );
    assert_eq!(
        runtime
            .processes()
            .snapshot(grandchild)
            .expect("grandchild topology snapshot")
            .parent,
        Some(child)
    );

    for value in 0..80 {
        runtime
            .send(parent, child, ReplValue::Int(value))
            .expect("fill traced mailbox");
    }
    let high = runtime
        .environment_snapshot(profile())
        .expect("high mailbox observation");
    assert_eq!(high.mailbox_messages, 80);
    assert!(
        runtime
            .memory_metrics(child)
            .expect("high mailbox memory")
            .current_bytes
            > 0
    );
    for value in 0..20 {
        assert_eq!(receive_payload(&mut runtime, child), ReplValue::Int(value));
    }
    assert_eq!(
        runtime
            .environment_snapshot(profile())
            .expect("lower mailbox observation")
            .mailbox_messages,
        60
    );
    for value in 80..100 {
        runtime
            .send(parent, child, ReplValue::Int(value))
            .expect("refill traced mailbox");
    }
    assert_eq!(
        runtime
            .environment_snapshot(profile())
            .expect("refilled mailbox observation")
            .mailbox_messages,
        80
    );
    for value in 20..100 {
        assert_eq!(receive_payload(&mut runtime, child), ReplValue::Int(value));
    }
    assert_eq!(
        runtime
            .memory_metrics(child)
            .expect("drained mailbox memory")
            .current_bytes,
        0
    );

    assert!(runtime
        .unlink_actors(parent, child)
        .expect("disable inherited link relationship"));
    runtime
        .exit_actor(child, VmExitReason::Error("session-child-exit".to_string()))
        .expect("exit traced child");
    let down = receive_payload(&mut runtime, parent);
    assert!(is_monitor_down_message(&down, &monitor_ref));
    assert!(runtime.is_alive(parent));
    assert!(runtime.is_alive(grandchild));
    assert_eq!(
        runtime
            .processes()
            .snapshot(child)
            .expect("postmortem child state")
            .state,
        VmProcessState::Exited(VmExitReason::Error("session-child-exit".to_string()))
    );
    runtime
        .unregister_name("trace.session.parent")
        .expect("unregister traced parent");
    assert!(runtime.registered_names().is_empty());

    let transitions = runtime
        .system_profile_since(cursor)
        .expect("topology scheduler transitions");
    assert!(transitions
        .events
        .iter()
        .any(|event| event.pid == child.as_u64() && event.transition == "exit"));
    assert!(transitions
        .events
        .windows(2)
        .all(|events| events[0].sequence + 1 == events[1].sequence));
}

#[test]
fn trace_session_suite_stack_and_function_profile_lifecycle_contract() {
    let mut runtime = VmActorRuntime::default();
    let worker = runtime.spawn_root(source("worker", 0));
    let traced = source("profiled", 1);
    let wrong_arity = source("profiled", 2);
    runtime.enable_function_call_count(traced.clone());
    runtime.enable_function_call_time(traced.clone());
    runtime.enable_function_call_memory(traced.clone());

    runtime
        .record_function_entries(&traced, 4)
        .expect("record session call count");
    runtime
        .record_function_time(&traced, worker, 4, 19)
        .expect("record session logical time");
    runtime
        .record_function_allocations(&traced, worker, 4, 512)
        .expect("record session logical memory");
    assert!(!runtime
        .record_function_time(&wrong_arity, worker, 1, 1)
        .expect("wrong arity time is not enabled"));
    assert!(!runtime
        .record_function_allocations(&wrong_arity, worker, 1, 1)
        .expect("wrong arity memory is not enabled"));

    let mut observed_stack = Vec::new();
    runtime
        .run_next(|process, _| {
            process.set_current_location(source("top", 1), 5);
            process
                .enter_execution_frame(source("middle", 1), 7, 11)
                .expect("enter middle frame");
            process
                .enter_execution_frame(source("bottom", 1), 13, 17)
                .expect("enter bottom frame");
            observed_stack = process.current_stacktrace();
            process.pop_execution_frame().expect("return to middle");
            assert_eq!(process.current_location().source, source("middle", 1));
            process.pop_execution_frame().expect("return to top");
            assert_eq!(process.current_location().source, source("top", 1));
            VmSchedulerDecision::Block { reductions: 3 }
        })
        .expect("run profiled stack");
    assert_eq!(observed_stack.len(), 3);
    assert_eq!(observed_stack[0].source, source("bottom", 1));
    assert_eq!(observed_stack[1].source, source("middle", 1));
    assert_eq!(observed_stack[2].source, source("top", 1));

    runtime
        .pause_function_call_time(&traced)
        .expect("pause time profile independently");
    assert!(!runtime
        .record_function_time(&traced, worker, 2, 9)
        .expect("paused time is a no-op"));
    assert!(runtime
        .record_function_entries(&traced, 2)
        .expect("call count remains active"));
    assert!(runtime
        .record_function_allocations(&traced, worker, 2, 128)
        .expect("memory profile remains active"));
    let VmCallCountState::Active { count } = runtime.function_call_count_state(&traced) else {
        panic!("call count must remain active")
    };
    assert_eq!(count, 6);
    let VmCallTimeState::Paused { processes: timed } = runtime.function_call_time_state(&traced)
    else {
        panic!("time profile must be paused")
    };
    assert_eq!(timed[0].calls, 4);
    assert_eq!(timed[0].exclusive_ticks, 19);
    let VmCallMemoryState::Enabled { processes: memory } =
        runtime.function_call_memory_state(&traced)
    else {
        panic!("memory profile must remain enabled")
    };
    assert_eq!(memory[0].calls, 6);
    assert_eq!(memory[0].allocated_bytes, 640);

    runtime
        .exit_actor(worker, VmExitReason::Normal)
        .expect("exit profiled actor");
    assert_eq!(
        runtime.function_call_time_state(&traced),
        VmCallTimeState::Paused { processes: timed }
    );
    assert_eq!(
        runtime.function_call_memory_state(&traced),
        VmCallMemoryState::Enabled { processes: memory }
    );
    assert!(runtime.disable_function_call_count(&traced));
    assert!(runtime.disable_function_call_time(&traced));
    assert!(runtime.disable_function_call_memory(&traced));
    assert_eq!(
        runtime.function_call_count_state(&traced),
        VmCallCountState::Disabled
    );
    assert_eq!(
        runtime.function_call_time_state(&traced),
        VmCallTimeState::Disabled
    );
    assert_eq!(
        runtime.function_call_memory_state(&traced),
        VmCallMemoryState::Disabled
    );
    assert!(runtime.restart_function_call_time(&traced).is_err());
    assert!(runtime.restart_function_call_memory(&traced).is_err());
}
