//! Full-cycle evidence for native WebSocket lifecycle callbacks.

use std::fs;
use std::sync::Arc;

use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::ReplValue;
use crate::support::test_fs;
use crate::{ColorChoice, DiagnosticFormat};
use terlan_http_native::routing::{RouteMethod, RouteTarget, RouterOutcome};

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
pub bytes_inbound(_frame: String): Unit ->
    let _wake = Process.receive_bytes();
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
    WebSocketEndpointPlan<ReplValue>,
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
    let image = crate::commands::build::vm_artifact::native_image::compile_serve_native_image(
        &web_root,
        "app_SocketCallbacks",
        &artifacts.core,
    )
    .expect("compile callback native image")
    .expect("callbacks produce native image");
    let runtime = AotHandlerRuntime::load("app.SocketCallbacks".to_string(), &image, None)
        .expect("load callback runtime");
    let router = runtime
        .execute_http_router("app.SocketCallbacks", "router", &mut |_| {})
        .unwrap();
    let RouterOutcome::Matched(route) = router.dispatch(RouteMethod::Get, "/socket").unwrap()
    else {
        panic!("expected WebSocket route");
    };
    let RouteTarget::WebSocketEndpoint(endpoint) = route.target else {
        panic!("WebSocket endpoint")
    };
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
    let mut session = crate::commands::serve::handler::websocket_invocation::open(
        Arc::clone(&runtime),
        "app.SocketCallbacks".to_string(),
        Session::open(endpoint.clone()),
    )
    .expect("dispatch open callback");
    assert_eq!(
        session.executor().completed_events(),
        &[AotWebSocketCallbackEvent::Open]
    );

    let waiting = session
        .inbound("hello".to_string())
        .expect("dispatch inbound callback");
    let AotWebSocketCallbackState::Waiting(wait) = waiting else {
        panic!("inbound callback must park")
    };
    assert_eq!(wait.boundary_type(), &TvmBoundaryType::String);
    let error = session
        .writable()
        .expect_err("parallel callback must be rejected");
    assert!(
        error
            .to_string()
            .contains("error[serve.websocket.callback_busy]"),
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
        session.executor().completed_events(),
        &[
            AotWebSocketCallbackEvent::Open,
            AotWebSocketCallbackEvent::Inbound,
            AotWebSocketCallbackEvent::Writable,
            AotWebSocketCallbackEvent::Close,
        ]
    );

    let mut cancelled = crate::commands::serve::handler::websocket_invocation::open(
        Arc::clone(&runtime),
        "app.SocketCallbacks".to_string(),
        Session::open(endpoint.clone()),
    )
    .expect("dispatch second open callback");
    assert!(matches!(
        cancelled
            .inbound("pending".to_string())
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
        cancelled.executor().completed_events(),
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
    }
    .into_value();
    let terminal_plan = WebSocketEndpointPlan::new(4, 1024)
        .expect("terminal endpoint")
        .with_callbacks(callbacks)
        .expect("terminal callbacks");
    let mut terminal = crate::commands::serve::handler::websocket_invocation::open(
        runtime,
        "app.SocketCallbacks".to_string(),
        Session::open(terminal_plan),
    )
    .expect("open terminal callback session");
    let error = terminal
        .cancel("transport lost".to_string())
        .expect_err("terminal callback suspension must be cancelled");
    assert!(
        error
            .to_string()
            .contains("error[serve.websocket.terminal_wait]"),
        "{error}"
    );
    assert!(!terminal.is_open());

    fs::remove_dir_all(root).expect("cleanup callback fixture");
}

