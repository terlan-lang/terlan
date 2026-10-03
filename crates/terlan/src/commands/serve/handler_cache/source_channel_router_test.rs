//! Channel builder semantics must execute from source, including imported callbacks.

use super::*;
use crate::commands::serve::handler_cache::{cached_source_entry, invalidate_vm_handler_cache};
use terlan_http_native::source_descriptor::paired_transition;

const MODULE: &str = "app.Channels";
const PROVIDER: &str = r#"module app.Callbacks.
import std.core.Int.{to_string}.
pub opened(): Unit -> Unit.
pub received(_data: String): Unit -> Unit.
pub writable(): Unit -> Unit.
pub closed(): Unit -> Unit.
pub cancelled(_reason: String): Unit -> Unit.
pub inbound(frame: String): String -> "provider:" + frame.
pub waiting(): String -> "waiting".
pub peer_left(): String -> "left".
pub matched(room: String, role: Int, first: String, second: String): String ->
    room + ":" + to_string(role) + ":" + first + ":" + second.
pub restored(room: String, state: String, role: Int, first: String, second: String): String ->
    state + ":" + matched(room, role, first, second).
pub transition(state: String, role: Int, frame: String, first: String, second: String): {String, String, String} ->
    {state + ":" + frame, to_string(role) + ":" + first + ":" + second, ""}.
"#;

const ROUTER: &str = r#"module app.Channels.
import app.Callbacks.{opened, received, writable, closed, cancelled, inbound, waiting, peer_left, matched, restored, transition}.
import std.http.{Router, Sse, WebSocket}.
import type std.http.Router.Router.
pub router(): Router ->
    let prefix = "/computed";
    Router.new()
        .sse(prefix + "/events", Sse.endpoint_with_keep_alive(2 + 2, 1024, 15000)
            .callbacks(opened, received, writable, closed, cancelled))
        .websocket(prefix + "/socket", WebSocket.endpoint(4, 1024)
            .callbacks(opened, received, writable, closed, cancelled))
        .websocket(prefix + "/paired", WebSocket.endpoint(4, 1024)
            .paired_callbacks("waiting", "first", "second", "left", inbound, cancelled))
        .websocket(prefix + "/stateful", WebSocket.endpoint(4, 1024)
            .stateful_paired_callbacks("waiting", "first", "second", "left", transition, cancelled))
        .websocket(prefix + "/restored", WebSocket.endpoint(4, 1024)
            .restorable_stateful_paired_callbacks(
                waiting, peer_left, "room_id", "player_id", "room-",
                "player-1", "player-2", 300000, 1024,
                matched, restored, transition, cancelled)).
"#;

