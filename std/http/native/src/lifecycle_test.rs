use std::cell::RefCell;
use std::rc::Rc;

use super::*;

type Event = LifecycleEvent<u64, (), ()>;
type Hook = Option<Box<dyn LifecycleHook<u64, (), ()>>>;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Failure {
    None,
    NoHook,
    Admission,
    Authorize,
    Start,
    Handler,
    End,
    HandlerAndEnd,
    PanicStart,
    PanicHandler,
    PanicEnd,
}

struct RecordingHook {
    failure: Failure,
    events: Rc<RefCell<Vec<(&'static str, Event)>>>,
}

impl LifecycleHook<u64, (), ()> for RecordingHook {
    fn authorize(&mut self, event: &Event) -> Result<(), String> {
        self.events.borrow_mut().push(("authorize", event.clone()));
        if self.failure == Failure::Authorize {
            Err("authorization".into())
        } else {
            Ok(())
        }
    }

    fn observe(&mut self, event: &Event) -> Result<(), String> {
        self.events.borrow_mut().push(("observe", event.clone()));
        match (self.failure, event) {
            (Failure::Start, Event::RequestStart { .. }) => Err("start".into()),
            (Failure::End | Failure::HandlerAndEnd, Event::RequestEnd { .. }) => Err("end".into()),
            (Failure::PanicStart, Event::RequestStart { .. }) => panic!("start panic"),
            (Failure::PanicEnd, Event::RequestEnd { .. }) => panic!("end panic"),
            _ => Ok(()),
        }
    }
}

fn request() -> http::Request<String> {
    http::Request::builder()
        .method("POST")
        .uri("/sum?private=ignored")
        .body("secret body".into())
        .unwrap()
}

#[test]
fn hook_and_handler_failure_matrix_preserves_order_and_cleanup() {
    for failure in [
        Failure::None,
        Failure::NoHook,
        Failure::Admission,
        Failure::Authorize,
        Failure::Start,
        Failure::Handler,
        Failure::End,
        Failure::HandlerAndEnd,
    ] {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut hook: Hook = Some(Box::new(RecordingHook {
            failure,
            events: events.clone(),
        }));
        if failure == Failure::NoHook {
            hook = None;
        }
        let mut tracker = RequestResourceTracker::default();
        let previous = (failure == Failure::Admission).then(|| tracker.begin(7, 99).unwrap());
        let mut invoked = false;
        let result = dispatch_handler(
            &mut tracker,
            &mut hook,
            7,
            request(),
            &mut |request| {
                invoked = true;
                assert_eq!(request.body(), "secret body");
                assert_eq!(
                    events.borrow().len(),
                    if failure == Failure::NoHook { 0 } else { 2 }
                );
                if matches!(failure, Failure::Handler | Failure::HandlerAndEnd) {
                    Err("handler".into())
                } else {
                    Ok(http::Response::builder()
                        .status(201)
                        .body(vec![1_u8, 2])
                        .unwrap())
                }
            },
            |error| format!("resource: {error:?}"),
        );
        let expected = match failure {
            Failure::None | Failure::NoHook => None,
            Failure::Admission => Some("resource: AlreadyActive { owner: 7, request_id: 1 }"),
            Failure::Authorize => Some("authorization"),
            Failure::Start => Some("start"),
            Failure::Handler => Some("handler"),
            Failure::End => Some("end"),
            Failure::HandlerAndEnd => {
                Some("handler; lifecycle observation failed after cleanup: end")
            }
            _ => unreachable!(),
        };
        if let Some(error) = expected {
            assert_eq!(result.unwrap_err().to_string(), error);
        } else {
            assert_eq!(result.unwrap().body(), &[1, 2]);
        }
        assert_eq!(
            invoked,
            !matches!(
                failure,
                Failure::Authorize | Failure::Start | Failure::Admission
            )
        );
        if let Some(id) = previous {
            assert_eq!(tracker.metrics().active_body_bytes, 99);
            assert_eq!(
                tracker.leaks(),
                vec![crate::request_resources::RequestResourceLeak {
                    owner: 7,
                    request_id: id,
                }]
            );
            tracker.finish(7, id).unwrap();
        }
        assert!(tracker.leaks().is_empty(), "{failure:?}");
        let metrics = tracker.metrics();
        assert_eq!(metrics.active_body_bytes, 0);
        assert_eq!(
            metrics.completed_requests,
            usize::from(failure != Failure::Authorize)
        );
        let start = Event::RequestStart {
            process: 7,
            method: "POST".into(),
            path: "/sum".into(),
        };
        let mut expected_events = vec![("authorize", start.clone())];
        if !matches!(failure, Failure::Authorize | Failure::Admission) {
            expected_events.push(("observe", start));
        }
        if failure == Failure::NoHook {
            expected_events.clear();
        }
        if invoked && failure != Failure::NoHook {
            let outcome = if matches!(failure, Failure::Handler | Failure::HandlerAndEnd) {
                RequestOutcome::Error {
                    message: "handler".into(),
                }
            } else {
                RequestOutcome::Response { status: 201 }
            };
            expected_events.push((
                "observe",
                Event::RequestEnd {
                    process: 7,
                    method: "POST".into(),
                    path: "/sum".into(),
                    outcome,
                },
            ));
        }
        assert_eq!(*events.borrow(), expected_events);
    }
}

#[test]
fn host_unwinding_releases_accounting_without_swallowing_the_panic() {
    for failure in [
        Failure::PanicStart,
        Failure::PanicHandler,
        Failure::PanicEnd,
    ] {
        let mut hook: Hook = Some(Box::new(RecordingHook {
            failure,
            events: Rc::default(),
        }));
        let mut tracker = RequestResourceTracker::default();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dispatch_handler(
                &mut tracker,
                &mut hook,
                1,
                request(),
                &mut |_| {
                    assert_ne!(failure, Failure::PanicHandler, "handler panic");
                    Ok(http::Response::new(()))
                },
                |error| format!("{error:?}"),
            )
        }));
        assert!(outcome.is_err());
        assert!(tracker.leaks().is_empty());
        assert_eq!(tracker.metrics().active_body_bytes, 0);
        assert_eq!(tracker.metrics().completed_requests, 1);
        let id = tracker.begin(1, 0).unwrap();
        tracker.finish(1, id).unwrap();
    }
}

