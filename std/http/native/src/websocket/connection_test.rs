use super::super::session::Session;
use super::*;
use crate::channel_plan::{WebSocketPairing, WebSocketRestoration};
use crate::http_test_io::{complete, MemoryIo, State};
use std::future::{pending, ready};
use std::io::{self, Cursor};
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};
use tungstenite::protocol::{Role, WebSocket};

struct Source {
    live: Session<()>,
    events: Vec<String>,
    failure: Option<&'static str>,
    cancel_failure: bool,
    identity: Option<(String, i64)>,
    pending_identity: bool,
    broadcast: bool,
}

impl Source {
    fn new(pair: Option<WebSocketPairing<()>>) -> Self {
        let plan = WebSocketEndpointPlan::new(4, 1024).unwrap();
        let plan = match pair {
            Some(pair) => plan.with_pairing(pair).unwrap(),
            None => plan,
        };
        Self {
            live: Session::open(plan),
            events: Vec::new(),
            failure: None,
            cancel_failure: false,
            identity: None,
            pending_identity: false,
            broadcast: false,
        }
    }

    fn event(&mut self, name: &str) -> Result<(), crate::ServiceError> {
        self.events.push(name.into());
        if self.failure == Some(name) {
            Err(format!("source failure: {name}").into())
        } else {
            Ok(())
        }
    }
}

impl AdmissionCallbacks for Source {
    fn room_identity(&mut self, sequence: i64) -> Result<String, crate::ServiceError> {
        self.event("room_identity")?;
        Ok(format!("room{sequence}"))
    }
    fn matched(
        &mut self,
        room: String,
        _: String,
        _: String,
    ) -> Result<(String, String), crate::ServiceError> {
        self.event("matched")?;
        Ok((format!("{room}/1"), format!("{room}/2")))
    }
    fn restored(
        &mut self,
        room: String,
        state: String,
        role: i64,
        _: String,
        _: String,
    ) -> Result<String, crate::ServiceError> {
        self.event("restored")?;
        Ok(format!("{room}/{state}/{role}"))
    }
}

impl Callbacks for Source {
    type Callback = ();
    fn plan(&self) -> &WebSocketEndpointPlan<()> {
        self.live.plan()
    }
    async fn identity(&mut self, _: String) -> Result<Option<(String, i64)>, crate::ServiceError> {
        self.event("identity")?;
        if self.pending_identity {
            pending::<()>().await;
        }
        Ok(self.identity.clone())
    }
    fn waiting(&mut self) -> Result<String, crate::ServiceError> {
        self.event("waiting")?;
        Ok("source-waiting".into())
    }
    fn peer_left(&mut self) -> Result<String, crate::ServiceError> {
        self.event("peer_left")?;
        Ok("source-left".into())
    }
    fn enqueue(&mut self, text: Utf8Bytes) -> Result<(), crate::ServiceError> {
        self.event("enqueue")?;
        self.live
            .enqueue_inbound(text)
            .map_err(|error| crate::ServiceError::from(error.to_string()))
    }
    fn next_inbound(&mut self) -> Result<(bool, Option<String>), crate::ServiceError> {
        self.event("inbound")?;
        Ok(match self.live.next_inbound() {
            Some(text) => (true, Some(format!("echo:{text}"))),
            None => (false, None),
        })
    }
    fn next_paired(
        &mut self,
        context: Option<PairContext>,
    ) -> Result<Option<PairedTransition>, crate::ServiceError> {
        self.event("stateful")?;
        if self.broadcast {
            let state = context.map(|context| context.0).unwrap_or_default();
            return Ok(self.live.next_inbound().map(|text| {
                let payload = format!("echo:{text}");
                (state, Some(payload.clone()), Some(payload))
            }));
        }
        let (state, role, first, second) = context.expect("paired stateful fixture");
        assert_eq!(role, 2);
        assert_eq!((first.as_str(), second.as_str()), ("/first", "/second"));
        Ok(self.live.next_inbound().map(|text| {
            (
                format!("{state}{text}"),
                Some("first-value".into()),
                Some("second-value".into()),
            )
        }))
    }
    fn writable(&mut self) -> Result<(), crate::ServiceError> {
        self.event("writable")
    }
    fn close(&mut self) -> Result<(), crate::ServiceError> {
        self.live.close();
        self.event("close")
    }
    fn cancel(&mut self, reason: String) -> Result<(), crate::ServiceError> {
        self.events.push(format!("cancel:{reason}"));
        self.live.close();
        if self.cancel_failure {
            Err("cleanup failed".into())
        } else {
            Ok(())
        }
    }
}

