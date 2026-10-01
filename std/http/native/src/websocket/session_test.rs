use super::*;
use crate::channel_plan::{WebSocketCallbacks, WebSocketPairing};
use crate::websocket::{Message, Server};

#[test]
fn source_callbacks_and_pairing_remain_opaque_session_policy() {
    let callbacks = WebSocketCallbacks {
        open: "open",
        inbound: "inbound",
        writable: "writable",
        close: "close",
        cancellation: "cancel",
    };
    let plan = WebSocketEndpointPlan::new(2, 16)
        .unwrap()
        .with_callbacks(callbacks)
        .unwrap();
    let session = Session::open(plan.clone());
    assert_eq!(session.plan(), &plan);
    assert!(session.is_open());
    assert_eq!(
        session.inspect(),
        InboundQueueInfo {
            pending_frames: 0,
            max_pending_frames: 2,
            queued_frame_bytes: 0,
            max_frame_bytes: 16,
        }
    );
    let plan = WebSocketEndpointPlan::new(1, 32)
        .unwrap()
        .with_pairing(WebSocketPairing {
            waiting: "waiting".into(),
            first_matched: "first".into(),
            second_matched: "second".into(),
            peer_left: "left".into(),
            stateful: true,
            restoration: None,
            inbound: "pair_inbound",
            cancellation: "pair_cancel",
        })
        .unwrap();
    assert_eq!(Session::open(plan.clone()).plan(), &plan);
}

#[test]
fn queue_moves_maintained_text_buffers_in_order_and_counts_utf8_bytes() {
    let mut session = Session::open(WebSocketEndpointPlan::<()>::new(3, 8).unwrap());
    let text = Utf8Bytes::from(String::from("\u{e9}abc"));
    let pointer = text.as_ptr();
    session.enqueue_inbound(text).unwrap();
    session.enqueue_inbound("".into()).unwrap();
    session.enqueue_inbound("last".into()).unwrap();
    assert_eq!(session.inspect().pending_frames, 3);
    assert_eq!(session.inspect().queued_frame_bytes, 9);
    let first = session.next_inbound().unwrap();
    assert_eq!(first.as_ptr(), pointer);
    assert_eq!(first.as_str(), "\u{e9}abc");
    assert_eq!(session.inspect().queued_frame_bytes, 4);
    assert_eq!(session.next_inbound().unwrap().as_str(), "");
    assert_eq!(session.next_inbound().unwrap().as_str(), "last");
    assert_eq!(session.next_inbound(), None);
    assert_eq!(session.inspect().queued_frame_bytes, 0);
}

#[test]
fn size_pressure_overflow_and_closed_rejections_are_atomic() {
    let mut session = Session::open(WebSocketEndpointPlan::<()>::new(1, 2).unwrap());
    let empty = session.inspect();
    assert_eq!(
        session.enqueue_inbound("\u{e9}x".into()),
        Err(QueueError::FrameTooLarge)
    );
    assert_eq!(session.inspect(), empty);
    session.queued_frame_bytes = usize::MAX;
    assert_eq!(
        session.enqueue_inbound("x".into()),
        Err(QueueError::ByteCountOverflow)
    );
    assert_eq!(session.inspect().pending_frames, 0);
    assert_eq!(session.inspect().queued_frame_bytes, usize::MAX);
    session.queued_frame_bytes = 0;
    session.enqueue_inbound("\u{e9}".into()).unwrap();
    let full = session.inspect();
    assert_eq!(session.enqueue_inbound("".into()), Err(QueueError::Full));
    assert_eq!(session.inspect(), full);
    session.close();
    session.close();
    assert!(!session.is_open());
    assert_eq!(
        session.enqueue_inbound("late".into()),
        Err(QueueError::Closed)
    );
    assert_eq!(session.inspect(), full);
    assert_eq!(session.next_inbound().unwrap().as_str(), "\u{e9}");
    assert_eq!(session.inspect(), empty);
    assert_eq!(session.enqueue_inbound("".into()), Err(QueueError::Closed));
    for (error, text) in [
        (QueueError::Closed, "session is closed"),
        (QueueError::FrameTooLarge, "frame exceeds max_frame_bytes"),
        (QueueError::Full, "pending frame queue is full"),
        (
            QueueError::ByteCountOverflow,
            "queued frame byte count overflow",
        ),
    ] {
        assert_eq!(
            error.to_string(),
            format!("error[http.websocket.queue]: {text}")
        );
    }
}

#[test]
fn maintained_codec_messages_enter_independent_bounded_sessions() {
    use std::io::Cursor;
    use tungstenite::protocol::{Role, WebSocket};

    let mut client = WebSocket::from_raw_socket(Cursor::new(Vec::new()), Role::Client, None);
    for text in ["first", "second", "too long"] {
        client.send(Message::text(text)).unwrap();
    }
    let mut codec = Server::new(Cursor::new(client.into_inner().into_inner()), 6);
    let plan = WebSocketEndpointPlan::<()>::new(1, 6).unwrap();
    let mut first = Session::open(plan.clone());
    let mut second = Session::open(plan);
    for session in [&mut first, &mut second] {
        let Message::Text(text) = codec.read().unwrap() else {
            panic!("text frame")
        };
        session.enqueue_inbound(text).unwrap();
    }
    assert_eq!(first.next_inbound().unwrap().as_str(), "first");
    first.close();
    assert!(second.is_open());
    assert_eq!(second.next_inbound().unwrap().as_str(), "second");
    assert!(codec
        .read()
        .unwrap_err()
        .to_string()
        .contains("Space limit exceeded"));
    assert_eq!(second.inspect().pending_frames, 0);
}
