use super::*;

fn join(
    hub: &Arc<WebSocketHub>,
    pairing: &WebSocketPairing<()>,
    capacity: usize,
) -> Result<WebSocketHubLease, String> {
    Ok(hub.join("/ws".into(), "/request".into(), capacity, pairing, None)?)
}

#[test]
fn failed_admission_preserves_waiter_and_does_not_leak_sessions() {
    let hub = Arc::new(WebSocketHub::default());
    let mut pairing = restorable_pairing(1000, 4);
    pairing.restoration = None;
    pairing.first_matched = "first".into();
    pairing.second_matched = "second".into();
    assert!(join(&hub, &pairing, 0).err().unwrap().contains("positive"));
    assert!(hub.state.lock().unwrap().sessions.is_empty());
    let first = join(&hub, &pairing, 1).unwrap();
    for _ in 0..3 {
        assert!(join(&hub, &pairing, 1)
            .err()
            .unwrap()
            .contains("queue is full"));
        let state = hub.state.lock().unwrap();
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.pairs.len(), 1);
        assert_eq!(state.waiting.get("/ws"), Some(&first.id));
        assert_eq!(state.sessions[&first.id].pair, None);
    }
    assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
    let second = join(&hub, &pairing, 1).unwrap();
    assert_eq!(first.outbound.try_recv().unwrap(), "first");
    assert_eq!(second.outbound.try_recv().unwrap(), "second");
    drop(second);
    drop(first);
    let state = hub.state.lock().unwrap();
    assert!(state.sessions.is_empty() && state.pairs.is_empty() && state.waiting.is_empty());
}

#[test]
fn source_controls_unpaired_admission_without_creating_peer_state() {
    let hub = Arc::new(WebSocketHub::default());
    let mut pairing = restorable_pairing(1000, 4);
    pairing.restoration = None;
    let first = join(&hub, &pairing, 4).unwrap();
    first.outbound.try_recv().unwrap();
    assert_eq!(
        first
            .transition(|context| {
                assert!(context.is_none());
                Err("source requires peer".into())
            })
            .unwrap_err()
            .to_string(),
        "source requires peer"
    );
    assert!(first.outbound.try_recv().is_err());
    first
        .transition(|context| {
            assert!(context.is_none());
            Ok((
                "not retained before pairing".into(),
                Some(String::new()),
                Some("no recipient".into()),
            ))
        })
        .unwrap();
    assert_eq!(first.outbound.try_recv().unwrap(), "");
    assert!(first.outbound.try_recv().is_err());
    let second = join(&hub, &pairing, 4).unwrap();
    first.outbound.try_recv().unwrap();
    second.outbound.try_recv().unwrap();
    first
        .transition(|context| {
            let (state, role, first, second) = context.unwrap();
            assert_eq!(
                (state.as_str(), role, first.as_str(), second.as_str()),
                ("", 1, "/request", "/request")
            );
            Ok(("saved".into(), None, None))
        })
        .unwrap();
    drop(second);
    first.outbound.try_recv().unwrap();
    first
        .transition(|context| {
            assert!(context.is_none());
            Ok((String::new(), Some("alone again".into()), None))
        })
        .unwrap();
    assert_eq!(first.outbound.try_recv().unwrap(), "alone again");
}

#[test]
fn identifier_exhaustion_does_not_overwrite_live_state() {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = restorable_pairing(1000, 4);
    let first = join(&hub, &pairing, 4).unwrap();
    hub.state.lock().unwrap().next_room = i64::MAX;
    assert!(join(&hub, &pairing, 4)
        .err()
        .unwrap()
        .contains("room identifiers exhausted"));
    {
        let mut state = hub.state.lock().unwrap();
        assert_eq!(state.waiting.get("/ws"), Some(&first.id));
        assert_eq!(state.sessions.len(), 1);
        assert!(state.rooms.is_empty());
        state.next_id = u64::MAX;
    }
    assert!(join(&hub, &pairing, 4)
        .err()
        .unwrap()
        .contains("session identifiers exhausted"));
    assert_eq!(hub.state.lock().unwrap().sessions.len(), 1);
}

