use super::*;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::{poll_fn, Future};
use std::task::{Context, Poll, Waker};
use terlan_runtime_abi::{native_record, NativeValue as V};

#[derive(Default)]
struct Host {
    replies: RefCell<VecDeque<Result<V, String>>>,
    calls: RefCell<Vec<(V, Vec<V>)>>,
    dropped: Cell<usize>,
    invalid_response: bool,
}

struct Active<'a>(&'a Cell<usize>);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

impl AsyncHost for Host {
    type Value = V;
    type Response = V;

    async fn invoke(&self, callable: V, args: Vec<V>) -> Result<V, String> {
        let _active = Active(&self.dropped);
        self.calls.borrow_mut().push((callable, args));
        let mut waiting = true;
        poll_fn(|cx| {
            if std::mem::take(&mut waiting) {
                cx.waker().wake_by_ref();
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
        self.replies
            .borrow_mut()
            .pop_front()
            .expect("unexpected source call")
    }
    fn arity(&self, _: &V) -> Option<usize> {
        panic!("channel is not a handler")
    }
    fn request(&self, params: &[(String, String)]) -> V {
        params.to_vec().into()
    }
    fn scalar(value: V) -> V {
        value
    }
    fn admit(&self, value: V) -> Result<V, String> {
        if self.invalid_response {
            Err("invalid response resource".into())
        } else {
            Ok(value)
        }
    }
}

fn host(replies: impl IntoIterator<Item = Result<V, String>>) -> Host {
    Host {
        replies: RefCell::new(replies.into_iter().collect()),
        ..Host::default()
    }
}

fn response() -> V {
    V::Record {
        name: "Response".into(),
        fields: vec![],
    }
}

fn rejected() -> V {
    let response = response();
    native_record!(Respond, { response })
}

fn endpoint() -> RouteTarget<V> {
    RouteTarget::SseEndpoint(
        SseEndpointPlan::new(1, 64)
            .unwrap()
            .with_producer("next".into())
            .unwrap(),
    )
}

fn router(target: RouteTarget<V>) -> Router<V> {
    Router::new()
        .scoped_target(
            RouteMethod::Get,
            "/items/{id:Int}",
            target,
            vec!["first".into(), "second".into()],
            vec!["outer".into(), "inner".into()],
        )
        .unwrap()
}

fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    for _ in 0..16 {
        if let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return result;
        }
    }
    panic!("admission exceeded callback budget")
}

#[test]
fn captures_survive_suspension_and_continuation_does_not_run_response_stages() {
    let host = host([
        Ok(V::Atom("continue".into())),
        Ok(V::Record {
            name: "Continue".into(),
            fields: vec![],
        }),
    ]);
    let router = router(endpoint());
    let result = run(sse_suspendable(
        &router,
        "/items/{id:Int}",
        "/items/%34%32",
        &host,
    ))
    .unwrap();
    let SourceAdmission::Open(plan) = result else {
        panic!("stream plan")
    };
    assert_eq!(plan.producer(), Some(&V::from("next")));
    let params: V = vec![("id".to_string(), "42".to_string())].into();
    assert_eq!(
        *host.calls.borrow(),
        [
            ("first".into(), vec![params.clone()]),
            ("second".into(), vec![params]),
        ]
    );
    assert_eq!(host.dropped.get(), 2);
}

#[test]
fn response_precedes_stale_binding_and_unwinds_in_reverse_with_captures() {
    let host = host([Ok(rejected()), Ok(response()), Ok(response())]);
    let router = router(RouteTarget::Handler("not a channel".into()));
    let result = run(sse_suspendable(&router, "/stale", "/items/42", &host)).unwrap();
    let SourceAdmission::Respond(value) = result else {
        panic!("rejection")
    };
    assert_eq!(value, response());
    let params: V = vec![("id".to_string(), "42".to_string())].into();
    assert_eq!(
        *host.calls.borrow(),
        [
            ("first".into(), vec![params.clone()]),
            ("inner".into(), vec![params.clone(), response()]),
            ("outer".into(), vec![params, response()]),
        ]
    );
    assert_eq!(host.dropped.get(), 3);
}

#[test]
fn callback_and_response_failures_are_terminal_without_replay() {
    for (replies, expected) in [
        (vec![Err("middleware failed".into())], "middleware failed"),
        (vec![Ok(V::Unit)], "expected Continue or Respond"),
        (
            vec![Ok(rejected()), Err("response failed".into())],
            "response failed",
        ),
        (vec![Ok(rejected()), Ok(V::Unit)], "response_middleware"),
    ] {
        let count = replies.len();
        let host = host(replies);
        let router = router(endpoint());
        let Err(error) = run(sse_suspendable(
            &router,
            "/items/{id:Int}",
            "/items/42",
            &host,
        )) else {
            panic!("terminal error")
        };
        assert!(error.contains(expected), "{error}");
        assert_eq!(host.calls.borrow().len(), count);
        assert_eq!(host.dropped.get(), count);
    }
    let mut host = host([Ok(rejected()), Ok(response()), Ok(response())]);
    host.invalid_response = true;
    let router = router(endpoint());
    assert!(
        matches!(run(sse_suspendable(&router, "/items/{id:Int}", "/items/42", &host)),
        Err(error) if error == "invalid response resource")
    );
}

#[test]
fn unmatched_or_invalid_routes_never_execute_and_binding_errors_follow_middleware() {
    for (router, path, expected) in [
        (Router::new(), "/missing", "did not match SSE"),
        (router(endpoint()), "/items/no", "did not match SSE"),
        (router(endpoint()), "relative", ""),
    ] {
        let host = Host::default();
        assert!(
            matches!(run(sse_suspendable(&router, "/items/{id:Int}", path, &host)),
            Err(error) if error.contains(expected))
        );
        assert!(host.calls.borrow().is_empty());
    }
    for (target, declared, expected) in [
        (endpoint(), "/stale", "does not match materialized"),
        (
            RouteTarget::Handler("handler".into()),
            "/items/{id:Int}",
            "did not resolve to an SSE",
        ),
    ] {
        let router = router(target);
        let host = host([
            Ok(V::Atom("continue".into())),
            Ok(V::Atom("continue".into())),
        ]);
        assert!(
            matches!(run(sse_suspendable(&router, declared, "/items/42", &host)),
            Err(error) if error.contains(expected))
        );
        assert_eq!(host.calls.borrow().len(), 2);
    }
}

#[test]
fn cancellation_drops_pending_middleware_or_response_without_starting_later_calls() {
    for polls in 1..=3 {
        let host = host([Ok(rejected()), Ok(response()), Ok(response())]);
        let router = router(endpoint());
        {
            let mut future = std::pin::pin!(sse_suspendable(
                &router,
                "/items/{id:Int}",
                "/items/42",
                &host
            ));
            for _ in 0..polls {
                assert!(future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending());
            }
        }
        assert_eq!(host.calls.borrow().len(), polls);
        assert_eq!(host.dropped.get(), polls);
    }
}

#[test]
fn websocket_uses_shared_async_admission_without_coercing_other_endpoint_kinds() {
    let plan = WebSocketEndpointPlan::new(4, 128).unwrap();
    let router = router(RouteTarget::WebSocketEndpoint(plan.clone()));
    let host = host([
        Ok(V::Atom("continue".into())),
        Ok(V::Atom("continue".into())),
    ]);
    let result = run(websocket_suspendable(
        &router,
        "/items/{id:Int}",
        "/items/42",
        &host,
    ))
    .unwrap();
    let SourceAdmission::Open(actual) = result else {
        panic!("websocket plan")
    };
    assert_eq!(actual, plan);
    assert_eq!(host.dropped.get(), 2);

    for (target, replies, expected) in [
        (
            endpoint(),
            vec![
                Ok(V::Atom("continue".into())),
                Ok(V::Atom("continue".into())),
            ],
            "did not resolve to a WebSocket",
        ),
        (
            RouteTarget::WebSocketEndpoint(plan),
            vec![Err("authorization failed".into())],
            "authorization failed",
        ),
    ] {
        let router = self::router(target);
        let host = self::host(replies);
        assert!(
            matches!(run(websocket_suspendable(&router, "/items/{id:Int}", "/items/42", &host)),
            Err(error) if error.contains(expected))
        );
    }
    let host = self::host([Ok(rejected()), Ok(response()), Ok(response())]);
    let router = self::router(endpoint());
    assert!(matches!(
        run(websocket_suspendable(&router, "/stale", "/items/42", &host)),
        Ok(SourceAdmission::Respond(_))
    ));
    assert!(
        matches!(run(websocket_suspendable(&Router::new(), "/missing", "/missing", &Host::default())),
        Err(error) if error.contains("did not match websocket"))
    );
}
