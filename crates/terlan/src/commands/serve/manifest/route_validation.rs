//! Validation of static, WebSocket, and SSE route namespaces.

use super::*;

/// Validates one browser package asset kind.
///
/// Inputs:
/// - `kind`: asset kind from the package manifest.
///
/// Output:
/// - `Ok(())` when the kind belongs to the current browser package contract.
///
/// Transformation:
/// - Rejects unknown manifest asset categories before the server treats them as
///   static files.
pub(super) fn validate_asset_kind(kind: &str) -> Result<(), String> {
    match kind {
        "javascript-module"
        | "javascript-source-map"
        | "asset-file"
        | "asset-css"
        | "asset-markdown"
        | "static-asset"
        | "css" => Ok(()),
        other => Err(format!(
            "error[serve_package]: unsupported browser package asset kind `{other}`"
        )),
    }
}

/// Validates route ambiguity for static response manifest rows.
///
/// Inputs:
/// - `responses`: manifest-declared cacheable static responses.
///
/// Output:
/// - `Ok(())` when no method/route pattern is duplicated or ambiguous.
/// - Stable `error[serve_package]` diagnostic otherwise.
///
/// Transformation:
/// - Applies the same route ambiguity key used by dynamic handlers so static
///   response manifests cannot encode two equivalent route shapes.
pub(super) fn validate_static_response_routes(
    responses: &[WebPackageStaticResponse],
) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for response in responses {
        let key = (
            response.method.as_str(),
            crate::web_route::route_ambiguity_key(&response.route)?,
        );
        if !seen.insert(key) {
            return Err(format!(
                "error[serve_package]: duplicate or ambiguous static response route `{}` `{}`",
                response.method, response.route
            ));
        }
    }
    Ok(())
}

/// Validates route ambiguity for file response manifest rows.
///
/// Inputs:
/// - `responses`: manifest-declared route-backed file responses.
///
/// Output:
/// - `Ok(())` when no method/route pattern is duplicated or ambiguous.
/// - Stable `error[serve_package]` diagnostic otherwise.
///
/// Transformation:
/// - Applies the same route ambiguity key used by dynamic handlers so file
///   response manifests cannot encode two equivalent route shapes.
pub(super) fn validate_file_response_routes(
    responses: &[WebPackageFileResponse],
) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for response in responses {
        let key = (
            response.method.as_str(),
            crate::web_route::route_ambiguity_key(&response.route)?,
        );
        if !seen.insert(key) {
            return Err(format!(
                "error[serve_package]: duplicate or ambiguous file response route `{}` `{}`",
                response.method, response.route
            ));
        }
    }
    Ok(())
}

/// Validates route ambiguity for WebSocket manifest rows.
///
/// Inputs:
/// - `websockets`: manifest-declared WebSocket routes.
///
/// Output:
/// - `Ok(())` when no route pattern is duplicated or ambiguous.
/// - Stable `error[serve_package]` diagnostic otherwise.
///
/// Transformation:
/// - Treats WebSocket routes as GET upgrade paths for route-shape ambiguity
///   while keeping them in a distinct manifest section.
pub(super) fn validate_websocket_routes(websockets: &[WebPackageWebSocket]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for websocket in websockets {
        let key = crate::web_route::route_ambiguity_key(&websocket.route)?;
        if !seen.insert(key) {
            return Err(format!(
                "error[serve_package]: duplicate or ambiguous websocket route `{}`",
                websocket.route
            ));
        }
    }
    Ok(())
}

/// Rejects duplicate or ambiguous SSE route shapes.
pub(super) fn validate_sse_routes(endpoints: &[WebPackageSse]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for endpoint in endpoints {
        let key = crate::web_route::route_ambiguity_key(&endpoint.route)?;
        if !seen.insert(key) {
            return Err(format!(
                "error[serve_package]: duplicate or ambiguous SSE route `{}`",
                endpoint.route
            ));
        }
    }
    Ok(())
}

/// Validates the combined dynamic/static route namespace.
///
/// Inputs:
/// - `handlers`: dynamic VM handler routes.
/// - `websockets`: WebSocket upgrade routes.
/// - `responses`: manifest-cached static response routes.
/// - `file_responses`: manifest file response routes.
///
/// Output:
/// - `Ok(())` when no method/route shape is claimed by multiple sections.
/// - Stable `error[serve_package]` diagnostic otherwise.
///
/// Transformation:
/// - Normalizes route parameters with the shared route ambiguity key so
///   `/users/:id` and `/users/:name` collide even across manifest sections.
pub(super) fn validate_manifest_route_namespace(
    handlers: &[WebPackageHandler],
    websockets: &[WebPackageWebSocket],
    sse: &[WebPackageSse],
    responses: &[WebPackageStaticResponse],
    file_responses: &[WebPackageFileResponse],
) -> Result<(), String> {
    let mut seen = std::collections::BTreeMap::new();
    for handler in handlers {
        let key = (
            handler.method.as_str(),
            crate::web_route::route_ambiguity_key(&handler.route)?,
        );
        seen.insert(
            key,
            format!("handler route `{}` `{}`", handler.method, handler.route),
        );
    }
    for websocket in websockets {
        let key = (
            "GET",
            crate::web_route::route_ambiguity_key(&websocket.route)?,
        );
        if let Some(existing) = seen.get(&key) {
            return Err(format!(
                "error[serve_package]: websocket route `GET` `{}` conflicts with {existing}",
                websocket.route
            ));
        }
        seen.insert(key, format!("websocket route `GET` `{}`", websocket.route));
    }
    for endpoint in sse {
        let key = (
            "GET",
            crate::web_route::route_ambiguity_key(&endpoint.route)?,
        );
        if let Some(existing) = seen.get(&key) {
            return Err(format!(
                "error[serve_package]: SSE route `GET` `{}` conflicts with {existing}",
                endpoint.route
            ));
        }
        seen.insert(key, format!("SSE route `GET` `{}`", endpoint.route));
    }
    for response in responses {
        let key = (
            response.method.as_str(),
            crate::web_route::route_ambiguity_key(&response.route)?,
        );
        if let Some(existing) = seen.get(&key) {
            return Err(format!(
                "error[serve_package]: static response route `{}` `{}` conflicts with {existing}",
                response.method, response.route
            ));
        }
        seen.insert(
            key,
            format!(
                "static response route `{}` `{}`",
                response.method, response.route
            ),
        );
    }
    for response in file_responses {
        let key = (
            response.method.as_str(),
            crate::web_route::route_ambiguity_key(&response.route)?,
        );
        if let Some(existing) = seen.get(&key) {
            return Err(format!(
                "error[serve_package]: file response route `{}` `{}` conflicts with {existing}",
                response.method, response.route
            ));
        }
        seen.insert(
            key,
            format!(
                "file response route `{}` `{}`",
                response.method, response.route
            ),
        );
    }
    Ok(())
}
