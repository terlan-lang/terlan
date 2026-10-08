use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::commands::emit_js::target_contract::JsTargetContract;

use super::super::{fingerprint, write_build_file};
use super::routes::WebRouteManifestRows;

/// Writes the browser package manifest.
///
/// Inputs:
/// - `web_root`: root browser package directory.
/// - `contract`: selected JS artifact contract.
/// - `assets`: copied asset metadata.
/// - `handlers`: dynamic route-handler rows.
/// - `websockets`: WebSocket upgrade route rows.
/// - `sse`: SSE route rows.
/// - `static_responses`: route-backed constant response rows.
/// - `file_responses`: route-backed file response rows.
/// - `error_handler`: optional router-level error handler.
/// - `incremental`: whether unchanged writes may be skipped.
///
/// Output:
/// - `Ok(())` after `_build/web/manifest.json` exists.
///
/// Transformation:
/// - Serializes the web package manifest consumed by `terlc serve`, including
///   deterministic route identity and a stable build id.
pub(super) fn write_browser_manifest(
    web_root: &Path,
    contract: JsTargetContract,
    assets: Vec<WebAssetArtifact>,
    routes: WebRouteManifestRows,
    error_handler: Option<WebErrorHandlerArtifact>,
    incremental: bool,
) -> Result<(), String> {
    write_web_manifest(
        web_root,
        contract.profile_name,
        Some("../js/manifest.json"),
        assets,
        routes,
        error_handler,
        incremental,
    )
}

/// Writes the route manifest for a native VM service package.
pub(super) fn write_vm_service_manifest(
    web_root: &Path,
    assets: Vec<WebAssetArtifact>,
    routes: WebRouteManifestRows,
    error_handler: Option<WebErrorHandlerArtifact>,
    incremental: bool,
) -> Result<(), String> {
    write_web_manifest(
        web_root,
        "vm",
        None,
        assets,
        routes,
        error_handler,
        incremental,
    )
}

fn write_web_manifest(
    web_root: &Path,
    target_profile: &'static str,
    source_js_manifest: Option<&'static str>,
    assets: Vec<WebAssetArtifact>,
    routes: WebRouteManifestRows,
    error_handler: Option<WebErrorHandlerArtifact>,
    incremental: bool,
) -> Result<(), String> {
    validate_unique_web_asset_paths(&assets)?;
    // Build and serve admit the same package-owned records.
    for handler in &routes.handlers {
        terlan_http_native::manifest::validate_handler(handler)?;
    }
    for websocket in &routes.websockets {
        terlan_http_native::manifest::validate_websocket(websocket)?;
    }
    for endpoint in &routes.sse {
        terlan_http_native::manifest::validate_sse(endpoint)?;
    }
    for response in &routes.static_responses {
        terlan_http_native::manifest::validate_static_response(response)?;
    }
    for response in &routes.file_responses {
        terlan_http_native::manifest::validate_file_response(response)?;
    }
    if let Some(handler) = &error_handler {
        terlan_http_native::manifest::validate_error_handler(handler)?;
    }
    terlan_http_native::manifest::validate_route_namespace(
        &routes.handlers,
        &routes.websockets,
        &routes.sse,
        &routes.static_responses,
        &routes.file_responses,
    )?;
    let build_id = web_build_id(
        target_profile,
        source_js_manifest,
        &assets,
        &routes,
        error_handler.as_ref(),
    );
    let WebRouteManifestRows {
        handlers,
        websockets,
        sse,
        static_responses,
        file_responses,
    } = routes;
    let manifest = WebBuildManifest {
        schema: "terlan-web-build-v1",
        target_profile,
        build_id,
        source_js_manifest,
        index: "index.html",
        assets,
        handlers,
        websockets,
        sse,
        static_responses,
        file_responses,
        error_handler,
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("cannot serialize browser package manifest: {err}"))?;
    write_build_file(
        &web_root.join("manifest.json"),
        manifest_json.as_bytes(),
        incremental,
    )
}

/// Rejects duplicate final browser asset paths before manifest serialization.
fn validate_unique_web_asset_paths(assets: &[WebAssetArtifact]) -> Result<(), String> {
    let mut seen = BTreeMap::<&str, &WebAssetArtifact>::new();
    for asset in assets {
        if let Some(existing) = seen.get(asset.web_relative_path.as_str()) {
            return Err(format!(
                "error[web_assets]: duplicate browser asset path `{}` from `{}` and `{}`",
                asset.web_relative_path, existing.source_relative_path, asset.source_relative_path
            ));
        }
        seen.insert(asset.web_relative_path.as_str(), asset);
    }
    Ok(())
}

