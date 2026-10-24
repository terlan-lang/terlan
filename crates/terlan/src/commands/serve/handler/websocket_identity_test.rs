//! Compile the public endpoint builder and invoke its captured source reconnect policy.

use super::*;
use crate::runtime::vm::pure_native::PureNativeExecutionImage;
use crate::support::test_fs::TestDirectory;
use crate::{CliCommand, CliState};
use std::process::ExitCode;

fn open_endpoint(name: &str) -> (TestDirectory, AotWebSocketCallbackSession) {
    open_endpoint_with_policy(name, false)
}

fn open_endpoint_with_policy(
    name: &str,
    renamed: bool,
) -> (TestDirectory, AotWebSocketCallbackSession) {
    let directory = TestDirectory::new("serve", name);
    std::fs::create_dir_all(directory.join("app")).unwrap();
    let source = directory.join("app/Main.terl");
    let application = r#"
module app.Main.
import std.http.WebSocket.
import type std.http.WebSocket.Endpoint.
import std.core.Unit.
import std.core.Int.
import std.vm.Process.
pub waiting(): String -> Process.sleep(Process.timer(1)); "waiting".
pub peer_left(): String -> Process.sleep(Process.timer(1)); "left".
pub endpoint(): Endpoint ->
    WebSocket.endpoint(4, 1024).restorable_stateful_paired_callbacks(
        waiting, peer_left, "room", "player", "room-", "first", "second",
        1000, 4,
        (room: String, role: Int, first: String, second: String) ->
            Process.sleep(Process.timer(1));
            if {
                first == "park1" and role == 1 -> Process.receive_string();
                first == "park2" and role == 2 -> Process.receive_string();
                true -> room + ":" + Int.to_string(role)
            },
        (room: String, state: String, role: Int, first: String, second: String) ->
            Process.sleep(Process.timer(1)); state,
        (state: String, role: Int, data: String, first: String, second: String) -> {state, first, second},
        (reason: String) -> Unit
    ).
"#;
    let application = if renamed {
        let policy = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../std/http/WebSocket.terl"
        ))
        .unwrap();
        assert!(policy.contains("room_prefix + Int.to_string(sequence)"));
        assert!(policy.contains("matched(room, 1, first, second)"));
        assert!(policy.contains("matched(room, 2, first, second)"));
        std::fs::write(
            directory.join("app/RoomPolicy.terl"),
            policy
                .replace(
                    "module std.http.WebSocket.",
                    "module app.RoomPolicy.\nimport std.vm.Process.",
                )
                .replace(
                    "room_prefix + Int.to_string(sequence)",
                    "(Process.sleep(Process.timer(1)); Int.to_string(sequence) + room_prefix)",
                )
                .replace(
                    "matched(room, 1, first, second)",
                    "matched(room, 11, first, second)",
                )
                .replace(
                    "matched(room, 2, first, second)",
                    "matched(room, 22, first, second)",
                )
                .replace(
                    "inbound frame arrived before a peer joined",
                    "copied source requires a peer",
                ),
        )
        .unwrap();
        application
            .replace("std.http.WebSocket", "app.RoomPolicy")
            .replace("WebSocket.endpoint", "RoomPolicy.endpoint")
    } else {
        application.to_owned()
    };
    std::fs::write(&source, application).unwrap();
    let output = directory.join("build");
    // A directory build discovers the copied policy as an ordinary dependency.
    let input = if renamed {
        directory.as_ref()
    } else {
        source.as_path()
    };
    assert_eq!(
        crate::commands::build::run(
            CliCommand {
                verb: Some("build".into()),
                args: vec![input.display().to_string()]
            },
            CliState {
                out_dir: output.clone(),
                ..CliState::default()
            }
        ),
        ExitCode::SUCCESS
    );
    let path = output.join("vm/app_Main.tvm");
    let image = PureNativeExecutionImage::load(&path).unwrap();
    let value = image.spawn_shard().unwrap().call("endpoint", &[]).unwrap();
    let plan =
        terlan_http_native::source_descriptor::websocket_endpoint(&value, |callback, _arity| {
            Ok(callback.clone())
        })
        .unwrap();
    drop(image);
    let runtime = Arc::new(AotHandlerRuntime::load("app.Main".into(), &path, None).unwrap());
    let session = crate::commands::serve::handler::websocket_invocation::open(
        runtime,
        "app.Main".into(),
        Session::open(plan),
    )
    .unwrap();
    (directory, session)
}

#[test]
fn compiled_websocket_room_naming_follows_source_even_in_a_renamed_module() {
    for renamed in [false, true] {
        let (_directory, mut session) = open_endpoint_with_policy(
            if renamed {
                "renamed_room_naming"
            } else {
                "source_room_naming"
            },
            renamed,
        );
        for sequence in [1, 42, i64::MAX] {
            let expected = if renamed {
                format!("{sequence}room-")
            } else {
                format!("room-{sequence}")
            };
            assert_eq!(
                super::super::protocol_test_support::complete(
                    session.dispatch_pair_room_identity_output(sequence)
                )
                .unwrap(),
                expected
            );
        }
        assert_eq!(
            super::super::protocol_test_support::complete(session.dispatch_pair_matched_output(
                "room".into(),
                "first".into(),
                "second".into()
            ))
            .unwrap(),
            if renamed {
                ("room:11".into(), "room:22".into())
            } else {
                ("room:1".into(), "room:2".into())
            }
        );
        session.enqueue_inbound("early".into()).unwrap();
        let (dispatched, value) = super::super::protocol_test_support::complete(
            session.dispatch_next_paired_inbound_output(None),
        )
        .unwrap();
        assert!(dispatched);
        let error =
            terlan_http_native::source_descriptor::paired_callback_transition(value.unwrap())
                .unwrap_err();
        assert_eq!(
            error.message(),
            if renamed {
                "error[serve.websocket.pairing]: copied source requires a peer"
            } else {
                "error[serve.websocket.pairing]: inbound frame arrived before a peer joined"
            }
        );
        super::super::protocol_test_support::complete(session.close_suspendable()).unwrap();
    }
}

