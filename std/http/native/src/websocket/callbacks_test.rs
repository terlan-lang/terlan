use super::*;
use crate::callback_test_support::{ready, Executor, State, Wait};
use crate::channel_plan::{WebSocketPairing, WebSocketRestoration};
use crate::websocket::connection::Callbacks;
use crate::websocket::hub::AdmissionCallbacks;
use terlan_runtime_abi::NativeValue as V;

fn session() -> WebSocketCallbacks<Executor<WebSocketEvent>> {
    let plan = WebSocketEndpointPlan::new(2, 64)
        .unwrap()
        .with_pairing(WebSocketPairing {
            waiting: String::new(),
            first_matched: String::new(),
            second_matched: String::new(),
            peer_left: String::new(),
            inbound: V::Int(1),
            cancellation: V::Int(2),
            restoration: Some(WebSocketRestoration {
                matched: V::Int(3),
                restored: V::Int(4),
                identity: V::Int(5),
                room_identity: V::Int(6),
                waiting: V::Int(7),
                peer_left: V::Int(8),
                retention_ms: 100,
                retained_room_capacity: 4,
            }),
        })
        .unwrap();
    WebSocketCallbacks::open(Executor::default(), Session::open(plan)).unwrap()
}

fn last(
    session: &WebSocketCallbacks<Executor<WebSocketEvent>>,
    event: WebSocketEvent,
    id: i64,
    args: Vec<V>,
) {
    assert_eq!(
        session.executor().calls.last().unwrap(),
        &(event, Some(V::Int(id)), args)
    );
}

#[test]
fn pairing_callbacks_suspend_once_and_release_their_invocation_when_dropped() {
    use crate::callback_test_support::Suspension;
    use std::future::Future;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Waker};

    for event in [
        WebSocketEvent::PairRoomIdentity,
        WebSocketEvent::PairMatched,
        WebSocketEvent::PairRestored,
        WebSocketEvent::Inbound,
    ] {
        for abandon in [false, true] {
            let mut session = session();
            if event == WebSocketEvent::Inbound {
                session.enqueue_inbound("frame".into()).unwrap();
            }
            session
                .invocation
                .returns(if event == WebSocketEvent::PairMatched {
                    V::Tuple(vec!["first".into(), "second".into()])
                } else {
                    "value".into()
                });
            let trace = Arc::new(Mutex::new(Suspension::default()));
            session.invocation.suspension = Some(trace.clone());
            let mut future = Box::pin(async {
                match event {
                    WebSocketEvent::PairRoomIdentity => {
                        assert_eq!(session.room_identity(7).await.unwrap(), "value");
                    }
                    WebSocketEvent::PairMatched => {
                        assert_eq!(
                            session
                                .matched("room".into(), "a".into(), "b".into())
                                .await
                                .unwrap(),
                            ("first".into(), "second".into())
                        );
                    }
                    WebSocketEvent::PairRestored => {
                        assert_eq!(
                            session
                                .restored("room".into(), "state".into(), 2, "a".into(), "b".into())
                                .await
                                .unwrap(),
                            "value"
                        );
                    }
                    WebSocketEvent::Inbound => {
                        assert_eq!(
                            session
                                .dispatch_next_paired_inbound_output(None)
                                .await
                                .unwrap(),
                            (true, Some("value".into()))
                        );
                    }
                    _ => unreachable!(),
                }
            });
            for _ in 0..2 {
                assert!(future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending());
            }
            assert_eq!(trace.lock().unwrap().events, [format!("{event:?}")]);
            if !abandon {
                trace.lock().unwrap().released = true;
                ready(&mut future);
            }
            drop(future);
            assert_eq!(
                trace.lock().unwrap().events,
                [format!("{event:?}"), "future dropped".into()]
            );
            assert_eq!(
                session
                    .executor()
                    .calls
                    .iter()
                    .filter(|(seen, _, _)| *seen == event)
                    .count(),
                1
            );
        }
    }
}

