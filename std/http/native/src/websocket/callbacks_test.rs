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
fn package_builds_admission_arguments_and_validates_each_output() {
    let mut session = session();
    assert!(session.is_open());
    assert_eq!(session.plan().max_pending_frames(), 2);
    session.invocation.returns("room".into());
    assert_eq!(session.room_identity(7).unwrap(), "room");
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
        session
            .matched("room".into(), "a".into(), "b".into())
            .unwrap(),
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
        session
            .restored("room".into(), "state".into(), 2, "a".into(), "b".into())
            .unwrap(),
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
    assert_eq!(session.waiting().unwrap(), "wait");
    last(&session, WebSocketEvent::PairWaiting, 7, vec![]);
    session.invocation.returns("left".into());
    assert_eq!(session.peer_left().unwrap(), "left");
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
    assert!(session
        .room_identity(1)
        .unwrap_err()
        .to_string()
        .contains("expected String"));
    assert!(session.matched("r".into(), "a".into(), "b".into()).is_err());
    session
        .invocation
        .results
        .push_back(Ok(State::Waiting(Wait::Text)));
    assert!(session
        .waiting()
        .unwrap_err()
        .to_string()
        .contains("suspended"));
    session.cancel("cleanup".into()).unwrap();
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
        assert!(session
            .dispatch_next_paired_inbound_output(Some(("s".into(), 1, "a".into(), "b".into())))
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
        session
            .dispatch_next_paired_inbound_output(Some(("s".into(), 1, "a".into(), "b".into())))
            .unwrap(),
        (true, Some("second".into()))
    );
    assert!(!session.is_waiting());
    assert!(!session.dispatch_next_inbound().unwrap());
    assert_eq!(
        session
            .next_paired(Some(("s".into(), 1, "a".into(), "b".into())))
            .unwrap(),
        None
    );
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
        assert_eq!(session.next_inbound().unwrap(), (true, expected));
        last(&session, WebSocketEvent::Inbound, 1, vec!["input".into()]);
    }
    session.invocation.returns(V::Bool(false));
    session.enqueue("bad".into()).unwrap();
    assert!(session
        .next_inbound()
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
        session
            .next_paired(Some(("state".into(), 2, "a".into(), "b".into())))
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
    assert!(session
        .next_paired(Some(("s".into(), 1, "a".into(), "b".into())))
        .is_err());
    session
        .invocation
        .returns(Err::<V, _>("source requires peer".to_string()).into());
    session.enqueue("early".into()).unwrap();
    assert!(session
        .next_paired(None)
        .unwrap_err()
        .to_string()
        .contains("source requires peer"));
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
    assert!(session
        .next_paired(Some(("s".into(), 1, "a".into(), "b".into())))
        .unwrap_err()
        .to_string()
        .contains("suspended"));
    let calls = session.executor().calls.len();
    Callbacks::writable(&mut session).unwrap();
    assert_eq!(session.executor().calls.len(), calls);
    Callbacks::cancel(&mut session, "stop".into()).unwrap();
    assert!(!session.is_open());
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
                session.cancel("end".into())
            } else {
                session.close()
            };
            assert!(result.is_err());
            assert!(!session.is_open());
            assert!(session.enqueue("late".into()).is_err());
        }
    }
    let mut session = session();
    Callbacks::writable(&mut session).unwrap();
    Callbacks::close(&mut session).unwrap();
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
                session
                    .next_paired(Some(("s".into(), 1, "a".into(), "b".into())))
                    .map(|_| ())
            } else {
                session.next_inbound().map(|_| ())
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
    assert!(Callbacks::writable(&mut session)
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
    assert!(session
        .room_identity(1)
        .unwrap_err()
        .to_string()
        .contains("name failed"));
    session
        .invocation
        .results
        .push_back(Err("match failed".into()));
    assert!(session
        .matched("r".into(), "a".into(), "b".into())
        .unwrap_err()
        .to_string()
        .contains("match failed"));
    session
        .invocation
        .results
        .push_back(Ok(State::Waiting(Wait::Text)));
    assert!(session
        .matched("r".into(), "a".into(), "b".into())
        .unwrap_err()
        .to_string()
        .contains("suspended"));
    session.cancel("stop".into()).unwrap();
}
