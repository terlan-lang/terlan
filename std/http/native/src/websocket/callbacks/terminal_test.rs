use super::*;
use crate::callback_test_support::{ready, Executor, Suspension, Wait};
use crate::channel_plan::WebSocketCallbacks as PlanCallbacks;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Waker};
use terlan_runtime_abi::NativeValue as V;

fn session() -> WebSocketCallbacks<Executor<WebSocketEvent>> {
    let plan = WebSocketEndpointPlan::new(2, 64)
        .unwrap()
        .with_callbacks(PlanCallbacks {
            open: V::Int(1),
            inbound: V::Int(2),
            writable: V::Int(3),
            close: V::Int(4),
            cancellation: V::Int(5),
        })
        .unwrap();
    WebSocketCallbacks::open(Executor::default(), Session::open(plan)).unwrap()
}

#[test]
fn close_cancels_prior_work_then_waits_and_cannot_dispatch_twice() {
    for abandoned in [false, true] {
        let mut session = session();
        session.invocation.pending = Some(Wait::Text);
        let trace = Arc::new(Mutex::new(Suspension::default()));
        session.invocation.suspension = Some(trace.clone());
        let mut close = Box::pin(session.close_suspendable());
        assert!(close
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        assert_eq!(trace.lock().unwrap().events, ["cancel pending", "Close"]);
        if !abandoned {
            trace.lock().unwrap().released = true;
            ready(&mut close).unwrap();
        }
        drop(close);
        assert!(!session.is_open());
        assert!(!session.is_waiting());
        assert!(session.enqueue_inbound("late".into()).is_err());
        ready(session.close_suspendable()).unwrap();
        assert_eq!(
            trace.lock().unwrap().events,
            ["cancel pending", "Close", "future dropped"]
        );
        assert_eq!(
            session.executor().calls.last().unwrap(),
            &(WebSocketEvent::Close, Some(V::Int(4)), vec![])
        );
    }
}

#[test]
fn failed_close_or_prior_cancellation_cannot_reenter_terminal_code() {
    for cancel_pending in [false, true] {
        let mut session = session();
        session.invocation.cancel_error = cancel_pending;
        session
            .invocation
            .results
            .push_back(Err("close failed".into()));
        let error = ready(session.close_suspendable()).unwrap_err().to_string();
        assert_eq!(
            error,
            if cancel_pending {
                "cancel failed"
            } else {
                "close failed"
            }
        );
        assert!(!session.is_open());
        ready(session.close_suspendable()).unwrap();
        assert_eq!(
            session.executor().calls.len(),
            if cancel_pending { 1 } else { 2 }
        );
        assert_eq!(
            session.executor().cancellations,
            ["websocket transport closed"]
        );
    }
    let mut session = WebSocketCallbacks::open(
        Executor::default(),
        Session::open(WebSocketEndpointPlan::<V>::new(2, 64).unwrap()),
    )
    .unwrap();
    ready(session.close_suspendable()).unwrap();
    assert_eq!(
        session.executor().calls.last().unwrap(),
        &(WebSocketEvent::Close, None, vec![])
    );
}
