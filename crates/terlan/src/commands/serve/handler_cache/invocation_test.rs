//! Full-cycle evidence for actors sharing one persistent handler shard.

use std::collections::BTreeMap;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::*;
use crate::commands::serve::handler::HandlerResponse;
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::actor_directory::VmActorLifecycle;
use crate::runtime::vm::debugger_control::{VmDebuggerControlCommand, VmDebuggerExecutionState};
use crate::runtime::vm::fixed_scheduler_telemetry::{
    VmFixedSchedulerEventKind, VM_FIXED_SCHEDULER_TRACE_CAPACITY,
};
use crate::runtime::vm::http_session::{VmHttpSessionRuntime, VmHttpSessionService};
use crate::runtime::vm::multicore_replay::VmMulticoreEventKind;
use crate::runtime::vm::scheduler::VmSchedulerClass;
use crate::runtime::vm::scheduler_topology::VmSchedulerId;

const SOURCE: &str = r#"module app.AsyncHandler.

import std.http.Response.
import std.io.File.
import std.vm.Process.
import type std.http.{Request, Response}.

pub delayed(_request: Request): Response ->
    Response.text(Process.receive_string()).

pub delayed_yield(_request: Request): Response ->
    let body = Process.receive_string();
    Process.yield_now();
    Response.text(body).

pub ready(): Bool ->
    true.

pub file_exists(path: String): Bool ->
    File.exists(path).

pub yielded(): Bool ->
    Process.yield_now();
    Process.yield_now();
    true.

pub timer_then_true(): Bool ->
    Process.sleep(Process.timer(20));
    true.

pub long_timer_then_true(): Bool ->
    Process.sleep(Process.timer(100));
    true.

pub stealable(): Bool ->
    Process.yield_now();
    true.

pub steal_wait(): String ->
    Process.yield_now();
    Process.receive_string().

pub priority_work(): Bool ->
    Process.schedule(Process.priority());
    Process.yield_now();
    Process.yield_now();
    true.

pub normal_work(): Bool ->
    Process.schedule(Process.normal());
    Process.yield_now();
    Process.yield_now();
    true.

pub background_work(): Bool ->
    Process.schedule(Process.background());
    Process.yield_now();
    Process.yield_now();
    true.
"#;

/// Compiles one real Terlan HTTP handler into an admitted native runtime.
fn runtime() -> (std::path::PathBuf, AotHandlerRuntime) {
    runtime_with_shards(None)
}

pub(super) fn runtime_with_shards(
    shard_count: Option<usize>,
) -> (std::path::PathBuf, AotHandlerRuntime) {
    let (root, image, router) = compiled_handler();
    let runtime = match shard_count {
        Some(shard_count) => AotHandlerRuntime::load_with_shard_count(
            "app.AsyncHandler".to_string(),
            &image,
            router,
            shard_count,
        ),
        None => AotHandlerRuntime::load("app.AsyncHandler".to_string(), &image, router),
    }
    .expect("load handler runtime");
    (root, runtime)
}

/// Compiles the generated-handler fixture without admitting a generation.
fn compiled_handler() -> (
    std::path::PathBuf,
    std::path::PathBuf,
    Option<crate::compiler::router::AotRouterPlan>,
) {
    let fixture = super::super::handler_cache_test_support::compile_native_handler_fixture(
        "aot_request_owned_invocation",
        "src/app/AsyncHandler.terl",
        "app_AsyncHandler",
        SOURCE,
    );
    (fixture.root, fixture.image, fixture.router)
}

