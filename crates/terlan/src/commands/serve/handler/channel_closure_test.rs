//! Captured channel callbacks outlive their producer and use shared actor ownership.

use std::process::ExitCode;
use std::sync::Arc;

use super::{AotChannelCallbackState, AotChannelInvocation};
use crate::commands::serve::handler::sse_invocation::AotSseCallbackSession;
use crate::commands::serve::handler::websocket_invocation::AotWebSocketCallbackSession;
use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::native_image::managed::{
    ManagedClosureDescriptor, ManagedClosureImageGeneration,
};
use crate::runtime::vm::pure_native::PureNativeExecutionImage;
use crate::runtime::vm::sse::{VmSseCallbackPlan, VmSseEndpointPlan, VmSseLiveSession};
use crate::runtime::vm::websocket::{
    VmWebSocketCallbackPlan, VmWebSocketEndpointPlan, VmWebSocketFrame, VmWebSocketLiveSession,
    VmWebSocketPairingPlan,
};
use crate::runtime::vm::{NativeClosureValue, ReplValue};
use crate::support::test_fs::TestDirectory;
use crate::{CliCommand, CliState};
use terlan_http_native::channel_plan::WebSocketRestoration;

#[test]
fn captured_channel_callbacks_preserve_state_wakes_and_recovery() {
    let directory = TestDirectory::new("serve", "channel_closures");
    let source = directory.join("channels.terl");
    std::fs::write(
        &source,
        r#"
module channels.
import std.core.Unit.
import std.vm.Process.
type Idle = () -> Unit.
type Event = (String) -> Unit.
type TextCallback = () -> String.
type Matched = (String, Int, String, String) -> String.
type Restored = (String, String, Int, String, String) -> String.
type Inbound = (String, Int, String, String, String) -> {String, String, String}.
checked(value: String): Unit ->
    case value {
        "captured:" -> Unit;
        _ -> let _unexpected = Process.receive_string(); Unit
    }.
pub idle(prefix: String): Idle -> () -> checked(prefix).
pub event(prefix: String): Event -> (_value: String) -> checked(prefix).
pub waiting(prefix: String): Event ->
    (_value: String) -> let wake = Process.receive_string(); checked(prefix + wake).
pub text(prefix: String): TextCallback -> () -> prefix.
pub matched(prefix: String): Matched ->
    (room: String, _role: Int, first: String, second: String) -> prefix + room + first + second.
pub restored(prefix: String): Restored ->
    (room: String, state: String, _role: Int, first: String, second: String) ->
        prefix + room + state + first + second.
pub inbound(prefix: String): Inbound ->
    (state: String, _role: Int, data: String, first: String, second: String) ->
        {prefix + state + data, first, second}.
"#,
    )
    .unwrap();
    let output = directory.join("build");
    assert_eq!(
        crate::commands::build::run(
            CliCommand {
                verb: Some("build".into()),
                args: vec![source.display().to_string()],
            },
            CliState {
                out_dir: output.clone(),
                ..CliState::default()
            }
        ),
        ExitCode::SUCCESS
    );
    let path = output.join("vm/channels.tvm");
    let image = PureNativeExecutionImage::load(&path).unwrap();
    let mut producer = image.spawn_shard().unwrap();
    let mut make = |name| {
        producer
            .call(name, &[ReplValue::String("captured:".into())])
            .unwrap()
    };
    let idle = make("idle");
    let event = make("event");
    let waiting = make("waiting");
    let text = make("text");
    let matched = make("matched");
    let restored = make("restored");
    let inbound = make("inbound");
    drop(producer);
    drop(image);
    let runtime = Arc::new(AotHandlerRuntime::load("channels".into(), &path, None).unwrap());
    exercise_sse(&runtime, &idle, &event, &waiting);
    exercise_websocket(&runtime, &idle, &event, &waiting);
    exercise_recovery(&runtime, event, text.clone(), matched, restored, inbound);
    reject_invalid_callbacks(runtime, &text, &waiting);
}

