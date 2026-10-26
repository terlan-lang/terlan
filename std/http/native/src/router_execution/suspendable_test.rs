use super::super::test_support::{response, router, Script};
use super::*;
use crate::admitted_response::AdmittedResponse;
use futures_util::FutureExt;
use std::cell::RefCell;
use std::future::poll_fn;
use std::task::{Context, Poll};
use terlan_runtime_abi::{native_record, NativeValue as V};

struct AsyncScript {
    script: RefCell<Script>,
    events: RefCell<Vec<(V, &'static str)>>,
}

impl AsyncScript {
    fn new(replies: impl IntoIterator<Item = (&'static str, Result<V, String>)>) -> Self {
        Self {
            script: RefCell::new(Script::new(replies)),
            events: RefCell::new(vec![]),
        }
    }
}

struct ActiveCall<'a>(&'a AsyncScript, V);
impl Drop for ActiveCall<'_> {
    fn drop(&mut self) {
        self.0.events.borrow_mut().push((self.1.clone(), "dropped"));
    }
}

impl AsyncHost for AsyncScript {
    type Value = V;
    type Response = AdmittedResponse;

    async fn invoke(&self, callable: V, args: Vec<V>) -> Result<V, String> {
        let _active = ActiveCall(self, callable.clone());
        self.events.borrow_mut().push((callable.clone(), "started"));
        let mut suspended = false;
        poll_fn(|cx| {
            if suspended {
                Poll::Ready(())
            } else {
                suspended = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        })
        .await;
        self.script.borrow_mut().invoke(&callable, args)
    }
    fn arity(&self, callable: &V) -> Option<usize> {
        self.script.borrow().arity(callable)
    }
    fn request(&self, params: &[(String, String)]) -> V {
        self.script.borrow().request(params)
    }
    fn scalar(value: V) -> V {
        Script::scalar(value)
    }
    fn admit(&self, value: V) -> Result<AdmittedResponse, String> {
        self.script.borrow_mut().admit(value)
    }
}

fn run(router: &Router<V>, host: &AsyncScript) -> Result<AdmittedResponse, WebRouteError> {
    // Every invocation suspends once; bounded manual polling detects replay loops.
    let future = execute_suspendable(router, RouteMethod::Get, "/item/{id:Int}", "/item/42", host);
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(futures_util::task::noop_waker_ref());
    for _ in 0..32 {
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return result;
        }
    }
    panic!("pipeline did not complete within its callback budget");
}

#[test]
fn suspended_calls_preserve_capture_arguments_and_reverse_unwind() {
    let host = AsyncScript::new([
        ("before", Ok(V::Atom("continue".into()))),
        ("handler", Ok(response("handler"))),
        ("inner", Ok(response("inner"))),
        ("outer", Ok(response("outer"))),
    ]);
    host.script.borrow_mut().arity = Some(2);
    let result = run(&router(&["before"], &["outer", "inner"], false), &host).unwrap();
    assert_eq!(result.body.as_bytes(), Some(b"outer".as_slice()));
    let script = host.script.borrow();
    let captures: V = vec![("id".to_owned(), "42".to_owned())].into();
    assert_eq!(script.calls[0].1, vec![captures.clone()]);
    assert_eq!(script.calls[1].1, vec![captures.clone(), V::Int(42)]);
    assert_eq!(
        script.calls[2].1,
        vec![captures.clone(), response("handler")]
    );
    assert_eq!(script.calls[3].1, vec![captures, response("inner")]);
    assert_eq!(script.admitted, 1);
    assert!(script.replies.is_empty());
}

#[test]
fn short_circuit_skips_handler_admission_but_runs_response_stages() {
    let response = response("denied");
    let host = AsyncScript::new([
        ("auth", Ok(native_record!(Respond, { response }))),
        ("after", Ok(self::response("decorated"))),
    ]);
    host.script.borrow_mut().arity = None;
    let router = router(&["auth", "unreachable"], &["after"], true);
    let future = execute_suspendable(&router, RouteMethod::Get, "/stale", "/item/42", &host);
    let mut future = std::pin::pin!(future);
    let mut cx = Context::from_waker(futures_util::task::noop_waker_ref());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    let Poll::Ready(result) = future.as_mut().poll(&mut cx) else {
        panic!("completed");
    };
    assert_eq!(
        result.unwrap().body.as_bytes(),
        Some(b"decorated".as_slice())
    );
    assert!(host.script.borrow().replies.is_empty());
}

