//! Channel admission uses the same source executor and response chain as handlers.

use super::*;
use crate::router_execution::{finish_suspendable, AsyncHost};
use crate::routing::MiddlewareResult;

/// A source-selected endpoint or a fully admitted middleware response.
pub enum SourceAdmission<P, R> {
    Open(P),
    Respond(R),
}

/// Runs SSE middleware with materialized route captures before binding checks.
/// No session is allocated here; cancellation drops the pending source call.
pub async fn sse_suspendable<H: AsyncHost>(
    router: &Router<H::Value>,
    declared_route: &str,
    path: &str,
    host: &H,
) -> Result<SourceAdmission<SseEndpointPlan<H::Value>, H::Response>, WebRouteError> {
    admit_suspendable(
        router,
        declared_route,
        path,
        host,
        "SSE",
        "an SSE",
        |target| match target {
            RouteTarget::SseEndpoint(plan) => Some(plan),
            _ => None,
        },
    )
    .await
}

/// Runs WebSocket authorization before allocating a live callback session.
/// The transport must validate its opening handshake before calling this.
pub async fn websocket_suspendable<H: AsyncHost>(
    router: &Router<H::Value>,
    declared_route: &str,
    path: &str,
    host: &H,
) -> Result<SourceAdmission<WebSocketEndpointPlan<H::Value>, H::Response>, WebRouteError> {
    admit_suspendable(
        router,
        declared_route,
        path,
        host,
        "websocket",
        "a WebSocket",
        |target| match target {
            RouteTarget::WebSocketEndpoint(plan) => Some(plan),
            _ => None,
        },
    )
    .await
}

async fn admit_suspendable<H: AsyncHost, P>(
    router: &Router<H::Value>,
    declared_route: &str,
    path: &str,
    host: &H,
    label: &str,
    description: &str,
    extract: impl FnOnce(RouteTarget<H::Value>) -> Option<P>,
) -> Result<SourceAdmission<P, H::Response>, WebRouteError> {
    let RouterOutcome::Matched(dispatch) = router.dispatch(RouteMethod::Get, path)? else {
        return Err((format!(
            "error[serve_router]: materialized router did not match {label} GET {path}"
        ))
        .into());
    };
    let request = host.request(&dispatch.route_params);
    for callback in &dispatch.middleware {
        let value = host.invoke(callback.clone(), vec![request.clone()]).await?;
        if let MiddlewareResult::Respond(response) = MiddlewareResult::from_value(value)? {
            return finish_suspendable(
                response,
                &dispatch.route_params,
                &dispatch.response_middleware,
                host,
            )
            .await
            .map(SourceAdmission::Respond);
        }
    }
    validate_endpoint(*dispatch, declared_route, label, description, extract)
        .map(SourceAdmission::Open)
}

#[cfg(test)]
#[path = "suspendable_test.rs"]
mod tests;