fn exercise_sse(
    runtime: &Arc<AotHandlerRuntime>,
    idle: &ReplValue,
    event: &ReplValue,
    waiting: &ReplValue,
) {
    let sse = VmSseEndpointPlan::new(2, 128)
        .unwrap()
        .with_callbacks(VmSseCallbackPlan {
            open: idle.clone(),
            event_ready: waiting.clone(),
            keep_alive: idle.clone(),
            drain: idle.clone(),
            cancellation: event.clone(),
        })
        .unwrap();
    let mut session = AotSseCallbackSession::open(
        Arc::clone(runtime),
        "channels".into(),
        VmSseLiveSession::open(sse.clone()).unwrap(),
    )
    .unwrap();
    let AotChannelCallbackState::Waiting(wait) = session.event_ready("event".into()).unwrap()
    else {
        panic!("captured event must park");
    };
    assert!(session.keep_alive().unwrap_err().contains("callback_busy"));
    completed(
        session
            .resume(wait.wake(ReplValue::String(String::new())))
            .unwrap(),
    );
    completed(session.keep_alive().unwrap());
    completed(session.drain().unwrap());
    assert!(!session.is_open());
    let mut session = AotSseCallbackSession::open(
        Arc::clone(runtime),
        "channels".into(),
        VmSseLiveSession::open(sse).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        session.event_ready("event".into()).unwrap(),
        AotChannelCallbackState::Waiting(_)
    ));
    completed(session.cancel("disconnect".into()).unwrap());
    assert!(!session.is_waiting());
}

fn exercise_websocket(
    runtime: &Arc<AotHandlerRuntime>,
    idle: &ReplValue,
    event: &ReplValue,
    waiting: &ReplValue,
) {
    let ws = VmWebSocketEndpointPlan::new(2, 128)
        .unwrap()
        .with_callbacks(VmWebSocketCallbackPlan {
            open: idle.clone(),
            inbound: waiting.clone(),
            writable: idle.clone(),
            close: idle.clone(),
            cancellation: event.clone(),
        })
        .unwrap();
    let mut session = AotWebSocketCallbackSession::open(
        Arc::clone(runtime),
        "channels".into(),
        VmWebSocketLiveSession::open(ws.clone()),
    )
    .unwrap();
    let AotChannelCallbackState::Waiting(wait) = session
        .inbound(VmWebSocketFrame::Text("event".into()))
        .unwrap()
    else {
        panic!("captured inbound must park");
    };
    assert!(session.writable().unwrap_err().contains("callback_busy"));
    completed(
        session
            .resume(wait.wake(ReplValue::String(String::new())))
            .unwrap(),
    );
    completed(session.writable().unwrap());
    completed(session.close().unwrap());
    assert!(!session.is_open());
    let mut session = AotWebSocketCallbackSession::open(
        Arc::clone(runtime),
        "channels".into(),
        VmWebSocketLiveSession::open(ws),
    )
    .unwrap();
    assert!(matches!(
        session
            .inbound(VmWebSocketFrame::Text("event".into()))
            .unwrap(),
        AotChannelCallbackState::Waiting(_)
    ));
    completed(session.cancel("disconnect".into()).unwrap());
    assert!(!session.is_open());
}