fn pairing(restorable: bool) -> WebSocketPairing<()> {
    WebSocketPairing {
        waiting: "waiting".into(),
        first_matched: "first-matched".into(),
        second_matched: "second-matched".into(),
        peer_left: "left".into(),
        inbound: (),
        cancellation: (),
        restoration: restorable.then_some(WebSocketRestoration {
            waiting: (),
            peer_left: (),
            identity: (),
            room_identity: (),
            retention_ms: 1000,
            retained_room_capacity: 4,
            matched: (),
            restored: (),
        }),
    }
}

fn io(messages: Vec<Message>) -> (MemoryIo<0>, Arc<Mutex<State>>) {
    let mut client = WebSocket::from_raw_socket(Cursor::new(Vec::new()), Role::Client, None);
    for message in messages {
        client.send(message).unwrap();
    }
    let state = Arc::new(Mutex::new(State {
        incoming: client.into_inner().into_inner().into(),
        ..State::default()
    }));
    (MemoryIo(state.clone()), state)
}

fn output(state: &Arc<Mutex<State>>) -> Vec<Message> {
    let bytes = state.lock().unwrap().outgoing.clone();
    let mut client = WebSocket::from_raw_socket(Cursor::new(bytes), Role::Client, None);
    let mut messages = Vec::new();
    while let Ok(message) = client.read() {
        messages.push(message);
    }
    messages
}

fn run(io: MemoryIo<0>, source: &mut Source, hub: &Arc<WebSocketHub>) -> Result<(), String> {
    Ok(complete(serve(
        ready(Ok(io)),
        source,
        hub,
        "/ws".into(),
        "/second".into(),
        || ready(()),
    ))?)
}

#[test]
fn maintained_frames_dispatch_source_events_and_flush_pong_and_close() {
    let (io, state) = io(vec![
        Message::text("hello"),
        Message::Ping(vec![1].into()),
        Message::Pong(vec![2].into()),
        Message::Close(None),
    ]);
    let mut source = Source::new(None);
    run(io, &mut source, &Arc::default()).unwrap();
    assert_eq!(
        source.events,
        ["writable", "enqueue", "inbound", "inbound", "writable", "close"]
    );
    assert!(!source.live.is_open());
    assert_eq!(
        output(&state),
        [Message::Pong(vec![1].into()), Message::Close(None)]
    );
    assert_eq!(state.lock().unwrap().drops, 1);
}

#[test]
fn all_nonterminal_failures_cancel_once_and_preserve_primary_error() {
    for failure in [None, Some("writable"), Some("enqueue"), Some("inbound")] {
        for cancel_failure in [false, true] {
            let mut source = Source::new(None);
            source.failure = failure;
            source.cancel_failure = cancel_failure;
            let (io, _) = io(vec![Message::text("hello"), Message::binary(vec![1])]);
            let error = run(io, &mut source, &Arc::default()).unwrap_err();
            assert!(
                error.contains(failure.unwrap_or("serve.websocket.binary")),
                "{error}"
            );
            assert_eq!(error.contains("cancellation failed"), cancel_failure);
            assert_eq!(
                source
                    .events
                    .iter()
                    .filter(|s| s.starts_with("cancel:"))
                    .count(),
                1
            );
            assert!(!source.events.contains(&"close".into()));
            assert!(!source.live.is_open());
        }
    }
}

#[test]
fn close_callback_failure_does_not_cancel_twice_or_skip_close_reply() {
    let mut source = Source::new(None);
    source.failure = Some("close");
    let (io, state) = io(vec![Message::Close(None)]);
    assert_eq!(
        run(io, &mut source, &Arc::default()),
        Err("source failure: close".into())
    );
    assert_eq!(source.events, ["writable", "close"]);
    assert_eq!(output(&state), [Message::Close(None)]);
}

#[test]
fn paired_delivery_and_stateful_transitions_use_package_registry() {
    for stateful in [false, true] {
        let pair = pairing(false);
        let hub = Arc::new(WebSocketHub::default());
        let first = hub
            .join("/ws".into(), "/first".into(), 8, &pair, None)
            .unwrap();
        assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
        let mut source = Source::new(Some(pair));
        source.broadcast = !stateful;
        let (io, state) = io(vec![Message::text("update"), Message::Close(None)]);
        run(io, &mut source, &hub).unwrap();
        let payload = if stateful {
            "second-value"
        } else {
            "echo:update"
        };
        assert_eq!(
            output(&state),
            [
                Message::text("second-matched"),
                Message::text(payload),
                Message::Close(None)
            ]
        );
        assert_eq!(
            first.outbound.try_iter().collect::<Vec<_>>(),
            [
                "first-matched",
                if stateful {
                    "first-value"
                } else {
                    "echo:update"
                },
                "left"
            ]
        );
    }
}

