use super::*;
use crate::callback_test_support::{Executor, State, Wait};
use terlan_runtime_abi::NativeValue as V;

fn session() -> SseCallbacks<Executor<SseEvent>> {
    let plan = SseEndpointPlan::new(2, 64)
        .unwrap()
        .with_callbacks(crate::channel_plan::SseCallbacks {
            open: V::Int(1),
            event_ready: V::Int(2),
            keep_alive: V::Int(3),
            drain: V::Int(4),
            cancellation: V::Int(5),
        })
        .unwrap();
    SseCallbacks::open(Executor::default(), SseSession::open(plan)).unwrap()
}

#[test]
fn transport_rejection_dispatches_source_cancellation_and_closes_admission() {
    for fail in [false, true] {
        let mut session = session();
        session.invocation.cancel_error = fail;
        let result = SseCancellation::cancel(&mut session, "transport rejected".into());
        assert_eq!(result.is_err(), fail);
        assert!(!session.is_open());
        assert_eq!(session.executor().cancellations, ["transport rejected"]);
        if !fail {
            assert_eq!(
                session.executor().calls.last().unwrap().0,
                SseEvent::Cancellation
            );
        }
    }
}

#[test]
fn package_sse_lifecycle_encodes_and_dispatches_without_vm_types() {
    let mut session = session();
    assert!(session.is_open());
    assert_eq!(session.plan().max_pending_events(), 2);
    assert_eq!(
        session.executor().calls,
        [(SseEvent::Open, Some(V::Int(1)), vec![])]
    );
    session.enqueue_event("first".into()).unwrap();
    assert_eq!(
        session.executor().calls.last().unwrap(),
        &(SseEvent::EventReady, Some(V::Int(2)), vec!["first".into()])
    );
    session.keep_alive().unwrap();
    assert_eq!(
        session.executor().calls.last().unwrap(),
        &(SseEvent::KeepAlive, Some(V::Int(3)), vec![])
    );
    session.invocation.pending = Some(Wait::Text);
    let State::Complete(value) = session.enqueue_event("second".into()).unwrap() else {
        panic!("text wake")
    };
    assert_eq!(value, V::String("second".into()));
    assert!(!session.is_waiting());
    assert!(session
        .enqueue_event("overflow".into())
        .unwrap_err()
        .to_string()
        .contains("BackpressureExceeded"));
    session.drain().unwrap();
    assert!(!session.is_open());
    assert_eq!(
        session.executor().calls.last().unwrap(),
        &(SseEvent::Drain, Some(V::Int(4)), vec![])
    );
    assert_eq!(
        session.flush_next_event().unwrap(),
        crate::encode_event(None, None, None, "first")
            .unwrap()
            .into_bytes()
    );
    assert_eq!(
        session.flush_next_event().unwrap(),
        crate::encode_event(None, None, None, "second")
            .unwrap()
            .into_bytes()
    );
    assert_eq!(session.flush_next_event(), None);
    assert!(session.enqueue_event("late".into()).is_err());
}

#[test]
fn incompatible_sse_wake_cannot_enqueue_or_dispatch() {
    let mut session = session();
    session.invocation.pending = Some(Wait::Bytes);
    let before = session.inspect();
    let calls = session.executor().calls.len();
    for _ in 0..3 {
        assert!(session
            .enqueue_event("wrong".into())
            .unwrap_err()
            .to_string()
            .contains("wake_type"));
        assert_eq!(session.inspect(), before);
        assert_eq!(session.executor().calls.len(), calls);
    }
    session.cancel("stop".into()).unwrap();
    assert_eq!(
        session.executor().calls.last().unwrap(),
        &(SseEvent::Cancellation, Some(V::Int(5)), vec!["stop".into()])
    );
    assert_eq!(session.executor().cancellations, ["stop"]);
    assert!(!session.is_open());
}

#[test]
fn sse_terminal_cleanup_failure_still_closes_admission() {
    for cancel in [false, true] {
        for failure in [0, 1, 2] {
            let mut session = session();
            if failure == 0 {
                session.invocation.cancel_error = true;
            } else if failure == 1 {
                session
                    .invocation
                    .results
                    .push_back(Err("callback failed".into()));
            } else {
                session
                    .invocation
                    .results
                    .push_back(Ok(State::Waiting(Wait::Text)));
            }
            let result = if cancel {
                session.cancel("end".into())
            } else {
                session.drain()
            };
            assert!(result.is_err());
            assert!(!session.is_open());
        }
    }
}

#[test]
fn sse_open_and_wait_inspection_errors_do_not_dispatch_events() {
    let mut executor = Executor::<SseEvent>::default();
    executor.results.push_back(Err("open failed".into()));
    assert!(SseCallbacks::open(
        executor,
        SseSession::open(SseEndpointPlan::new(2, 64).unwrap())
    )
    .unwrap_err()
    .to_string()
    .contains("open failed"));
    let mut session = session();
    session.invocation.wait_error = true;
    assert!(session
        .enqueue_event("frame".into())
        .unwrap_err()
        .to_string()
        .contains("wait failed"));
    assert_eq!(session.inspect().pending_events, 0);
    assert_eq!(session.executor().calls.len(), 1);
}