fn exercise_recovery(
    runtime: &Arc<AotHandlerRuntime>,
    event: ReplValue,
    text: ReplValue,
    matched: ReplValue,
    restored: ReplValue,
    inbound: ReplValue,
) {
    let paired = VmWebSocketEndpointPlan::new(2, 128)
        .unwrap()
        .with_pairing(VmWebSocketPairingPlan {
            waiting: String::new(),
            first_matched: String::new(),
            second_matched: String::new(),
            peer_left: String::new(),
            stateful: true,
            inbound,
            cancellation: event,
            restoration: Some(WebSocketRestoration {
                waiting: text.clone(),
                peer_left: text.clone(),
                room_query: "room".into(),
                player_query: "player".into(),
                room_prefix: "room-".into(),
                first_player: "one".into(),
                second_player: "two".into(),
                retention_ms: 1000,
                retained_room_capacity: 4,
                matched,
                restored,
            }),
        })
        .unwrap();
    let mut session = AotWebSocketCallbackSession::open(
        Arc::clone(runtime),
        "channels".into(),
        VmWebSocketLiveSession::open(paired),
    )
    .unwrap();
    assert_eq!(session.dispatch_pair_waiting_output().unwrap(), "captured:");
    assert_eq!(
        session.dispatch_pair_peer_left_output().unwrap(),
        "captured:"
    );
    assert_eq!(
        session
            .dispatch_pair_matched_output("room".into(), 1, "first".into(), "second".into())
            .unwrap(),
        "captured:roomfirstsecond"
    );
    assert_eq!(
        session
            .dispatch_pair_restored_output(
                "room".into(),
                "state".into(),
                2,
                "first".into(),
                "second".into()
            )
            .unwrap(),
        "captured:roomstatefirstsecond"
    );
    session
        .enqueue_inbound(VmWebSocketFrame::Text("data".into()))
        .unwrap();
    let (_, value) = session
        .dispatch_next_stateful_inbound_output("state".into(), 1, "first".into(), "second".into())
        .unwrap();
    assert_eq!(
        value,
        Some(ReplValue::Tuple(vec![
            ReplValue::String("captured:statedata".into()),
            ReplValue::String("first".into()),
            ReplValue::String("second".into())
        ]))
    );
    completed(session.cancel("disconnect".into()).unwrap());
}

fn completed(state: AotChannelCallbackState) {
    let AotChannelCallbackState::Complete(value) = state else {
        panic!("callback must complete");
    };
    assert_eq!(value, ReplValue::Unit);
}

fn reject_invalid_callbacks(
    runtime: Arc<AotHandlerRuntime>,
    text: &ReplValue,
    waiting: &ReplValue,
) {
    let ReplValue::Closure(original) = text else {
        panic!("source closure");
    };
    let stale = ReplValue::Closure(Arc::new(NativeClosureValue {
        descriptor: Arc::new(
            ManagedClosureDescriptor::new(
                ManagedClosureImageGeneration::new([99; 32]).unwrap(),
                original.descriptor.callable_id(),
                original.descriptor.parameters().to_vec(),
                original.descriptor.results().to_vec(),
                original.descriptor.captures().to_vec(),
            )
            .unwrap(),
        ),
        captures: original.captures.clone(),
    }));
    let mut invocation =
        AotChannelInvocation::new("channel", Arc::clone(&runtime), "channels".into());
    for value in [stale, ReplValue::Unit, waiting.clone()] {
        let error = invocation.invoke(0, Some(&value), vec![]).unwrap_err();
        assert!(!error.contains("captured:"), "{error}");
        assert!(!invocation.is_waiting());
    }
    let state = invocation
        .invoke(1, Some(waiting), vec![ReplValue::String("event".into())])
        .unwrap();
    assert!(matches!(state, AotChannelCallbackState::Waiting(_)));
    assert!(invocation
        .finish_terminal(1, state)
        .unwrap_err()
        .contains("terminal_wait"));
    assert!(!invocation.is_waiting());
    let AotChannelCallbackState::Complete(value) =
        invocation.invoke(2, Some(text), vec![]).unwrap()
    else {
        panic!("recovered channel");
    };
    assert_eq!(value, ReplValue::String("captured:".into()));
    let mut foreign = AotChannelInvocation::new("channel", runtime, "foreign".into());
    assert!(foreign
        .invoke(0, Some(text), vec![])
        .unwrap_err()
        .contains("different module"));
}
