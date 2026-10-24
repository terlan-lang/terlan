//! Source-router channel admission, independent of code-image execution.

use terlan_runtime_abi::DescriptorValue;

use crate::channel_plan::{SseEndpointPlan, WebSocketEndpointPlan};
use crate::route_pattern::WebRouteError;
use crate::routing::{
    MiddlewareContinuation, RouteDispatch, RouteMethod, RouteShortCircuit, RouteTarget, Router,
    RouterOutcome,
};

mod suspendable;
pub use suspendable::{sse_suspendable, websocket_suspendable, SourceAdmission};

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

/// Resolves transport rejection and HEAD before the host opens a source session.
/// An externally supplied live adapter may also admit callback-only plans.
pub fn sse_transport_response<C>(
    plan: &SseEndpointPlan<C>,
    method: &str,
    external_transport_available: bool,
) -> Option<crate::admitted_response::AdmittedResponse> {
    use crate::admitted_response::{AdmittedResponse, ResponseBody};
    if !external_transport_available && !plan.supports_pull_transport() {
        return Some(AdmittedResponse {
            status: 501,
            content_type: "text/plain; charset=utf-8".into(),
            headers: Vec::new(),
            body: ResponseBody::Text(
                "error[serve_http.upgrade_adapter_missing]: maintained async Hyper adapter is required for SSE".into(),
            ),
        });
    }
    (method == "HEAD").then(|| AdmittedResponse {
        status: 200,
        content_type: "text/event-stream".into(),
        headers: plan.response_headers().to_vec(),
        body: ResponseBody::Text(String::new()),
    })
}

/// Finite admission metadata; live transport replaces this body with its stream.
pub fn sse_stream_response<C>(
    plan: &SseEndpointPlan<C>,
) -> crate::admitted_response::AdmittedResponse {
    use crate::admitted_response::{AdmittedResponse, ResponseBody};
    AdmittedResponse {
        status: 200,
        content_type: "text/event-stream".into(),
        headers: plan
            .response_headers()
            .iter()
            .cloned()
            .chain([("connection".into(), "keep-alive".into())])
            .collect(),
        body: ResponseBody::Text(": connected\n\n".into()),
    }
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
        RouterOutcome::Matched(dispatch) => validate_endpoint(
            *dispatch,
            declared_route,
            label,
            endpoint_description,
            extract,
        )
        .map(Admission::Open),
        RouterOutcome::NotFound => Err(format!(
            "error[serve_router]: materialized router did not match {label} GET {path}"
        )
        .into()),
    }
}

fn validate_endpoint<C, P>(
    dispatch: RouteDispatch<C>,
    declared_route: &str,
    label: &str,
    endpoint_description: &str,
    extract: impl FnOnce(RouteTarget<C>) -> Option<P>,
) -> Result<P, WebRouteError> {
    if dispatch.route_pattern != declared_route {
        return Err(format!(
                    "error[serve_router]: {label} route `GET` `{declared_route}` does not match materialized route `{}` `{}`",
                    dispatch.method.as_str(), dispatch.route_pattern
        ).into());
    }
    extract(dispatch.target).ok_or_else(|| format!(
                "error[serve_router]: {label} route `GET` `{declared_route}` did not resolve to {endpoint_description} endpoint"
    ).into())
}

#[cfg(test)]
#[path = "channel_admission_test.rs"]
mod tests;
