use super::WebRouteManifestRows;

/// Validates the aggregate namespace after projecting each source router.
pub(super) fn validate_discovered_web_routes(rows: &WebRouteManifestRows) -> Result<(), String> {
    terlan_http_native::manifest::validate_route_namespace(
        &rows.handlers,
        &rows.websockets,
        &rows.sse,
        &rows.static_responses,
        &rows.file_responses,
    )
    .map_err(|error| {
        error
            .to_string()
            .replacen("error[serve_package]", "error[web_router]", 1)
    })
}
