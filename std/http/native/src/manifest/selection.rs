//! Executable manifest selection shares the package's source-route matcher.

use super::{FileResponse, HandlerRoute, SseRoute, StaticResponse, WebSocketRoute};
use crate::route_pattern::select_route;

/// Borrowed rows from one admitted manifest generation. Assets and compiled
/// code remain host-owned; no row is rewritten into a synthetic handler.
#[derive(Clone, Copy, Default)]
pub struct RouteTable<'a> {
    pub handlers: &'a [HandlerRoute],
    pub websockets: &'a [WebSocketRoute],
    pub sse: &'a [SseRoute],
    pub static_responses: &'a [StaticResponse],
    pub file_responses: &'a [FileResponse],
}

/// An executable endpoint retains its kind and exact manifest identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint<'a> {
    Handler(&'a HandlerRoute),
    Sse(&'a SseRoute),
    StaticResponse(&'a StaticResponse),
    FileResponse(&'a FileResponse),
}

/// The selected endpoint and validated path captures, without host value types.
#[derive(Debug, PartialEq, Eq)]
pub struct EndpointMatch<'a> {
    pub endpoint: Endpoint<'a>,
    pub params: Vec<(String, String)>,
}

impl<'a> RouteTable<'a> {
    /// Exact WebSocket paths reserve upgrade admission before asset lookup.
    /// Method/version/header rejection belongs to the maintained handshake.
    pub fn websocket(&self, path: &str) -> Option<&'a WebSocketRoute> {
        self.websockets.iter().find(|row| row.route == path)
    }

    /// Hosts may cache handler-only results against their manifest generation.
    pub fn is_handler_only(&self) -> bool {
        self.websockets.is_empty()
            && self.sse.is_empty()
            && self.static_responses.is_empty()
            && self.file_responses.is_empty()
    }

    /// Selects by method and specificity across every non-upgrade endpoint.
    /// Explicit HEAD wins before GET fallback, regardless of endpoint kind.
    /// Ambiguous declarations must already have failed namespace admission.
    pub fn select(&self, method: &str, path: &str) -> Option<EndpointMatch<'a>> {
        let candidates: Vec<_> = self
            .handlers
            .iter()
            .map(Endpoint::Handler)
            .chain(self.sse.iter().map(Endpoint::Sse))
            .chain(self.static_responses.iter().map(Endpoint::StaticResponse))
            .chain(self.file_responses.iter().map(Endpoint::FileResponse))
            .collect();
        select_route(&candidates, method, path, |endpoint| endpoint.describe()).map(|selected| {
            EndpointMatch {
                endpoint: *selected.route,
                params: selected.params,
            }
        })
    }
}

impl Endpoint<'_> {
    fn describe(&self) -> (&str, &str) {
        match self {
            Self::Handler(row) => (&row.method, &row.route),
            Self::Sse(row) => ("GET", &row.route),
            Self::StaticResponse(row) => (&row.method, &row.route),
            Self::FileResponse(row) => (&row.method, &row.route),
        }
    }
}

#[cfg(test)]
#[path = "selection_test.rs"]
mod tests;