#[test]
fn binding_and_target_errors_are_not_recovered_or_executed() {
    for (declared, path, arity, expected) in [
        ("/stale", "/item/42", Some(1), "routes disagree"),
        ("/item/{id:Int}", "/missing", Some(1), "did not match"),
        ("/item/{id:Int}", "/../item/42", Some(1), "not safe"),
        ("/item/{id:Int}", "/item/42", None, "not callable"),
        ("/item/{id:Int}", "/item/42", Some(0), "expects 0"),
        ("/item/{id:Int}", "/item/42", Some(3), "expects 3"),
    ] {
        let host = AsyncScript::new([]);
        host.script.borrow_mut().arity = arity;
        let error = execute_suspendable(
            &router(&[], &[], true),
            RouteMethod::Get,
            declared,
            path,
            &host,
        )
        .now_or_never()
        .unwrap()
        .unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(host.events.borrow().is_empty());
    }
    let channel = Router::new()
        .scoped_target(
            RouteMethod::Get,
            "/events",
            RouteTarget::SseEndpoint(crate::channel_plan::SseEndpointPlan::new(1, 64).unwrap()),
            vec![],
            vec![],
        )
        .unwrap();
    let host = AsyncScript::new([]);
    let error = execute_suspendable(&channel, RouteMethod::Get, "/events", "/events", &host)
        .now_or_never()
        .unwrap()
        .unwrap_err();
    assert!(error.contains("expected source HTTP handler"));
}

#[test]
fn admission_failure_recovers_once_and_does_not_replay_response_middleware() {
    for recovery in [
        Ok(response("recovered")),
        Ok(V::Unit),
        Err("recovery failed".into()),
    ] {
        let valid = recovery == Ok(response("recovered"));
        let failed = recovery.is_err();
        let host = AsyncScript::new([
            ("handler", Ok(response("handler"))),
            (
                "after",
                Ok(V::Record {
                    name: "Response".into(),
                    fields: vec![],
                }),
            ),
            ("recover", recovery),
        ]);
        let result = run(&router(&[], &["after"], true), &host);
        if valid {
            assert_eq!(
                result.unwrap().body.as_bytes(),
                Some(b"recovered".as_slice())
            );
        } else {
            let error = result.unwrap_err();
            if failed {
                assert!(
                    error.contains("router failed with") && error.contains("recovery failed"),
                    "{error}"
                );
            }
        }
        let script = host.script.borrow();
        assert!(script.replies.is_empty());
        assert_eq!(script.calls.len(), 3);
        assert_eq!(script.admitted, if failed { 1 } else { 2 });
    }
}

#[test]
fn callback_failure_uses_source_recovery_and_preserves_original_error() {
    for recovery in [false, true] {
        let mut replies = vec![("handler", Err("original failure".into()))];
        if recovery {
            replies.push(("recover", Ok(response("recovered"))));
        }
        let host = AsyncScript::new(replies);
        let result = run(&router(&[], &[], recovery), &host);
        if recovery {
            assert_eq!(
                result.unwrap().body.as_bytes(),
                Some(b"recovered".as_slice())
            );
        } else {
            assert!(result.unwrap_err().contains("original failure"));
        }
        assert!(host.script.borrow().replies.is_empty());
    }
}

#[test]
fn cancellation_at_each_stage_drops_host_call_and_never_recovers() {
    for cancelled in 0..4 {
        let host = AsyncScript::new([
            ("before", Ok(V::Atom("continue".into()))),
            ("handler", Err("handler failed".into())),
            ("recover", Ok(response("recovered"))),
            ("after", Ok(response("after"))),
        ]);
        let router = router(&["before"], &["after"], true);
        let mut future = Box::pin(execute_suspendable(
            &router,
            RouteMethod::Get,
            "/item/{id:Int}",
            "/item/42",
            &host,
        ));
        let mut cx = Context::from_waker(futures_util::task::noop_waker_ref());
        for _ in 0..=cancelled {
            assert!(future.as_mut().poll(&mut cx).is_pending());
        }
        drop(future);
        let events = host.events.borrow();
        assert_eq!(events.len(), 2 * (cancelled + 1));
        for (event, name) in events
            .chunks_exact(2)
            .zip(["before", "handler", "recover", "after"])
        {
            assert_eq!(
                event,
                [(V::from(name), "started"), (V::from(name), "dropped")]
            );
        }
        assert_eq!(host.script.borrow().calls.len(), cancelled);
        assert_eq!(host.script.borrow().admitted, 0);
    }
}
