use super::*;
use crate::channel_plan::SseCallbacks as Callbacks;
use futures_util::FutureExt;
use hyper::body::Body;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use terlan_runtime_abi::{native_record, NativeValue as V};

#[derive(Debug, Default)]
struct Trace {
    events: Vec<SseEvent>,
    cancellations: Vec<String>,
    future_drops: usize,
    release: bool,
    waker: Option<Waker>,
}

struct Active(Arc<Mutex<Trace>>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.lock().unwrap().future_drops += 1;
    }
}

#[derive(Debug)]
struct Producer {
    trace: Arc<Mutex<Trace>>,
    results: VecDeque<Result<V, String>>,
    park: bool,
    park_open: bool,
    fail_open: bool,
    park_drain: bool,
    fail_drain: bool,
    fail_cancel: bool,
    fail_ready: bool,
}

impl CallbackInvocation<SseEvent> for Producer {
    type Value = V;
    type Wait = ();
    type Wake = ();

    fn is_waiting(&self) -> bool {
        false
    }
    fn pending_wait(&self) -> Result<Option<()>, String> {
        Ok(None)
    }
    fn validate_text_wait(_: &()) -> Result<(), String> {
        Ok(())
    }
    fn text_wake(_: (), _: String) {}
    fn resume(&mut self, _: ()) -> Result<CallbackState<V, ()>, String> {
        panic!("pull producers use their owned future, not a text wake")
    }
    fn cancel_pending(&mut self, reason: String) -> Result<(), String> {
        self.trace.lock().unwrap().cancellations.push(reason);
        if self.fail_cancel {
            Err("cleanup failed".into())
        } else {
            Ok(())
        }
    }
    fn invoke(
        &mut self,
        event: SseEvent,
        _: Option<&V>,
        _: Vec<V>,
    ) -> Result<CallbackState<V, ()>, String> {
        self.trace.lock().unwrap().events.push(event);
        Ok(CallbackState::Complete(V::Unit))
    }
    async fn invoke_suspendable(
        &mut self,
        event: SseEvent,
        _: &V,
        args: Vec<V>,
    ) -> Result<V, String> {
        self.trace.lock().unwrap().events.push(event);
        if event == SseEvent::EventReady {
            assert_eq!(args, vec![V::from("payload")]);
            if self.fail_ready {
                return Err("event-ready failed".into());
            }
            return Ok(V::Unit);
        }
        assert!(matches!(
            event,
            SseEvent::Open | SseEvent::Produce | SseEvent::Drain
        ));
        let _active = Active(Arc::clone(&self.trace));
        if (event == SseEvent::Produce && self.park)
            || (event == SseEvent::Open && self.park_open)
            || (event == SseEvent::Drain && self.park_drain)
        {
            std::future::poll_fn(|context| {
                let mut trace = self.trace.lock().unwrap();
                if trace.release {
                    Poll::Ready(())
                } else {
                    trace.waker = Some(context.waker().clone());
                    Poll::Pending
                }
            })
            .await;
        }
        if event == SseEvent::Open {
            return if self.fail_open {
                Err("open failed".into())
            } else {
                Ok(V::Unit)
            };
        }
        if event == SseEvent::Drain {
            return if self.fail_drain {
                Err("drain failed".into())
            } else {
                Ok(V::Unit)
            };
        }
        self.results
            .pop_front()
            .unwrap_or(Ok(V::Atom("none".into())))
    }
}

fn produced(encoded: &str) -> V {
    let value = V::Tuple(vec![encoded.into(), "payload".into()]);
    some(value)
}

fn some(value: V) -> V {
    native_record!(Some, { value })
}

fn session(
    results: Vec<Result<V, String>>,
    park: bool,
    limit: usize,
) -> (SseCallbacks<Producer>, Arc<Mutex<Trace>>) {
    let (executor, plan, trace) = session_parts(results, park, limit);
    (
        SseCallbacks::open(executor, SseSession::open(plan)).unwrap(),
        trace,
    )
}

fn prepared_session() -> (SseCallbacks<Producer>, Arc<Mutex<Trace>>) {
    let (executor, plan, trace) = session_parts(vec![Ok(produced("data: payload\n\n"))], false, 64);
    (
        SseCallbacks::prepare_pull(executor, SseSession::open(plan)).unwrap(),
        trace,
    )
}

