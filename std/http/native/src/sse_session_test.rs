use super::*;
use crate::channel_plan::SseCallbacks;

#[test]
fn resource_admission_runs_once_after_validation_and_before_publication() {
    let mut session = SseSession::open(SseEndpointPlan::<()>::new(1, 32).unwrap());
    let before = session.inspect();
    let mut calls = 0;
    for accepted in [false, true] {
        let result = session.enqueue_with(Some("id"), None, None, "payload", |bytes| {
            calls += 1;
            assert_eq!(bytes, b"id: id\ndata: payload\n\n".len());
            if accepted {
                Ok(())
            } else {
                Err(SseError::Codec("resource rejected".into()))
            }
        });
        if accepted {
            result.unwrap();
        } else {
            assert_eq!(result, Err(SseError::Codec("resource rejected".into())));
            assert_eq!(session.inspect(), before);
        }
    }
    assert_eq!(calls, 2);
    assert_eq!(session.inspect().pending_events, 1);
    assert_eq!(session.flush_next().unwrap(), b"id: id\ndata: payload\n\n");
}

#[test]
fn invalid_full_or_closed_streams_never_reserve_resources() {
    let mut session = SseSession::open(SseEndpointPlan::<()>::new(1, 16).unwrap());
    for (id, data) in [
        (Some("bad\n"), "ok"),
        (None, "\r"),
        (None, "payload-too-large"),
    ] {
        assert!(session
            .enqueue_with::<SseError>(id, None, None, data, |_| panic!(
                "invalid frame reserved resources"
            ))
            .is_err());
        assert_eq!(session.inspect().pending_events, 0);
    }
    session.enqueue(None, None, None, "ok").unwrap();
    assert_eq!(
        session.enqueue_with(None, None, None, "ok", |_| -> Result<(), SseError> {
            panic!("full queue reserved resources")
        }),
        Err(SseError::BackpressureExceeded)
    );
    session.close();
    assert_eq!(
        session.enqueue_with(None, None, None, "ok", |_| -> Result<(), SseError> {
            panic!("closed queue reserved resources")
        }),
        Err(SseError::Closed)
    );
}

#[test]
fn discard_closes_without_counting_dropped_frames_as_emitted() {
    let mut session = SseSession::open(SseEndpointPlan::<()>::new(2, 32).unwrap());
    session.enqueue(None, None, None, "sent").unwrap();
    session.enqueue(None, None, None, "discarded").unwrap();
    assert!(session.flush_next().is_some());
    session.discard_pending();
    session.discard_pending();
    assert_eq!(session.inspect().emitted_events, 1);
    assert_eq!(session.inspect().pending_events, 0);
    assert_eq!(session.flush_next(), None);
    assert!(!session.is_open());
    assert_eq!(
        session.enqueue(None, None, None, "late"),
        Err(SseError::Closed)
    );
}

#[test]
fn session_retains_source_policy_and_callbacks_without_invoking_them() {
    let callbacks = SseCallbacks {
        open: "open",
        event_ready: "event",
        keep_alive: "heartbeat",
        drain: "drain",
        cancellation: "cancel",
    };
    let plan = SseEndpointPlan::new(2, 128)
        .unwrap()
        .with_keep_alive_ms(250)
        .unwrap()
        .with_callbacks(callbacks)
        .unwrap();
    let session = SseSession::open(plan.clone());
    assert_eq!(session.plan(), &plan);
    assert!(session.is_open());
    assert_eq!(
        session.inspect(),
        SseStreamInfo {
            pending_events: 0,
            max_pending_events: 2,
            max_event_bytes: 128,
            closed: false,
            emitted_events: 0,
        }
    );
}

#[test]
fn maintained_frames_drain_in_order_after_close_without_reencoding() {
    let mut session = SseSession::open(SseEndpointPlan::<()>::new(2, 128).unwrap());
    session
        .enqueue(Some("1"), Some("update"), Some(25), "a\nb")
        .unwrap();
    session.enqueue(None, None, None, "\u{e9}\n").unwrap();
    let first_pointer = session.pending.front().unwrap().as_ptr();
    session.close();
    session.close();
    assert!(!session.is_open());
    let before = session.inspect();
    assert_eq!(
        session.enqueue(None, None, None, "late"),
        Err(SseError::Closed)
    );
    assert_eq!(session.inspect(), before);
    let first = session.flush_next().unwrap();
    assert_eq!(first.as_ptr(), first_pointer);
    assert_eq!(
        first,
        b"id: 1\nevent: update\nretry: 25\ndata: a\ndata: b\n\n"
    );
    assert_eq!(
        session.flush_next().unwrap(),
        "data: \u{e9}\ndata: \n\n".as_bytes()
    );
    assert_eq!(session.flush_next(), None);
    assert_eq!(session.inspect().emitted_events, 2);
    assert_eq!(session.inspect().pending_events, 0);
    assert!(session.inspect().closed);
}

#[test]
fn byte_limits_include_framing_and_failures_do_not_consume_capacity() {
    let expected = crate::encode_event(None, None, None, "\u{e9}").unwrap();
    let mut session = SseSession::open(SseEndpointPlan::<()>::new(1, expected.len()).unwrap());
    let before = session.inspect();
    assert_eq!(
        session.enqueue(None, None, None, "\u{e9}x"),
        Err(SseError::EventTooLarge)
    );
    assert_eq!(session.inspect(), before);
    for data in ["\r", "secret\r\npayload"] {
        let Err(SseError::Codec(error)) = session.enqueue(None, None, None, data) else {
            panic!("unnormalized data must fail before queue admission");
        };
        assert!(error.contains("http.sse.unnormalized_data"));
        assert!(!error.contains("secret"));
        assert_eq!(session.inspect(), before);
    }
    for (id, event, retry) in [
        (Some("secret\n"), None, None),
        (None, Some("bad\0"), None),
        (None, None, Some(0)),
        (None, None, Some(-1)),
    ] {
        let Err(SseError::Codec(error)) = session.enqueue(id, event, retry, "") else {
            panic!("maintained codec failure");
        };
        assert!(!error.contains("secret"));
        assert_eq!(session.inspect(), before);
    }
    session.enqueue(None, None, None, "\u{e9}").unwrap();
    let full = session.inspect();
    assert_eq!(
        session.enqueue(Some("bad\n"), None, None, ""),
        Err(SseError::BackpressureExceeded)
    );
    assert_eq!(session.inspect(), full);
    assert_eq!(session.flush_next().unwrap(), expected.as_bytes());
    session.enqueue(None, None, None, "").unwrap();
    assert_eq!(session.flush_next().unwrap(), b"\n");
}

#[test]
fn sessions_do_not_share_queue_state_and_counter_saturates() {
    let plan = SseEndpointPlan::<()>::new(1, 32).unwrap();
    let mut first = SseSession::open(plan.clone());
    let mut second = SseSession::open(plan);
    first.enqueue(None, None, None, "first").unwrap();
    assert_eq!(second.flush_next(), None);
    first.close();
    second.enqueue(None, None, None, "second").unwrap();
    second.emitted_events = usize::MAX;
    assert_eq!(second.flush_next().unwrap(), b"data: second\n\n");
    assert_eq!(second.inspect().emitted_events, usize::MAX);
    assert_eq!(first.flush_next().unwrap(), b"data: first\n\n");
}