#[test]
fn disconnected_waiter_rejects_match_without_registering_new_session() {
    let hub = Arc::new(WebSocketHub::default());
    let mut pairing = restorable_pairing(1000, 4);
    pairing.restoration = None;
    let mut first = join(&hub, &pairing, 4).unwrap();
    first.outbound.try_recv().unwrap();
    let (_, replacement) = mpsc::sync_channel(1);
    drop(std::mem::replace(&mut first.outbound, replacement));
    assert!(join(&hub, &pairing, 4)
        .err()
        .unwrap()
        .contains("disconnected"));
    assert_eq!(hub.state.lock().unwrap().sessions.len(), 1);
    drop(first);
    let next = join(&hub, &pairing, 4).unwrap();
    assert_eq!(next.outbound.try_recv().unwrap(), "waiting");
}

#[test]
fn panicking_transition_poison_is_reported_without_further_callback_execution() {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = restorable_pairing(1000, 4);
    let first = join(&hub, &pairing, 4).unwrap();
    let second = join(&hub, &pairing, 4).unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        first
            .transition(|_| panic!("injected transition panic"))
            .unwrap();
    }))
    .is_err());
    for error in [
        join(&hub, &pairing, 4).err().unwrap(),
        hub.deliver(first.id, "value".into())
            .unwrap_err()
            .to_string(),
        hub.complete_match(first.id, "room-1".into(), "first".into(), "second".into())
            .unwrap_err()
            .to_string(),
        first
            .transition(|_| Ok(("".into(), Some("value".into()), Some("value".into()))))
            .unwrap_err()
            .to_string(),
        first
            .transition(|_| panic!("poison must reject before invocation"))
            .unwrap_err()
            .to_string(),
    ] {
        assert!(error.contains("lock poisoned"), "{error}");
    }
    drop(first);
    drop(second);
}

#[derive(Default)]
struct Callbacks {
    calls: Vec<(String, i64)>,
    fail_match: bool,
    requests: Option<(String, String)>,
}

impl AdmissionCallbacks for Callbacks {
    fn room_identity(&mut self, sequence: i64) -> Result<String, crate::ServiceError> {
        Ok(format!("room-{sequence}"))
    }
    fn matched(
        &mut self,
        room: String,
        first: String,
        second: String,
    ) -> Result<(String, String), crate::ServiceError> {
        let expected = self
            .requests
            .clone()
            .unwrap_or(("/request".into(), "/request".into()));
        assert_eq!((first, second), expected);
        self.calls.push((room, 0));
        if self.fail_match {
            return Err("source callback rejected".into());
        }
        Ok(("matched-1".into(), "matched-2".into()))
    }

    fn restored(
        &mut self,
        room: String,
        state: String,
        role: i64,
        first: String,
        second: String,
    ) -> Result<String, crate::ServiceError> {
        assert_eq!((first.as_str(), second.as_str()), ("/request", "/request"));
        self.calls.push((room, role));
        Ok(format!("restored-{role}:{state}"))
    }
}

fn admit_pair(first: &WebSocketHubLease, second: &mut WebSocketHubLease) {
    let Some(WebSocketHubAdmission::Matched {
        first_request,
        second_request,
        ..
    }) = &second.admission
    else {
        panic!("expected pending match");
    };
    let mut callbacks = Callbacks {
        requests: Some((first_request.clone(), second_request.clone())),
        ..Callbacks::default()
    };
    second.dispatch_admission(&mut callbacks).unwrap();
    assert_eq!(first.outbound.try_recv().unwrap(), "matched-1");
    assert_eq!(second.outbound.try_recv().unwrap(), "matched-2");
}

struct NamingCallbacks<'a> {
    hub: &'a WebSocketHub,
    identity: Result<String, String>,
    sequences: Vec<i64>,
    departing: Option<WebSocketHubLease>,
}

impl AdmissionCallbacks for NamingCallbacks<'_> {
    fn room_identity(&mut self, sequence: i64) -> Result<String, crate::ServiceError> {
        assert!(
            self.hub.state.try_lock().is_ok(),
            "source must run outside registry lock"
        );
        self.sequences.push(sequence);
        drop(self.departing.take());
        self.identity.clone().map_err(crate::ServiceError::from)
    }

    fn matched(
        &mut self,
        room: String,
        _: String,
        _: String,
    ) -> Result<(String, String), crate::ServiceError> {
        assert_eq!(Ok(&room), self.identity.as_ref());
        Ok((room.clone(), room))
    }

    fn restored(
        &mut self,
        room: String,
        _: String,
        _: i64,
        _: String,
        _: String,
    ) -> Result<String, crate::ServiceError> {
        Ok(room)
    }
}

