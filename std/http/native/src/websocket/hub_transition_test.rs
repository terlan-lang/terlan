use super::*;
use crate::callback_test_support::ready;
use futures_util::future::poll_fn;
use futures_util::task::{waker, ArcWake};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};

#[derive(Default)]
struct Wakes(AtomicUsize);

impl ArcWake for Wakes {
    fn wake_by_ref(wake: &Arc<Self>) {
        wake.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn paired() -> (Arc<WebSocketHub>, WebSocketHubLease, WebSocketHubLease) {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = restorable_pairing(300_000, 8);
    let first = join(&hub, &pairing, 8).unwrap();
    first.outbound.try_recv().unwrap();
    let mut second = join(&hub, &pairing, 8).unwrap();
    admit_pair(&first, &mut second);
    (hub, first, second)
}

#[test]
fn waiting_transitions_serialize_only_their_room_and_release_on_every_exit() {
    for outcome in ["complete", "error", "drop", "panic"] {
        let (hub, first, second) = paired();
        let released = AtomicBool::new(false);
        let calls = AtomicUsize::new(0);
        let mut active = Box::pin(first.transition(|context| async {
            assert_eq!(context.unwrap().0, "");
            calls.fetch_add(1, Ordering::SeqCst);
            poll_fn(|_| {
                if released.load(Ordering::SeqCst) {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
            match outcome {
                "error" => Err("source failed".into()),
                "panic" => panic!("source panicked outside registry lock"),
                _ => Ok(("one".into(), Some("one".into()), Some("one".into()))),
            }
        }));
        for _ in 0..2 {
            assert!(active
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending());
        }
        let started = AtomicBool::new(false);
        let mut queued = Box::pin(second.transition(|context| async {
            started.store(true, Ordering::SeqCst);
            assert_eq!(
                context.unwrap().0,
                if outcome == "complete" { "one" } else { "" }
            );
            Ok(("two".into(), Some("two".into()), Some("two".into())))
        }));
        let wakes = Arc::new(Wakes::default());
        let waker = waker(wakes.clone());
        assert!(queued
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending());
        assert!(!started.load(Ordering::SeqCst));
        assert!(first.outbound.try_recv().is_err());
        assert!(second.outbound.try_recv().is_err());

        // Same route, different room: neither admission nor source execution
        // waits for the active room's source callback.
        let pairing = restorable_pairing(300_000, 8);
        let third = join(&hub, &pairing, 8).unwrap();
        third.outbound.try_recv().unwrap();
        let mut fourth = join(&hub, &pairing, 8).unwrap();
        admit_pair(&third, &mut fourth);
        ready(third.transition(|_| async { Ok(("independent".into(), None, None)) })).unwrap();

        if outcome != "drop" {
            released.store(true, Ordering::SeqCst);
            if outcome == "panic" {
                assert!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ready(&mut active)))
                        .is_err()
                );
            } else {
                let result = ready(&mut active);
                assert_eq!(result.is_ok(), outcome == "complete");
            }
        }
        drop(active);
        assert!(
            wakes.0.load(Ordering::SeqCst) > 0,
            "queued peer must be woken"
        );
        ready(queued).unwrap();
        assert!(started.load(Ordering::SeqCst));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        for lease in [&first, &second] {
            let messages: Vec<_> = lease.outbound.try_iter().collect();
            assert_eq!(
                messages,
                if outcome == "complete" {
                    vec!["one", "two"]
                } else {
                    vec!["two"]
                }
            );
        }
        assert_eq!(hub.state.lock().unwrap().pairs[&first.id].state, "two");
    }
}

#[test]
fn membership_changes_discard_waiting_results_without_replaying_source() {
    for change in [
        "departure",
        "restoration",
        "session_removed",
        "pair_removed",
    ] {
        let (hub, first, second) = paired();
        let mut second = Some(second);
        if change == "restoration" {
            drop(second.take());
            first.outbound.try_recv().unwrap();
        }
        let calls = AtomicUsize::new(0);
        let released = AtomicBool::new(false);
        let mut future = Box::pin(first.transition(|_| async {
            calls.fetch_add(1, Ordering::SeqCst);
            poll_fn(|_| {
                if released.load(Ordering::SeqCst) {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
            Ok(("stale".into(), Some("stale".into()), Some("stale".into())))
        }));
        assert!(future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        let restored = match change {
            "departure" => {
                drop(second.take());
                None
            }
            "restoration" => Some(
                hub.join(
                    "/ws".into(),
                    "restored".into(),
                    8,
                    &restorable_pairing(300_000, 8),
                    Some(("room-1".into(), 2)),
                )
                .unwrap(),
            ),
            "session_removed" => {
                hub.leave(first.id);
                None
            }
            _ => {
                hub.state.lock().unwrap().pairs.remove(&first.id);
                None
            }
        };
        released.store(true, Ordering::SeqCst);
        let error = ready(future).unwrap_err().to_string();
        assert!(
            error.contains("room changed")
                || error.contains("no longer registered")
                || error.contains("state disappeared"),
            "{error}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        for lease in [Some(&first), second.as_ref(), restored.as_ref()]
            .into_iter()
            .flatten()
        {
            assert!(lease.outbound.try_iter().all(|message| message != "stale"));
        }
        if let Some(pair) = hub.state.lock().unwrap().pairs.get(&first.id) {
            assert_eq!(pair.state, "");
        };
    }
}

#[test]
fn unpaired_work_cannot_publish_after_a_peer_arrives() {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = restorable_pairing(300_000, 8);
    let first = join(&hub, &pairing, 8).unwrap();
    first.outbound.try_recv().unwrap();
    let release = AtomicBool::new(false);
    let mut active = Box::pin(first.transition(|context| async {
        assert_eq!(context.map(|_| ()), None);
        poll_fn(|_| {
            if release.load(Ordering::SeqCst) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
        Ok(("unpaired".into(), Some("unpaired".into()), None))
    }));
    assert!(active
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
        .is_pending());
    let mut queued = Box::pin(
        first.transition(|_| async { panic!("changed room must reject before invocation") }),
    );
    assert!(queued
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
        .is_pending());
    let second = join(&hub, &pairing, 8).unwrap();
    release.store(true, Ordering::SeqCst);
    assert!(ready(active)
        .unwrap_err()
        .to_string()
        .contains("room changed"));
    assert!(ready(queued)
        .unwrap_err()
        .to_string()
        .contains("room changed"));
    assert!(first.outbound.try_recv().is_err());
    assert!(second.outbound.try_recv().is_err());
}

#[test]
fn match_publication_and_source_transition_cannot_commit_conflicting_views() {
    for match_first in [false, true] {
        let hub = Arc::new(WebSocketHub::default());
        let pairing = restorable_pairing(300_000, 8);
        let first = join(&hub, &pairing, 8).unwrap();
        first.outbound.try_recv().unwrap();
        let mut second = join(&hub, &pairing, 8).unwrap();
        let release = AtomicBool::new(false);
        let mut transition = Box::pin(first.transition(|_| async {
            poll_fn(|_| {
                if release.load(Ordering::SeqCst) {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
            Ok((
                "changed".into(),
                Some("changed".into()),
                Some("changed".into()),
            ))
        }));
        assert!(transition
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        if match_first {
            admit_pair(&first, &mut second);
            release.store(true, Ordering::SeqCst);
            assert!(ready(transition)
                .unwrap_err()
                .to_string()
                .contains("room changed"));
            assert!(first.outbound.try_recv().is_err());
            assert_eq!(hub.state.lock().unwrap().pairs[&first.id].state, "");
        } else {
            release.store(true, Ordering::SeqCst);
            ready(transition).unwrap();
            assert!(ready(second.dispatch_admission(&mut Callbacks::default()))
                .unwrap_err()
                .to_string()
                .contains("room changed during match admission"));
            assert_eq!(first.outbound.try_iter().collect::<Vec<_>>(), ["changed"]);
            assert!(hub.state.lock().unwrap().rooms.is_empty());
        }
    }
}

#[test]
fn invalidated_registry_is_rechecked_after_permit_wait_and_source_wait() {
    for phase in ["entry", "permit", "source"] {
        for invalid in ["session", "pair", "poison"] {
            let (hub, first, _second) = paired();
            let execution = Arc::clone(&hub.state.lock().unwrap().pairs[&first.id].execution);
            let permit = (phase == "permit").then(|| execution.try_lock().unwrap());
            let released = AtomicBool::new(false);
            let calls = AtomicUsize::new(0);
            let mut future = Box::pin(first.transition(|_| async {
                calls.fetch_add(1, Ordering::SeqCst);
                poll_fn(|_| {
                    if released.load(Ordering::SeqCst) {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                })
                .await;
                Ok(("never committed".into(), None, None))
            }));
            if phase != "entry" {
                assert!(future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending());
            }
            match invalid {
                "session" => {
                    hub.state.lock().unwrap().sessions.remove(&first.id);
                }
                "pair" => {
                    hub.state.lock().unwrap().pairs.remove(&first.id);
                }
                _ => {
                    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let _guard = hub.state.lock().unwrap();
                        panic!("injected registry corruption");
                    }))
                    .is_err());
                }
            }
            drop(permit);
            released.store(true, Ordering::SeqCst);
            let error = ready(future).unwrap_err().to_string();
            assert!(
                error.contains(match invalid {
                    "session" => "no longer registered",
                    "pair" => "state disappeared",
                    _ => "lock poisoned",
                }),
                "{phase}/{invalid}: {error}"
            );
            assert_eq!(calls.load(Ordering::SeqCst), usize::from(phase == "source"));
            assert!(first.outbound.try_recv().is_err());
        }
    }
}
