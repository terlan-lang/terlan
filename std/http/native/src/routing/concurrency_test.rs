use std::sync::Arc;

use super::{RouteMethod, RouteTarget, Router, RouterOutcome};
use terlan_runtime_abi::NativeValue;

const ROUTE_COUNT: usize = 64;
const WORKER_COUNT: usize = 8;
const REQUESTS_PER_WORKER: usize = 256;

#[test]
fn router_middleware_bounded_concurrency_smoke() {
    let mut router = Router::new();
    for route in 0..ROUTE_COUNT {
        router = router
            .scoped_target(
                RouteMethod::Get,
                format!("/items/{route}"),
                RouteTarget::Handler(NativeValue::Int(route as i64)),
                vec![atom("trace")],
                vec![atom("secure")],
            )
            .expect("register bounded smoke route");
    }
    let router = Arc::new(router);

    let completed = std::thread::scope(|scope| {
        let workers = (0..WORKER_COUNT)
            .map(|worker| {
                let router = Arc::clone(&router);
                scope.spawn(move || {
                    for request in 0..REQUESTS_PER_WORKER {
                        let route = (worker * REQUESTS_PER_WORKER + request) % ROUTE_COUNT;
                        let outcome = router
                            .dispatch(RouteMethod::Get, &format!("/items/{route}"))
                            .expect("dispatch bounded concurrent route");
                        assert!(matches!(
                            outcome,
                            RouterOutcome::Matched(dispatch)
                                if dispatch.target
                                    == RouteTarget::Handler(NativeValue::Int(route as i64))
                                    && dispatch.middleware == vec![atom("trace")]
                                    && dispatch.response_middleware == vec![atom("secure")]
                        ));
                        assert_eq!(
                            router
                                .dispatch(RouteMethod::Post, &format!("/items/{route}"))
                                .unwrap(),
                            RouterOutcome::NotFound,
                        );
                        assert_eq!(
                            router.dispatch(RouteMethod::Get, "/missing").unwrap(),
                            RouterOutcome::NotFound,
                        );
                    }
                    REQUESTS_PER_WORKER
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("concurrent router worker"))
            .sum::<usize>()
    });

    assert_eq!(completed, WORKER_COUNT * REQUESTS_PER_WORKER);
}

fn atom(value: &str) -> NativeValue {
    NativeValue::Atom(value.to_string())
}