#[test]
fn source_room_names_are_opaque_unique_and_not_published_before_admission() {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = restorable_pairing(300_000, 4);
    let first = join(&hub, &pairing, 4).unwrap();
    first.outbound.try_recv().unwrap();
    let mut second = join(&hub, &pairing, 4).unwrap();
    let name = "custom/\u{754c}?not-a-prefix";
    let mut callbacks = NamingCallbacks {
        hub: &hub,
        identity: Ok(name.into()),
        sequences: vec![],
        departing: None,
    };
    assert!(hub.state.lock().unwrap().rooms.is_empty());
    second.dispatch_admission(&mut callbacks).unwrap();
    second.dispatch_admission(&mut callbacks).unwrap();
    assert_eq!(callbacks.sequences, [1]);
    assert_eq!(first.outbound.try_recv().unwrap(), name);
    assert_eq!(second.outbound.try_recv().unwrap(), name);
    let third = join(&hub, &pairing, 4).unwrap();
    third.outbound.try_recv().unwrap();
    let mut fourth = join(&hub, &pairing, 4).unwrap();
    assert!(fourth
        .dispatch_admission(&mut callbacks)
        .unwrap_err()
        .to_string()
        .contains("duplicate room identity"));
    assert_eq!(callbacks.sequences, [1, 2]);
    assert!(third.outbound.try_recv().is_err());
    assert!(fourth.outbound.try_recv().is_err());
    drop(third);
    drop(fourth);
    drop(second);
    let mut restored = hub
        .join(
            "/ws".into(),
            "/opaque".into(),
            4,
            &pairing,
            Some((name.into(), 2)),
        )
        .unwrap();
    restored.dispatch_admission(&mut callbacks).unwrap();
    assert_eq!(restored.outbound.try_recv().unwrap(), name);
    assert_eq!(
        callbacks.sequences,
        [1, 2],
        "restoration must not rename rooms"
    );
    let state = hub.state.lock().unwrap();
    assert_eq!(state.rooms.len(), 1);
    assert_eq!(state.rooms[&("/ws".into(), name.into())], first.id);
}

#[test]
fn rejected_room_names_and_departing_peers_leave_no_restorable_state() {
    for (identity, depart, error) in [
        (
            Err("source naming failed".into()),
            false,
            "source naming failed",
        ),
        (Ok(String::new()), false, "empty or duplicate"),
        (Ok("valid".into()), true, "state disappeared"),
    ] {
        let hub = Arc::new(WebSocketHub::default());
        let pairing = restorable_pairing(300_000, 4);
        let first = join(&hub, &pairing, 4).unwrap();
        first.outbound.try_recv().unwrap();
        let mut second = join(&hub, &pairing, 4).unwrap();
        let mut first = Some(first);
        let mut callbacks = NamingCallbacks {
            hub: &hub,
            identity,
            sequences: vec![],
            departing: if depart { first.take() } else { None },
        };
        assert!(second
            .dispatch_admission(&mut callbacks)
            .unwrap_err()
            .to_string()
            .contains(error));
        assert_eq!(callbacks.sequences, [1]);
        assert!(hub.state.lock().unwrap().rooms.is_empty());
        drop(first);
        drop(second);
        let state = hub.state.lock().unwrap();
        assert!(state.rooms.is_empty() && state.pairs.is_empty() && state.sessions.is_empty());
    }
}

#[test]
fn room_publication_requires_two_peers_and_cannot_be_repeated() {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = restorable_pairing(300_000, 4);
    let first = join(&hub, &pairing, 4).unwrap();
    first.outbound.try_recv().unwrap();
    assert!(hub
        .complete_match(
            first.id,
            "premature".into(),
            "first".into(),
            "second".into()
        )
        .unwrap_err()
        .to_string()
        .contains("peer left before room admission"));
    assert!(hub.state.lock().unwrap().rooms.is_empty());
    let mut second = join(&hub, &pairing, 4).unwrap();
    admit_pair(&first, &mut second);
    assert!(hub
        .complete_match(
            first.id,
            "different".into(),
            "first".into(),
            "second".into()
        )
        .unwrap_err()
        .to_string()
        .contains("duplicate room identity"));
    assert!(first.outbound.try_recv().is_err());
    assert!(second.outbound.try_recv().is_err());
    let state = hub.state.lock().unwrap();
    assert_eq!(state.rooms.len(), 1);
    assert_eq!(state.rooms[&("/ws".into(), "room-1".into())], first.id);
}

