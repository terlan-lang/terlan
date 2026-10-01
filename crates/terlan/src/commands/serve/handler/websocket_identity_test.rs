//! Compile the public endpoint builder and invoke its captured source reconnect policy.

use super::*;
use crate::runtime::vm::protocol_task_executor::{
    next_protocol_task_route, with_protocol_task_for_test,
};
use crate::runtime::vm::pure_native::PureNativeExecutionImage;
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;
use crate::support::test_fs::TestDirectory;
use crate::{CliCommand, CliState};
use std::future::Future;
use std::process::ExitCode;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

fn open_endpoint(name: &str) -> (TestDirectory, AotWebSocketCallbackSession) {
    let directory = TestDirectory::new("serve", name);
    let source = directory.join("identity.terl");
    std::fs::write(&source, r#"
module identity.
import std.http.WebSocket.
import type std.http.WebSocket.Endpoint.
import std.core.Unit.
pub endpoint(): Endpoint ->
    WebSocket.endpoint(4, 1024).restorable_stateful_paired_callbacks(
        () -> "waiting", () -> "left", "room", "player", "room-", "first", "second",
        1000, 4,
        (room: String, role: Int, first: String, second: String) -> room,
        (room: String, state: String, role: Int, first: String, second: String) -> state,
        (state: String, role: Int, data: String, first: String, second: String) -> {state, first, second},
        (reason: String) -> Unit
    ).
"#).unwrap();
    let output = directory.join("build");
    assert_eq!(
        crate::commands::build::run(
            CliCommand {
                verb: Some("build".into()),
                args: vec![source.display().to_string()]
            },
            CliState {
                out_dir: output.clone(),
                ..CliState::default()
            }
        ),
        ExitCode::SUCCESS
    );
    let path = output.join("vm/identity.tvm");
    let image = PureNativeExecutionImage::load(&path).unwrap();
    let value = image.spawn_shard().unwrap().call("endpoint", &[]).unwrap();
    let plan =
        terlan_http_native::source_descriptor::websocket_endpoint(&value, |callback, _arity| {
            Ok(callback.clone())
        })
        .unwrap();
    drop(image);
    let runtime = Arc::new(AotHandlerRuntime::load("identity".into(), &path, None).unwrap());
    let session =
        AotWebSocketCallbackSession::open(runtime, "identity".into(), Session::open(plan)).unwrap();
    (directory, session)
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
    session.close().unwrap();
}

#[test]
fn source_websocket_delivery_adapts_captured_callback_output() {
    let (_directory, mut session) = open_endpoint("source_websocket_delivery");
    for (first, second) in [("", ""), ("view", ""), ("", "view"), (" ", "view")] {
        session.enqueue_inbound("frame".into()).unwrap();
        let (dispatched, output) = session
            .dispatch_next_stateful_inbound_output(
                "retained".into(),
                2,
                first.into(),
                second.into(),
            )
            .unwrap();
        assert!(dispatched);
        let actual =
            terlan_http_native::source_descriptor::paired_transition(output.unwrap()).unwrap();
        assert_eq!(
            actual,
            (
                "retained".into(),
                (!first.is_empty()).then(|| first.to_owned()),
                (!second.is_empty()).then(|| second.to_owned()),
            )
        );
    }
    session.close().unwrap();
}

fn resolve(
    session: &mut AotWebSocketCallbackSession,
    target: impl Into<String>,
) -> Result<Option<(String, i64)>, String> {
    let scheduler = VmSchedulerTopology::new(1)
        .unwrap()
        .schedulers()
        .next()
        .unwrap();
    let route = next_protocol_task_route(scheduler).unwrap();
    let mut future = Box::pin(session.dispatch_pair_identity_output(target.into()));
    let mut context = Context::from_waker(Waker::noop());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match with_protocol_task_for_test(route, || future.as_mut().poll(&mut context)) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                if Instant::now() >= deadline {
                    with_protocol_task_for_test(route, || drop(future));
                    panic!("source reconnect callback did not finish");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}