#[test]
fn source_channels_execute_imported_callbacks_and_restorable_policy() {
    let root = crate::support::test_fs::temp_path("serve", "source_channel_router");
    let source_dir = root.join("src/app");
    let web_root = root.join("_build/web");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&web_root).unwrap();
    std::fs::write(source_dir.join("Callbacks.terl"), PROVIDER).unwrap();
    let path = source_dir.join("Channels.terl");
    std::fs::write(&path, ROUTER).unwrap();
    let entry = cached_source_entry(&web_root, &path, MODULE).unwrap();
    let runtime = &entry.runtime;
    assert!(runtime.has_function(MODULE, "router", 0));
    let router = runtime
        .execute_http_router(MODULE, "router", &mut |_| {})
        .unwrap();
    let call = |callback: &ReplValue, args: Vec<ReplValue>| {
        runtime
            .execute_callable(MODULE, callback, args, &mut |_| {})
            .unwrap()
    };
    for suffix in ["events", "socket", "paired", "stateful", "restored"] {
        let VmHttpRouterOutcome::Matched(route) = router
            .dispatch(VmHttpRouteMethod::Get, &format!("/computed/{suffix}"))
            .unwrap()
        else {
            panic!("source-computed channel path missing: {suffix}");
        };
        match route.target {
            VmHttpRouteTarget::SseEndpoint(plan) => {
                assert_eq!(plan.max_pending_events(), 4);
                assert_eq!(plan.max_event_bytes(), 1024);
                assert_eq!(plan.keep_alive_ms(), Some(15000));
                let callbacks = plan.callbacks().unwrap();
                for callback in [&callbacks.open, &callbacks.keep_alive, &callbacks.drain] {
                    assert_eq!(call(callback, vec![]), ReplValue::Unit);
                }
                for callback in [&callbacks.event_ready, &callbacks.cancellation] {
                    assert_eq!(call(callback, vec![text("event")]), ReplValue::Unit);
                }
            }
            VmHttpRouteTarget::WebSocketEndpoint(plan) => {
                assert_eq!(plan.max_pending_frames(), 4);
                assert_eq!(plan.max_frame_bytes(), 1024);
                if let Some(callbacks) = plan.callbacks() {
                    for callback in [&callbacks.open, &callbacks.writable, &callbacks.close] {
                        assert_eq!(call(callback, vec![]), ReplValue::Unit);
                    }
                    for callback in [&callbacks.inbound, &callbacks.cancellation] {
                        assert_eq!(call(callback, vec![text("event")]), ReplValue::Unit);
                    }
                    assert!(plan.pairing().is_none());
                    continue;
                }
                let pairing = plan.pairing().unwrap();
                assert_eq!(
                    call(&pairing.cancellation, vec![text("reason")]),
                    ReplValue::Unit
                );
                if let Some(restoration) = &pairing.restoration {
                    assert_eq!(restoration.room_prefix, "room-");
                    assert_eq!(restoration.retention_ms, 300000);
                    assert_eq!(restoration.retained_room_capacity, 1024);
                    assert_eq!(call(&restoration.waiting, vec![]), text("waiting"));
                    assert_eq!(call(&restoration.peer_left, vec![]), text("left"));
                    assert_eq!(
                        call(
                            &restoration.matched,
                            vec![text("room-1"), ReplValue::Int(2), text("one"), text("two")]
                        ),
                        text("room-1:2:one:two")
                    );
                    assert_eq!(
                        call(
                            &restoration.restored,
                            vec![
                                text("room-1"),
                                text("saved"),
                                ReplValue::Int(1),
                                text("one"),
                                text("two")
                            ]
                        ),
                        text("saved:room-1:1:one:two")
                    );
                    // Identity parsing is a source closure over the configured query keys.
                    assert!(matches!(restoration.identity, ReplValue::Closure(_)));
                } else {
                    assert_eq!(
                        (
                            &*pairing.waiting,
                            &*pairing.first_matched,
                            &*pairing.second_matched,
                            &*pairing.peer_left
                        ),
                        ("waiting", "first", "second", "left")
                    );
                }
                assert_eq!(pairing.stateful, suffix != "paired");
                if pairing.stateful {
                    let value = call(
                        &pairing.inbound,
                        vec![
                            text("saved"),
                            ReplValue::Int(2),
                            text("frame"),
                            text("one"),
                            text("two"),
                        ],
                    );
                    assert_eq!(
                        paired_transition(value).unwrap(),
                        ("saved:frame".into(), Some("2:one:two".into()), None)
                    );
                    assert!(runtime
                        .execute_callable(
                            MODULE,
                            &pairing.inbound,
                            vec![text("wrong arity")],
                            &mut |_| {}
                        )
                        .is_err());
                } else {
                    assert_eq!(
                        call(&pairing.inbound, vec![text("frame")]),
                        text("provider:frame")
                    );
                }
            }
            _ => panic!("channel target required"),
        }
    }
    drop(entry);
    invalidate_vm_handler_cache();
    std::fs::remove_dir_all(root).unwrap();
}

fn text(value: &str) -> ReplValue {
    ReplValue::String(value.into())
}