#[test]
fn admissions_invoke_opaque_callbacks_once_and_deliver_to_the_correct_seat() {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = restorable_pairing(1000, 4);
    let mut first = join(&hub, &pairing, 4).unwrap();
    let mut callbacks = Callbacks::default();
    first.dispatch_admission(&mut callbacks).unwrap();
    assert!(callbacks.calls.is_empty());
    assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
    let mut second = join(&hub, &pairing, 4).unwrap();
    second.dispatch_admission(&mut callbacks).unwrap();
    second.dispatch_admission(&mut callbacks).unwrap();
    assert_eq!(callbacks.calls, vec![("room-1".into(), 0)]);
    assert_eq!(first.outbound.try_recv().unwrap(), "matched-1");
    assert_eq!(second.outbound.try_recv().unwrap(), "matched-2");
    first
        .transition(|_| Ok(("retained".into(), None, None)))
        .unwrap();
    drop(second);
    assert_eq!(first.outbound.try_recv().unwrap(), "left");
    let mut restored = hub
        .join(
            "/ws".into(),
            "/opaque".into(),
            4,
            &pairing,
            Some(("room-1".into(), 2)),
        )
        .unwrap();
    restored.dispatch_admission(&mut callbacks).unwrap();
    assert_eq!(restored.outbound.try_recv().unwrap(), "restored-2:retained");
    assert!(first.outbound.try_recv().is_err());
}

#[test]
fn callback_failures_do_not_publish_partial_match_or_mutate_transition_state() {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = restorable_pairing(1000, 4);
    let first = join(&hub, &pairing, 4).unwrap();
    first.outbound.try_recv().unwrap();
    let mut second = join(&hub, &pairing, 4).unwrap();
    let mut callbacks = Callbacks {
        fail_match: true,
        ..Callbacks::default()
    };
    assert_eq!(
        second
            .dispatch_admission(&mut callbacks)
            .unwrap_err()
            .to_string(),
        "source callback rejected"
    );
    assert!(first.outbound.try_recv().is_err());
    assert!(second.outbound.try_recv().is_err());
    assert_eq!(
        first
            .transition(|_| Err("rejected".into()))
            .unwrap_err()
            .to_string(),
        "rejected"
    );
    first
        .transition(|context| {
            let (state, _, _, _) = context.unwrap();
            assert!(state.is_empty());
            Ok(("valid".into(), None, None))
        })
        .unwrap();
    drop(second);
    assert_eq!(first.outbound.try_recv().unwrap(), "left");
}

#[test]
fn concurrent_transitions_are_serialized_without_lost_updates() {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = restorable_pairing(1000, 4);
    let first = join(&hub, &pairing, 4).unwrap();
    let second = join(&hub, &pairing, 4).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let workers = [first, second].map(|lease| {
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            for _ in 0..100 {
                lease
                    .transition(|context| {
                        let (state, _, _, _) = context.unwrap();
                        let count = if state.is_empty() {
                            0
                        } else {
                            state.parse::<usize>().unwrap()
                        };
                        Ok(((count + 1).to_string(), None, None))
                    })
                    .unwrap();
            }
            lease
        })
    });
    let leases = workers.map(|worker| worker.join().unwrap());
    leases[0]
        .transition(|context| {
            let (state, _, _, _) = context.unwrap();
            assert_eq!(state, "200");
            Ok((state, None, None))
        })
        .unwrap();
}

fn restorable_pairing(retention_ms: u64, retained_room_capacity: usize) -> WebSocketPairing<()> {
    use crate::channel_plan::WebSocketRestoration;

    WebSocketPairing {
        waiting: "waiting".into(),
        first_matched: String::new(),
        second_matched: String::new(),
        peer_left: "left".into(),
        restoration: Some(WebSocketRestoration {
            waiting: (),
            peer_left: (),
            identity: (),
            room_identity: (),
            retention_ms,
            retained_room_capacity,
            matched: (),
            restored: (),
        }),
        inbound: (),
        cancellation: (),
    }
}

