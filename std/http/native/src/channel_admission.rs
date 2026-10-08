//! Source-router channel admission, independent of code-image execution.

use terlan_runtime_abi::DescriptorValue;

use crate::channel_plan::{SseEndpointPlan, WebSocketEndpointPlan};
use crate::route_pattern::WebRouteError;
use crate::routing::{
    MiddlewareContinuation, RouteMethod, RouteShortCircuit, RouteTarget, Router, RouterOutcome,
};

#[derive(Debug, PartialEq)]
pub enum Admission<C, P> {
    Open(P),
    Respond(RouteShortCircuit<C>),
}

/// Middleware rejection precedes endpoint checks and never opens a session.
pub fn websocket<C: DescriptorValue + Clone>(
    router: &Router<C>,
    declared_route: &str,
    path: &str,
    invoke: impl FnMut(&C, &MiddlewareContinuation<C>) -> Result<C, String>,
) -> Result<Admission<C, WebSocketEndpointPlan<C>>, WebRouteError> {
    admit(
        router,
        declared_route,
        path,
        invoke,
        "websocket",
        "a WebSocket",
        |target| match target {
            RouteTarget::WebSocketEndpoint(plan) => Some(plan),
            _ => None,
        },
    )
}

/// The host must check transport availability before executing open callbacks.
pub fn sse<C: DescriptorValue + Clone>(
    router: &Router<C>,
    declared_route: &str,
    path: &str,
    invoke: impl FnMut(&C, &MiddlewareContinuation<C>) -> Result<C, String>,
) -> Result<Admission<C, SseEndpointPlan<C>>, WebRouteError> {
    admit(
        router,
        declared_route,
        path,
        invoke,
        "SSE",
        "an SSE",
        |target| match target {
            RouteTarget::SseEndpoint(plan) => Some(plan),
            _ => None,
        },
    )
}

fn admit<C: DescriptorValue + Clone, P>(
    router: &Router<C>,
    declared_route: &str,
    path: &str,
    invoke: impl FnMut(&C, &MiddlewareContinuation<C>) -> Result<C, String>,
    label: &str,
    endpoint_description: &str,
    extract: impl FnOnce(RouteTarget<C>) -> Option<P>,
) -> Result<Admission<C, P>, WebRouteError> {
    match router.dispatch_with_typed_middleware(RouteMethod::Get, path, invoke)? {
        RouterOutcome::ShortCircuited(response) => Ok(Admission::Respond(response)),
        RouterOutcome::Matched(dispatch) => {
            if dispatch.route_pattern != declared_route {
                return Err(format!(
                    "error[serve_router]: {label} route `GET` `{declared_route}` does not match materialized route `{}` `{}`",
                    dispatch.method.as_str(), dispatch.route_pattern
                ).into());
            }
            extract(dispatch.target).map(Admission::Open).ok_or_else(|| format!(
                "error[serve_router]: {label} route `GET` `{declared_route}` did not resolve to {endpoint_description} endpoint"
            ).into())
        }
        RouterOutcome::NotFound => Err(format!(
            "error[serve_router]: materialized router did not match {label} GET {path}"
        )
        .into()),
    }
}

#[cfg(test)]
#[path = "channel_admission_test.rs"]
mod tests;