#[test]
fn package_builds_admission_arguments_and_validates_each_output() {
    let mut session = session();
    assert!(session.is_open());
    assert_eq!(session.plan().max_pending_frames(), 2);
    session.invocation.returns("room".into());
    assert_eq!(ready(session.room_identity(7)).unwrap(), "room");
    last(
        &session,
        WebSocketEvent::PairRoomIdentity,
        6,
        vec![V::Int(7)],
    );
    session
        .invocation
        .returns(V::Tuple(vec!["first".into(), "second".into()]));
    assert_eq!(
        ready(session.matched("room".into(), "a".into(), "b".into())).unwrap(),
        ("first".into(), "second".into())
    );
    last(
        &session,
        WebSocketEvent::PairMatched,
        3,
        vec!["room".into(), "a".into(), "b".into()],
    );
    session.invocation.returns("view".into());
    assert_eq!(
        ready(session.restored("room".into(), "state".into(), 2, "a".into(), "b".into())).unwrap(),
        "view"
    );
    last(
        &session,
        WebSocketEvent::PairRestored,
        4,
        vec![
            "room".into(),
            "state".into(),
            V::Int(2),
            "a".into(),
            "b".into(),
        ],
    );
    session.invocation.returns("wait".into());
    assert_eq!(ready(session.waiting()).unwrap(), "wait");
    last(&session, WebSocketEvent::PairWaiting, 7, vec![]);
    session.invocation.returns("left".into());
    assert_eq!(ready(session.peer_left()).unwrap(), "left");
    last(&session, WebSocketEvent::PairPeerLeft, 8, vec![]);
    session.invocation.returns(V::Record {
        name: "Ok".into(),
        fields: vec![("value".into(), V::Atom("none".into()))],
    });
    assert_eq!(ready(session.identity("/ws".into())).unwrap(), None);
    last(
        &session,
        WebSocketEvent::PairIdentity,
        5,
        vec!["/ws".into()],
    );
    assert!(ready(session.identity("/ws".into())).is_err());
    assert!(ready(session.room_identity(1))
        .unwrap_err()
        .to_string()
        .contains("expected String"));
    assert!(ready(session.matched("r".into(), "a".into(), "b".into())).is_err());
    session
        .invocation
        .results
        .push_back(Ok(State::Waiting(Wait::Text)));
    assert!(ready(session.waiting())
        .unwrap_err()
        .to_string()
        .contains("still waiting"));
    session.cancel("cleanup".into()).unwrap();
    let mut session = self::session();
    session
        .invocation
        .results
        .push_back(Ok(State::Waiting(Wait::Text)));
    assert!(
        ready(session.restored("room".into(), "state".into(), 1, "a".into(), "b".into()))
            .unwrap_err()
            .to_string()
            .contains("still waiting")
    );
    session.cancel("failed restoration".into()).unwrap();
    let mut empty = WebSocketCallbacks::open(
        Executor::<WebSocketEvent>::default(),
        Session::open(WebSocketEndpointPlan::new(2, 64).unwrap()),
    )
    .unwrap();
    assert!(ready(empty.identity("/ws".into()))
        .unwrap_err()
        .to_string()
        .contains("no reconnect identity"));
}

