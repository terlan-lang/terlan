use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use terlan_runtime_abi::BoundaryError;

use crate::runtime::vm::ReplValue;

use super::{
    WebPackageFileResponse, WebPackageHandler, WebPackageSse, WebPackageStaticResponse,
    WebPackageWebSocket,
};
use crate::commands::serve::manifest::{
    manifest_static_file_from_manifest, with_web_manifest, WebPackageManifest,
};
use crate::commands::serve::package_relative_path;

/// A manifest handler selected for one concrete HTTP request.
///
/// Inputs:
/// - Produced by `select_handler_for_request` after route matching.
///
/// Output:
/// - Matched handler metadata plus decoded route params.
///
/// Transformation:
/// - Separates static manifest handler identity from per-request values such as
///   `:id` captures so handler execution can pass both through the stable
///   request bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MatchedWebPackageHandler {
    pub(in crate::commands::serve) handler: WebPackageHandler,
    pub(in crate::commands::serve) params: Vec<(String, String)>,
}

/// Converts a package-admitted route scalar into the host value representation.
pub(super) fn route_param_argument(
    pattern: &str,
    name: &str,
    value: &str,
) -> Result<ReplValue, BoundaryError> {
    terlan_http_native::route_pattern::route_param_argument(pattern, name, value)
        .map(crate::runtime::vm::native_value::from_native)
}

/// One best route selected across all executable manifest response kinds.
pub(in crate::commands::serve) enum MatchedWebPackageRoute {
    WebSocket(WebPackageWebSocket),
    Handler(Rc<MatchedWebPackageHandler>),
    StaticFile(PathBuf),
    StaticResponse(WebPackageStaticResponse),
    FileResponse(WebPackageFileResponse, PathBuf),
    Sse(WebPackageSse),
}

thread_local! {
    /// The common repeated exact route stays owner-local and generation-safe.
    static LAST_SIMPLE_HANDLER_ROUTE: RefCell<Option<LastSimpleHandlerRoute>> =
        const { RefCell::new(None) };
}

struct LastSimpleHandlerRoute {
    manifest: Arc<WebPackageManifest>,
    method: String,
    request_path: String,
    matched: Option<Rc<MatchedWebPackageHandler>>,
}

/// Selects one route across dynamic, folded-static, and file-backed rows.
pub(in crate::commands::serve) fn manifest_route_for_request(
    web_root: &Path,
    method: &str,
    request_path: &str,
) -> Option<MatchedWebPackageRoute> {
    with_web_manifest(web_root, |manifest| {
        manifest_route_for_loaded_manifest(web_root, method, request_path, manifest)
    })
    .ok()
    .flatten()
}

