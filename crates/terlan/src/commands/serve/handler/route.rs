use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use percent_encoding::percent_decode_str;

use crate::commands::web_route::{route_ambiguity_key, route_segments, typed_route_param_segment};

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

/// One best route selected across all executable manifest response kinds.
pub(in crate::commands::serve) enum MatchedWebPackageRoute {
    WebSocket(WebPackageWebSocket),
    Handler(Arc<MatchedWebPackageHandler>),
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
    matched: Arc<MatchedWebPackageHandler>,
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
                .then(|| Arc::clone(&cached.matched))
        }) {
            return Some(MatchedWebPackageRoute::Handler(matched));
        }
        let matched = Arc::new(select_handler_for_request_ref(
            &manifest.handlers,
            method,
            request_path,
        )?);
        LAST_SIMPLE_HANDLER_ROUTE.with(|cached| {
            *cached.borrow_mut() = Some(LastSimpleHandlerRoute {
                manifest: Arc::clone(manifest),
                method: method.to_string(),
                request_path: request_path.to_string(),
                matched: Arc::clone(&matched),
            });
        });
        return Some(MatchedWebPackageRoute::Handler(matched));
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
                module: "static".to_string(),
                function: "file".to_string(),
                arity: 1,
                source: response.source.clone(),
            }),
    );
    let matched = select_handler_for_request(candidates, method, request_path)?;

    if manifest.handlers.iter().any(|handler| {
        handler.method == matched.handler.method && handler.route == matched.handler.route
    }) {
        return Some(MatchedWebPackageRoute::Handler(Arc::new(matched)));
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

/// One parsed route match candidate.
///
/// Inputs:
/// - Produced by `match_route_pattern` for a concrete request path.
///
/// Output:
/// - Route params and ordering score used for precedence resolution.
///
/// Transformation:
/// - Keeps precedence data local to the server matcher while exposing only
///   params through `MatchedWebPackageHandler`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RoutePatternMatch {
    params: Vec<(String, String)>,
    score: RouteScore,
}

/// Route precedence score.
///
/// Inputs:
/// - Produced from a successful route pattern match.
///
/// Output:
/// - Comparable score where exact/static-heavy routes sort ahead of parameter,
///   wildcard, and fallback routes.
///
/// Transformation:
/// - Encodes the 0.0.5 precedence rule: exact, then parameter, then wildcard,
///   then fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RouteScore {
    class: u8,
    static_segments: usize,
    total_segments: usize,
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

/// Matches one route pattern against one request path.
///
/// Inputs:
/// - `pattern`: manifest route pattern such as `/users/:id`,
///   `/users/{id:Int}`, or `/assets/*`.
/// - `request_path`: concrete URL path without query text.
///
/// Output:
/// - Route params and precedence score if the pattern matches.
///
/// Transformation:
/// - Captures decoded named params by segment, lets final wildcard consume the
///   remaining decoded path, treats `*` as a no-param global fallback, and
///   scores exact/static-heavy matches above generic ones.
fn match_route_pattern(pattern: &str, request_path: &str) -> Option<RoutePatternMatch> {
    let pattern_segments = route_segments(pattern);
    let request_segments = route_segments(request_path);
    if pattern == "*" {
        return Some(RoutePatternMatch {
            params: Vec::new(),
            score: RouteScore {
                class: 0,
                static_segments: 0,
                total_segments: 0,
            },
        });
    }
    if pattern == "/" {
        return request_segments.is_empty().then_some(RoutePatternMatch {
            params: Vec::new(),
            score: RouteScore {
                class: 3,
                static_segments: 0,
                total_segments: 0,
            },
        });
    }

    let has_wildcard = pattern_segments
        .last()
        .is_some_and(|segment| *segment == "*");
    if !has_wildcard && pattern_segments.len() != request_segments.len() {
        return None;
    }
    if has_wildcard && request_segments.len() < pattern_segments.len().saturating_sub(1) {
        return None;
    }

    let mut params = Vec::new();
    let mut static_segments = 0;
    let mut has_param = false;
    for (index, pattern_segment) in pattern_segments.iter().enumerate() {
        if *pattern_segment == "*" {
            let wildcard = decode_wildcard_route_param(&request_segments[index..])?;
            params.push(("*".to_string(), wildcard));
            return Some(RoutePatternMatch {
                params,
                score: RouteScore {
                    class: if index == 0 { 0 } else { 1 },
                    static_segments,
                    total_segments: pattern_segments.len(),
                },
            });
        }
        let request_segment = request_segments.get(index)?;
        if let Some(name) = pattern_segment.strip_prefix(':') {
            has_param = true;
            params.push((
                name.to_string(),
                decode_route_param_segment(request_segment)?,
            ));
        } else if let Some((name, type_name)) = typed_route_param_segment(pattern_segment) {
            if !route_param_segment_matches_type(request_segment, type_name) {
                return None;
            }
            has_param = true;
            params.push((
                name.to_string(),
                decode_route_param_segment(request_segment)?,
            ));
        } else if pattern_segment == request_segment {
            static_segments += 1;
        } else {
            return None;
        }
    }

    Some(RoutePatternMatch {
        params,
        score: RouteScore {
            class: if has_param { 2 } else { 3 },
            static_segments,
            total_segments: pattern_segments.len(),
        },
    })
}

/// Returns whether one raw request segment satisfies a route parameter type.
///
/// Inputs:
/// - `segment`: raw URL path segment before percent decoding.
/// - `type_name`: route-declared parameter type.
///
/// Output:
/// - `true` when the request segment can be decoded as that route type.
///
/// Transformation:
/// - Applies the same 0.0.5 supported route-param type set as serve-time
///   handler invocation, but does it during matching so invalid typed routes
///   do not dispatch to handlers.
fn route_param_segment_matches_type(segment: &str, type_name: &str) -> bool {
    let Some(decoded) = decode_route_param_segment(segment) else {
        return false;
    };
    match type_name {
        "String" => true,
        "Int" => decoded.parse::<i64>().is_ok(),
        "Bool" => decoded == "true" || decoded == "false",
        _ => false,
    }
}

/// Decodes one captured route parameter segment.
///
/// Inputs:
/// - `segment`: raw URL path segment from the request path.
///
/// Output:
/// - UTF-8 decoded segment, or `None` if the capture is not valid UTF-8 after
///   percent decoding.
///
/// Transformation:
/// - Applies URL percent decoding without query-string `+` handling so path
///   parameters keep path semantics.
fn decode_route_param_segment(segment: &str) -> Option<String> {
    percent_decode_str(segment)
        .decode_utf8()
        .ok()
        .map(|decoded| decoded.into_owned())
}

/// Decodes the final wildcard route capture.
///
/// Inputs:
/// - `segments`: remaining raw URL path segments consumed by `*`.
///
/// Output:
/// - UTF-8 decoded wildcard value joined with `/`.
///
/// Transformation:
/// - Decodes each consumed segment independently, then rejoins them so wildcard
///   captures preserve their visible path shape.
fn decode_wildcard_route_param(segments: &[&str]) -> Option<String> {
    segments
        .iter()
        .map(|segment| decode_route_param_segment(segment))
        .collect::<Option<Vec<_>>>()
        .map(|decoded| decoded.join("/"))
}

#[cfg(test)]
#[path = "route_test.rs"]
mod route_test;