#[test]
fn optional_transition_deliveries_preserve_empty_frames_and_state() {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = restorable_pairing(1000, 4);
    let first = join(&hub, &pairing, 1).unwrap();
    first.outbound.try_recv().unwrap();
    let mut second = join(&hub, &pairing, 1).unwrap();
    admit_pair(&first, &mut second);
    first
        .transition(|_| Ok(("silent".into(), None, None)))
        .unwrap();
    assert!(first.outbound.try_recv().is_err());
    assert!(second.outbound.try_recv().is_err());
    first
        .transition(|context| {
            let (state, _, _, _) = context.unwrap();
            assert_eq!(state, "silent");
            Ok(("empty-frame".into(), Some(String::new()), None))
        })
        .unwrap();
    // An absent delivery must not touch even a full queue.
    second
        .transition(|context| {
            let (state, _, _, _) = context.unwrap();
            assert_eq!(state, "empty-frame");
            Ok((state, None, Some(String::new())))
        })
        .unwrap();
    assert!(second
        .transition(|context| Ok((context.unwrap().0, None, Some(String::new()))))
        .unwrap_err()
        .to_string()
        .contains("queue is full"));
    assert_eq!(first.outbound.try_recv().unwrap(), "");
    assert_eq!(second.outbound.try_recv().unwrap(), "");
    assert!(first.outbound.try_recv().is_err());
    assert!(second.outbound.try_recv().is_err());
    drop(second);
    first.outbound.try_recv().unwrap();
    first
        .transition(|context| {
            let (state, _, _, _) = context.unwrap();
            assert_eq!(state, "empty-frame");
            Ok((state, Some("survivor".into()), Some("disconnected".into())))
        })
        .unwrap();
    assert_eq!(first.outbound.try_recv().unwrap(), "survivor");
}

#[test]
fn websocket_hub_pairs_broadcasts_and_notifies_disconnect() {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = WebSocketPairing {
        waiting: "waiting".into(),
        first_matched: "first".into(),
        second_matched: "second".into(),
        peer_left: "left".into(),
        restoration: None,
        inbound: (),
        cancellation: (),
    };
    let first = hub
        .join("/ws".into(), "/ws?player=first".into(), 4, &pairing, None)
        .expect("join first peer");
    assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
    let second = hub
        .join("/ws".into(), "/ws?player=second".into(), 4, &pairing, None)
        .expect("join second peer");
    assert_eq!(first.outbound.try_recv().unwrap(), "first");
    assert_eq!(second.outbound.try_recv().unwrap(), "second");

    second
        .transition(|context| {
            Ok((
                context.unwrap().0,
                Some("update".into()),
                Some("update".into()),
            ))
        })
        .expect("broadcast update");
    assert_eq!(first.outbound.try_recv().unwrap(), "update");
    assert_eq!(second.outbound.try_recv().unwrap(), "update");
    drop(second);
    assert_eq!(first.outbound.try_recv().unwrap(), "left");
}

#[test]
fn websocket_hub_serializes_stateful_pair_transitions_and_addresses_peers() {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = WebSocketPairing {
        waiting: "waiting".into(),
        first_matched: "first".into(),
        second_matched: "second".into(),
        peer_left: "left".into(),
        restoration: None,
        inbound: (),
        cancellation: (),
    };
    let first = hub
        .join("/ws".into(), "/ws?player=Ada".into(), 4, &pairing, None)
        .expect("join first peer");
    assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
    let second = hub
        .join("/ws".into(), "/ws?player=Grace".into(), 4, &pairing, None)
        .expect("join second peer");
    assert_eq!(first.outbound.try_recv().unwrap(), "first");
    assert_eq!(second.outbound.try_recv().unwrap(), "second");

    second
        .transition(|context| {
            let (state, role, first_request, second_request) = context.unwrap();
            assert_eq!(state, "");
            assert_eq!(role, 2);
            assert_eq!(first_request, "/ws?player=Ada");
            assert_eq!(second_request, "/ws?player=Grace");
            Ok((
                "move-1".into(),
                Some("view-1a".into()),
                Some("view-1b".into()),
            ))
        })
        .expect("first transition");
    assert_eq!(first.outbound.try_recv().unwrap(), "view-1a");
    assert_eq!(second.outbound.try_recv().unwrap(), "view-1b");

    first
        .transition(|context| {
            let (state, role, _, _) = context.unwrap();
            assert_eq!(state, "move-1");
            assert_eq!(role, 1);
            Ok((
                "move-2".into(),
                Some("view-2a".into()),
                Some("view-2b".into()),
            ))
        })
        .expect("second transition");
    assert_eq!(first.outbound.try_recv().unwrap(), "view-2a");
    assert_eq!(second.outbound.try_recv().unwrap(), "view-2b");
}

