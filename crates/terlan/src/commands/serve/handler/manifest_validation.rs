//! Host-only route reservation; HTTP metadata admission belongs to the package.
use super::*;
use terlan_http_native::manifest;

pub(in crate::commands::serve) use manifest::validate_error_handler;

fn validate_reserved_route(route: &str) -> Result<(), String> {
    if route == super::super::RELOAD_ENDPOINT {
        return Err(format!(
            "error[serve_package]: handler route `{route}` is reserved for live reload"
        ));
    }
    Ok(())
}

pub(in crate::commands::serve) fn validate_handler(row: &WebPackageHandler) -> Result<(), String> {
    validate_reserved_route(&row.route)?;
    manifest::validate_handler(row).map_err(String::from)
}

pub(in crate::commands::serve) fn validate_static_response(
    row: &WebPackageStaticResponse,
) -> Result<(), String> {
    validate_reserved_route(&row.route)?;
    manifest::validate_static_response(row).map_err(String::from)
}

pub(in crate::commands::serve) fn validate_file_response(
    row: &WebPackageFileResponse,
) -> Result<(), String> {
    validate_reserved_route(&row.route)?;
    manifest::validate_file_response(row).map_err(String::from)
}

pub(in crate::commands::serve) fn validate_sse(row: &WebPackageSse) -> Result<(), String> {
    validate_reserved_route(&row.route)?;
    manifest::validate_sse(row).map_err(String::from)
}

pub(in crate::commands::serve) fn validate_websocket(
    row: &WebPackageWebSocket,
) -> Result<(), String> {
    validate_reserved_route(&row.route)?;
    manifest::validate_websocket(row).map_err(String::from)
}
