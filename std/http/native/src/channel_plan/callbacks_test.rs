use super::*;
use crate::channel_plan::{
    SseCallbacks, WebSocketCallbacks, WebSocketPairing, WebSocketRestoration,
};

// Deliberately not Clone: callback selection must only borrow opaque host state.
#[derive(Debug, Eq, PartialEq)]
struct Opaque(u8);

const WS_EVENTS: [WebSocketEvent; 11] = [
    WebSocketEvent::Open,
    WebSocketEvent::Inbound,
    WebSocketEvent::PairMatched,
    WebSocketEvent::PairRestored,
    WebSocketEvent::PairIdentity,
    WebSocketEvent::PairRoomIdentity,
    WebSocketEvent::PairWaiting,
    WebSocketEvent::PairPeerLeft,
    WebSocketEvent::Writable,
    WebSocketEvent::Close,
    WebSocketEvent::Cancellation,
];

fn assert_websocket_selection(plan: &WebSocketEndpointPlan<Opaque>, expected: [Option<u8>; 11]) {
    for (event, expected) in WS_EVENTS.into_iter().zip(expected) {
        assert_eq!(
            plan.callback(event).map(|callback| callback.0),
            expected,
            "{event:?}"
        );
    }
}

#[test]
fn ordinary_websocket_callbacks_never_resolve_pairing_events() {
    let plan = WebSocketEndpointPlan::new(4, 1024).unwrap();
    assert_websocket_selection(&plan, [None; 11]);
    let plan = plan
        .with_callbacks(WebSocketCallbacks {
            open: Opaque(0),
            inbound: Opaque(1),
            writable: Opaque(8),
            close: Opaque(9),
            cancellation: Opaque(10),
        })
        .unwrap();
    assert_websocket_selection(
        &plan,
        [
            Some(0),
            Some(1),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(8),
            Some(9),
            Some(10),
        ],
    );
    assert!(std::ptr::eq(
        plan.callback(WebSocketEvent::Open).unwrap(),
        &plan.callbacks().unwrap().open
    ));
}

#[test]
fn paired_websocket_callbacks_do_not_fabricate_missing_lifecycle_or_recovery_callbacks() {
    for restore in [false, true] {
        let plan = WebSocketEndpointPlan::new(4, 1024)
            .unwrap()
            .with_pairing(WebSocketPairing {
                waiting: "waiting".into(),
                first_matched: "first".into(),
                second_matched: "second".into(),
                peer_left: "left".into(),
                inbound: Opaque(1),
                cancellation: Opaque(10),
                restoration: restore.then_some(WebSocketRestoration {
                    matched: Opaque(2),
                    restored: Opaque(3),
                    identity: Opaque(4),
                    room_identity: Opaque(5),
                    waiting: Opaque(6),
                    peer_left: Opaque(7),
                    retention_ms: 1000,
                    retained_room_capacity: 4,
                }),
            })
            .unwrap();
        let mut expected = [None; 11];
        expected[1] = Some(1);
        expected[10] = Some(10);
        if restore {
            for (index, value) in expected.iter_mut().enumerate().take(8).skip(2) {
                *value = Some(index as u8);
            }
        }
        assert_websocket_selection(&plan, expected);
        assert!(std::ptr::eq(
            plan.callback(WebSocketEvent::Inbound).unwrap(),
            &plan.pairing().unwrap().inbound
        ));
        if restore {
            assert!(std::ptr::eq(
                plan.callback(WebSocketEvent::PairIdentity).unwrap(),
                &plan
                    .pairing()
                    .unwrap()
                    .restoration
                    .as_ref()
                    .unwrap()
                    .identity
            ));
        }
    }
}

#[test]
fn sse_callbacks_are_borrowed_for_every_event_and_absent_without_configuration() {
    let events = [
        SseEvent::Open,
        SseEvent::EventReady,
        SseEvent::KeepAlive,
        SseEvent::Drain,
        SseEvent::Cancellation,
    ];
    let empty = SseEndpointPlan::<Opaque>::new(4, 1024).unwrap();
    for event in events {
        assert_eq!(empty.callback(event), None, "{event:?}");
    }
    let plan = empty
        .with_callbacks(SseCallbacks {
            open: Opaque(0),
            event_ready: Opaque(1),
            keep_alive: Opaque(2),
            drain: Opaque(3),
            cancellation: Opaque(4),
        })
        .unwrap();
    for (index, event) in events.into_iter().enumerate() {
        assert_eq!(
            plan.callback(event),
            Some(&Opaque(index as u8)),
            "{event:?}"
        );
    }
    assert!(std::ptr::eq(
        plan.callback(SseEvent::Open).unwrap(),
        &plan.callbacks().unwrap().open
    ));
}