#[test]
fn failed_admission_never_observes_start_or_consumes_another_request() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut hook: Hook = Some(Box::new(RecordingHook {
        failure: Failure::None,
        events: events.clone(),
    }));
    let mut tracker = RequestResourceTracker::default();
    let id = tracker.begin(1, 99).unwrap();
    let before = tracker.metrics();
    let result = dispatch_handler(
        &mut tracker,
        &mut hook,
        1,
        request(),
        &mut |_| -> Result<http::Response<()>, String> {
            panic!("duplicate owner must not run");
        },
        |error| format!("{error:?}"),
    );
    assert_eq!(
        result.unwrap_err().to_string(),
        format!("AlreadyActive {{ owner: 1, request_id: {id} }}")
    );
    assert_eq!(events.borrow().len(), 1);
    assert_eq!(tracker.metrics(), before);
    tracker.finish(1, id).unwrap();
}

#[test]
fn absent_and_default_hooks_both_allow_dispatch() {
    struct DefaultHook;
    impl LifecycleHook<u64, (), ()> for DefaultHook {}
    for mut hook in [
        None,
        Some(Box::new(DefaultHook) as Box<dyn LifecycleHook<u64, (), ()>>),
    ] {
        let mut tracker = RequestResourceTracker::default();
        let response = dispatch_handler(
            &mut tracker,
            &mut hook,
            1,
            request(),
            &mut |_| Ok(http::Response::new("ok")),
            |error| format!("{error:?}"),
        )
        .unwrap();
        assert_eq!(response.body(), &"ok");
        assert_eq!(tracker.metrics().completed_requests, 1);
        assert!(tracker.leaks().is_empty());
    }
}
