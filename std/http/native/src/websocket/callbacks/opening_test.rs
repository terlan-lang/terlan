use super::*;
use crate::callback_test_support::{ready, Executor, Suspension};
use crate::channel_plan::WebSocketCallbacks as PlanCallbacks;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Waker};
use terlan_runtime_abi::NativeValue as V;

fn live() -> Session<V> {
    Session::open(
        WebSocketEndpointPlan::new(2, 64)
            .unwrap()
            .with_callbacks(PlanCallbacks {
                open: V::Int(1),
                inbound: V::Int(2),
                writable: V::Int(3),
                close: V::Int(4),
                cancellation: V::Int(5),
            })
            .unwrap(),
    )
}

#[test]
fn opening_waits_for_source_and_transfers_only_a_completed_session() {
    let trace = Arc::new(Mutex::new(Suspension::default()));
    let executor = Executor {
        suspension: Some(trace.clone()),
        ..Executor::default()
    };
    let mut opening = Box::pin(WebSocketCallbacks::open_suspendable(executor, live()));
    assert!(trace.lock().unwrap().events.is_empty());
    assert!(opening
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
        .is_pending());
    assert_eq!(trace.lock().unwrap().events, ["Open"]);
    trace.lock().unwrap().released = true;
    let mut session = ready(&mut opening).unwrap();
    drop(opening);
    assert!(session.is_open());
    assert!(!session.is_waiting());
    assert_eq!(
        session.executor().calls,
        [(WebSocketEvent::Open, Some(V::Int(1)), vec![])]
    );
    assert_eq!(trace.lock().unwrap().events, ["Open", "future dropped"]);
    session.cancel("finished".into()).unwrap();
    assert_eq!(
        trace.lock().unwrap().events,
        ["Open", "future dropped", "cancel pending", "Cancellation"]
    );
}

#[test]
fn abandoning_open_drops_the_source_future_before_cancelling_exactly_once() {
    for polled in [false, true] {
        let trace = Arc::new(Mutex::new(Suspension::default()));
        let executor = Executor {
            suspension: Some(trace.clone()),
            ..Executor::default()
        };
        let mut opening = Box::pin(WebSocketCallbacks::open_suspendable(executor, live()));
        if polled {
            assert!(opening
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending());
        }
        drop(opening);
        let trace = trace.lock().unwrap();
        if polled {
            assert_eq!(
                trace.events,
                ["Open", "future dropped", "cancel pending", "Cancellation"]
            );
        } else {
            assert!(trace.events.is_empty());
        }
    }
}

#[test]
fn opening_failure_preserves_original_and_cleanup_errors() {
    for cleanup in [None, Some("pending"), Some("callback")] {
        let trace = Arc::new(Mutex::new(Suspension {
            released: true,
            ..Suspension::default()
        }));
        let mut executor = Executor {
            suspension: Some(trace.clone()),
            cancel_error: cleanup == Some("pending"),
            ..Executor::default()
        };
        executor.results.push_back(Err("open failed".into()));
        if cleanup == Some("callback") {
            executor.results.push_back(Err("cleanup failed".into()));
        }
        let error = ready(WebSocketCallbacks::open_suspendable(executor, live()))
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("open failed"), "{error}");
        assert_eq!(
            error.contains("cancellation failed"),
            cleanup.is_some(),
            "{error}"
        );
        let trace = trace.lock().unwrap();
        assert_eq!(
            &trace.events[..3],
            ["Open", "future dropped", "cancel pending"]
        );
        assert_eq!(
            trace.events.len(),
            if cleanup == Some("pending") { 3 } else { 4 }
        );
    }
}

#[test]
fn absent_open_callback_is_optional_but_closed_sessions_are_rejected() {
    let session = ready(WebSocketCallbacks::open_suspendable(
        Executor::default(),
        Session::open(WebSocketEndpointPlan::<V>::new(2, 64).unwrap()),
    ))
    .unwrap();
    assert!(session.is_open());
    assert_eq!(
        session.executor().calls,
        [(WebSocketEvent::Open, None, vec![])]
    );
    let mut closed = live();
    closed.close();
    let error = ready(WebSocketCallbacks::open_suspendable(
        Executor::default(),
        closed,
    ))
    .unwrap_err();
    assert!(error.to_string().contains("connection is closed"));
}

#[test]
fn absent_open_cannot_admit_a_host_wait_as_a_completed_session() {
    use crate::callback_test_support::{State, Wait};
    let trace = Arc::new(Mutex::new(Suspension::default()));
    let mut executor = Executor {
        suspension: Some(trace.clone()),
        ..Executor::default()
    };
    executor.results.push_back(Ok(State::Waiting(Wait::Text)));
    let result = ready(WebSocketCallbacks::open_suspendable(
        executor,
        Session::open(WebSocketEndpointPlan::<V>::new(2, 64).unwrap()),
    ));
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("callback cannot suspend"));
    assert_eq!(
        trace.lock().unwrap().events,
        ["Open", "cancel pending", "cancel pending", "Cancellation"]
    );
}
