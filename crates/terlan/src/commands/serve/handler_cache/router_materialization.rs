//! Test-only legacy metadata adapter; production admits source-composed callbacks.

use crate::runtime::vm::aot_metadata::{AotRouterCallable, AotRouterPlan, AotRouterRouteTarget};
use crate::runtime::vm::http_router::{
    VmHttpCompiledCallableRef, VmHttpRouteMethod, VmHttpRouteTarget, VmHttpRouter,
};
use crate::runtime::vm::ReplValue;

pub(super) fn materialize_router(plan: AotRouterPlan) -> Result<VmHttpRouter, String> {
    let mut router = VmHttpRouter::new();
    let middleware: Vec<_> = plan.middleware.into_iter().map(callable_value).collect();
    let response_middleware: Vec<_> = plan
        .response_middleware
        .into_iter()
        .map(callable_value)
        .collect();
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
            AotRouterRouteTarget::Sse(plan) => VmHttpRouteTarget::SseEndpoint(
                plan.map_callbacks(VmHttpCompiledCallableRef::into_value),
            ),
            AotRouterRouteTarget::WebSocket(plan) => VmHttpRouteTarget::WebSocketEndpoint(
                plan.map_callbacks(VmHttpCompiledCallableRef::into_value),
            ),
        };
        router = router.scoped_target(
            method,
            route.path,
            target,
            middleware
                .iter()
                .cloned()
                .chain(route.middleware.into_iter().map(callable_value))
                .collect(),
            response_middleware
                .iter()
                .cloned()
                .chain(route.response_middleware.into_iter().map(callable_value))
                .collect(),
        )?;
    }
    if let Some(fallback) = plan.fallback {
        router = router.fallback_target(terlan_http_native::routing::Fallback {
            handler: callable_value(fallback),
            middleware,
            response_middleware,
        });
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
