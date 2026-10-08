//! Test-only legacy metadata adapter; production admits source-composed callbacks.

use crate::runtime::vm::aot_metadata::{AotRouterCallable, AotRouterPlan, AotRouterRouteTarget};
use crate::runtime::vm::native_callable::VmNativeCallableRef;
use crate::runtime::vm::ReplValue;
use terlan_http_native::routing::{RouteMethod, RouteTarget, Router};

pub(super) fn materialize_router(plan: AotRouterPlan) -> Result<Router<ReplValue>, String> {
    let mut router = Router::new();
    let middleware: Vec<_> = plan.middleware.into_iter().map(callable_value).collect();
    let response_middleware: Vec<_> = plan
        .response_middleware
        .into_iter()
        .map(callable_value)
        .collect();
    for route in plan.routes {
        let method = RouteMethod::from_name(&route.method).ok_or_else(|| {
            format!(
                "error[serve.aot.router]: unsupported route method `{}`",
                route.method
            )
        })?;
        let target = match route.target {
            AotRouterRouteTarget::Handler(handler) => RouteTarget::Handler(callable_value(handler)),
            AotRouterRouteTarget::Sse(plan) => {
                RouteTarget::SseEndpoint(plan.map_callbacks(VmNativeCallableRef::into_value))
            }
            AotRouterRouteTarget::WebSocket(plan) => {
                RouteTarget::WebSocketEndpoint(plan.map_callbacks(VmNativeCallableRef::into_value))
            }
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
    VmNativeCallableRef {
        module: callable.module,
        function: callable.function,
        arity: callable.arity,
    }
    .into_value()
}