#[test]
fn failed_pair_admission_and_transition_release_the_live_seat() {
    for failure in ["admission", "stateful"] {
        let pair = pairing(false);
        let hub = Arc::new(WebSocketHub::default());
        let capacity = if failure == "admission" { 1 } else { 8 };
        let first = hub
            .join("/ws".into(), "/first".into(), capacity, &pair, None)
            .unwrap();
        let mut source = Source::new(Some(pair));
        source.failure = Some("stateful");
        let (io, _) = io(vec![Message::text("update")]);
        let error = run(io, &mut source, &hub).unwrap_err();
        assert!(
            error.contains(if failure == "admission" {
                "queue is full"
            } else {
                "stateful"
            }),
            "{error}"
        );
        assert!(source.events.last().unwrap().starts_with("cancel:"));
        if failure == "stateful" {
            assert_eq!(first.outbound.try_iter().last().unwrap(), "left");
        }
    }
}

#[test]
fn restoration_invokes_source_identity_and_payloads_and_propagates_failures() {
    for failure in [None, Some("identity"), Some("waiting"), Some("peer_left")] {
        let mut source = Source::new(Some(pairing(true)));
        source.failure = failure;
        let (io, state) = io(vec![Message::Close(None)]);
        let result = run(io, &mut source, &Arc::default());
        match failure {
            None => {
                result.unwrap();
                assert_eq!(
                    output(&state),
                    [Message::text("source-waiting"), Message::Close(None)]
                );
                assert_eq!(
                    source.events,
                    ["identity", "waiting", "peer_left", "writable", "close"]
                );
            }
            Some(failure) => {
                assert!(result.unwrap_err().contains(failure));
                assert!(source.events.last().unwrap().starts_with("cancel:"));
            }
        }
    }
}

#[test]
fn dropped_waits_cancel_source_and_release_transport_and_hub_membership() {
    for identity in [false, true] {
        let pair = pairing(identity);
        let hub = Arc::new(WebSocketHub::default());
        let mut source = Source::new(Some(pair.clone()));
        source.pending_identity = identity;
        let (io, state) = io(vec![]);
        let mut future = Box::pin(serve(
            ready(Ok(io)),
            &mut source,
            &hub,
            "/ws".into(),
            "/second".into(),
            pending,
        ));
        assert!(future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        drop(future);
        assert_eq!(
            source.events.last().unwrap(),
            "cancel:websocket connection task dropped"
        );
        assert_eq!(state.lock().unwrap().drops, 1);
        let next = hub
            .join("/ws".into(), "/next".into(), 4, &pair, None)
            .unwrap();
        assert_eq!(next.outbound.try_recv().unwrap(), "waiting");
    }
}

#[test]
fn read_failures_distinguish_retry_disconnect_and_protocol_failure() {
    for kind in [
        io::ErrorKind::WouldBlock,
        io::ErrorKind::Interrupted,
        io::ErrorKind::BrokenPipe,
        io::ErrorKind::PermissionDenied,
    ] {
        let mut source = Source::new(None);
        let (io, state) = io(vec![Message::Close(None)]);
        state.lock().unwrap().read_error = Some(kind);
        let result = complete(serve(
            ready(Ok(io)),
            &mut source,
            &Arc::default(),
            "/ws".into(),
            "/second".into(),
            || {
                state.lock().unwrap().read_error = None;
                ready(())
            },
        ));
        if kind == io::ErrorKind::PermissionDenied {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("serve.websocket.transport"));
            assert!(source.events.last().unwrap().starts_with("cancel:"));
        } else {
            result.unwrap();
            assert_eq!(source.events.last().unwrap(), "close");
        }
    }
}

#[test]
fn saturated_input_yields_before_processing_more_frames() {
    let mut source = Source::new(None);
    let mut frames = vec![Message::Pong(vec![1].into()); 64];
    frames.push(Message::Close(None));
    let (io, state) = io(frames);
    let hub = Arc::default();
    let mut future = Box::pin(serve(
        ready(Ok(io)),
        &mut source,
        &hub,
        "/ws".into(),
        "/second".into(),
        pending,
    ));
    let mut context = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_pending());
    assert!(future.as_mut().poll(&mut context).is_pending());
    assert!(matches!(
        future.as_mut().poll(&mut context),
        Poll::Ready(Ok(()))
    ));
    drop(future);
    assert_eq!(source.events, ["writable", "close"]);
    assert_eq!(output(&state), [Message::Close(None)]);
}