#[test]
fn websocket_incompatible_wakes_preserve_queued_frames_and_callback_owner() {
    let (root, runtime, endpoint) = runtime();
    let mut callbacks = endpoint.callbacks().unwrap().clone();
    callbacks.inbound = crate::runtime::vm::native_callable::VmNativeCallableRef {
        module: "app.SocketCallbacks".into(),
        function: "bytes_inbound".into(),
        arity: 1,
    }
    .into_value();
    let plan = WebSocketEndpointPlan::new(4, 1024)
        .unwrap()
        .with_callbacks(callbacks)
        .unwrap();
    let mut session = crate::commands::serve::handler::websocket_invocation::open(
        runtime,
        "app.SocketCallbacks".into(),
        Session::open(plan),
    )
    .unwrap();
    session.enqueue_inbound("first".into()).unwrap();
    session.enqueue_inbound("second".into()).unwrap();
    let AotWebSocketCallbackState::Waiting(wait) = session.inbound("park".into()).unwrap() else {
        panic!("bytes callback must park");
    };
    let before = session.inspect();
    for _ in 0..8 {
        for error in [
            session.enqueue_inbound("rejected".into()).unwrap_err(),
            session.dispatch_next_inbound_output().unwrap_err(),
            session
                .dispatch_next_paired_inbound_output(Some((
                    "state".into(),
                    1,
                    "first".into(),
                    "second".into(),
                )))
                .unwrap_err(),
        ] {
            assert!(
                error.to_string().contains("serve.websocket.wake_type"),
                "{error}"
            );
            assert_eq!(session.inspect(), before);
            assert!(session.is_waiting());
        }
    }
    completed(
        session
            .resume(wait.wake(ReplValue::Bytes([7].into())))
            .unwrap(),
    );
    assert_eq!(session.inspect().pending_frames, 2);
    completed(session.close().unwrap());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn package_websocket_connection_executes_compiled_source_and_cancels_parked_code() {
    use std::future::{pending, Future};
    use std::io::Cursor;
    use std::task::{Context, Poll, Waker};
    use terlan_http_native::websocket::{connection, hub::WebSocketHub, Message};
    use tungstenite::protocol::{Role, WebSocket};

    let (root, runtime, endpoint) = runtime();
    for failure in [false, true] {
        let mut session = crate::commands::serve::handler::websocket_invocation::open(
            Arc::clone(&runtime),
            "app.SocketCallbacks".into(),
            Session::open(endpoint.clone()),
        )
        .unwrap();
        let mut client = WebSocket::from_raw_socket(Cursor::new(Vec::new()), Role::Client, None);
        client.send(Message::text("start source callback")).unwrap();
        if failure {
            client.send(Message::binary(vec![1])).unwrap();
        } else {
            client
                .send(Message::text("wake parked source callback"))
                .unwrap();
            client.close(None).unwrap();
        }
        let hub = Arc::new(WebSocketHub::default());
        let io = Cursor::new(client.into_inner().into_inner());
        let mut future = Box::pin(connection::serve(
            std::future::ready(Ok(io)),
            &mut session,
            &hub,
            "/socket".into(),
            "/socket".into(),
            pending,
        ));
        let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("complete input must not park transport");
        };
        drop(future);
        if failure {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("serve.websocket.binary"));
            assert_eq!(
                session.executor().completed_events(),
                &[
                    AotWebSocketCallbackEvent::Open,
                    AotWebSocketCallbackEvent::Writable,
                    AotWebSocketCallbackEvent::Cancellation
                ]
            );
        } else {
            result.unwrap();
            assert_eq!(
                session.executor().completed_events(),
                &[
                    AotWebSocketCallbackEvent::Open,
                    AotWebSocketCallbackEvent::Writable,
                    AotWebSocketCallbackEvent::Inbound,
                    AotWebSocketCallbackEvent::Close
                ]
            );
        }
        assert!(!session.is_open());
        assert!(!session.is_waiting());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn package_upgrade_rejection_executes_source_cancellation_and_reports_cleanup_failure() {
    let (root, runtime, endpoint) = runtime();
    for terminal_wait in [false, true] {
        let mut callbacks = endpoint.callbacks().unwrap().clone();
        if terminal_wait {
            callbacks.cancellation = crate::runtime::vm::native_callable::VmNativeCallableRef {
                module: "app.SocketCallbacks".into(),
                function: "terminal_cancelled".into(),
                arity: 1,
            }
            .into_value();
        }
        let plan = WebSocketEndpointPlan::new(4, 1024)
            .unwrap()
            .with_callbacks(callbacks)
            .unwrap();
        let mut session = crate::commands::serve::handler::websocket_invocation::open(
            Arc::clone(&runtime),
            "app.SocketCallbacks".into(),
            Session::open(plan),
        )
        .unwrap();
        assert!(matches!(
            session.inbound("parked".into()).unwrap(),
            AotWebSocketCallbackState::Waiting(_)
        ));
        let error = terlan_http_native::websocket::upgrade::admit(
            None,
            None,
            session,
            "/socket".into(),
            "/socket".into(),
        )
        .unwrap_err();
        assert_eq!(error.status(), 501);
        assert_eq!(error.code(), "serve_http.upgrade_adapter_missing");
        assert_eq!(
            error.message().contains("cancellation failed"),
            terminal_wait
        );
        assert_eq!(
            error.message().contains("serve.websocket.terminal_wait"),
            terminal_wait
        );
    }
    fs::remove_dir_all(root).unwrap();
}
