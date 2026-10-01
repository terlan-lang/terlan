//! Full-cycle evidence for native SSE lifecycle callbacks.

use std::fs;
use std::sync::Arc;

use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::http_router::{VmHttpRouteMethod, VmHttpRouteTarget, VmHttpRouterOutcome};
use crate::runtime::vm::ReplValue;
use crate::support::test_fs;
use crate::{ColorChoice, DiagnosticFormat};

use super::*;

const SOURCE: &str = r#"module app.SseCallbacks.

import std.core.Unit.
import std.http.{Router, Sse, Response}.
import std.http.Router.{Respond, MiddlewareResult}.
import std.vm.Process.
import type std.http.Router.
import type std.http.Request.

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
pub bytes_event(_data: String): Unit ->
    let _wake = Process.receive_bytes();
    Unit.
pub waiting_open(): Unit ->
    let _wake = Process.receive_string();
    Unit.
pub deny(_request: Request): MiddlewareResult -> Respond(Response.text("denied", 401)).
pub router(): Router ->
    Router.new().sse(
        "/events",
        Sse.endpoint_with_keep_alive(4, 1024, 15000)
            .callbacks(opened, event_ready, keep_alive, drained, cancelled)
    ).sse("/unsupported", Sse.endpoint().callbacks(
        waiting_open, event_ready, keep_alive, drained, cancelled
    )).group("/private", (child: Router) ->
        child.use(deny)
            .sse("/events", Sse.endpoint().callbacks(
                waiting_open, event_ready, keep_alive, drained, cancelled
            ))
    ).
"#;

/// Compiles one real source router and admits its generated callback image.
fn runtime() -> (
    std::path::PathBuf,
    Arc<AotHandlerRuntime>,
    SseEndpointPlan<ReplValue>,
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
    let image = crate::commands::build::vm_artifact::native_image::compile_serve_native_image(
        &web_root,
        "app_SseCallbacks",
        &artifacts.core,
    )
    .expect("compile callback native image")
    .expect("callbacks produce native image");
    let runtime = AotHandlerRuntime::load("app.SseCallbacks".to_string(), &image, None)
        .expect("load callback runtime");
    let router = runtime
        .execute_http_router("app.SseCallbacks", "router", &mut |_| {})
        .unwrap();
    let VmHttpRouterOutcome::Matched(route) =
        router.dispatch(VmHttpRouteMethod::Get, "/events").unwrap()
    else {
        panic!("expected SSE route");
    };
    let VmHttpRouteTarget::SseEndpoint(endpoint) = route.target else {
        panic!("SSE endpoint")
    };
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
        SseSession::open(endpoint.clone()),
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
        SseSession::open(endpoint.clone()),
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
    }
    .into_value();
    let terminal_plan = SseEndpointPlan::new(4, 1024)
        .expect("terminal endpoint")
        .with_callbacks(callbacks)
        .expect("terminal callbacks");
    let mut terminal = AotSseCallbackSession::open(
        runtime,
        "app.SseCallbacks".to_string(),
        SseSession::open(terminal_plan),
    )
    .expect("open terminal callback session");
    let error = terminal
        .drain()
        .expect_err("terminal callback suspension must be cancelled");
    assert!(error.contains("error[serve.sse.terminal_wait]"), "{error}");
    assert!(!terminal.is_open());

    fs::remove_dir_all(root).expect("cleanup callback fixture");
}

#[test]
fn incompatible_sse_wake_preserves_the_queue_and_pending_callback() {
    let (root, runtime, endpoint) = runtime();
    let mut callbacks = endpoint.callbacks().unwrap().clone();
    callbacks.event_ready = crate::runtime::vm::native_callable::VmNativeCallableRef {
        module: "app.SseCallbacks".into(),
        function: "bytes_event".into(),
        arity: 1,
    }
    .into_value();
    let plan = SseEndpointPlan::new(4, 1024)
        .unwrap()
        .with_callbacks(callbacks)
        .unwrap();
    let mut session =
        AotSseCallbackSession::open(runtime, "app.SseCallbacks".into(), SseSession::open(plan))
            .unwrap();
    let AotSseCallbackState::Waiting(wait) = session.enqueue_event("first".into()).unwrap() else {
        panic!("bytes callback must park")
    };
    assert_eq!(wait.boundary_type(), &TvmBoundaryType::Bytes);
    let before = session.inspect();
    for _ in 0..8 {
        let error = session.enqueue_event("rejected".into()).unwrap_err();
        assert!(error.contains("serve.sse.wake_type"), "{error}");
        assert_eq!(session.inspect(), before);
        assert!(session.is_waiting());
    }
    completed(
        session
            .resume(wait.wake(ReplValue::Bytes([7].into())))
            .unwrap(),
    );
    assert_eq!(session.flush_next_event().unwrap(), b"data: first\n\n");
    assert!(session.flush_next_event().is_none());
    completed(session.drain().unwrap());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unsupported_sse_transport_preserves_middleware_without_opening_a_stream() {
    use super::super::sse::{
        execute_vm_router_sse_admission_with_package_root, VmSseRouterAdmission,
    };
    use super::super::types::{WebPackageSourceSpan, WebPackageSse};

    let (root, runtime, _) = runtime();
    let admit = |path: &str, available| {
        let endpoint = WebPackageSse {
            module: "app.SseCallbacks".into(),
            route: path.into(),
            source: WebPackageSourceSpan {
                path: "src/app/SseCallbacks.terl".into(),
                line: 1,
                column: 1,
            },
        };
        execute_vm_router_sse_admission_with_package_root(
            Arc::clone(&runtime),
            &endpoint,
            &crate::terlan_native::http::Request::from_parts("GET", path, ""),
            &root,
            available,
            &mut |_| {},
        )
    };
    for (path, expected) in [
        ("/events", 501),
        ("/unsupported", 501),
        ("/private/events", 401),
    ] {
        let VmSseRouterAdmission::Respond(response) = admit(path, false).unwrap() else {
            panic!("unsupported transport must not allocate a stream")
        };
        assert_eq!(response.status, expected);
        if expected == 401 {
            assert_eq!(response.body.as_bytes(), b"denied");
        } else {
            assert!(String::from_utf8_lossy(response.body.as_bytes())
                .contains("serve_http.upgrade_adapter_missing"));
        }
    }
    // Available transports execute open and retain its actual typed wait.
    let VmSseRouterAdmission::Stream(mut waiting) = admit("/unsupported", true).unwrap() else {
        panic!("available transport should execute open")
    };
    assert!(waiting.is_waiting());
    assert!(waiting.completed_events().is_empty());
    completed(waiting.cancel("test completed".into()).unwrap());
    let VmSseRouterAdmission::Stream(session) = admit("/events", true).unwrap() else {
        panic!("available transport should admit the ordinary endpoint")
    };
    assert_eq!(session.completed_events(), &[AotSseCallbackEvent::Open]);
    assert!(admit("/missing", false)
        .unwrap_err()
        .contains("did not match"));
    fs::remove_dir_all(root).unwrap();
}
