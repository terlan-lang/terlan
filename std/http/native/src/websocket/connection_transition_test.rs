use super::*;

#[test]
fn ordinary_callbacks_suspend_without_replay_and_cancel_once_on_failure_or_drop() {
    for event in ["inbound", "writable"] {
        for outcome in ["complete", "failure", "drop"] {
            let hub = Arc::new(WebSocketHub::default());
            let mut source = Source::new(None);
            let release = Arc::new(AtomicBool::new(false));
            source.pending_payload = Some((event, release.clone()));
            source.failure = (outcome == "failure").then_some(event);
            let (transport, state) = io(vec![Message::text("update"), Message::Close(None)]);
            let mut task = Box::pin(serve(
                ready(Ok(transport)),
                &mut source,
                &hub,
                "/ws".into(),
                "/".into(),
                || ready(()),
            ));
            for _ in 0..2 {
                assert!(task
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending());
            }
            assert_eq!(state.lock().unwrap().drops, 0);
            assert!(output(&state).is_empty());
            if outcome != "drop" {
                release.store(true, Ordering::SeqCst);
                let result = crate::callback_test_support::ready(&mut task);
                if outcome == "complete" {
                    result.unwrap();
                } else {
                    assert!(result.unwrap_err().to_string().contains("source failure"));
                }
            }
            drop(task);
            assert_eq!(state.lock().unwrap().drops, 1);
            assert!(!source.live.is_open());
            assert_eq!(
                source
                    .events
                    .iter()
                    .filter(|seen| seen.starts_with("cancel:"))
                    .count(),
                usize::from(outcome != "complete")
            );
            // Inbound is polled once more to observe the empty queue on success.
            assert_eq!(
                source.events.iter().filter(|seen| **seen == event).count(),
                if event == "inbound" && outcome == "complete" {
                    2
                } else {
                    1
                }
            );
        }
    }
}

#[test]
fn suspended_source_transition_owns_transport_and_releases_room_on_failure_or_drop() {
    for outcome in ["complete", "failure", "drop", "departure"] {
        let pair = pairing(false);
        let hub = Arc::new(WebSocketHub::default());
        let mut first = Some(
            hub.join("/ws".into(), "/first".into(), 8, &pair, None)
                .unwrap(),
        );
        first.as_ref().unwrap().outbound.try_recv().unwrap();
        let mut source = Source::new(Some(pair));
        let release = Arc::new(AtomicBool::new(false));
        source.pending_payload = Some(("stateful", release.clone()));
        source.failure = (outcome == "failure").then_some("stateful");
        let (transport, state) = io(vec![Message::text("update"), Message::Close(None)]);
        let mut task = Box::pin(serve(
            ready(Ok(transport)),
            &mut source,
            &hub,
            "/ws".into(),
            "/second".into(),
            || ready(()),
        ));
        for _ in 0..2 {
            assert!(task
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending());
        }
        assert_eq!(output(&state), [Message::text("second-matched")]);
        assert_eq!(
            first
                .as_ref()
                .unwrap()
                .outbound
                .try_iter()
                .collect::<Vec<_>>(),
            ["first-matched"]
        );
        assert_eq!(state.lock().unwrap().drops, 0);
        if outcome == "departure" {
            drop(first.take());
        }
        if outcome != "drop" {
            release.store(true, Ordering::SeqCst);
            let result = crate::callback_test_support::ready(&mut task);
            if outcome == "complete" {
                result.unwrap();
            } else {
                let error = result.unwrap_err().to_string();
                assert!(
                    error.contains(if outcome == "failure" {
                        "source failure"
                    } else {
                        "room changed"
                    }),
                    "{error}"
                );
            }
        }
        drop(task);
        assert_eq!(state.lock().unwrap().drops, 1);
        assert!(!source.live.is_open());
        assert_eq!(
            source
                .events
                .iter()
                .filter(|event| event.starts_with("cancel:"))
                .count(),
            usize::from(outcome != "complete")
        );
        if outcome == "complete" {
            assert_eq!(
                output(&state),
                [
                    Message::text("second-matched"),
                    Message::text("second-value"),
                    Message::Close(None)
                ]
            );
        } else {
            assert_eq!(
                source
                    .events
                    .iter()
                    .filter(|event| *event == "stateful")
                    .count(),
                1
            );
            assert_eq!(output(&state), [Message::text("second-matched")]);
        }
        if let Some(first) = &first {
            let payloads: Vec<_> = first.outbound.try_iter().collect();
            assert_eq!(
                payloads,
                if outcome == "complete" {
                    vec!["first-value", "left"]
                } else {
                    vec!["left"]
                }
            );
            crate::callback_test_support::ready(first.transition(|context| async move {
                assert!(context.is_none());
                Ok((String::new(), None, None))
            }))
            .unwrap();
        }
    }
}