#[test]
fn failed_and_cancelled_upgrade_cancel_the_already_admitted_source() {
    let hub = Arc::default();
    let mut source = Source::new(None);
    let error = complete(serve(
        ready(Err::<MemoryIo<0>, _>("upgrade failed".into())),
        &mut source,
        &hub,
        "/ws".into(),
        "/second".into(),
        pending,
    ))
    .unwrap_err();
    assert_eq!(error.to_string(), "upgrade failed");
    assert_eq!(source.events, ["cancel:upgrade failed"]);
    let mut source = Source::new(None);
    let mut future = Box::pin(serve(
        pending::<Result<MemoryIo<0>, String>>(),
        &mut source,
        &hub,
        "/ws".into(),
        "/second".into(),
        pending,
    ));
    assert!(future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
        .is_pending());
    drop(future);
    assert_eq!(source.events, ["cancel:websocket connection task dropped"]);
}

#[test]
fn output_failure_and_disconnect_finish_source_without_reading_more_input() {
    for paired in [false, true] {
        for kind in [io::ErrorKind::BrokenPipe, io::ErrorKind::PermissionDenied] {
            let mut source = Source::new(paired.then(|| pairing(false)));
            let (io, state) = io(vec![Message::Ping(vec![1].into())]);
            state.lock().unwrap().write_error = Some(kind);
            let result = run(io, &mut source, &Arc::default());
            if kind == io::ErrorKind::BrokenPipe {
                result.unwrap();
                assert_eq!(source.events, ["writable", "close"]);
            } else {
                assert!(result.unwrap_err().contains("serve.websocket.transport"));
                assert!(source.events.last().unwrap().starts_with("cancel:"));
            }
            assert_eq!(state.lock().unwrap().reads, usize::from(!paired));
        }
    }
}

#[test]
fn malformed_frames_cancel_before_application_admission() {
    let mut source = Source::new(None);
    let (io, state) = io(vec![]);
    let mut peer = WebSocket::from_raw_socket(Cursor::new(Vec::new()), Role::Server, None);
    peer.send(Message::text("unmasked")).unwrap();
    state
        .lock()
        .unwrap()
        .incoming
        .extend(peer.into_inner().into_inner());
    let error = run(io, &mut source, &Arc::default()).unwrap_err();
    assert!(error.contains("serve.websocket.transport"), "{error}");
    assert_eq!(source.events.len(), 2);
    assert!(source.events[1].starts_with("cancel:"));
    assert_eq!(source.live.inspect().pending_frames, 0);
}

#[test]
fn restored_seats_and_failed_match_callbacks_follow_source_policy() {
    for failure in [None, Some("matched"), Some("room_identity")] {
        let pair = pairing(true);
        let hub = Arc::new(WebSocketHub::default());
        let first = hub
            .join("/ws".into(), "/first".into(), 8, &pair, None)
            .unwrap();
        assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
        let mut source = Source::new(Some(pair.clone()));
        source.failure = failure;
        let (stream, state) = io(vec![Message::Close(None)]);
        let result = run(stream, &mut source, &hub);
        if let Some(failure) = failure {
            assert!(result.unwrap_err().contains(failure));
            assert!(source.events.last().unwrap().starts_with("cancel:"));
            assert_eq!(first.outbound.try_iter().collect::<Vec<_>>(), ["left"]);
            assert!(hub
                .join(
                    "/ws".into(),
                    "/restore".into(),
                    8,
                    &pair,
                    Some(("room1".into(), 2))
                )
                .err()
                .unwrap()
                .to_string()
                .contains("room not found"));
            continue;
        }
        result.unwrap();
        assert_eq!(
            output(&state),
            [Message::text("room1/2"), Message::Close(None)]
        );
        assert_eq!(
            first.outbound.try_iter().collect::<Vec<_>>(),
            ["room1/1", "left"]
        );
        let mut restored = Source::new(Some(pair));
        restored.identity = Some(("room1".into(), 2));
        let (stream, state) = io(vec![Message::Close(None)]);
        run(stream, &mut restored, &hub).unwrap();
        assert!(restored.events.contains(&"restored".into()));
        assert_eq!(
            output(&state),
            [Message::text("room1//2"), Message::Close(None)]
        );
    }
}

#[test]
fn full_peer_queue_cancels_broadcast_without_consuming_more_frames() {
    let pair = pairing(false);
    let hub = Arc::new(WebSocketHub::default());
    let first = hub
        .join("/ws".into(), "/first".into(), 1, &pair, None)
        .unwrap();
    first.outbound.try_recv().unwrap();
    let mut source = Source::new(Some(pair));
    source.broadcast = true;
    let (stream, _) = io(vec![Message::text("update"), Message::text("unread")]);
    let error = run(stream, &mut source, &hub).unwrap_err();
    assert!(error.contains("queue is full"), "{error}");
    assert_eq!(
        source
            .events
            .iter()
            .filter(|event| event.as_str() == "enqueue")
            .count(),
        1
    );
    assert!(source.events.last().unwrap().starts_with("cancel:"));
}
