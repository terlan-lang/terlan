use std::sync::Arc;

use super::{VmHttpRouteMethod, VmHttpRouteTarget, VmHttpRouter, VmHttpRouterOutcome};
use crate::runtime::vm::ReplValue;

const ROUTE_COUNT: usize = 64;
const WORKER_COUNT: usize = 8;
const REQUESTS_PER_WORKER: usize = 256;

#[test]
fn vm_http_router_middleware_bounded_concurrency_smoke() {
    let mut router = VmHttpRouter::new().use_middleware(atom("trace"));
    for route in 0..ROUTE_COUNT {
        router = router
            .get(format!("/items/{route}"), ReplValue::Int(route as i64))
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
                            .dispatch(VmHttpRouteMethod::Get, &format!("/items/{route}"))
                            .expect("dispatch bounded concurrent route");
                        assert!(matches!(
                            outcome,
                            VmHttpRouterOutcome::Matched(dispatch)
                                if dispatch.target
                                    == VmHttpRouteTarget::Handler(ReplValue::Int(route as i64))
                                    && dispatch.middleware == vec![atom("trace")]
                        ));
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

fn atom(value: &str) -> ReplValue {
    ReplValue::Atom(value.to_string())
}
