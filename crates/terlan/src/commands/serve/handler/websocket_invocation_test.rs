//! Full-cycle evidence for native WebSocket lifecycle callbacks.

use std::fs;
use std::sync::Arc;

use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::aot_metadata::AotRouterRouteTarget;
use crate::runtime::vm::websocket::{VmWebSocketFrame, VmWebSocketLiveSession};
use crate::runtime::vm::ReplValue;
use crate::support::test_fs;
use crate::{ColorChoice, DiagnosticFormat};

use super::*;

const SOURCE: &str = r#"module app.SocketCallbacks.

import std.core.Unit.
import std.http.{Router, WebSocket}.
import std.vm.Process.
import type std.http.Router.
pub opened(): Unit -> Unit.
pub inbound(_frame: String): Unit ->
    let _wake = Process.receive_string();
    Unit.
pub writable(): Unit -> Unit.
pub closed(): Unit -> Unit.
pub cancelled(_reason: String): Unit -> Unit.
pub terminal_cancelled(_reason: String): Unit ->
    let _wake = Process.receive_string();
    Unit.
pub router(): Router ->
    Router.new().websocket(
        "/socket",
        WebSocket.endpoint(4, 1024).callbacks(opened, inbound, writable, closed, cancelled)
    ).
"#;

/// Compiles one real source router and admits its generated callback image.
fn runtime() -> (
    std::path::PathBuf,
    Arc<AotHandlerRuntime>,
    crate::runtime::vm::websocket::VmWebSocketEndpointPlan,
) {
    let root = test_fs::temp_path("serve", "aot_websocket_callbacks");
    let web_root = root.join("_build/web");
    fs::create_dir_all(&web_root).expect("create callback output");
    let artifacts = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "src/app/SocketCallbacks.terl",
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
    let AotRouterRouteTarget::WebSocket(endpoint) = &router.routes[0].target else {
        panic!("expected WebSocket endpoint")
    };
    let endpoint = endpoint.clone();
    let image = crate::commands::build::vm_artifact::native_image::compile_serve_native_image(
        &web_root,
        "app_SocketCallbacks",
        &core,
    )
    .expect("compile callback native image")
    .expect("callbacks produce native image");
    let runtime = AotHandlerRuntime::load("app.SocketCallbacks".to_string(), &image, Some(router))
        .expect("load callback runtime");
    (root, Arc::new(runtime), endpoint)
}

/// Requires one callback to complete with the managed Unit value.
fn completed(state: AotWebSocketCallbackState) {
    let AotWebSocketCallbackState::Complete(value) = state else {
        panic!("callback unexpectedly parked")
    };
    assert_eq!(value, ReplValue::Unit);
}

/// Proves every WebSocket event uses shared entry/resume and linear cleanup.
#[test]
fn websocket_callbacks_share_native_invocation_entry_resume_and_cancellation() {
    let (root, runtime, endpoint) = runtime();
    let mut session = AotWebSocketCallbackSession::open(
        Arc::clone(&runtime),
        "app.SocketCallbacks".to_string(),
        VmWebSocketLiveSession::open(endpoint.clone()),
    )
    .expect("dispatch open callback");
    assert_eq!(
        session.completed_events(),
        &[AotWebSocketCallbackEvent::Open]
    );

    let waiting = session
        .inbound(VmWebSocketFrame::Text("hello".to_string()))
        .expect("dispatch inbound callback");
    let AotWebSocketCallbackState::Waiting(wait) = waiting else {
        panic!("inbound callback must park")
    };
    assert_eq!(wait.boundary_type(), &TvmBoundaryType::String);
    let error = session
        .writable()
        .expect_err("parallel callback must be rejected");
    assert!(
        error.contains("error[serve.websocket.callback_busy]"),
        "{error}"
    );
    completed(
        session
            .resume(wait.wake(ReplValue::String("ready".to_string())))
            .expect("resume inbound callback"),
    );
    completed(session.writable().expect("dispatch writable callback"));
    completed(session.close().expect("dispatch close callback"));
    assert!(!session.is_open());
    assert_eq!(
        session.completed_events(),
        &[
            AotWebSocketCallbackEvent::Open,
            AotWebSocketCallbackEvent::Inbound,
            AotWebSocketCallbackEvent::Writable,
            AotWebSocketCallbackEvent::Close,
        ]
    );

    let mut cancelled = AotWebSocketCallbackSession::open(
        Arc::clone(&runtime),
        "app.SocketCallbacks".to_string(),
        VmWebSocketLiveSession::open(endpoint.clone()),
    )
    .expect("dispatch second open callback");
    assert!(matches!(
        cancelled
            .inbound(VmWebSocketFrame::Text("pending".to_string()))
            .expect("park second inbound callback"),
        AotWebSocketCallbackState::Waiting(_)
    ));
    completed(
        cancelled
            .cancel("transport lost".to_string())
            .expect("cancel parked callback and dispatch cancellation"),
    );
    assert!(!cancelled.is_open());
    assert_eq!(
        cancelled.completed_events(),
        &[
            AotWebSocketCallbackEvent::Open,
            AotWebSocketCallbackEvent::Cancellation,
        ]
    );

    let mut callbacks = endpoint.callbacks().expect("WebSocket callbacks").clone();
    callbacks.cancellation = crate::runtime::vm::native_callable::VmNativeCallableRef {
        module: "app.SocketCallbacks".to_string(),
        function: "terminal_cancelled".to_string(),
        arity: 1,
    };
    let terminal_plan = crate::runtime::vm::websocket::VmWebSocketEndpointPlan::new(4, 1024)
        .expect("terminal endpoint")
        .with_callbacks(callbacks)
        .expect("terminal callbacks");
    let mut terminal = AotWebSocketCallbackSession::open(
        runtime,
        "app.SocketCallbacks".to_string(),
        VmWebSocketLiveSession::open(terminal_plan),
    )
    .expect("open terminal callback session");
    let error = terminal
        .cancel("transport lost".to_string())
        .expect_err("terminal callback suspension must be cancelled");
    assert!(
        error.contains("error[serve.websocket.terminal_wait]"),
        "{error}"
    );
    assert!(!terminal.is_open());

    fs::remove_dir_all(root).expect("cleanup callback fixture");
}
