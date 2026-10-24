use super::*;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct Audit {
    events: Vec<SseEvent>,
    cancellations: Vec<String>,
    waiting: bool,
}

#[derive(Debug)]
struct Observed {
    executor: Executor<SseEvent>,
    audit: Arc<Mutex<Audit>>,
}

impl Drop for Observed {
    fn drop(&mut self) {
        *self.audit.lock().unwrap() = Audit {
            events: self.executor.calls.iter().map(|call| call.0).collect(),
            cancellations: self.executor.cancellations.clone(),
            waiting: self.executor.is_waiting(),
        };
    }
}

impl CallbackInvocation<SseEvent> for Observed {
    type Value = V;
    type Wait = Wait;
    type Wake = (Wait, String);

    fn is_waiting(&self) -> bool {
        self.executor.is_waiting()
    }
    fn pending_wait(&self) -> Result<Option<Wait>, String> {
        self.executor.pending_wait()
    }
    fn validate_text_wait(wait: &Wait) -> Result<(), String> {
        Executor::<SseEvent>::validate_text_wait(wait)
    }
    fn text_wake(wait: Wait, text: String) -> Self::Wake {
        (wait, text)
    }
    fn invoke(
        &mut self,
        event: SseEvent,
        callback: Option<&V>,
        args: Vec<V>,
    ) -> Result<State, String> {
        self.executor.invoke(event, callback, args)
    }
    async fn invoke_suspendable(
        &mut self,
        event: SseEvent,
        callback: &V,
        args: Vec<V>,
    ) -> Result<V, String> {
        self.executor
            .invoke_suspendable(event, callback, args)
            .await
    }
    fn resume(&mut self, wake: Self::Wake) -> Result<State, String> {
        self.executor.resume(wake)
    }
    fn cancel_pending(&mut self, reason: String) -> Result<(), String> {
        self.executor.cancel_pending(reason)
    }
}

fn observed(
    executor: Executor<SseEvent>,
    closed: bool,
) -> (
    Result<SseCallbacks<Observed>, ServiceError>,
    Arc<Mutex<Audit>>,
) {
    let audit = Arc::new(Mutex::new(Audit::default()));
    let mut live = SseSession::open(SseEndpointPlan::new(2, 64).unwrap());
    if closed {
        live.close();
    }
    let result = SseCallbacks::open(
        Observed {
            executor,
            audit: Arc::clone(&audit),
        },
        live,
    );
    (result, audit)
}

#[test]
fn dropping_live_sessions_cancels_once_including_parked_open_callbacks() {
    for waiting in [false, true] {
        for cleanup_fails in [false, true] {
            let mut executor = Executor {
                cancel_error: cleanup_fails,
                ..Executor::default()
            };
            if waiting {
                executor.results.push_back(Ok(State::Waiting(Wait::Text)));
            }
            let (session, audit) = observed(executor, false);
            drop(session.unwrap());
            let audit = audit.lock().unwrap();
            assert_eq!(audit.cancellations, ["SSE session owner dropped"]);
            assert!(!audit.waiting);
            assert_eq!(
                audit.events,
                if cleanup_fails {
                    vec![SseEvent::Open]
                } else {
                    vec![SseEvent::Open, SseEvent::Cancellation]
                }
            );
        }
    }
}

#[test]
fn dropping_terminal_sessions_does_not_replay_source_cleanup() {
    for cancel in [false, true] {
        for failure in [None, Some("terminal failed")] {
            let (session, audit) = observed(Executor::default(), false);
            let mut session = session.unwrap();
            if let Some(error) = failure {
                session
                    .invocation
                    .executor
                    .results
                    .push_back(Err(error.into()));
            }
            let result = if cancel {
                session.cancel("finished".into())
            } else {
                session.drain()
            };
            assert_eq!(result.is_err(), failure.is_some());
            drop(session);
            let audit = audit.lock().unwrap();
            assert_eq!(
                audit.events,
                [
                    SseEvent::Open,
                    if cancel {
                        SseEvent::Cancellation
                    } else {
                        SseEvent::Drain
                    }
                ]
            );
            assert_eq!(audit.cancellations.len(), 1);
        }
    }
}

#[test]
fn failed_open_preserves_primary_and_cleanup_errors_without_double_cancellation() {
    for cleanup_fails in [false, true] {
        let mut executor = Executor {
            cancel_error: cleanup_fails,
            ..Executor::default()
        };
        executor.results.push_back(Err("open failed".into()));
        let (session, audit) = observed(executor, false);
        let error = session.unwrap_err().to_string();
        assert!(error.starts_with("open failed"), "{error}");
        assert_eq!(
            error.contains("cancellation failed: cancel failed"),
            cleanup_fails
        );
        let audit = audit.lock().unwrap();
        assert_eq!(audit.cancellations, ["open failed"]);
        assert_eq!(audit.events.len(), if cleanup_fails { 1 } else { 2 });
    }
    let (session, audit) = observed(Executor::default(), true);
    assert!(session
        .unwrap_err()
        .to_string()
        .contains("serve.sse.closed"));
    let audit = audit.lock().unwrap();
    assert!(audit.events.is_empty());
    assert!(audit.cancellations.is_empty());
}