#[test]
fn incompatible_wakes_preserve_queue_order_and_text_wakes_consume_once() {
    let mut session = session();
    session.enqueue_inbound("first".into()).unwrap();
    session.enqueue_inbound("second".into()).unwrap();
    assert!(session.enqueue_inbound("overflow".into()).is_err());
    session.invocation.pending = Some(Wait::Bytes);
    let before = session.inspect();
    for _ in 0..3 {
        assert!(session
            .enqueue_inbound("rejected".into())
            .unwrap_err()
            .to_string()
            .contains("wake_type"));
        assert!(session
            .dispatch_next_inbound()
            .unwrap_err()
            .to_string()
            .contains("wake_type"));
        assert!(crate::callback_test_support::ready(
            session.dispatch_next_paired_inbound_output(Some((
                "s".into(),
                1,
                "a".into(),
                "b".into()
            )))
        )
        .unwrap_err()
        .to_string()
        .contains("wake_type"));
        assert_eq!(session.inspect(), before);
    }
    session.invocation.pending = Some(Wait::Text);
    assert_eq!(
        session.dispatch_next_inbound_output().unwrap(),
        (true, Some("first".into()))
    );
    session.invocation.pending = Some(Wait::Text);
    assert_eq!(
        crate::callback_test_support::ready(session.dispatch_next_paired_inbound_output(Some((
            "s".into(),
            1,
            "a".into(),
            "b".into()
        ))))
        .unwrap(),
        (true, Some("second".into()))
    );
    assert!(!session.is_waiting());
    assert!(!session.dispatch_next_inbound().unwrap());
    assert_eq!(
        crate::callback_test_support::ready(session.next_paired(Some((
            "s".into(),
            1,
            "a".into(),
            "b".into()
        ))))
        .unwrap(),
        None
    );
}

#[test]
fn synchronous_dispatch_preserves_results_waits_and_callback_errors() {
    let mut session = session();
    session.invocation.returns("reply".into());
    session.enqueue_inbound("request".into()).unwrap();
    assert_eq!(
        session.dispatch_next_inbound_output().unwrap(),
        (true, Some("reply".into()))
    );
    last(&session, WebSocketEvent::Inbound, 1, vec!["request".into()]);
    assert!(matches!(
        session.writable().unwrap(),
        State::Complete(V::Unit)
    ));
    assert_eq!(
        session.executor().calls.last().unwrap(),
        &(WebSocketEvent::Writable, None, vec![])
    );
    session
        .invocation
        .results
        .push_back(Ok(State::Waiting(Wait::Text)));
    session.enqueue_inbound("park".into()).unwrap();
    assert_eq!(
        session.dispatch_next_inbound_output().unwrap(),
        (true, None)
    );
    let calls = session.executor().calls.len();
    assert!(session.writable().unwrap_err().to_string().contains("busy"));
    assert_eq!(session.executor().calls.len(), calls);
    session.enqueue_inbound("wake".into()).unwrap();
    assert_eq!(
        session.dispatch_next_inbound_output().unwrap(),
        (true, Some("wake".into()))
    );
    session
        .invocation
        .results
        .push_back(Err("callback failed".into()));
    session.enqueue_inbound("bad".into()).unwrap();
    assert!(session
        .dispatch_next_inbound_output()
        .unwrap_err()
        .to_string()
        .contains("callback failed"));
    assert_eq!(session.inspect().pending_frames, 0);
    assert!(!session.dispatch_next_inbound().unwrap());
    session.invocation.pending = Some(Wait::Text);
    session.invocation.resume_error = true;
    session.enqueue_inbound("failed wake".into()).unwrap();
    assert!(session
        .dispatch_next_inbound_output()
        .unwrap_err()
        .to_string()
        .contains("resume failed"));
    assert!(!session.is_waiting());
    assert_eq!(session.inspect().pending_frames, 0);
}

#[test]
fn paired_text_resume_that_waits_again_cannot_commit_a_partial_transition() {
    let mut session = session();
    session.invocation.pending = Some(Wait::Text);
    session.invocation.resume_wait = Some(Wait::Text);
    session.enqueue_inbound("consumed once".into()).unwrap();
    session.enqueue_inbound("not consumed".into()).unwrap();
    let calls = session.executor().calls.len();
    assert!(ready(session.next_paired(None))
        .unwrap_err()
        .to_string()
        .contains("suspended without producing a transition"));
    assert_eq!(session.executor().calls.len(), calls);
    assert_eq!(session.inspect().pending_frames, 1);
    assert!(session.is_waiting());
    session.cancel("connection cleanup".into()).unwrap();
    assert!(!session.is_waiting());
    assert!(!session.is_open());
}

