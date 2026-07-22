//! Full-cycle evidence for native SSE lifecycle callbacks.

use std::fs;
use std::sync::Arc;

use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::compiler::router::AotRouterRouteTarget;
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::sse::{VmSseEndpointPlan, VmSseLiveSession};
use crate::runtime::vm::ReplValue;
use crate::support::test_fs;
use crate::{ColorChoice, DiagnosticFormat};

use super::*;

const SOURCE: &str = r#"module app.SseCallbacks.

import std.core.Unit.
import std.http.{Router, Sse}.
import std.vm.Process.
import type std.http.Router.

pub opened(): Unit -> Unit.
pub event_ready(_data: String): Unit ->
    let _wake = Process.receive_string();
    Unit.
pub keep_alive(): Unit -> Unit.
pub drained(): Unit -> Unit.
pub cancelled(_reason: String): Unit -> Unit.
pub terminal_drained(): Unit ->
    let _wake = Process.receive_string();
    Unit.
pub router(): Router ->
    Router.new().sse(
        "/events",
        Sse.endpoint_with_keep_alive(4, 1024, 15000)
            .callbacks(opened, event_ready, keep_alive, drained, cancelled)
    ).
"#;

/// Compiles one real source router and admits its generated callback image.
fn runtime() -> (
    std::path::PathBuf,
    Arc<AotHandlerRuntime>,
    VmSseEndpointPlan,
) {
    let root = test_fs::temp_path("serve", "aot_sse_callbacks");
    let web_root = root.join("_build/web");
    fs::create_dir_all(&web_root).expect("create callback output");
    let artifacts = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "src/app/SseCallbacks.terl",
        SOURCE,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        None,
        crate::validation::native_policy::NativePolicy::NativeBoundaryOptional,
        crate::validation::target_profile::TargetProfile::Vm,
    )
    .expect("compile callback source");
    let (core, router) = crate::compiler::router::prepare_aot_router_module(&artifacts.core)
        .expect("prepare callback router");
    let router = router.expect("callback router plan");
    let AotRouterRouteTarget::Sse(endpoint) = &router.routes[0].target else {
        panic!("expected SSE endpoint")
    };
    let endpoint = endpoint.clone();
    let image = crate::commands::build::vm_artifact::native_image::compile_serve_native_image(
        &web_root,
        "app_SseCallbacks",
        &core,
    )
    .expect("compile callback native image")
    .expect("callbacks produce native image");
    let runtime = AotHandlerRuntime::load("app.SseCallbacks".to_string(), &image, Some(router))
        .expect("load callback runtime");
    (root, Arc::new(runtime), endpoint)
}

/// Requires one callback to complete with the managed Unit value.
fn completed(state: AotSseCallbackState) {
    let AotSseCallbackState::Complete(value) = state else {
        panic!("callback unexpectedly parked")
    };
    assert_eq!(value, ReplValue::Unit);
}

/// Proves every SSE event uses shared entry/resume and linear cleanup.
#[test]
fn sse_callbacks_share_native_invocation_entry_resume_and_cancellation() {
    let (root, runtime, endpoint) = runtime();
    let mut session = AotSseCallbackSession::open(
        Arc::clone(&runtime),
        "app.SseCallbacks".to_string(),
        VmSseLiveSession::open(endpoint.clone()).expect("open SSE stream"),
    )
    .expect("dispatch open callback");
    assert_eq!(session.completed_events(), &[AotSseCallbackEvent::Open]);
    assert_eq!(session.plan().keep_alive_ms(), Some(15000));

    let waiting = session
        .event_ready("counter:1".to_string())
        .expect("dispatch event-ready callback");
    let AotSseCallbackState::Waiting(wait) = waiting else {
        panic!("event-ready callback must park")
    };
    assert_eq!(wait.boundary_type(), &TvmBoundaryType::String);
    let error = session
        .keep_alive()
        .expect_err("parallel callback must be rejected");
    assert!(error.contains("error[serve.sse.callback_busy]"), "{error}");
    completed(
        session
            .resume(wait.wake(ReplValue::String("ready".to_string())))
            .expect("resume event-ready callback"),
    );
    completed(session.keep_alive().expect("dispatch keep-alive callback"));
    completed(session.drain().expect("dispatch drain callback"));
    assert!(!session.is_open());
    assert_eq!(
        session.completed_events(),
        &[
            AotSseCallbackEvent::Open,
            AotSseCallbackEvent::EventReady,
            AotSseCallbackEvent::KeepAlive,
            AotSseCallbackEvent::Drain,
        ]
    );

    let mut cancelled = AotSseCallbackSession::open(
        Arc::clone(&runtime),
        "app.SseCallbacks".to_string(),
        VmSseLiveSession::open(endpoint.clone()).expect("open cancelled SSE stream"),
    )
    .expect("dispatch second open callback");
    assert!(matches!(
        cancelled
            .event_ready("pending".to_string())
            .expect("park second event callback"),
        AotSseCallbackState::Waiting(_)
    ));
    completed(
        cancelled
            .cancel("client disconnected".to_string())
            .expect("cancel parked callback and dispatch cancellation"),
    );
    assert!(!cancelled.is_open());
    assert_eq!(
        cancelled.completed_events(),
        &[AotSseCallbackEvent::Open, AotSseCallbackEvent::Cancellation,]
    );

    let mut callbacks = endpoint.callbacks().expect("SSE callbacks").clone();
    callbacks.drain = crate::runtime::vm::native_callable::VmNativeCallableRef {
        module: "app.SseCallbacks".to_string(),
        function: "terminal_drained".to_string(),
        arity: 0,
    };
    let terminal_plan = VmSseEndpointPlan::new(4, 1024)
        .expect("terminal endpoint")
        .with_callbacks(callbacks)
        .expect("terminal callbacks");
    let mut terminal = AotSseCallbackSession::open(
        runtime,
        "app.SseCallbacks".to_string(),
        VmSseLiveSession::open(terminal_plan).expect("open terminal stream"),
    )
    .expect("open terminal callback session");
    let error = terminal
        .drain()
        .expect_err("terminal callback suspension must be cancelled");
    assert!(error.contains("error[serve.sse.terminal_wait]"), "{error}");
    assert!(!terminal.is_open());

    fs::remove_dir_all(root).expect("cleanup callback fixture");
}
