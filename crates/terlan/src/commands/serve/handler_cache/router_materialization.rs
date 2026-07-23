//! Compiler router metadata admission into the VM dispatch model.

use crate::compiler::router::{AotRouterCallable, AotRouterPlan, AotRouterRouteTarget};
use crate::runtime::vm::http_router::{
    VmHttpCompiledCallableRef, VmHttpRouteMethod, VmHttpRouteTarget, VmHttpRouter,
};
use crate::runtime::vm::ReplValue;

pub(super) fn materialize_router(plan: AotRouterPlan) -> Result<VmHttpRouter, String> {
    let mut router = VmHttpRouter::new();
    for middleware in plan.middleware {
        router = router.use_middleware(callable_value(middleware));
    }
    for middleware in plan.response_middleware {
        router = router.map_response(callable_value(middleware));
    }
    for route in plan.routes {
        let method = VmHttpRouteMethod::from_name(&route.method).ok_or_else(|| {
            format!(
                "error[serve.aot.router]: unsupported route method `{}`",
                route.method
            )
        })?;
        let target = match route.target {
            AotRouterRouteTarget::Handler(handler) => {
                VmHttpRouteTarget::Handler(callable_value(handler))
            }
            AotRouterRouteTarget::Sse(plan) => VmHttpRouteTarget::SseEndpoint(plan),
            AotRouterRouteTarget::WebSocket(plan) => VmHttpRouteTarget::WebSocketEndpoint(plan),
        };
        router = router.scoped_target(
            method,
            route.path,
            target,
            route.middleware.into_iter().map(callable_value).collect(),
            route
                .response_middleware
                .into_iter()
                .map(callable_value)
                .collect(),
        )?;
    }
    if let Some(fallback) = plan.fallback {
        router = router.fallback(callable_value(fallback));
    }
    if let Some(error) = plan.error {
        router = router.error(callable_value(error));
    }
    Ok(router)
}

fn callable_value(callable: AotRouterCallable) -> ReplValue {
    VmHttpCompiledCallableRef {
        module: callable.module,
        function: callable.function,
        arity: callable.arity,
    }
    .into_value()
}