/// Builds a deterministic browser package identifier.
///
/// Inputs:
/// - `contract`: selected JavaScript target contract.
/// - `assets`: copied browser asset manifest entries.
/// - `handlers`: generated dynamic handler manifest entries.
///
/// Output:
/// - Stable `web-<hex>` build id for the manifest content currently known to
///   the browser package writer.
///
/// Transformation:
/// - Serializes the route/static asset identity fields into one deterministic
///   byte stream and hashes it with the compiler's existing manifest
///   fingerprint helper. The id intentionally excludes timestamps and absolute
///   paths so local logs can correlate requests to reproducible build output.
fn web_build_id(
    target_profile: &str,
    source_js_manifest: Option<&str>,
    assets: &[WebAssetArtifact],
    routes: &WebRouteManifestRows,
    error_handler: Option<&WebErrorHandlerArtifact>,
) -> String {
    let WebRouteManifestRows {
        handlers,
        websockets,
        sse,
        static_responses,
        file_responses,
    } = routes;
    let mut text = String::new();
    text.push_str("schema=terlan-web-build-v1\n");
    text.push_str("target_profile=");
    text.push_str(target_profile);
    text.push('\n');
    if let Some(source_js_manifest) = source_js_manifest {
        text.push_str("source_js_manifest=");
        text.push_str(source_js_manifest);
        text.push('\n');
    }
    text.push_str("index=index.html\n");
    for asset in assets {
        text.push_str("asset=");
        text.push_str(&asset.module);
        text.push('|');
        text.push_str(&asset.kind);
        text.push('|');
        text.push_str(&asset.source_relative_path);
        text.push('|');
        text.push_str(&asset.web_relative_path);
        text.push('|');
        text.push_str(&asset.fingerprint.to_string());
        text.push('|');
        text.push_str(&asset.integrity);
        text.push('\n');
    }
    for handler in handlers {
        text.push_str("handler=");
        text.push_str(&handler.method);
        text.push('|');
        text.push_str(&handler.route);
        text.push('|');
        text.push_str(&handler.module);
        text.push('|');
        text.push_str(&handler.function);
        text.push('|');
        text.push_str(&handler.arity.to_string());
        text.push('\n');
    }
    for websocket in websockets {
        text.push_str("websocket=");
        text.push_str(&websocket.module);
        text.push('|');
        text.push_str(&websocket.route);
        text.push('|');
        text.push_str(&websocket.protocol);
        text.push('\n');
    }
    for endpoint in sse {
        text.push_str("sse=");
        text.push_str(&endpoint.module);
        text.push('|');
        text.push_str(&endpoint.route);
        text.push('\n');
    }
    for response in static_responses {
        text.push_str("static_response=");
        text.push_str(&response.module);
        text.push('|');
        text.push_str(&response.function);
        text.push('|');
        text.push_str(&response.arity.to_string());
        text.push('|');
        text.push_str(&response.method);
        text.push('|');
        text.push_str(&response.route);
        text.push('|');
        text.push_str(&response.status.to_string());
        text.push('|');
        text.push_str(&response.content_type);
        text.push('|');
        text.push_str(&response.body);
        for header in &response.headers {
            text.push('|');
            text.push_str(&header.name);
            text.push('=');
            text.push_str(&header.value);
        }
        text.push('\n');
    }
    for response in file_responses {
        text.push_str("file_response=");
        text.push_str(&response.method);
        text.push('|');
        text.push_str(&response.route);
        text.push('|');
        text.push_str(&response.path);
        text.push('|');
        text.push_str(&response.status.to_string());
        text.push('|');
        if let Some(content_type) = &response.content_type {
            text.push_str(content_type);
        }
        text.push('\n');
    }
    if let Some(handler) = error_handler {
        text.push_str("error_handler=");
        text.push_str(&handler.module);
        text.push('|');
        text.push_str(&handler.function);
        text.push('|');
        text.push_str(&handler.arity.to_string());
        text.push('\n');
    }
    format!("web-{:016x}", fingerprint(text.as_bytes()))
}

/// Browser package manifest.
///
/// Inputs:
/// - Created after JS browser builds copy module assets into `_build/web/`.
///
/// Output:
/// - Serializable manifest stored at `_build/web/manifest.json`.
///
/// Transformation:
/// - Records the source JS manifest, HTML entrypoint, target profile, route
///   rows, static responses, and copied asset list without embedding source text.
#[derive(Debug, Serialize)]
struct WebBuildManifest {
    schema: &'static str,
    target_profile: &'static str,
    build_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_js_manifest: Option<&'static str>,
    index: &'static str,
    assets: Vec<WebAssetArtifact>,
    handlers: Vec<WebHandlerArtifact>,
    websockets: Vec<WebSocketArtifact>,
    sse: Vec<WebSseArtifact>,
    static_responses: Vec<WebStaticResponseArtifact>,
    file_responses: Vec<WebFileResponseArtifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_handler: Option<WebErrorHandlerArtifact>,
}

/// Browser asset manifest entry.
///
/// Inputs:
/// - Created per copied JavaScript module asset.
///
/// Output:
/// - Serializable asset entry inside the browser package manifest.
///
/// Transformation:
/// - Connects a Terlan module to its original JS artifact path and copied web
///   asset path, plus deterministic fingerprint and browser integrity metadata
///   for release checks.
#[derive(Debug, Serialize)]
pub(super) struct WebAssetArtifact {
    pub(super) module: String,
    pub(super) kind: String,
    pub(super) source_relative_path: String,
    pub(super) web_relative_path: String,
    pub(super) fingerprint: u64,
    pub(super) integrity: String,
}

pub(super) use terlan_http_native::manifest::{
    ErrorHandler as WebErrorHandlerArtifact, FileResponse as WebFileResponseArtifact,
    HandlerRoute as WebHandlerArtifact, SourceSpan as WebSourceSpanArtifact,
    SseRoute as WebSseArtifact, StaticResponse as WebStaticResponseArtifact,
    WebSocketRoute as WebSocketArtifact,
};