/// A parked actor retains its owner shard while that same owner admits peers.
#[test]
fn one_owner_loop_services_multiple_parked_actors_without_migration() {
    let (root, runtime) = runtime_with_shards(Some(1));
    let first = waiting(&runtime);
    let first_shard = first.route.scheduler().index();
    let first_wake = first
        .wait()
        .expect("first wait")
        .wake(ReplValue::String("first".to_string()));

    let second = waiting(&runtime);
    assert_eq!(second.route.scheduler().index(), first_shard);
    let second_wake = second
        .wait()
        .expect("second wait")
        .wake(ReplValue::String("second".to_string()));

    assert!(matches!(
        second.resume(second_wake).expect("resume second actor"),
        AotHandlerInvocationStep::Complete(_)
    ));
    assert!(matches!(
        first.resume(first_wake).expect("resume first actor"),
        AotHandlerInvocationStep::Complete(_)
    ));
    assert_eq!(runtime.completed_call_count().expect("completed calls"), 2);
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Only new actors are balanced; a live invocation keeps its original shard.
#[test]
fn new_actors_balance_across_shards_and_resume_sticky() {
    let (root, runtime) = runtime_with_shards(Some(2));
    let first = waiting(&runtime);
    let first_shard = first.route.scheduler().index();
    let first_wake = first
        .wait()
        .expect("first wait")
        .wake(ReplValue::String("first".to_string()));
    let second = waiting(&runtime);
    let second_shard = second.route.scheduler().index();
    let second_wake = second
        .wait()
        .expect("second wait")
        .wake(ReplValue::String("second".to_string()));

    assert_ne!(first_shard, second_shard);
    assert!(matches!(
        first.resume(first_wake).expect("sticky first resume"),
        AotHandlerInvocationStep::Complete(_)
    ));
    assert!(matches!(
        second.resume(second_wake).expect("sticky second resume"),
        AotHandlerInvocationStep::Complete(_)
    ));
    assert_eq!(runtime.completed_call_count().expect("completed calls"), 2);
    assert_eq!(
        runtime.generation.shards[first_shard]
            .telemetry_snapshot()
            .io_completions,
        1
    );
    assert_eq!(
        runtime.generation.shards[second_shard]
            .telemetry_snapshot()
            .io_completions,
        1
    );
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// One real generated handler survives repeated explicit scheduler migration.
#[test]
fn parked_generated_handler_migrates_one_hundred_times_then_resumes_once() {
    let (root, runtime) = runtime_with_shards(Some(2));
    let mut invocation = waiting(&runtime);
    let home = invocation.route.home_scheduler().index();

    for _ in 0..100 {
        let destination = if invocation.route.scheduler().index() == home {
            1 - home
        } else {
            home
        };
        invocation = invocation
            .migrate_to_scheduler(destination)
            .expect("migrate parked generated handler");
        assert_eq!(invocation.route.home_scheduler().index(), home);
        assert_eq!(invocation.route.scheduler().index(), destination);
        assert_eq!(
            invocation.wait().expect("migrated wait epoch").epoch(),
            runtime.generation.shards[destination].shard_epoch()
        );
    }

    let wake = invocation
        .wait()
        .expect("migrated wait")
        .wake(ReplValue::String("migrated".to_string()));
    let result = invocation.resume(wake).expect("resume migrated handler");
    let AotHandlerInvocationStep::Complete(value) = result else {
        panic!("migrated handler did not return an HTTP response");
    };
    let response = HandlerResponse::from_vm_response_with_package_root(&value, &root)
        .expect("decode migrated response");
    assert_eq!(response.status, 200);
    assert_eq!(response.body.as_bytes(), b"migrated");
    assert_eq!(runtime.completed_call_count().expect("completed calls"), 1);
    assert!(runtime
        .generation
        .active_actors
        .iter()
        .all(|count| count.load(std::sync::atomic::Ordering::Relaxed) == 0));
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// One actor-owning scheduler panic retains bounded evidence and fails closed.
#[test]
fn scheduler_panic_fails_the_whole_handler_generation_closed() {
    let (root, runtime) = runtime_with_shards(Some(2));
    let route = runtime
        .generation
        .route_new_actor_on(VmSchedulerId::primary())
        .expect("route panic actor");
    runtime.generation.shards[0]
        .fill_panic_replay_pressure(VM_FIXED_SCHEDULER_TRACE_CAPACITY + 17)
        .expect("fill bounded panic trace");
    runtime.generation.shards[0]
        .panic_scheduler_while_owning(route)
        .expect("inject actor-owning scheduler panic");
    let failure = runtime.generation.shards[0]
        .shutdown()
        .expect_err("panicked scheduler must fail closed");
    assert!(failure.contains("scheduler 0 panicked"), "{failure}");

    let evidence = runtime.generation.shards[0]
        .panic_evidence()
        .expect("panic evidence lock")
        .expect("retained panic evidence");
    assert_eq!(evidence.scheduler, VmSchedulerId::primary());
    assert!(evidence.reason.contains("while owning actor"));
    assert_eq!(
        evidence.scheduler_replay.events.len(),
        VM_FIXED_SCHEDULER_TRACE_CAPACITY
    );
    assert_eq!(evidence.scheduler_replay.dropped_events, 22);
    assert!(!evidence.scheduler_replay.is_complete());
    let panic_event = evidence
        .scheduler_replay
        .events
        .last()
        .expect("terminal scheduler event");
    assert_eq!(
        panic_event.kind,
        VmFixedSchedulerEventKind::SchedulerPanicked
    );
    assert_eq!(panic_event.context.actor_id, Some(route.actor_id().get()));
    assert_eq!(panic_event.context.actor_generation, Some(1));
    assert_eq!(panic_event.context.owner_generation, Some(1));
    assert_eq!(panic_event.context.shard_epoch, Some(1));
    assert_eq!(panic_event.context.execution_interval, Some(1));
    let execution_started = evidence
        .scheduler_replay
        .events
        .iter()
        .rev()
        .find(|event| event.kind == VmFixedSchedulerEventKind::ExecutionStarted)
        .expect("pre-failure execution start");
    assert_eq!(execution_started.context, panic_event.context);
    assert!(!evidence.scheduler_replay.events.iter().any(|event| {
        event.kind == VmFixedSchedulerEventKind::ExecutionFinished
            && event.context.execution_interval == panic_event.context.execution_interval
    }));
    assert_eq!(
        evidence
            .shard_lifecycle
            .events
            .iter()
            .map(|event| (
                event.kind,
                event.context.shard_epoch,
                event.context.operation_sequence,
            ))
            .collect::<Vec<_>>(),
        vec![
            (VmMulticoreEventKind::ImageGeneration, Some(1), None),
            (VmMulticoreEventKind::SupervisionFailed, Some(1), Some(1),),
            (
                VmMulticoreEventKind::SupervisionRestartScheduled,
                Some(1),
                Some(1),
            ),
        ]
    );
    let generation_evidence = runtime
        .generation
        .multicore_replay_evidence()
        .expect("generation panic evidence");
    assert_eq!(generation_evidence.dropped_events, 22);
    assert!(!generation_evidence.replayable);
    assert_eq!(
        runtime
            .generation
            .scheduler_panic_evidence()
            .expect("generation panic artifacts"),
        vec![evidence]
    );

    let rejected = runtime
        .begin_request_invocation("app.AsyncHandler", "delayed", vec![request()])
        .expect_err("peer scheduler admission must close after shard failure");
    assert!(rejected.contains("scheduler_shard_failed"), "{rejected}");
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Two fixed schedulers overlap real generated AOT export execution.
#[test]
fn two_schedulers_overlap_generated_aot_execution_on_distinct_threads() {
    let (root, runtime) = runtime_with_shards(Some(2));
    let first = runtime.generation.route_new_actor().expect("first route");
    let second = runtime.generation.route_new_actor().expect("second route");
    assert_ne!(first.scheduler(), second.scheduler());
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let maximum = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let (first_result, second_result) = std::thread::scope(|scope| {
        let first_owner = runtime
            .generation
            .shard(first.scheduler().index())
            .expect("first scheduler");
        let second_owner = runtime
            .generation
            .shard(second.scheduler().index())
            .expect("second scheduler");
        let first_join = {
            let barrier = Arc::clone(&barrier);
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            scope.spawn(move || {
                first_owner.probe_execution(
                    first,
                    "app.AsyncHandler.ready".to_string(),
                    barrier,
                    active,
                    maximum,
                )
            })
        };
        let second_join = {
            let barrier = Arc::clone(&barrier);
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            scope.spawn(move || {
                second_owner.probe_execution(
                    second,
                    "app.AsyncHandler.ready".to_string(),
                    barrier,
                    active,
                    maximum,
                )
            })
        };
        (
            first_join.join().expect("first host thread"),
            second_join.join().expect("second host thread"),
        )
    });
    let (first_value, first_thread) = first_result.expect("first AOT execution");
    let (second_value, second_thread) = second_result.expect("second AOT execution");
    assert_eq!(first_value, ReplValue::Bool(true));
    assert_eq!(second_value, ReplValue::Bool(true));
    assert_ne!(first_thread, second_thread);
    assert_eq!(maximum.load(std::sync::atomic::Ordering::SeqCst), 2);
    for route in [first, second] {
        let owner = &runtime.generation.shards[route.scheduler().index()];
        let metrics = owner.telemetry_snapshot();
        assert_eq!(metrics.entries, 1);
        assert_eq!(metrics.completions, 1);
        let trace = owner.telemetry_trace().expect("scheduler trace");
        assert!(trace
            .windows(2)
            .all(|events| events[0].sequence < events[1].sequence));
        assert!(trace
            .iter()
            .all(|event| event.scheduler == route.scheduler()));
        assert!(trace
            .iter()
            .any(|event| event.kind == VmFixedSchedulerEventKind::Entry));
        assert!(trace
            .iter()
            .any(|event| event.kind == VmFixedSchedulerEventKind::Completed));
    }
    runtime
        .generation
        .release_actor_route(first.scheduler().index());
    runtime
        .generation
        .release_actor_route(second.scheduler().index());
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Generated yields become separate scheduler-owned runnable slices.
#[test]
fn generated_aot_yields_requeue_before_each_resume() {
    let (root, runtime) = runtime_with_shards(Some(1));
    let result = runtime
        .begin_request_invocation("app.AsyncHandler", "yielded", Vec::new())
        .expect("execute yielding handler");
    assert!(matches!(
        result,
        AotHandlerInvocationStep::Complete(ReplValue::Bool(true))
    ));
    assert_eq!(runtime.completed_call_count().expect("completed calls"), 1);

    let trace = runtime.generation.shards[0]
        .telemetry_trace()
        .expect("scheduler trace");
    assert_eq!(
        trace
            .iter()
            .filter(|event| event.kind == VmFixedSchedulerEventKind::Yielded)
            .count(),
        2
    );
    let capture = runtime.generation.shards[0]
        .telemetry_replay_capture()
        .expect("canonical scheduler capture");
    let evidence = runtime
        .generation
        .multicore_replay_evidence()
        .expect("generation replay evidence");
    assert_eq!(evidence.runtime_generation, runtime.generation.identity);
    assert_eq!(evidence.schedulers.len(), 1);
    assert_eq!(evidence.retained_events, capture.events.len());
    assert_eq!(evidence.dropped_events, 0);
    assert!(evidence.replayable);
    let starts = capture
        .events
        .iter()
        .filter(|event| event.kind == VmFixedSchedulerEventKind::ExecutionStarted)
        .map(|event| event.context)
        .collect::<Vec<_>>();
    let finishes = capture
        .events
        .iter()
        .filter(|event| event.kind == VmFixedSchedulerEventKind::ExecutionFinished)
        .map(|event| event.context)
        .collect::<Vec<_>>();
    assert_eq!(starts, finishes);
    assert_eq!(starts.len(), 3);
    assert!(starts.iter().all(|context| {
        context.actor_id.is_some()
            && context.actor_generation.is_some()
            && context.owner_generation.is_some()
            && context.execution_interval.is_some()
    }));
    assert!(starts
        .windows(2)
        .all(|pair| pair[0].execution_interval < pair[1].execution_interval));
    assert_eq!(
        trace
            .iter()
            .filter(|event| event.kind == VmFixedSchedulerEventKind::Resumed)
            .count(),
        2
    );
    let transitions = runtime
        .generation
        .scheduler_control
        .transition_events()
        .expect("actor transitions");
    assert_eq!(
        transitions
            .windows(3)
            .filter(|events| {
                events[0].to == VmActorLifecycle::Yielding
                    && events[1].to == VmActorLifecycle::Queued
                    && events[2].to == VmActorLifecycle::Executing
            })
            .count(),
        2
    );
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// A generated Timer parks and resumes only through its fixed scheduler event.
#[test]
fn generated_aot_timer_parks_until_scheduler_owned_deadline() {
    let (root, runtime) = runtime_with_shards(Some(1));
    let started = Instant::now();
    let result = runtime
        .begin_request_invocation("app.AsyncHandler", "timer_then_true", Vec::new())
        .expect("execute timer handler");
    assert!(matches!(
        result,
        AotHandlerInvocationStep::Complete(ReplValue::Bool(true))
    ));
    assert!(started.elapsed() >= Duration::from_millis(15));

    let trace = runtime.generation.shards[0]
        .telemetry_trace()
        .expect("scheduler trace");
    assert_eq!(runtime.generation.shards[0].telemetry_snapshot().timers, 1);
    let parked = trace
        .iter()
        .position(|event| event.kind == VmFixedSchedulerEventKind::Parked)
        .expect("timer park event");
    let published = trace
        .iter()
        .position(|event| event.kind == VmFixedSchedulerEventKind::TimerPublished)
        .expect("timer publication event");
    let dispatched = trace
        .iter()
        .position(|event| event.kind == VmFixedSchedulerEventKind::TimerDispatched)
        .expect("timer dispatch event");
    let completed = trace
        .iter()
        .position(|event| event.kind == VmFixedSchedulerEventKind::Completed)
        .expect("timer completion event");
    assert!(parked < published && published < dispatched && dispatched < completed);
    let capture = runtime.generation.shards[0]
        .telemetry_replay_capture()
        .expect("canonical scheduler capture");
    let published = capture
        .events
        .iter()
        .find(|event| event.kind == VmFixedSchedulerEventKind::TimerPublished)
        .expect("identified timer publication");
    let dispatched = capture
        .events
        .iter()
        .find(|event| event.kind == VmFixedSchedulerEventKind::TimerDispatched)
        .expect("identified timer dispatch");
    assert_eq!(published.context.actor_id, dispatched.context.actor_id);
    assert_eq!(
        published.context.actor_generation,
        dispatched.context.actor_generation
    );
    assert_eq!(
        published.context.operation_sequence,
        dispatched.context.operation_sequence
    );
    assert_eq!(published.context.owner_generation, None);
    assert!(dispatched.context.owner_generation.is_some());
    assert_eq!(
        published.context.shard_epoch,
        Some(runtime.generation.shards[0].shard_epoch().as_u64())
    );
    assert_eq!(
        published.context.shard_epoch,
        dispatched.context.shard_epoch
    );
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// A parked timer leaves its scheduler owner available for another actor.
#[test]
fn generated_aot_timer_does_not_block_peer_execution() {
    let (root, runtime) = runtime_with_shards(Some(1));
    let result = std::thread::scope(|scope| {
        let timer = scope.spawn(|| {
            runtime.begin_request_invocation("app.AsyncHandler", "long_timer_then_true", Vec::new())
        });
        wait_for_scheduler_event(&runtime, 0, VmFixedSchedulerEventKind::Parked);
        let peer = runtime
            .begin_request_invocation("app.AsyncHandler", "ready", Vec::new())
            .expect("execute peer while timer is parked");
        assert!(matches!(
            peer,
            AotHandlerInvocationStep::Complete(ReplValue::Bool(true))
        ));
        timer.join().expect("timer invocation thread")
    });
    assert!(matches!(
        result.expect("timer completion"),
        AotHandlerInvocationStep::Complete(ReplValue::Bool(true))
    ));
    assert_eq!(runtime.completed_call_count().expect("completed calls"), 2);
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Scheduler shutdown cancels a timer and settles its retained caller.
#[test]
fn generated_aot_timer_is_cancelled_by_scheduler_shutdown() {
    let (root, runtime) = runtime_with_shards(Some(1));
    let result = std::thread::scope(|scope| {
        let timer = scope.spawn(|| {
            runtime.begin_request_invocation("app.AsyncHandler", "long_timer_then_true", Vec::new())
        });
        wait_for_scheduler_event(&runtime, 0, VmFixedSchedulerEventKind::Parked);
        runtime.generation.shards[0]
            .shutdown()
            .expect("shutdown timer owner");
        timer.join().expect("timer invocation thread")
    });
    let error = result.expect_err("shutdown must cancel the parked timer");
    assert!(error.contains("scheduler_shutdown"), "{error}");
    assert!(runtime
        .generation
        .active_actors
        .iter()
        .all(|count| count.load(std::sync::atomic::Ordering::Relaxed) == 0));
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// A mailbox wake retains its reply while resumed code crosses a yield queue.
#[test]
fn resumed_generated_aot_actor_yields_before_replying() {
    let (root, runtime) = runtime_with_shards(Some(1));
    let invocation = match runtime
        .begin_request_invocation("app.AsyncHandler", "delayed_yield", vec![request()])
        .expect("park yielding handler")
    {
        AotHandlerInvocationStep::Waiting(invocation) => invocation,
        AotHandlerInvocationStep::Complete(_) => panic!("handler did not park"),
        AotHandlerInvocationStep::CapabilityWaiting(_) => {
            panic!("handler parked on an unexpected capability")
        }
    };
    let wake = invocation
        .wait()
        .expect("typed wait")
        .wake(ReplValue::String("queued".to_string()));
    let AotHandlerInvocationStep::Complete(value) =
        invocation.resume(wake).expect("resume through yield queue")
    else {
        panic!("handler parked twice")
    };
    let response = HandlerResponse::from_vm_response_with_package_root(&value, &root)
        .expect("decode response");
    assert_eq!(response.body.as_bytes(), b"queued");
    let trace = runtime.generation.shards[0]
        .telemetry_trace()
        .expect("scheduler trace");
    let dispatched = trace
        .iter()
        .position(|event| event.kind == VmFixedSchedulerEventKind::IoCompletionDispatched)
        .expect("message dispatch event");
    let yielded = trace
        .iter()
        .position(|event| event.kind == VmFixedSchedulerEventKind::Yielded)
        .expect("yield event");
    let resumed = trace
        .iter()
        .rposition(|event| event.kind == VmFixedSchedulerEventKind::Resumed)
        .expect("resume event");
    assert!(dispatched < yielded && yielded < resumed);
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Proves a worker result reaches generated code only through its fixed actor owner.
#[test]
fn generated_capability_completion_is_published_before_owner_dispatch() {
    let (root, runtime) = runtime_with_shards(Some(1));
    let invocation = match runtime
        .begin_request_invocation(
            "app.AsyncHandler",
            "file_exists",
            vec![ReplValue::String("/tmp/terlan-capability".to_string())],
        )
        .expect("park capability handler")
    {
        AotHandlerInvocationStep::CapabilityWaiting(invocation) => invocation,
        AotHandlerInvocationStep::Complete(value) => {
            panic!("capability handler completed early: {value:?}")
        }
        AotHandlerInvocationStep::Waiting(_) => panic!("capability handler parked on I/O"),
    };
    assert_eq!(
        invocation.request().expect("request").capability,
        "filesystem"
    );
    assert_eq!(
        invocation.request().expect("request").operation,
        "std.io.file.exists"
    );

    let completed = invocation
        .resume(
            crate::terlan_native_boundary::term::NativeBoundaryReplyTerm::Ok(
                crate::terlan_native_boundary::term::NativeBoundaryTerm::Bool(true),
            ),
        )
        .expect("resume capability handler");
    assert!(matches!(
        completed,
        AotHandlerInvocationStep::Complete(ReplValue::Bool(true))
    ));

    let owner = &runtime.generation.shards[0];
    assert_eq!(owner.telemetry_snapshot().capability_completions, 1);
    let trace = owner.telemetry_trace().expect("scheduler trace");
    let published = trace
        .iter()
        .position(|event| event.kind == VmFixedSchedulerEventKind::CapabilityCompletionPublished)
        .expect("published event");
    let dispatched = trace
        .iter()
        .position(|event| event.kind == VmFixedSchedulerEventKind::CapabilityCompletionDispatched)
        .expect("dispatched event");
    assert!(published < dispatched);
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Proves a packaged worker completes one generated call without caller dispatch code.
#[test]
#[ignore = "requires a built terlan-native-worker and the automatic pump test flag"]
fn generated_capability_event_pump_executes_real_worker_full_cycle() {
    assert!(
        std::env::var_os("TERLAN_TEST_AOT_CAPABILITY_PUMP").is_some(),
        "automatic capability pump must be enabled explicitly"
    );
    assert!(
        std::env::var_os("TERLAN_NATIVE_WORKER").is_some(),
        "test must identify the packaged capability worker"
    );
    assert!(
        std::env::var_os("TERLAN_TEST_CAPABILITY_NETWORK_SANDBOX").is_some(),
        "restricted test hosts must select the explicit network sandbox fixture"
    );
    let (root, runtime) = runtime_with_shards(Some(1));
    let completed = runtime
        .begin_request_invocation(
            "app.AsyncHandler",
            "file_exists",
            vec![ReplValue::String("/".to_string())],
        )
        .expect("execute capability handler through scheduler pump");
    assert!(matches!(
        completed,
        AotHandlerInvocationStep::Complete(ReplValue::Bool(true))
    ));

    let owner = &runtime.generation.shards[0];
    assert_eq!(owner.telemetry_snapshot().capability_completions, 1);
    let trace = owner.telemetry_trace().expect("scheduler trace");
    let published = trace
        .iter()
        .position(|event| event.kind == VmFixedSchedulerEventKind::CapabilityCompletionPublished)
        .expect("published event");
    let dispatched = trace
        .iter()
        .position(|event| event.kind == VmFixedSchedulerEventKind::CapabilityCompletionDispatched)
        .expect("dispatched event");
    assert!(published < dispatched);
    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Policy coordination transfers one queued continuation to an idle peer.
#[test]
fn generated_runnable_actor_is_stolen_between_scheduler_owners() {
    let (root, runtime) = runtime_with_shards(Some(2));
    runtime.generation.shards[0]
        .pause_runnable(true)
        .expect("pause source runnable service");

    let result = std::thread::scope(|scope| {
        let execution = scope.spawn(|| {
            runtime.begin_request_invocation("app.AsyncHandler", "stealable", Vec::new())
        });
        wait_for_scheduler_event(&runtime, 1, VmFixedSchedulerEventKind::Imported);
        execution.join().expect("generated handler thread")
    });

    assert!(matches!(
        result.expect("stolen handler result"),
        AotHandlerInvocationStep::Complete(ReplValue::Bool(true))
    ));
    let metrics = runtime.generation.work_metrics();
    assert!(metrics.steal_attempts >= 1, "{metrics:?}");
    assert!(metrics.transferred >= 1, "{metrics:?}");
    let source_trace = runtime.generation.shards[0]
        .telemetry_trace()
        .expect("source trace");
    let destination_trace = runtime.generation.shards[1]
        .telemetry_trace()
        .expect("destination trace");
    assert!(source_trace
        .iter()
        .any(|event| event.kind == VmFixedSchedulerEventKind::Stolen));
    assert!(destination_trace
        .iter()
        .any(|event| event.kind == VmFixedSchedulerEventKind::Imported));
    assert!(destination_trace
        .iter()
        .any(|event| event.kind == VmFixedSchedulerEventKind::Resumed));
    assert!(destination_trace
        .iter()
        .any(|event| event.kind == VmFixedSchedulerEventKind::Completed));
    let source_capture = runtime.generation.shards[0]
        .telemetry_replay_capture()
        .expect("source replay capture");
    let destination_capture = runtime.generation.shards[1]
        .telemetry_replay_capture()
        .expect("destination replay capture");
    let started = source_capture
        .events
        .iter()
        .find(|event| event.kind == VmFixedSchedulerEventKind::MigrationStarted)
        .expect("migration start");
    let completed = source_capture
        .events
        .iter()
        .find(|event| event.kind == VmFixedSchedulerEventKind::MigrationCompleted)
        .expect("migration completion");
    let stolen = source_capture
        .events
        .iter()
        .find(|event| event.kind == VmFixedSchedulerEventKind::Stolen)
        .expect("stolen actor");
    let imported = destination_capture
        .events
        .iter()
        .find(|event| event.kind == VmFixedSchedulerEventKind::Imported)
        .expect("imported actor");
    assert_eq!(started.context, completed.context);
    assert_eq!(completed.context, stolen.context);
    assert_eq!(started.context.actor_id, imported.context.actor_id);
    assert_eq!(
        started.context.actor_generation,
        imported.context.actor_generation
    );
    assert_eq!(
        started.context.owner_generation,
        imported.context.owner_generation
    );
    assert_eq!(started.context.peer_scheduler, Some(imported.scheduler));
    assert_eq!(imported.context.peer_scheduler, Some(started.scheduler));
    assert!(runtime
        .generation
        .active_actors
        .iter()
        .all(|count| count.load(std::sync::atomic::Ordering::Relaxed) == 0));
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Debugger pause survives runnable migration and each step runs one actor slice.
#[test]
fn debugger_pause_and_step_follow_owner_migration_without_duplicate_execution() {
    let (root, runtime) = runtime_with_shards(Some(2));
    let source = runtime.generation.shards[0].scheduler();
    for shard in &runtime.generation.shards {
        let snapshot = shard
            .debugger_control(VmDebuggerControlCommand::Pause)
            .expect("pause scheduler owner");
        assert_eq!(snapshot.state, VmDebuggerExecutionState::Paused);
    }
    let route = runtime
        .generation
        .route_new_actor_on(source)
        .expect("debug actor route");

    let result = std::thread::scope(|scope| {
        let execution = scope.spawn(|| {
            runtime.generation.shards[0].begin(
                route,
                "app.AsyncHandler.background_work".to_string(),
                Vec::new(),
                || {},
            )
        });
        wait_for_runnable_counts(&runtime, 0, [0, 0, 1]);
        let destination = runtime
            .generation
            .steal_one_runnable_in_class(0, 1, VmSchedulerClass::Background)
            .expect("migrate paused actor")
            .expect("paused actor is transferable");
        wait_for_runnable_counts(&runtime, 1, [0, 0, 1]);
        assert!(!execution.is_finished());

        let first = runtime.generation.shards[1]
            .debugger_control(VmDebuggerControlCommand::Step { slices: 1 })
            .expect("first debugger step");
        assert_eq!(first.state, VmDebuggerExecutionState::Stepping);
        wait_for_scheduler_event(&runtime, 1, VmFixedSchedulerEventKind::DebuggerStepped);
        wait_for_runnable_counts(&runtime, 1, [0, 0, 1]);
        assert!(!execution.is_finished());

        runtime.generation.shards[1]
            .debugger_control(VmDebuggerControlCommand::Step { slices: 1 })
            .expect("second debugger step");
        let result = execution.join().expect("debug actor thread");
        (destination, result)
    });

    let (
        destination,
        Ok(OwnedInvocationStep::Complete {
            route: completed_route,
            value,
        }),
    ) = result
    else {
        panic!("debug-stepped actor did not complete");
    };
    assert_eq!(completed_route, destination);
    assert_eq!(value, ReplValue::Bool(true));
    runtime
        .generation
        .release_actor_route(completed_route.scheduler().index());

    let source_capture = runtime.generation.shards[0]
        .telemetry_replay_capture()
        .expect("source debugger capture");
    let destination_capture = runtime.generation.shards[1]
        .telemetry_replay_capture()
        .expect("destination debugger capture");
    assert!(source_capture
        .events
        .iter()
        .any(|event| event.kind == VmFixedSchedulerEventKind::DebuggerPaused));
    let stepped = destination_capture
        .events
        .iter()
        .filter(|event| event.kind == VmFixedSchedulerEventKind::DebuggerStepped)
        .collect::<Vec<_>>();
    assert_eq!(stepped.len(), 2);
    assert!(stepped.iter().all(|event| {
        event.context.actor_id == Some(route.actor_id().get())
            && event.context.actor_generation.is_some()
            && event.context.owner_generation.is_some()
            && event.context.shard_epoch.is_some()
    }));
    assert!(runtime
        .generation
        .active_actors
        .iter()
        .all(|count| count.load(std::sync::atomic::Ordering::Relaxed) == 0));
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// An automatically stolen actor parks and receives its wake at destination.
#[test]
fn stolen_generated_actor_retains_destination_route_when_it_parks() {
    let (root, runtime) = runtime_with_shards(Some(2));
    runtime.generation.shards[0]
        .pause_runnable(true)
        .expect("pause source runnable service");

    let invocation = std::thread::scope(|scope| {
        let execution = scope.spawn(|| {
            runtime.begin_request_invocation("app.AsyncHandler", "steal_wait", Vec::new())
        });
        wait_for_scheduler_event(&runtime, 1, VmFixedSchedulerEventKind::Imported);
        match execution
            .join()
            .expect("generated handler thread")
            .expect("stolen handler wait")
        {
            AotHandlerInvocationStep::Waiting(invocation) => invocation,
            AotHandlerInvocationStep::Complete(_) => panic!("stolen handler did not park"),
            AotHandlerInvocationStep::CapabilityWaiting(_) => {
                panic!("stolen handler parked on an unexpected capability")
            }
        }
    });

    assert_eq!(invocation.route.home_scheduler().index(), 0);
    assert_eq!(invocation.route.scheduler().index(), 1);
    let wake = invocation
        .wait()
        .expect("destination wait")
        .wake(ReplValue::String("destination".to_string()));
    assert!(matches!(
        invocation.resume(wake).expect("destination resume"),
        AotHandlerInvocationStep::Complete(ReplValue::String(value)) if value == "destination"
    ));
    assert_eq!(
        runtime.generation.shards[1]
            .telemetry_snapshot()
            .io_completions,
        1
    );
    assert!(runtime
        .generation
        .active_actors
        .iter()
        .all(|count| count.load(std::sync::atomic::Ordering::Relaxed) == 0));
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Rejection rolls back the actor and drives bounded failed-steal backoff.
#[test]
fn rejected_generated_runnable_steal_rolls_back_without_actor_loss() {
    let (root, runtime) = runtime_with_shards(Some(2));
    runtime.generation.shards[0]
        .pause_runnable(true)
        .expect("pause source runnable service");
    runtime.generation.shards[1]
        .reject_runnable_imports(true)
        .expect("reject destination imports");

    let result = std::thread::scope(|scope| {
        let execution = scope.spawn(|| {
            runtime.begin_request_invocation("app.AsyncHandler", "stealable", Vec::new())
        });
        wait_for_work_metrics(&runtime, |metrics| {
            metrics.failed_steals >= 1 && metrics.backoff_directives >= 1
        });
        runtime.generation.shards[1]
            .reject_runnable_imports(false)
            .expect("restore destination imports");
        wait_for_scheduler_event(&runtime, 1, VmFixedSchedulerEventKind::Imported);
        execution.join().expect("generated handler thread")
    });

    assert!(matches!(
        result.expect("rolled-back handler result"),
        AotHandlerInvocationStep::Complete(ReplValue::Bool(true))
    ));
    let source_trace = runtime.generation.shards[0]
        .telemetry_trace()
        .expect("source trace");
    let destination_trace = runtime.generation.shards[1]
        .telemetry_trace()
        .expect("destination trace");
    assert!(source_trace
        .iter()
        .any(|event| event.kind == VmFixedSchedulerEventKind::Stolen));
    assert!(source_trace
        .iter()
        .any(|event| event.kind == VmFixedSchedulerEventKind::Imported));
    assert!(destination_trace
        .iter()
        .any(|event| event.kind == VmFixedSchedulerEventKind::Imported));
    let metrics = runtime.generation.work_metrics();
    assert!(metrics.failed_steals >= 1, "{metrics:?}");
    assert!(metrics.backoff_directives >= 1, "{metrics:?}");
    assert!(metrics.transferred >= 1, "{metrics:?}");
    assert!(runtime
        .generation
        .active_actors
        .iter()
        .all(|count| count.load(std::sync::atomic::Ordering::Relaxed) == 0));
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Generated owner service follows the canonical 3:2:1 class cycle.
#[test]
fn generated_runnable_classes_receive_weighted_local_service() {
    let (root, runtime) = runtime_with_shards(Some(1));
    let scheduler = runtime.generation.shards[0].scheduler();
    runtime.generation.shards[0]
        .pause_runnable(true)
        .expect("pause weighted service");
    let work = [
        ("priority_work", VmSchedulerClass::Priority),
        ("priority_work", VmSchedulerClass::Priority),
        ("priority_work", VmSchedulerClass::Priority),
        ("normal_work", VmSchedulerClass::Normal),
        ("normal_work", VmSchedulerClass::Normal),
        ("background_work", VmSchedulerClass::Background),
    ];
    let routes = work
        .iter()
        .map(|_| {
            runtime
                .generation
                .route_new_actor_on(scheduler)
                .expect("weighted actor route")
        })
        .collect::<Vec<_>>();
    let actor_classes = routes
        .iter()
        .zip(work.iter())
        .map(|(route, (_, class))| (route.actor_id().get(), *class))
        .collect::<BTreeMap<_, _>>();

    let results = std::thread::scope(|scope| {
        let runtime = &runtime;
        let handles = routes
            .iter()
            .copied()
            .zip(work.iter().map(|(export, _)| *export))
            .map(|(route, export)| {
                scope.spawn(move || {
                    runtime.generation.shards[0].begin(
                        route,
                        format!("app.AsyncHandler.{export}"),
                        Vec::new(),
                        || {},
                    )
                })
            })
            .collect::<Vec<_>>();
        wait_for_runnable_counts(&runtime, 0, [3, 2, 1]);
        runtime.generation.shards[0]
            .pause_runnable(false)
            .expect("resume weighted service");
        handles
            .into_iter()
            .map(|handle| handle.join().expect("weighted actor thread"))
            .collect::<Vec<_>>()
    });

    for result in results {
        let OwnedInvocationStep::Complete { route, value } =
            result.expect("weighted actor completion")
        else {
            panic!("weighted actor parked unexpectedly");
        };
        assert_eq!(value, ReplValue::Bool(true));
        runtime
            .generation
            .release_actor_route(route.scheduler().index());
    }
    let completed_classes = runtime.generation.shards[0]
        .telemetry_trace()
        .expect("weighted trace")
        .into_iter()
        .filter(|event| event.kind == VmFixedSchedulerEventKind::Completed)
        .map(|event| {
            actor_classes[&event
                .actor_id
                .expect("completed weighted event has actor identity")]
        })
        .collect::<Vec<_>>();
    assert_eq!(
        completed_classes,
        [
            VmSchedulerClass::Priority,
            VmSchedulerClass::Priority,
            VmSchedulerClass::Normal,
            VmSchedulerClass::Priority,
            VmSchedulerClass::Normal,
            VmSchedulerClass::Background,
        ]
    );
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Concurrent skew and class mutations remain work-conserving across owners.
#[test]
fn generated_multicore_fanout_completes_under_adversarial_class_skew() {
    const ACTORS: usize = 48;

    let (root, runtime) = runtime_with_shards(Some(4));
    for scheduler in [0, 2] {
        runtime.generation.shards[scheduler]
            .pause_runnable(true)
            .expect("pause skewed owner");
    }
    let completed = AtomicBool::new(false);
    let results = std::thread::scope(|scope| {
        let runtime = &runtime;
        let completed = &completed;
        let watchdog = scope.spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !completed.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            if !completed.load(Ordering::Acquire) {
                for scheduler in [0, 2] {
                    runtime.generation.shards[scheduler]
                        .pause_runnable(false)
                        .expect("watchdog releases skewed owner");
                }
            }
        });
        let handles = (0..ACTORS)
            .map(|index| {
                let runtime = runtime;
                let export = match index.wrapping_mul(17).wrapping_add(11) % 6 {
                    0..=2 => "priority_work",
                    3..=4 => "normal_work",
                    _ => "background_work",
                };
                scope.spawn(move || {
                    runtime.begin_request_invocation("app.AsyncHandler", export, Vec::new())
                })
            })
            .collect::<Vec<_>>();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().expect("fanout actor thread"))
            .collect::<Vec<_>>();
        completed.store(true, Ordering::Release);
        watchdog.join().expect("fanout watchdog");
        results
    });
    for scheduler in [0, 2] {
        runtime.generation.shards[scheduler]
            .pause_runnable(false)
            .expect("restore skewed owner");
    }
    assert_eq!(results.len(), ACTORS);
    assert!(results.into_iter().all(|result| matches!(
        result,
        Ok(AotHandlerInvocationStep::Complete(ReplValue::Bool(true)))
    )));
    let metrics = runtime.generation.work_metrics();
    assert!(metrics.steal_attempts > 0, "{metrics:?}");
    assert!(metrics.transferred > 0, "{metrics:?}");
    assert!(metrics.priority_transferred > 0, "{metrics:?}");
    assert!(metrics.normal_transferred > 0, "{metrics:?}");
    assert!(metrics.background_transferred > 0, "{metrics:?}");
    assert_eq!(
        runtime.completed_call_count().expect("fanout completions"),
        ACTORS as u64
    );
    assert!(runtime
        .generation
        .active_actors
        .iter()
        .all(|count| count.load(std::sync::atomic::Ordering::Relaxed) == 0));
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Shutdown cancels queued background work and releases its actor reservation.
#[test]
fn generated_runnable_shutdown_reclaims_queued_class_work() {
    let (root, runtime) = runtime_with_shards(Some(1));
    runtime.generation.shards[0]
        .pause_runnable(true)
        .expect("pause shutdown owner");

    let result = std::thread::scope(|scope| {
        let execution = scope.spawn(|| {
            runtime.begin_request_invocation("app.AsyncHandler", "background_work", Vec::new())
        });
        wait_for_runnable_counts(&runtime, 0, [0, 0, 1]);
        runtime.generation.shards[0]
            .shutdown()
            .expect("shutdown queued owner");
        execution.join().expect("shutdown actor thread")
    });
    let error = result.expect_err("queued actor must be cancelled by shutdown");
    assert!(error.contains("scheduler_shutdown"), "{error}");
    assert!(runtime
        .generation
        .active_actors
        .iter()
        .all(|count| count.load(std::sync::atomic::Ordering::Relaxed) == 0));
    let replay = runtime.generation.shards[0]
        .telemetry_replay_capture()
        .expect("shutdown scheduler replay");
    let shutdown = replay
        .events
        .iter()
        .find(|event| event.kind == VmFixedSchedulerEventKind::Shutdown)
        .expect("orderly scheduler shutdown event");
    assert_eq!(shutdown.context.shard_epoch, Some(1));
    assert_eq!(shutdown.context.actor_id, None);
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Waits for one exact priority/normal/background generated queue shape.
fn wait_for_runnable_counts(runtime: &AotHandlerRuntime, scheduler: usize, expected: [usize; 3]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let snapshot = runtime.generation.shards[scheduler]
            .runnable_snapshot()
            .expect("generated runnable snapshot");
        let actual = [
            snapshot.runnable_in(VmSchedulerClass::Priority),
            snapshot.runnable_in(VmSchedulerClass::Normal),
            snapshot.runnable_in(VmSchedulerClass::Background),
        ];
        if actual == expected {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    for shard in &runtime.generation.shards {
        let _ = shard.pause_runnable(false);
    }
    panic!("scheduler {scheduler} did not publish runnable counts {expected:?}");
}

/// Waits until one owner records a deterministic scheduling boundary.
fn wait_for_scheduler_event(
    runtime: &AotHandlerRuntime,
    scheduler: usize,
    kind: VmFixedSchedulerEventKind,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if runtime.generation.shards[scheduler]
            .telemetry_trace()
            .expect("scheduler trace")
            .iter()
            .any(|event| event.kind == kind)
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    for shard in &runtime.generation.shards {
        shard
            .pause_runnable(false)
            .expect("release timed-out scheduler");
    }
    panic!("scheduler {scheduler} did not record {kind:?}");
}

/// Waits until automatic queue coordination publishes expected counters.
fn wait_for_work_metrics(
    runtime: &AotHandlerRuntime,
    predicate: impl Fn(super::super::AotGeneratedWorkMetricsSnapshot) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if predicate(runtime.generation.work_metrics()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    runtime.generation.shards[1]
        .reject_runnable_imports(false)
        .expect("release timed-out destination");
    runtime.generation.shards[0]
        .pause_runnable(false)
        .expect("release timed-out source");
    panic!(
        "generated queue coordination did not reach expected metrics: {:?}",
        runtime.generation.work_metrics()
    );
}

/// Partial startup closes and joins already-started scheduler owners.
#[test]
fn partial_scheduler_startup_is_rolled_back_without_live_generation() {
    let (root, image, _router) = compiled_handler();
    let sessions = VmHttpSessionService::new(
        VmHttpSessionRuntime::new("terlc-serve-startup-failure", 86_400).expect("session runtime"),
    );
    let error = AotHandlerGeneration::load_with_start_failure(&image, sessions, 2, 1)
        .expect_err("second scheduler startup must fail");
    assert!(error.contains("shard_start_injected"), "{error}");
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Cancellation crosses the shared directory as a VM system signal.
#[test]
fn remote_cancellation_is_traced_as_signal_not_actor_message() {
    let (root, runtime) = runtime_with_shards(Some(2));
    let invocation = waiting(&runtime);
    let scheduler = invocation.route.scheduler().index();
    invocation
        .cancel("test cancellation".to_string())
        .expect("cancel parked actor");
    let metrics = runtime.generation.shards[scheduler].telemetry_snapshot();
    assert_eq!(metrics.signals, 1);
    assert_eq!(metrics.io_completions, 0);
    let trace = runtime.generation.shards[scheduler]
        .telemetry_trace()
        .expect("scheduler trace");
    assert!(trace
        .iter()
        .any(|event| event.kind == VmFixedSchedulerEventKind::SignalPublished));
    assert!(trace
        .iter()
        .any(|event| event.kind == VmFixedSchedulerEventKind::SignalDispatched));
    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}

/// Builds one managed request argument accepted by the generated handler.
fn request() -> ReplValue {
    let empty_map = || ReplValue::Map(Vec::new());
    ReplValue::Tuple(vec![
        ReplValue::Int(0),
        ReplValue::String("GET".to_string()),
        ReplValue::String("/delayed:".to_string()),
        empty_map(),
        ReplValue::String(String::new()),
        ReplValue::String(String::new()),
        empty_map(),
        empty_map(),
        empty_map(),
        ReplValue::Tuple(vec![empty_map(), ReplValue::List(Vec::new())]),
    ])
}

/// Starts one generated request and requires it to park on typed string I/O.
pub(super) fn waiting(runtime: &AotHandlerRuntime) -> AotHandlerInvocation {
    match runtime
        .begin_request_invocation("app.AsyncHandler", "delayed", vec![request()])
        .expect("enter generated handler")
    {
        AotHandlerInvocationStep::Waiting(invocation) => invocation,
        AotHandlerInvocationStep::Complete(value) => {
            panic!("handler completed before I/O wake: {value:?}")
        }
        AotHandlerInvocationStep::CapabilityWaiting(_) => {
            panic!("handler parked on an unexpected capability")
        }
    }
}

/// Proves exact typed wake ownership from generated handler entry to response.
#[test]
fn persistent_shard_actors_resume_only_from_exact_typed_io_wake() {
    let (root, runtime) = runtime();
    assert_eq!(
        runtime.completed_call_count().expect("initial call count"),
        0
    );
    let first = waiting(&runtime);
    let first_wait = first.wait().expect("first typed wait");
    assert_eq!(first_wait.boundary_type(), &TvmBoundaryType::String);

    let second = waiting(&runtime);
    let stale = first_wait.wake(ReplValue::String("ready".to_string()));
    let error = second
        .resume(stale.clone())
        .expect_err("foreign request wake must fail");
    assert!(error.contains("error[pure_native_io.identity]"), "{error}");

    let completed = first.resume(stale).expect("resume exact request wake");
    let AotHandlerInvocationStep::Complete(value) = completed else {
        panic!("single I/O handler must complete after wake")
    };
    let response = HandlerResponse::from_vm_response_with_package_root(&value, &root)
        .expect("decode generated response");
    assert_eq!(response.status, 200);
    assert_eq!(response.body.as_bytes(), b"ready");
    assert_eq!(
        runtime
            .completed_call_count()
            .expect("persistent call count"),
        1,
        "the admitted generation must retain completed-call state across requests"
    );

    let wrong_type = waiting(&runtime);
    let wake = wrong_type
        .wait()
        .expect("typed wait")
        .wake(ReplValue::Int(7));
    let error = wrong_type
        .resume(wake)
        .expect_err("wrong typed payload must fail");
    assert!(error.contains("String"), "{error}");

    let error = runtime
        .execute_immediate_native("app.AsyncHandler", "delayed", vec![request()], &mut |_| {})
        .expect_err("immediate native callback must reject asynchronous I/O");
    assert!(
        error.contains("error[serve.aot.async_io_unavailable]"),
        "{error}"
    );
    assert_eq!(
        runtime.completed_call_count().expect("final call count"),
        1,
        "cancelled actors must not replace or complete the persistent shard"
    );

    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}