#[test]
fn package_transport_distinguishes_unit_empty_text_and_bad_or_suspended_transitions() {
    let mut session = session();
    assert_eq!(Callbacks::plan(&session).max_pending_frames(), 2);
    for (value, expected) in [
        (V::Unit, None),
        ("".into(), Some(String::new())),
        ("frame".into(), Some("frame".into())),
    ] {
        session.invocation.returns(value);
        session.enqueue("input".into()).unwrap();
        assert_eq!(ready(session.next_inbound()).unwrap(), (true, expected));
        last(&session, WebSocketEvent::Inbound, 1, vec!["input".into()]);
    }
    session.invocation.returns(V::Bool(false));
    session.enqueue("bad".into()).unwrap();
    assert!(ready(session.next_inbound())
        .unwrap_err()
        .to_string()
        .contains("String or Unit"));
    session.invocation.returns(
        Result::<_, String>::Ok(V::Tuple(vec![
            "next".into(),
            V::Atom("none".into()),
            V::Atom("none".into()),
        ]))
        .into(),
    );
    session.enqueue("frame".into()).unwrap();
    assert_eq!(
        crate::callback_test_support::ready(session.next_paired(Some((
            "state".into(),
            2,
            "a".into(),
            "b".into()
        ))))
        .unwrap(),
        Some(("next".into(), None, None))
    );
    last(
        &session,
        WebSocketEvent::Inbound,
        1,
        vec![
            Some(V::Tuple(vec![
                "state".into(),
                V::Int(2),
                "a".into(),
                "b".into(),
            ]))
            .into(),
            "frame".into(),
        ],
    );
    session.enqueue("bad".into()).unwrap();
    assert!(
        crate::callback_test_support::ready(session.next_paired(Some((
            "s".into(),
            1,
            "a".into(),
            "b".into()
        ))))
        .is_err()
    );
    session
        .invocation
        .returns(Err::<V, _>("source requires peer".to_string()).into());
    session.enqueue("early".into()).unwrap();
    assert!(
        crate::callback_test_support::ready(session.next_paired(None))
            .unwrap_err()
            .to_string()
            .contains("source requires peer")
    );
    last(
        &session,
        WebSocketEvent::Inbound,
        1,
        vec![Option::<V>::None.into(), "early".into()],
    );
    session
        .invocation
        .results
        .push_back(Ok(State::Waiting(Wait::Text)));
    session.enqueue("park".into()).unwrap();
    assert!(
        crate::callback_test_support::ready(session.next_paired(Some((
            "s".into(),
            1,
            "a".into(),
            "b".into()
        ))))
        .unwrap_err()
        .to_string()
        .contains("still waiting")
    );
    let calls = session.executor().calls.len();
    ready(Callbacks::writable(&mut session)).unwrap();
    assert_eq!(session.executor().calls.len(), calls);
    Callbacks::cancel(&mut session, "stop".into()).unwrap();
    assert!(!session.is_open());
}

#[test]
fn streaming_dispatch_retains_text_waits_without_replaying_or_consuming_extra_frames() {
    let mut executor = Executor::<WebSocketEvent>::default();
    drop(executor.invoke_streaming(WebSocketEvent::Inbound, None, vec![]));
    assert!(executor.calls.is_empty());
    executor.pending = Some(Wait::Text);
    drop(executor.resume_streaming((Wait::Text, "not consumed".into())));
    assert!(executor.is_waiting());
    let mut session = session();
    assert_eq!(ready(session.next_inbound()).unwrap(), (false, None));
    session
        .invocation
        .results
        .push_back(Ok(State::Waiting(Wait::Text)));
    session.enqueue("first".into()).unwrap();
    assert_eq!(ready(session.next_inbound()).unwrap(), (true, None));
    let calls = session.executor().calls.len();
    session.invocation.resume_wait = Some(Wait::Text);
    for text in ["second", "third"] {
        session.enqueue(text.into()).unwrap();
    }
    assert_eq!(ready(session.next_inbound()).unwrap(), (true, None));
    assert_eq!(session.inspect().pending_frames, 1);
    ready(Callbacks::writable(&mut session)).unwrap();
    assert_eq!(
        ready(session.next_inbound()).unwrap(),
        (true, Some("third".into()))
    );
    assert_eq!(session.executor().calls.len(), calls);
    assert_eq!(ready(session.next_inbound()).unwrap(), (false, None));
    session.enqueue("retained".into()).unwrap();
    session.invocation.pending = Some(Wait::Bytes);
    assert!(ready(session.next_inbound())
        .unwrap_err()
        .to_string()
        .contains("cannot wake Bytes"));
    assert_eq!(session.inspect().pending_frames, 1);
    session.cancel("wrong wait".into()).unwrap();
}

