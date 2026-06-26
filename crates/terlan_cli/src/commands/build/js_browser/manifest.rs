use std::path::Path;

use serde::Serialize;

use crate::commands::emit_js::target_contract::JsTargetContract;

use super::super::{fingerprint, write_build_file};

/// Writes the browser package manifest.
///
/// Inputs:
/// - `web_root`: root browser package directory.
/// - `contract`: selected JS artifact contract.
/// - `assets`: copied asset metadata.
/// - `handlers`: dynamic route-handler rows.
/// - `websockets`: WebSocket upgrade route rows.
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
    handlers: Vec<WebHandlerArtifact>,
    websockets: Vec<WebSocketArtifact>,
    static_responses: Vec<WebStaticResponseArtifact>,
    file_responses: Vec<WebFileResponseArtifact>,
    error_handler: Option<WebErrorHandlerArtifact>,
    incremental: bool,
) -> Result<(), String> {
    let build_id = web_build_id(
        contract,
        &assets,
        &handlers,
        &websockets,
        &static_responses,
        &file_responses,
        error_handler.as_ref(),
    );
    let manifest = WebBuildManifest {
        schema: "terlan-web-build-v1",
        target_profile: contract.profile_name,
        build_id,
        source_js_manifest: "../js/manifest.json",
        index: "index.html",
        assets,
        handlers,
        websockets,
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
    contract: JsTargetContract,
    assets: &[WebAssetArtifact],
    handlers: &[WebHandlerArtifact],
    websockets: &[WebSocketArtifact],
    static_responses: &[WebStaticResponseArtifact],
    file_responses: &[WebFileResponseArtifact],
    error_handler: Option<&WebErrorHandlerArtifact>,
) -> String {
    let mut text = String::new();
    text.push_str("schema=terlan-web-build-v1\n");
    text.push_str("target_profile=");
    text.push_str(contract.profile_name);
    text.push('\n');
    text.push_str("source_js_manifest=../js/manifest.json\n");
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
        text.push_str(&websocket.route);
        text.push('|');
        text.push_str(&websocket.protocol);
        text.push('\n');
    }
    for response in static_responses {
        text.push_str("static_response=");
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
    source_js_manifest: &'static str,
    index: &'static str,
    assets: Vec<WebAssetArtifact>,
    handlers: Vec<WebHandlerArtifact>,
    websockets: Vec<WebSocketArtifact>,
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
///   asset path, plus a deterministic fingerprint for release checks.
#[derive(Debug, Serialize)]
pub(super) struct WebAssetArtifact {
    pub(super) module: String,
    pub(super) kind: String,
    pub(super) source_relative_path: String,
    pub(super) web_relative_path: String,
    pub(super) fingerprint: u64,
}

/// Browser dynamic handler manifest entry.
///
/// Inputs:
/// - Route metadata generated from supported Terlan `std.http.Router` builder
///   calls.
///
/// Output:
/// - Serializable handler entry inside the browser package manifest.
///
/// Transformation:
/// - Preserves route identity, BEAM callback identity, and optional source
///   metadata for dynamic routes that cannot be served as static responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct WebHandlerArtifact {
    pub(super) method: String,
    pub(super) route: String,
    pub(super) module: String,
    pub(super) function: String,
    pub(super) arity: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) source: Option<WebSourceSpanArtifact>,
}

/// Browser WebSocket route manifest entry.
///
/// Inputs:
/// - Generated from source-visible WebSocket route metadata.
///
/// Output:
/// - Serializable WebSocket entry inside the browser package manifest.
///
/// Transformation:
/// - Preserves only route, runtime protocol identity, and optional source
///   location while `terlc serve` owns the upgrade and connection lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct WebSocketArtifact {
    pub(super) route: String,
    pub(super) protocol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) source: Option<WebSourceSpanArtifact>,
}

/// Browser route-handler source span manifest entry.
///
/// Inputs:
/// - Generated from route source metadata and syntax-output spans.
///
/// Output:
/// - Serializable source coordinate attached to dynamic handler rows.
///
/// Transformation:
/// - Stores a package-safe relative path and one-based line/column so local
///   dev errors and logs can point back at Terlan source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct WebSourceSpanArtifact {
    pub(super) path: String,
    pub(super) line: usize,
    pub(super) column: usize,
}

/// Browser static response manifest entry.
///
/// Inputs:
/// - Generated from source handlers with constant `std.http.Response` bodies.
///
/// Output:
/// - Serializable response row inside the browser package manifest.
///
/// Transformation:
/// - Stores cacheable route responses directly in the manifest so local and
///   release servers can answer them without invoking BEAM-backed handlers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct WebStaticResponseArtifact {
    pub(super) method: String,
    pub(super) route: String,
    pub(super) status: u16,
    pub(super) content_type: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) headers: Vec<WebResponseHeaderArtifact>,
    pub(super) body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) source: Option<WebSourceSpanArtifact>,
}

/// Browser response header manifest entry.
///
/// Inputs:
/// - Generated from source-level constant response builders.
///
/// Output:
/// - Serializable header name/value pair inside a static response row.
///
/// Transformation:
/// - Keeps cacheable response metadata explicit in JSON so redirects and later
///   simple static responses can be served without dynamic handler execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct WebResponseHeaderArtifact {
    pub(super) name: String,
    pub(super) value: String,
}

/// Browser file response manifest entry.
///
/// Inputs:
/// - Generated from source handlers with constant `Response.file` bodies.
///
/// Output:
/// - Serializable file response row inside the browser package manifest.
///
/// Transformation:
/// - Stores a route-backed package-relative file response so local and release
///   servers can stream the file through the Rust HTTP path without invoking
///   BEAM-backed handlers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct WebFileResponseArtifact {
    pub(super) method: String,
    pub(super) route: String,
    pub(super) path: String,
    pub(super) status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) source: Option<WebSourceSpanArtifact>,
}

/// Browser router-level error handler manifest entry.
///
/// Inputs:
/// - Generated from source `std.http.Router.error` builder calls.
///
/// Output:
/// - Serializable error-handler entry inside the browser package manifest.
///
/// Transformation:
/// - Preserves only source-visible module/function identity and arity so
///   runtime error dispatch can be implemented without reparsing source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct WebErrorHandlerArtifact {
    pub(super) module: String,
    pub(super) function: String,
    pub(super) arity: usize,
}
