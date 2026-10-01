use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use terlan_http_native::route_pattern::match_route_pattern;
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

use crate::runtime::vm::ReplValue;
use crate::web_route::{route_ambiguity_key, route_segments, typed_route_param_segment};

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

/// Materializes one positional route capture according to its manifest type.
///
/// Untyped `:name` captures and wildcard captures remain strings. Typed
/// captures are converted only after route matching has validated their text,
/// so generated handler ABI validation sees the declared scalar type.
pub(super) fn route_param_argument(
    pattern: &str,
    name: &str,
    value: &str,
) -> Result<ReplValue, BoundaryError> {
    let declared_type = route_segments(pattern)
        .into_iter()
        .find_map(|segment| {
            typed_route_param_segment(segment)
                .filter(|(declared_name, _)| *declared_name == name)
                .map(|(_, type_name)| type_name)
        })
        .unwrap_or("String");
    match declared_type {
        "String" => Ok(ReplValue::String(value.to_string())),
        "Int" => value.parse::<i64>().map(ReplValue::Int).map_err(|error| {
            BoundaryError::message(
                ErrorDomain::CommandExecution,
                "materialize HTTP route parameter",
                format!(
                    "error[serve.route_param]: typed route capture `{name}:Int` could not materialize `{value}`: {error}"
                ),
            )
        }),
        "Bool" => match value {
            "true" => Ok(ReplValue::Bool(true)),
            "false" => Ok(ReplValue::Bool(false)),
            _ => Err(BoundaryError::message(
                ErrorDomain::CommandExecution,
                "materialize HTTP route parameter",
                format!(
                    "error[serve.route_param]: typed route capture `{name}:Bool` could not materialize `{value}`"
                ),
            )),
        },
        other => Err(BoundaryError::message(
            ErrorDomain::CommandExecution,
            "materialize HTTP route parameter",
            format!(
                "error[serve.route_param]: unsupported typed route capture `{name}:{other}`"
            ),
        )),
    }
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

/// Validates a set of dynamic HTTP handler routes.
///
/// Inputs:
/// - `handlers`: manifest-declared handler routes.
///
/// Output:
/// - `Ok(())` when no routes have the same method and ambiguous pattern shape.
/// - Stable `error[serve_package]` diagnostic otherwise.
///
/// Transformation:
/// - Normalizes parameter names out of route signatures so `/users/:id` and
///   `/users/:name` are rejected as ambiguous for the same method.
pub(crate) fn validate_handler_routes(handlers: &[WebPackageHandler]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for handler in handlers {
        let key = (
            handler.method.as_str(),
            route_ambiguity_key(&handler.route)?,
        );
        if !seen.insert(key.clone()) {
            return Err(format!(
                "error[serve_package]: duplicate or ambiguous handler route `{}` `{}`",
                handler.method, handler.route
            ));
        }
    }
    Ok(())
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
    let exact_method = select_best_handler_ref(
        handlers.iter().filter(|handler| handler.method == method),
        request_path,
    );
    if exact_method.is_some() || method != "HEAD" {
        return exact_method;
    }
    select_best_handler_ref(
        handlers.iter().filter(|handler| handler.method == "GET"),
        request_path,
    )
}

fn select_best_handler_ref<'a>(
    handlers: impl Iterator<Item = &'a WebPackageHandler>,
    request_path: &str,
) -> Option<MatchedWebPackageHandler> {
    handlers
        .filter_map(|handler| {
            match_route_pattern(&handler.route, request_path).map(|matched| {
                (
                    matched.score,
                    MatchedWebPackageHandler {
                        handler: handler.clone(),
                        params: matched.params,
                    },
                )
            })
        })
        .max_by_key(|(score, _)| *score)
        .map(|(_, matched)| matched)
}

#[cfg(test)]
#[path = "route_test.rs"]
#[cfg(test)]
mod route_test;
