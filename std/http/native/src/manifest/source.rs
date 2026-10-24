//! Manifest projection of source-composed routers; no builder syntax is inspected.

use super::*;
use crate::service_error::ServiceError;
use crate::source_descriptor::{RouteTarget, Router};
use terlan_runtime_abi::{DescriptorValue, DescriptorView};

/// Identifies package entrypoints from public export names and arities.
/// Hosts supply ordinary declaration metadata, not HTTP type-name guesses.
/// Callback providers are dependencies of the executed router, not entrypoints.
pub fn has_route_entrypoints<'a>(
    module: &str,
    public_exports: impl IntoIterator<Item = (&'a str, usize)>,
) -> bool {
    let mut route = false;
    let mut protocol = false;
    for (name, arity) in public_exports {
        if arity != 0 {
            continue;
        }
        match name {
            "router" => return true,
            "route" => route = true,
            "protocol" => protocol = true,
            _ => {}
        }
    }
    route && protocol && (module.ends_with(".WebSocket") || module.ends_with(".RoomProtocol"))
}

impl WebSocketRoute {
    /// Projects the legacy route()/protocol() exports after ordinary source execution.
    pub fn from_source_exports<V: DescriptorValue>(
        module: &str,
        source: SourceSpan,
        values: &[V],
    ) -> Result<Self, ServiceError> {
        let [route, protocol] = values else {
            return Err("error[web_router]: expected route() and protocol() results".into());
        };
        let (DescriptorView::String(route), DescriptorView::String(protocol)) =
            (route.descriptor_view(), protocol.descriptor_view())
        else {
            return Err(format!(
                "error[web_router]: websocket `{module}` route() and protocol() must return String"
            )
            .into());
        };
        if protocol.is_empty() {
            return Err(format!(
                "error[web_router]: websocket `{module}` protocol() cannot be empty"
            )
            .into());
        }
        let row = Self {
            module: module.into(),
            route: route.into(),
            protocol: protocol.into(),
            source: Some(source),
        };
        validate_websocket(&row)?;
        Ok(row)
    }
}

/// Route rows shared by package generation and manifest serialization.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Routes {
    pub handlers: Vec<HandlerRoute>,
    pub websockets: Vec<WebSocketRoute>,
    pub sse: Vec<SseRoute>,
    pub static_responses: Vec<StaticResponse>,
    pub file_responses: Vec<FileResponse>,
}

impl Routes {
    /// Projects evaluated declarations without inspecting response bodies or captures.
    ///
    /// Every row retains the router owner, not the callback provider: serving must
    /// dispatch through that owner's graph to preserve middleware and recovery.
    /// Handler names are diagnostic labels, not a replacement for graph dispatch.
    pub fn from_source_router<C: DescriptorValue + Clone>(
        module: &str,
        source: SourceSpan,
        router: Router<C>,
        mut label: impl FnMut(&C) -> Result<String, String>,
    ) -> Result<Self, ServiceError> {
        router
            .clone()
            .into_routing_table()
            .map_err(|error| ServiceError::from(error.to_string()))?;
        let mut rows = Self::default();
        for route in router.routes {
            if !matches!(&route.target, RouteTarget::Handler(_)) && route.method != "GET" {
                return Err("error[web_router]: channel declarations require GET".into());
            }
            match route.target {
                RouteTarget::Handler(handler) => rows.handlers.push(HandlerRoute {
                    method: route.method,
                    route: route.path,
                    module: module.into(),
                    function: label(&handler)?,
                    arity: 1,
                    source: Some(source.clone()),
                }),
                RouteTarget::Sse(_) => rows.sse.push(SseRoute {
                    module: module.into(),
                    route: route.path,
                    source: source.clone(),
                }),
                RouteTarget::WebSocket(_) => rows.websockets.push(WebSocketRoute {
                    module: module.into(),
                    route: route.path,
                    protocol: String::new(),
                    source: Some(source.clone()),
                }),
            }
        }
        if let Some(fallback) = router.fallback {
            let function = label(&fallback.handler)?;
            for method in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
                let declared =
                    rows.handlers.iter().any(|row| {
                        row.method == method && matches!(row.route.as_str(), "*" | "/*")
                    }) || (method == "GET"
                        && (rows
                            .sse
                            .iter()
                            .any(|row| matches!(row.route.as_str(), "*" | "/*"))
                            || rows
                                .websockets
                                .iter()
                                .any(|row| matches!(row.route.as_str(), "*" | "/*"))));
                if !declared {
                    rows.handlers.push(HandlerRoute {
                        method: method.into(),
                        route: "*".into(),
                        module: module.into(),
                        function: function.clone(),
                        arity: 1,
                        source: Some(source.clone()),
                    });
                }
            }
        }
        for row in &rows.handlers {
            validate_handler(row)?;
        }
        for row in &rows.sse {
            validate_sse(row)?;
        }
        for row in &rows.websockets {
            validate_websocket(row)?;
        }
        validate_route_namespace(&rows.handlers, &rows.websockets, &rows.sse, &[], &[])?;
        Ok(rows)
    }
}

#[cfg(test)]
#[path = "source_test.rs"]
mod tests;