#[test]
fn unsupported_first_or_second_match_wait_never_publishes_partial_output() {
    use terlan_http_native::websocket::hub::WebSocketHub;
    for target in ["park1", "park2"] {
        let (_directory, mut session) = open_endpoint(target);
        let hub = Arc::new(WebSocketHub::default());
        let mut pairing = session.plan().pairing().unwrap().clone();
        pairing.waiting =
            super::super::protocol_test_support::complete(session.dispatch_pair_waiting_output())
                .unwrap();
        pairing.peer_left =
            super::super::protocol_test_support::complete(session.dispatch_pair_peer_left_output())
                .unwrap();
        for event in [
            AotWebSocketCallbackEvent::PairWaiting,
            AotWebSocketCallbackEvent::PairPeerLeft,
        ] {
            assert_eq!(
                session
                    .executor()
                    .completed_events()
                    .iter()
                    .filter(|seen| **seen == event)
                    .count(),
                1
            );
        }
        let first = hub
            .join("/ws".into(), target.into(), 4, &pairing, None)
            .unwrap();
        assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
        let mut second = hub
            .join("/ws".into(), "second".into(), 4, &pairing, None)
            .unwrap();
        let error =
            super::super::protocol_test_support::complete(second.dispatch_admission(&mut session))
                .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("without a protocol operation adapter"),
            "{error}"
        );
        assert!(
            !session.is_waiting(),
            "unsupported waits are cancelled immediately"
        );
        assert!(first.outbound.try_recv().is_err());
        assert!(second.outbound.try_recv().is_err());
        assert!(hub
            .join(
                "/ws".into(),
                "restore".into(),
                4,
                &pairing,
                Some(("room-1".into(), 1))
            )
            .err()
            .unwrap()
            .to_string()
            .contains("room not found"));
        session.cancel("failed admission".into()).unwrap();
        assert!(!session.is_waiting());
        assert!(!session.is_open());
        drop(second);
        assert_eq!(first.outbound.try_recv().unwrap(), "left");
    }
}

#[test]
fn source_websocket_identity_resolves_encoded_duplicate_and_invalid_queries() {
    let (_directory, mut session) = open_endpoint("source_websocket_identity");
    for target in ["/ws", "/ws?", "/ws?unrelated=1"] {
        assert_eq!(resolve(&mut session, target).unwrap(), None, "{target}");
    }
    for (query, room, role) in [
        ("room=r&player=first", "r", 1),
        ("room=r&player=second", "r", 2),
        ("r%6fom=a%2Bb+c&player=sec%6fnd", "a+b c", 2),
        ("room=old&player=bad&room=new&player=first", "new", 1),
        ("room=%ZZ&player=first", "%ZZ", 1),
        ("room=%E7%95%8C&player=first", "\u{754c}", 1),
    ] {
        assert_eq!(
            resolve(&mut session, format!("/ws?{query}")).unwrap(),
            Some((room.into(), role)),
            "{query}"
        );
    }
    for (query, message) in [
        ("room=r", "must be supplied together"),
        ("player=first", "must be supplied together"),
        ("room=&player=first", "must be supplied together"),
        ("room=r&player=", "must be supplied together"),
        ("room=r&player=first&player=", "must be supplied together"),
        ("room=r&player=first&room=", "must be supplied together"),
        ("room=r&player=unknown", "unknown player"),
    ] {
        assert!(
            resolve(&mut session, format!("/ws?{query}"))
                .unwrap_err()
                .contains(message),
            "{query}"
        );
    }
    assert_eq!(
        resolve(&mut session, "/ws?room=after&player=second").unwrap(),
        Some(("after".into(), 2))
    );
    super::super::protocol_test_support::complete(session.close_suspendable()).unwrap();
}

#[test]
fn source_websocket_delivery_adapts_captured_callback_output() {
    let (_directory, mut session) = open_endpoint("source_websocket_delivery");
    for (first, second) in [("", ""), ("view", ""), ("", "view"), (" ", "view")] {
        session.enqueue_inbound("frame".into()).unwrap();
        let (dispatched, output) = super::super::protocol_test_support::complete(
            session.dispatch_next_paired_inbound_output(Some((
                "retained".into(),
                2,
                first.into(),
                second.into(),
            ))),
        )
        .unwrap();
        assert!(dispatched);
        let actual =
            terlan_http_native::source_descriptor::paired_callback_transition(output.unwrap())
                .unwrap();
        assert_eq!(
            actual,
            (
                "retained".into(),
                (!first.is_empty()).then(|| first.to_owned()),
                (!second.is_empty()).then(|| second.to_owned()),
            )
        );
    }
    super::super::protocol_test_support::complete(session.close_suspendable()).unwrap();
}

fn resolve(
    session: &mut AotWebSocketCallbackSession,
    target: impl Into<String>,
) -> Result<Option<(String, i64)>, String> {
    super::super::protocol_test_support::complete(
        session.dispatch_pair_identity_output(target.into()),
    )
    .map_err(String::from)
}