#[test]
fn terminal_failures_always_close_transport_admission() {
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
                session.cancel("end".into()).map(|_| ())
            } else {
                ready(Callbacks::close(&mut session))
            };
            assert!(result.is_err());
            assert!(!session.is_open());
            assert!(!session.is_waiting());
            assert!(session.enqueue("late".into()).is_err());
            let calls = session.executor().calls.len();
            ready(Callbacks::close(&mut session)).unwrap();
            assert_eq!(session.executor().calls.len(), calls);
        }
    }
    let mut session = session();
    ready(Callbacks::writable(&mut session)).unwrap();
    ready(Callbacks::close(&mut session)).unwrap();
    assert!(!session.is_open());
}

#[test]
fn host_failures_are_propagated_without_replaying_callbacks() {
    let mut executor = Executor::<WebSocketEvent>::default();
    executor.results.push_back(Err("open failed".into()));
    assert!(WebSocketCallbacks::open(
        executor,
        Session::open(WebSocketEndpointPlan::new(2, 64).unwrap())
    )
    .unwrap_err()
    .to_string()
    .contains("open failed"));
    let mut session = session();
    session.invocation.wait_error = true;
    assert!(session
        .enqueue_inbound("frame".into())
        .unwrap_err()
        .to_string()
        .contains("wait failed"));
    assert_eq!(session.inspect().pending_frames, 0);
    session.invocation.wait_error = false;
    for stateful in [false, true] {
        for resumed in [false, true] {
            session.enqueue("frame".into()).unwrap();
            if resumed {
                session.invocation.pending = Some(Wait::Text);
                session.invocation.resume_error = true;
            } else {
                session
                    .invocation
                    .results
                    .push_back(Err("invoke failed".into()));
            }
            let result = if stateful {
                crate::callback_test_support::ready(session.next_paired(Some((
                    "s".into(),
                    1,
                    "a".into(),
                    "b".into(),
                ))))
                .map(|_| ())
            } else {
                ready(session.next_inbound()).map(|_| ())
            };
            assert!(result.unwrap_err().to_string().contains("failed"));
            assert_eq!(session.inspect().pending_frames, 0);
            assert!(!session.is_waiting());
        }
    }
    session
        .invocation
        .results
        .push_back(Err("invoke failed".into()));
    assert!(ready(Callbacks::writable(&mut session))
        .unwrap_err()
        .to_string()
        .contains("invoke failed"));
    session
        .invocation
        .results
        .push_back(Err("identity failed".into()));
    assert!(ready(session.identity("/ws".into()))
        .unwrap_err()
        .to_string()
        .contains("identity failed"));
    session
        .invocation
        .results
        .push_back(Err("name failed".into()));
    assert!(ready(session.room_identity(1))
        .unwrap_err()
        .to_string()
        .contains("name failed"));
    session
        .invocation
        .results
        .push_back(Err("match failed".into()));
    assert!(ready(session.matched("r".into(), "a".into(), "b".into()))
        .unwrap_err()
        .to_string()
        .contains("match failed"));
    session
        .invocation
        .results
        .push_back(Ok(State::Waiting(Wait::Text)));
    assert!(ready(session.matched("r".into(), "a".into(), "b".into()))
        .unwrap_err()
        .to_string()
        .contains("still waiting"));
    session.cancel("stop".into()).unwrap();
}

