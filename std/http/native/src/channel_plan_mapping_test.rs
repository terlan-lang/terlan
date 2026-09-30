use super::*;

// No Clone or serialization: mapping must transfer arbitrary owned callbacks.
struct Owned(usize);

#[test]
fn callback_mapping_preserves_sse_policy_and_visits_each_callback_once() {
    let empty = SseEndpointPlan::<Owned>::new(3, 4096).unwrap();
    let empty = empty.map_callbacks(|_| -> usize { panic!("empty callback set") });
    assert!(empty.callbacks().is_none());
    assert_eq!(empty.keep_alive_ms(), None);
    let plan = SseEndpointPlan::new(3, 4096)
        .unwrap()
        .with_keep_alive_ms(15000)
        .unwrap()
        .with_callbacks(SseCallbacks {
            open: Owned(0),
            event_ready: Owned(1),
            keep_alive: Owned(2),
            drain: Owned(3),
            cancellation: Owned(4),
        })
        .unwrap();
    let mut seen = Vec::new();
    let mapped = plan.map_callbacks(|Owned(value)| {
        seen.push(value);
        value + 10
    });
    assert_eq!(seen, [0, 1, 2, 3, 4]);
    assert_eq!(mapped.max_pending_events(), 3);
    assert_eq!(mapped.max_event_bytes(), 4096);
    assert_eq!(mapped.keep_alive_ms(), Some(15000));
    assert_eq!(
        mapped.callbacks().unwrap(),
        &SseCallbacks {
            open: 10,
            event_ready: 11,
            keep_alive: 12,
            drain: 13,
            cancellation: 14,
        }
    );
}

#[test]
fn callback_mapping_preserves_websocket_policy_and_exclusive_modes() {
    let empty = WebSocketEndpointPlan::<Owned>::new(5, 8192).unwrap();
    let empty = empty.map_callbacks(|_| -> usize { panic!("empty callback set") });
    assert!(empty.callbacks().is_none());
    assert!(empty.pairing().is_none());
    let plan = WebSocketEndpointPlan::new(5, 8192)
        .unwrap()
        .with_callbacks(WebSocketCallbacks {
            open: Owned(0),
            inbound: Owned(1),
            writable: Owned(2),
            close: Owned(3),
            cancellation: Owned(4),
        })
        .unwrap();
    let mut seen = Vec::new();
    let mapped = plan.map_callbacks(|Owned(value)| {
        seen.push(value);
        value + 10
    });
    assert_eq!(seen, [0, 1, 2, 3, 4]);
    assert_eq!(mapped.max_pending_frames(), 5);
    assert_eq!(mapped.max_frame_bytes(), 8192);
    assert_eq!(mapped.binary_payload_policy(), BinaryPayloadPolicy::Reject);
    assert!(mapped.pairing().is_none());
    assert_eq!(
        mapped.callbacks().unwrap(),
        &WebSocketCallbacks {
            open: 10,
            inbound: 11,
            writable: 12,
            close: 13,
            cancellation: 14,
        }
    );
}

#[test]
fn callback_mapping_preserves_pairing_and_every_recovery_field() {
    for restore in [false, true] {
        let pairing = WebSocketPairing {
            waiting: "waiting".into(),
            first_matched: "first".into(),
            second_matched: "second".into(),
            peer_left: "left".into(),
            stateful: restore,
            restoration: restore.then(|| WebSocketRestoration {
                waiting: Owned(0),
                peer_left: Owned(1),
                room_query: "room".into(),
                player_query: "player".into(),
                room_prefix: "prefix".into(),
                first_player: "one".into(),
                second_player: "two".into(),
                retention_ms: 1234,
                retained_room_capacity: 17,
                matched: Owned(2),
                restored: Owned(3),
            }),
            inbound: Owned(4),
            cancellation: Owned(5),
        };
        let plan = WebSocketEndpointPlan::new(7, 2048)
            .unwrap()
            .with_pairing(pairing)
            .unwrap();
        let mut seen = Vec::new();
        let mapped = plan.map_callbacks(|Owned(value)| {
            seen.push(value);
            value + 10
        });
        assert_eq!(
            seen,
            if restore {
                vec![0, 1, 2, 3, 4, 5]
            } else {
                vec![4, 5]
            }
        );
        assert!(mapped.callbacks().is_none());
        assert_eq!(mapped.max_pending_frames(), 7);
        assert_eq!(mapped.max_frame_bytes(), 2048);
        let expected = WebSocketPairing {
            waiting: "waiting".into(),
            first_matched: "first".into(),
            second_matched: "second".into(),
            peer_left: "left".into(),
            stateful: restore,
            restoration: restore.then(|| WebSocketRestoration {
                waiting: 10,
                peer_left: 11,
                room_query: "room".into(),
                player_query: "player".into(),
                room_prefix: "prefix".into(),
                first_player: "one".into(),
                second_player: "two".into(),
                retention_ms: 1234,
                retained_room_capacity: 17,
                matched: 12,
                restored: 13,
            }),
            inbound: 14,
            cancellation: 15,
        };
        assert_eq!(mapped.pairing(), Some(&expected));
        assert!(matches!(
            mapped.with_callbacks(WebSocketCallbacks {
                open: 0,
                inbound: 0,
                writable: 0,
                close: 0,
                cancellation: 0,
            }),
            Err(WebSocketPlanError::CallbacksConflict)
        ));
    }
}