fn manifest_route_for_loaded_manifest(
    web_root: &Path,
    method: &str,
    request_path: &str,
    manifest: &Arc<WebPackageManifest>,
) -> Option<MatchedWebPackageRoute> {
    if let Some(websocket) = manifest
        .websockets
        .iter()
        .find(|websocket| websocket.route == request_path)
    {
        return Some(MatchedWebPackageRoute::WebSocket(websocket.clone()));
    }
    if method == "GET" || method == "HEAD" {
        if let Some(path) = manifest_static_file_from_manifest(web_root, manifest, request_path) {
            return Some(MatchedWebPackageRoute::StaticFile(path));
        }
    }
    if manifest.websockets.is_empty()
        && manifest.sse.is_empty()
        && manifest.static_responses.is_empty()
        && manifest.file_responses.is_empty()
    {
        if let Some(matched) = LAST_SIMPLE_HANDLER_ROUTE.with(|cached| {
            let cached = cached.borrow();
            let cached = cached.as_ref()?;
            (Arc::ptr_eq(&cached.manifest, manifest)
                && cached.method == method
                && cached.request_path == request_path)
                .then(|| cached.matched.clone())
        }) {
            return matched.map(MatchedWebPackageRoute::Handler);
        }
        let matched =
            select_handler_for_request_ref(&manifest.handlers, method, request_path).map(Rc::new);
        LAST_SIMPLE_HANDLER_ROUTE.with(|cached| {
            *cached.borrow_mut() = Some(LastSimpleHandlerRoute {
                manifest: Arc::clone(manifest),
                method: method.to_string(),
                request_path: request_path.to_string(),
                matched: matched.clone(),
            });
        });
        return matched.map(MatchedWebPackageRoute::Handler);
    }
    let mut candidates = manifest.handlers.clone();
    candidates.extend(manifest.sse.iter().map(|endpoint| WebPackageHandler {
        method: "GET".to_string(),
        route: endpoint.route.clone(),
        module: endpoint.module.clone(),
        function: "router".to_string(),
        arity: 0,
        source: Some(endpoint.source.clone()),
    }));
    candidates.extend(
        manifest
            .static_responses
            .iter()
            .map(|response| WebPackageHandler {
                method: response.method.clone(),
                route: response.route.clone(),
                module: response.module.clone(),
                function: response.function.clone(),
                arity: response.arity,
                source: response.source.clone(),
            }),
    );
    candidates.extend(
        manifest
            .file_responses
            .iter()
            .map(|response| WebPackageHandler {
                method: response.method.clone(),
                route: response.route.clone(),
                module: response.module.clone(),
                function: response.function.clone(),
                arity: response.arity,
                source: response.source.clone(),
            }),
    );
    let matched = select_handler_for_request(candidates, method, request_path)?;

    if manifest.handlers.iter().any(|handler| {
        handler.method == matched.handler.method && handler.route == matched.handler.route
    }) {
        return Some(MatchedWebPackageRoute::Handler(Rc::new(matched)));
    }
    if let Some(response) = manifest.static_responses.iter().find(|response| {
        response.method == matched.handler.method && response.route == matched.handler.route
    }) {
        return Some(MatchedWebPackageRoute::StaticResponse(response.clone()));
    }
    if let Some(endpoint) = manifest
        .sse
        .iter()
        .find(|endpoint| matched.handler.method == "GET" && endpoint.route == matched.handler.route)
    {
        return Some(MatchedWebPackageRoute::Sse(endpoint.clone()));
    }
    let response = manifest
        .file_responses
        .iter()
        .find(|response| {
            response.method == matched.handler.method && response.route == matched.handler.route
        })?
        .clone();
    let path = package_relative_path(web_root, &response.path)?;
    path.is_file()
        .then_some(MatchedWebPackageRoute::FileResponse(response, path))
}

/// Selects the best manifest handler for one request.
///
/// Inputs:
/// - `handlers`: manifest handler entries.
/// - `method`: parsed HTTP method.
/// - `request_path`: URL path without query text.
///
/// Output:
/// - Best matching handler plus route params, if any.
///
/// Transformation:
/// - Applies method filtering first. `HEAD` falls back to `GET` only when no
///   explicit `HEAD` handler matches. Route precedence is exact, parameter,
///   wildcard, then fallback.
pub(super) fn select_handler_for_request(
    handlers: Vec<WebPackageHandler>,
    method: &str,
    request_path: &str,
) -> Option<MatchedWebPackageHandler> {
    select_handler_for_request_ref(&handlers, method, request_path)
}

pub(super) fn select_handler_for_request_ref(
    handlers: &[WebPackageHandler],
    method: &str,
    request_path: &str,
) -> Option<MatchedWebPackageHandler> {
    terlan_http_native::route_pattern::select_route(handlers, method, request_path, |handler| {
        (&handler.method, &handler.route)
    })
    .map(|selected| MatchedWebPackageHandler {
        handler: selected.route.clone(),
        params: selected.params,
    })
}

#[cfg(test)]
#[path = "route_test.rs"]
#[cfg(test)]
mod route_test;
