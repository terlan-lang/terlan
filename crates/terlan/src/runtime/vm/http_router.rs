//! Host value aliases for HTTP package routing. No routing policy lives here.

pub(crate) use super::native_callable::VmNativeCallableRef as VmHttpCompiledCallableRef;
pub(crate) use terlan_http_native::routing::{
    validate_response_middleware_result, RouteMethod as VmHttpRouteMethod,
};
pub(crate) type VmHttpRouter = terlan_http_native::routing::Router<super::ReplValue>;
pub(crate) type VmHttpRouteTarget = terlan_http_native::routing::RouteTarget<super::ReplValue>;
pub(crate) type VmHttpRouterOutcome = terlan_http_native::routing::RouterOutcome<super::ReplValue>;
#[cfg(test)]
pub(crate) type VmHttpMiddlewareResult =
    terlan_http_native::routing::MiddlewareResult<super::ReplValue>;

#[cfg(test)]
#[path = "http_router/route_concurrency_test.rs"]
mod route_concurrency_test;