#[test]
fn websocket_hub_restores_disconnected_seat_with_retained_state() {
    let pairing = restorable_pairing(300_000, 1_024);
    let hub = Arc::new(WebSocketHub::default());
    let first = hub
        .join("/ws".into(), "/ws?board=first".into(), 4, &pairing, None)
        .expect("join first peer");
    assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
    let mut second = hub
        .join("/ws".into(), "/ws?board=second".into(), 4, &pairing, None)
        .expect("join second peer");
    admit_pair(&first, &mut second);
    first
        .transition(|context| {
            let (state, role, first_request, second_request) = context.unwrap();
            assert_eq!(state, "");
            assert_eq!(role, 1);
            assert_eq!(first_request, "/ws?board=first");
            assert_eq!(second_request, "/ws?board=second");
            Ok((
                "1,5,5".into(),
                Some("first-view".into()),
                Some("second-view".into()),
            ))
        })
        .expect("retain first move");
    assert_eq!(first.outbound.try_recv().unwrap(), "first-view");
    assert_eq!(second.outbound.try_recv().unwrap(), "second-view");

    drop(second);
    assert_eq!(first.outbound.try_recv().unwrap(), "left");
    let restored = hub
        .join(
            "/ws".into(),
            "/ws?room_id=room-1&player_id=player-2".into(),
            4,
            &pairing,
            Some(("room-1".into(), 2)),
        )
        .expect("restore second seat");
    restored
        .transition(|context| {
            let (state, role, first_request, second_request) = context.unwrap();
            assert_eq!(state, "1,5,5");
            assert_eq!(role, 2);
            assert_eq!(first_request, "/ws?board=first");
            assert_eq!(second_request, "/ws?board=second");
            Ok((
                "1,5,5;2,4,4".into(),
                Some("next-first".into()),
                Some("next-second".into()),
            ))
        })
        .expect("transition restored seat");
    assert_eq!(first.outbound.try_recv().unwrap(), "next-first");
    assert_eq!(restored.outbound.try_recv().unwrap(), "next-second");
}

#[test]
fn websocket_hub_retains_room_after_both_peers_disconnect() {
    let pairing = restorable_pairing(300_000, 8);
    let hub = Arc::new(WebSocketHub::default());
    let first = hub
        .join("/ws".into(), "/ws?board=first".into(), 4, &pairing, None)
        .expect("join first peer");
    assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
    let mut second = hub
        .join("/ws".into(), "/ws?board=second".into(), 4, &pairing, None)
        .expect("join second peer");
    admit_pair(&first, &mut second);
    drop(first);
    drop(second);

    let restored_first = hub
        .join(
            "/ws".into(),
            "/ws?room_id=room-1&player_id=player-1".into(),
            4,
            &pairing,
            Some(("room-1".into(), 1)),
        )
        .expect("restore first seat");
    let restored_second = hub
        .join(
            "/ws".into(),
            "/ws?room_id=room-1&player_id=player-2".into(),
            4,
            &pairing,
            Some(("room-1".into(), 2)),
        )
        .expect("restore second seat");
    restored_second
        .transition(|context| {
            let (state, role, first_request, second_request) = context.unwrap();
            assert_eq!(state, "");
            assert_eq!(role, 2);
            assert_eq!(first_request, "/ws?board=first");
            assert_eq!(second_request, "/ws?board=second");
            Ok((
                "retained".into(),
                Some("first".into()),
                Some("second".into()),
            ))
        })
        .expect("transition retained room");
    assert_eq!(restored_first.outbound.try_recv().unwrap(), "first");
    assert_eq!(restored_second.outbound.try_recv().unwrap(), "second");
}