#[test]
fn failed_open_cancels_its_session_and_retains_cleanup_errors() {
    for (cancel_error, terminal, expected) in [
        (false, Ok(State::Complete(V::Unit)), "open failed"),
        (
            true,
            Ok(State::Complete(V::Unit)),
            "open failed; cancellation failed: cancel failed",
        ),
        (
            false,
            Err("cancellation callback failed".into()),
            "open failed; cancellation failed: cancellation callback failed",
        ),
    ] {
        let executor = Executor::<WebSocketEvent> {
            results: [Err("open failed".into()), terminal].into(),
            cancel_error,
            ..Executor::default()
        };
        let error = WebSocketCallbacks::open(
            executor,
            Session::open(WebSocketEndpointPlan::new(2, 64).unwrap()),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), expected);
    }
    let mut live = Session::open(WebSocketEndpointPlan::new(2, 64).unwrap());
    live.close();
    let mut executor = Executor::<WebSocketEvent>::default();
    executor
        .results
        .push_back(Err("must not execute open".into()));
    assert_eq!(
        WebSocketCallbacks::open(executor, live)
            .unwrap_err()
            .to_string(),
        "error[serve.websocket.closed]: connection is closed"
    );
}

#[test]
fn paired_payloads_await_source_validate_results_and_drop_owned_work() {
    use crate::callback_test_support::Suspension;
    use std::future::Future;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Waker};

    for peer_left in [false, true] {
        for outcome in ["complete", "empty", "invalid", "failure", "drop"] {
            let mut session = session();
            let trace = Arc::new(Mutex::new(Suspension::default()));
            session.invocation.suspension = Some(trace.clone());
            session.invocation.results.push_back(match outcome {
                "failure" => Err("source payload failed".into()),
                "invalid" => Ok(State::Complete(V::Unit)),
                "empty" => Ok(State::Complete("".into())),
                _ => Ok(State::Complete("source payload".into())),
            });
            let mut future = Box::pin(async {
                if peer_left {
                    session.peer_left().await
                } else {
                    session.waiting().await
                }
            });
            for _ in 0..2 {
                assert!(future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending());
            }
            assert_eq!(
                trace.lock().unwrap().events,
                [if peer_left {
                    "PairPeerLeft"
                } else {
                    "PairWaiting"
                }]
            );
            if outcome != "drop" {
                trace.lock().unwrap().released = true;
                let result = ready(&mut future);
                match outcome {
                    "failure" => {
                        assert_eq!(result.unwrap_err().to_string(), "source payload failed")
                    }
                    "invalid" => {
                        assert!(result.unwrap_err().to_string().contains("expected String"))
                    }
                    "empty" => assert_eq!(result.unwrap(), ""),
                    _ => assert_eq!(result.unwrap(), "source payload"),
                }
            }
            drop(future);
            assert_eq!(
                trace.lock().unwrap().events.last().unwrap(),
                "future dropped"
            );
            assert_eq!(
                session.executor().calls.len(),
                2,
                "open plus one payload invocation"
            );
            session.cancel("owner cleanup".into()).unwrap();
            assert_eq!(
                &trace.lock().unwrap().events[1..],
                ["future dropped", "cancel pending", "Cancellation"]
            );
            assert!(!session.is_open());
        }
        let mut empty = WebSocketCallbacks::open(
            Executor::<WebSocketEvent>::default(),
            Session::open(WebSocketEndpointPlan::new(2, 64).unwrap()),
        )
        .unwrap();
        let error = if peer_left {
            ready(empty.peer_left())
        } else {
            ready(empty.waiting())
        }
        .unwrap_err();
        assert!(error.to_string().contains("no paired"));
        assert_eq!(
            empty.executor().calls.len(),
            1,
            "missing callback never dispatched"
        );
    }
}
