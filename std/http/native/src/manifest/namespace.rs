use std::collections::BTreeMap;

use crate::ServiceError;

use super::{FileResponse, HandlerRoute, SseRoute, StaticResponse, WebSocketRoute};
use crate::route_pattern::route_ambiguity_key;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RouteKind {
    Handler,
    WebSocket,
    Sse,
    Static,
    File,
}

impl RouteKind {
    fn label(self) -> &'static str {
        match self {
            Self::Handler => "handler",
            Self::WebSocket => "websocket",
            Self::Sse => "SSE",
            Self::Static => "static response",
            Self::File => "file response",
        }
    }
}

/// Rejects duplicate and ambiguous routes within and across manifest sections.
/// Individual record validation remains the caller's responsibility. Methods
/// retain their declared spelling; WebSocket and SSE routes occupy GET slots.
pub fn validate_route_namespace(
    handlers: &[HandlerRoute],
    websockets: &[WebSocketRoute],
    sse: &[SseRoute],
    responses: &[StaticResponse],
    file_responses: &[FileResponse],
) -> Result<(), ServiceError> {
    let routes = handlers
        .iter()
        .map(|row| (RouteKind::Handler, row.method.as_str(), row.route.as_str()))
        .chain(
            websockets
                .iter()
                .map(|row| (RouteKind::WebSocket, "GET", row.route.as_str())),
        )
        .chain(
            sse.iter()
                .map(|row| (RouteKind::Sse, "GET", row.route.as_str())),
        )
        .chain(
            responses
                .iter()
                .map(|row| (RouteKind::Static, row.method.as_str(), row.route.as_str())),
        )
        .chain(
            file_responses
                .iter()
                .map(|row| (RouteKind::File, row.method.as_str(), row.route.as_str())),
        );
    let mut seen = BTreeMap::new();
    for (kind, method, route) in routes {
        let key = (method, route_ambiguity_key(route).map_err(String::from)?);
        if let Some((previous_kind, previous_route)) = seen.insert(key, (kind, route)) {
            let label = kind.label();
            if kind == previous_kind {
                let method = if matches!(kind, RouteKind::WebSocket | RouteKind::Sse) {
                    String::new()
                } else {
                    format!("`{method}` ")
                };
                return Err(format!(
                    "error[serve_package]: duplicate or ambiguous {label} route {method}`{route}`"
                )
                .into());
            }
            let previous_label = previous_kind.label();
            return Err(format!(
                "error[serve_package]: {label} route `{method}` `{route}` conflicts with {previous_label} route `{method}` `{previous_route}`"
            ).into());
        }
    }
    Ok(())
}