#[test]
fn websocket_hub_expires_fully_disconnected_room() {
    let pairing = restorable_pairing(10, 8);
    let hub = Arc::new(WebSocketHub::default());
    let first = hub
        .join("/ws".into(), "/ws?board=first".into(), 4, &pairing, None)
        .expect("join first peer");
    assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
    let mut second = hub
        .join("/ws".into(), "/ws?board=second".into(), 4, &pairing, None)
        .expect("join second peer");
    admit_pair(&first, &mut second);
    drop(first);
    drop(second);

    assert!(hub
        .state
        .lock()
        .unwrap()
        .rooms
        .contains_key(&("/ws".into(), "room-1".into())));

    let error = hub
        .join_at(
            "/ws".into(),
            "/ws?room_id=room-1&player_id=player-1".into(),
            4,
            &pairing,
            Some(("room-1".into(), 1)),
            std::time::Instant::now() + Duration::from_secs(1),
        )
        .err()
        .expect("expired room must reject restoration");
    assert!(error.to_string().contains("room not found"));
}

#[test]
fn websocket_hub_requires_valid_source_identity_and_keeps_room_isolation() {
    let hub = Arc::new(WebSocketHub::default());
    let mut pairing = restorable_pairing(1000, 4);
    for identity in [("", 1), ("room-1", 0), ("room-1", 3)] {
        let error = hub
            .join(
                "/ws".into(),
                "/ws".into(),
                4,
                &pairing,
                Some((identity.0.into(), identity.1)),
            )
            .err()
            .unwrap();
        assert!(error.to_string().contains("invalid resolved identity"));
    }
    let first = hub
        .join("/ws".into(), "/ws".into(), 4, &pairing, None)
        .unwrap();
    assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
    let mut second = hub
        .join("/ws".into(), "/ws".into(), 4, &pairing, None)
        .unwrap();
    admit_pair(&first, &mut second);
    assert!(hub
        .join(
            "/ws".into(),
            "/ws".into(),
            4,
            &pairing,
            Some(("room-1".into(), 1))
        )
        .err()
        .unwrap()
        .to_string()
        .contains("already connected"));
    drop(first);
    drop(second);
    assert!(hub
        .join(
            "/other".into(),
            "/other".into(),
            4,
            &pairing,
            Some(("room-1".into(), 1))
        )
        .err()
        .unwrap()
        .to_string()
        .contains("room not found"));
    // The hub accepts resolved identity, not query syntax or query-key policy.
    let restored = hub
        .join(
            "/ws".into(),
            "/opaque-target".into(),
            4,
            &pairing,
            Some(("room-1".into(), 1)),
        )
        .unwrap();
    drop(restored);
    pairing.restoration = None;
    assert!(hub
        .join(
            "/ws".into(),
            "/ws".into(),
            4,
            &pairing,
            Some(("room-1".into(), 1))
        )
        .err()
        .unwrap()
        .to_string()
        .contains("invalid resolved identity"));
}

#[test]
fn websocket_hub_evicts_oldest_fully_disconnected_room_at_capacity() {
    let pairing = restorable_pairing(300_000, 1);
    let hub = Arc::new(WebSocketHub::default());
    for suffix in ["first", "second"] {
        let first = hub
            .join(
                "/ws".into(),
                format!("/ws?board={suffix}-1"),
                4,
                &pairing,
                None,
            )
            .expect("join first peer");
        assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
        let mut second = hub
            .join(
                "/ws".into(),
                format!("/ws?board={suffix}-2"),
                4,
                &pairing,
                None,
            )
            .expect("join second peer");
        admit_pair(&first, &mut second);
        drop(first);
        drop(second);
    }

    let oldest_error = hub
        .join(
            "/ws".into(),
            "/ws?room_id=room-1&player_id=player-1".into(),
            4,
            &pairing,
            Some(("room-1".into(), 1)),
        )
        .err()
        .expect("oldest retained room must be evicted");
    assert!(oldest_error.to_string().contains("room not found"));
    hub.join(
        "/ws".into(),
        "/ws?room_id=room-2&player_id=player-1".into(),
        4,
        &pairing,
        Some(("room-2".into(), 1)),
    )
    .expect("newest retained room remains restorable");
}