fn session_parts(
    results: Vec<Result<V, String>>,
    park: bool,
    limit: usize,
) -> (Producer, SseEndpointPlan<V>, Arc<Mutex<Trace>>) {
    let trace = Arc::new(Mutex::new(Trace::default()));
    let executor = Producer {
        trace: Arc::clone(&trace),
        results: results.into(),
        park,
        park_open: false,
        fail_open: false,
        park_drain: false,
        fail_drain: false,
        fail_cancel: false,
        fail_ready: false,
    };
    let plan = SseEndpointPlan::new(1, limit)
        .unwrap()
        .with_producer(V::Int(6))
        .unwrap()
        .with_callbacks(Callbacks {
            open: V::Int(1),
            event_ready: V::Int(2),
            keep_alive: V::Int(3),
            drain: V::Int(4),
            cancellation: V::Int(5),
        })
        .unwrap();
    (executor, plan, trace)
}

fn poll(
    body: &mut crate::response_body::ResponseBody,
) -> Poll<Option<Result<Frame<Bytes>, ServiceError>>> {
    Pin::new(body).poll_frame(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn production_is_lazy_bounded_and_drains_without_cancellation() {
    let encoded = crate::encode_event(None, None, None, "payload").unwrap();
    let (session, trace) = session(vec![Ok(produced(&encoded))], false, encoded.len());
    let mut body = session.into_body().unwrap();
    assert_eq!(trace.lock().unwrap().events, [SseEvent::Open]);
    assert_eq!(body.size_hint().exact(), None);
    assert!(!body.is_end_stream());
    let Poll::Ready(Some(Ok(frame))) = poll(&mut body) else {
        panic!("first event")
    };
    assert_eq!(frame.into_data().unwrap(), encoded);
    assert_eq!(
        trace.lock().unwrap().events,
        [SseEvent::Open, SseEvent::Produce, SseEvent::EventReady]
    );
    assert!(matches!(poll(&mut body), Poll::Ready(None)));
    assert!(matches!(poll(&mut body), Poll::Ready(None)));
    drop(body);
    let trace = trace.lock().unwrap();
    assert_eq!(
        trace.events,
        [
            SseEvent::Open,
            SseEvent::Produce,
            SseEvent::EventReady,
            SseEvent::Produce,
            SseEvent::Drain
        ]
    );
    assert_eq!(trace.future_drops, 3);
    assert_eq!(trace.cancellations, ["SSE transport began graceful drain"]);
}

#[test]
fn pending_producer_is_not_reentered_and_drop_releases_its_future() {
    let (session, trace) = session(vec![], true, 64);
    let mut body = session.into_body().unwrap();
    for _ in 0..3 {
        assert!(poll(&mut body).is_pending());
    }
    assert_eq!(
        trace.lock().unwrap().events,
        [SseEvent::Open, SseEvent::Produce]
    );
    drop(body);
    let trace = trace.lock().unwrap();
    assert_eq!(trace.future_drops, 1);
    assert_eq!(trace.events.last(), Some(&SseEvent::Cancellation));
    assert_eq!(trace.cancellations.len(), 1);
}

#[test]
fn unpolled_body_cancels_without_starting_a_producer() {
    let (session, trace) = session(vec![], true, 64);
    drop(session.into_body().unwrap());
    let trace = trace.lock().unwrap();
    assert_eq!(trace.events, [SseEvent::Open, SseEvent::Cancellation]);
    assert_eq!(trace.future_drops, 0);
    assert_eq!(trace.cancellations.len(), 1);
}

#[test]
fn admitted_frames_precede_production_and_record_none_drains() {
    let (mut session, trace) = session(
        vec![Ok(V::Record {
            name: "None".into(),
            fields: vec![],
        })],
        false,
        64,
    );
    session.enqueue_event("queued".into()).unwrap();
    let mut body = session.into_body().unwrap();
    let Poll::Ready(Some(Ok(frame))) = poll(&mut body) else {
        panic!("queued frame")
    };
    assert_eq!(
        frame.into_data().unwrap(),
        crate::encode_event(None, None, None, "queued").unwrap()
    );
    assert!(!trace.lock().unwrap().events.contains(&SseEvent::Produce));
    assert!(matches!(poll(&mut body), Poll::Ready(None)));
    assert_eq!(trace.lock().unwrap().events.last(), Some(&SseEvent::Drain));
}

#[test]
fn event_ready_failure_prevents_frame_delivery_and_cancels_once() {
    let (mut session, trace) = session(vec![Ok(produced("data:payload\n\n"))], false, 64);
    session.invocation.fail_ready = true;
    let mut body = session.into_body().unwrap();
    let Poll::Ready(Some(Err(error))) = poll(&mut body) else {
        panic!("event-ready failure")
    };
    assert!(error.to_string().contains("event-ready failed"));
    assert!(matches!(poll(&mut body), Poll::Ready(None)));
    drop(body);
    assert_eq!(trace.lock().unwrap().cancellations.len(), 1);
}

#[test]
fn producer_without_lifecycle_callbacks_and_invalid_internal_states_are_explicit() {
    let (mut live, _) = session(vec![Ok(produced("data:payload\n\n"))], false, 64);
    live.live = SseSession::open(
        SseEndpointPlan::new(1, 64)
            .unwrap()
            .with_producer(V::Int(6))
            .unwrap(),
    );
    assert!(live
        .next_produced_frame()
        .now_or_never()
        .unwrap()
        .unwrap()
        .is_some());
    assert_eq!(live.live.inspect().emitted_events, 1);
    live.live = SseSession::open(SseEndpointPlan::new(1, 64).unwrap());
    let error = live
        .next_produced_frame()
        .now_or_never()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("missing source producer"));
    live.live.close();
    assert!(live
        .next_produced_frame()
        .now_or_never()
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("serve.sse.closed"));

    let (mut live, _) = session(vec![], false, 64);
    live.live = SseSession::open(
        SseEndpointPlan::new(1, 64)
            .unwrap()
            .with_producer(V::Int(6))
            .unwrap()
            .with_keep_alive_ms(1)
            .unwrap(),
    );
    assert!(live.into_body().is_err());
}

#[test]
fn producer_wake_resumes_the_same_future() {
    let (session, trace) = session(vec![Ok(produced("data: payload\n\n"))], true, 64);
    let mut body = session.into_body().unwrap();
    assert!(poll(&mut body).is_pending());
    let waker = {
        let mut trace = trace.lock().unwrap();
        trace.release = true;
        trace.waker.take().unwrap()
    };
    waker.wake();
    assert!(matches!(poll(&mut body), Poll::Ready(Some(Ok(_)))));
    assert_eq!(trace.lock().unwrap().future_drops, 1);
    assert_eq!(
        trace.lock().unwrap().events,
        [SseEvent::Open, SseEvent::Produce, SseEvent::EventReady]
    );
}

#[test]
fn malformed_or_oversized_production_terminates_without_emitting() {
    for result in [
        Err("producer failed".into()),
        Ok(V::Unit),
        Ok(some(V::Unit)),
        Ok(some(V::Tuple(vec!["encoded".into(), V::Int(1)]))),
        Ok(some(V::Tuple(vec![V::Int(1), "data".into()]))),
        Ok(produced("")),
        Ok(produced(&"x".repeat(65))),
    ] {
        let (session, trace) = session(vec![result], false, 64);
        let mut body = session.into_body().unwrap();
        assert!(matches!(poll(&mut body), Poll::Ready(Some(Err(_)))));
        assert!(matches!(poll(&mut body), Poll::Ready(None)));
        drop(body);
        let trace = trace.lock().unwrap();
        assert_eq!(
            trace.events,
            [SseEvent::Open, SseEvent::Produce, SseEvent::Cancellation]
        );
        assert_eq!(trace.cancellations.len(), 1);
    }
}

#[test]
fn unsupported_policy_and_cleanup_failure_remain_explicit() {
    let (mut failed, trace) = session(vec![Err("producer failed".into())], false, 64);
    failed.invocation.fail_cancel = true;
    let mut body = failed.into_body().unwrap();
    let Poll::Ready(Some(Err(error))) = poll(&mut body) else {
        panic!("error frame")
    };
    assert!(error.to_string().contains("producer failed"));
    assert!(error.to_string().contains("cleanup failed"));
    drop(body);
    assert_eq!(trace.lock().unwrap().cancellations.len(), 1);

    let (mut draining, trace) = session(vec![], false, 64);
    draining.invocation.fail_cancel = true;
    let mut body = draining.into_body().unwrap();
    let Poll::Ready(Some(Err(error))) = poll(&mut body) else {
        panic!("drain cleanup failure")
    };
    assert!(error.to_string().contains("cleanup failed"));
    drop(body);
    assert_eq!(trace.lock().unwrap().cancellations.len(), 1);

    let plan = SseEndpointPlan::new(1, 64).unwrap();
    let executor = Producer {
        trace: Arc::clone(&trace),
        results: VecDeque::new(),
        park: false,
        park_open: false,
        fail_open: false,
        park_drain: false,
        fail_drain: false,
        fail_cancel: false,
        fail_ready: false,
    };
    let session = SseCallbacks::open(executor, SseSession::open(plan)).unwrap();
    assert!(session.into_body().is_err());
}

#[test]
fn graceful_eof_waits_for_source_drain_and_resumes_without_reentry() {
    let (mut session, trace) = session(vec![], false, 64);
    session.invocation.park_drain = true;
    let mut body = session.into_body().unwrap();
    for _ in 0..3 {
        assert!(poll(&mut body).is_pending());
        assert!(!body.is_end_stream());
    }
    assert_eq!(
        trace.lock().unwrap().events,
        [SseEvent::Open, SseEvent::Produce, SseEvent::Drain]
    );
    let waker = {
        let mut trace = trace.lock().unwrap();
        assert_eq!(trace.future_drops, 1);
        trace.release = true;
        trace.waker.take().unwrap()
    };
    waker.wake();
    assert!(matches!(poll(&mut body), Poll::Ready(None)));
    assert!(matches!(poll(&mut body), Poll::Ready(None)));
    drop(body);
    let trace = trace.lock().unwrap();
    assert_eq!(trace.future_drops, 2);
    assert_eq!(
        trace.events,
        [SseEvent::Open, SseEvent::Produce, SseEvent::Drain]
    );
    assert_eq!(trace.cancellations, ["SSE transport began graceful drain"]);
}

#[test]
fn disconnect_during_source_drain_cancels_once_and_releases_pending_future() {
    let (mut session, trace) = session(vec![], false, 64);
    session.invocation.park_drain = true;
    let mut body = session.into_body().unwrap();
    assert!(poll(&mut body).is_pending());
    drop(body);
    let trace = trace.lock().unwrap();
    assert_eq!(trace.future_drops, 2);
    assert_eq!(
        trace.events,
        [
            SseEvent::Open,
            SseEvent::Produce,
            SseEvent::Drain,
            SseEvent::Cancellation
        ]
    );
    assert_eq!(
        trace.cancellations,
        [
            "SSE transport began graceful drain",
            "SSE session owner dropped"
        ]
    );
}

#[test]
fn source_drain_failure_is_terminal_and_never_replayed() {
    let (mut session, trace) = session(vec![], false, 64);
    session.invocation.fail_drain = true;
    let mut body = session.into_body().unwrap();
    let Poll::Ready(Some(Err(error))) = poll(&mut body) else {
        panic!("drain failure");
    };
    assert!(error.to_string().contains("drain failed"));
    assert!(matches!(poll(&mut body), Poll::Ready(None)));
    drop(body);
    let trace = trace.lock().unwrap();
    assert_eq!(
        trace.events,
        [SseEvent::Open, SseEvent::Produce, SseEvent::Drain]
    );
    assert_eq!(trace.future_drops, 2);
    assert_eq!(trace.cancellations.len(), 1);
}

#[test]
fn producer_without_callbacks_completes_and_closed_drain_is_rejected() {
    let (mut live, trace) = session(vec![], false, 64);
    live.live = SseSession::open(
        SseEndpointPlan::new(1, 64)
            .unwrap()
            .with_producer(V::Int(6))
            .unwrap(),
    );
    assert!(live
        .next_produced_frame()
        .now_or_never()
        .unwrap()
        .unwrap()
        .is_none());
    assert!(!live.is_open());
    assert!(live
        .drain_producer()
        .now_or_never()
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("serve.sse.closed"));
    drop(live);
    assert_eq!(
        trace.lock().unwrap().events,
        [SseEvent::Open, SseEvent::Produce]
    );
}

#[test]
fn deferred_open_precedes_every_frame_and_is_not_reentered() {
    let (mut session, trace) = prepared_session();
    session.invocation.park_open = true;
    let mut body = session.into_body().unwrap();
    assert!(trace.lock().unwrap().events.is_empty());
    for _ in 0..3 {
        assert!(poll(&mut body).is_pending());
    }
    let waker = {
        let mut trace = trace.lock().unwrap();
        assert_eq!(trace.events, [SseEvent::Open]);
        assert_eq!(trace.future_drops, 0);
        trace.release = true;
        trace.waker.take().unwrap()
    };
    waker.wake();
    let Poll::Ready(Some(Ok(frame))) = poll(&mut body) else {
        panic!("first frame after open");
    };
    assert_eq!(frame.into_data().unwrap(), "data: payload\n\n");
    assert!(matches!(poll(&mut body), Poll::Ready(None)));
    drop(body);
    let trace = trace.lock().unwrap();
    assert_eq!(
        trace.events,
        [
            SseEvent::Open,
            SseEvent::Produce,
            SseEvent::EventReady,
            SseEvent::Produce,
            SseEvent::Drain
        ]
    );
    assert_eq!(trace.future_drops, 4);
    assert_eq!(trace.cancellations, ["SSE transport began graceful drain"]);
}

#[test]
fn disconnect_before_or_during_open_releases_the_lease_once() {
    for start_open in [false, true] {
        let (mut session, trace) = prepared_session();
        session.invocation.park_open = true;
        let mut body = session.into_body().unwrap();
        if start_open {
            assert!(poll(&mut body).is_pending());
        }
        drop(body);
        let trace = trace.lock().unwrap();
        assert_eq!(
            trace.events,
            if start_open {
                vec![SseEvent::Open, SseEvent::Cancellation]
            } else {
                vec![SseEvent::Cancellation]
            }
        );
        assert_eq!(trace.future_drops, usize::from(start_open));
        assert_eq!(trace.cancellations, ["SSE session owner dropped"]);
    }
}

#[test]
fn open_failure_aborts_production_and_preserves_cleanup_failure() {
    for fail_cancel in [false, true] {
        let (mut session, trace) = prepared_session();
        session.invocation.fail_open = true;
        session.invocation.fail_cancel = fail_cancel;
        let mut body = session.into_body().unwrap();
        let Poll::Ready(Some(Err(error))) = poll(&mut body) else {
            panic!("open failure");
        };
        assert!(error.to_string().contains("open failed"));
        assert_eq!(error.to_string().contains("cleanup failed"), fail_cancel);
        assert!(matches!(poll(&mut body), Poll::Ready(None)));
        drop(body);
        let trace = trace.lock().unwrap();
        assert_eq!(
            trace.events,
            if fail_cancel {
                vec![SseEvent::Open]
            } else {
                vec![SseEvent::Open, SseEvent::Cancellation]
            }
        );
        assert_eq!(trace.future_drops, 1);
        assert_eq!(trace.cancellations, ["open failed"]);
    }
}

#[test]
fn deferred_open_rejects_early_events_but_allows_cancellation() {
    let (mut session, trace) = prepared_session();
    session
        .live
        .enqueue(None, None, None, "queued before admission")
        .unwrap();
    for result in [
        session.enqueue_event("early".into()),
        session.event_ready("early".into()),
        session.keep_alive(),
        session.drain(),
        session.resume(()),
    ] {
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("serve.sse.open_pending"));
    }
    assert!(session.flush_next_event().is_none());
    assert!(trace.lock().unwrap().events.is_empty());
    session.cancel("cancel before open".into()).unwrap();
    assert!(session
        .open_producer()
        .now_or_never()
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("serve.sse.closed"));
    drop(session);
    assert_eq!(trace.lock().unwrap().events, [SseEvent::Cancellation]);
}

#[test]
fn deferred_open_requires_live_pull_policy_and_allows_absent_callbacks() {
    for mode in 0..3 {
        let (executor, plan, trace) = session_parts(vec![], false, 64);
        let mut live = SseSession::open(match mode {
            0 => SseEndpointPlan::new(1, 64).unwrap(),
            1 => plan.with_keep_alive_ms(1).unwrap(),
            _ => plan,
        });
        if mode == 2 {
            live.close();
        }
        assert!(SseCallbacks::prepare_pull(executor, live)
            .unwrap_err()
            .to_string()
            .contains("serve.sse.producer"));
        assert!(trace.lock().unwrap().events.is_empty());
    }
    let (executor, _, trace) = session_parts(vec![], false, 64);
    let plan = SseEndpointPlan::new(1, 64)
        .unwrap()
        .with_producer(V::Int(6))
        .unwrap();
    let session = SseCallbacks::prepare_pull(executor, SseSession::open(plan)).unwrap();
    let mut body = session.into_body().unwrap();
    assert!(matches!(poll(&mut body), Poll::Ready(None)));
    drop(body);
    assert_eq!(trace.lock().unwrap().events, [SseEvent::Produce]);
}
