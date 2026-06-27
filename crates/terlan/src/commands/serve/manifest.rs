use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::commands::build::project_manifest::{self, ProjectServerTls, ProjectServerTlsMode};

use super::compose_check;
use super::handler::{
    validate_error_handler, validate_file_response, validate_handler, validate_handler_routes,
    validate_static_response, validate_websocket, WebPackageErrorHandler, WebPackageFileResponse,
    WebPackageHandler, WebPackageStaticResponse, WebPackageWebSocket,
};
use super::package_relative_path;

/// Browser package manifest consumed by `terlc serve`.
///
/// Inputs:
/// - Deserialized from `_build/web/manifest.json`.
///
/// Output:
/// - Minimal manifest fields required to validate and serve the package.
///
/// Transformation:
/// - Ignores build-only metadata while preserving schema, index, asset paths,
///   and handler references needed by the local HTTP server.
#[derive(Debug, Deserialize)]
pub(super) struct WebPackageManifest {
    schema: String,
    #[serde(default = "default_build_id")]
    pub(super) build_id: String,
    index: String,
    assets: Vec<WebPackageAsset>,
    #[serde(default)]
    pub(super) handlers: Vec<WebPackageHandler>,
    #[serde(default)]
    pub(super) websockets: Vec<WebPackageWebSocket>,
    #[serde(default)]
    pub(super) static_responses: Vec<WebPackageStaticResponse>,
    #[serde(default)]
    pub(super) file_responses: Vec<WebPackageFileResponse>,
    #[serde(default)]
    pub(super) error_handler: Option<WebPackageErrorHandler>,
}

/// Returns the build id used for older or hand-authored manifests.
///
/// Inputs:
/// - None.
///
/// Output:
/// - `"unknown"` build id text.
///
/// Transformation:
/// - Supplies a stable local-development fallback when a manifest predates the
///   explicit build id field.
fn default_build_id() -> String {
    "unknown".to_string()
}

/// One asset entry inside the browser package manifest.
///
/// Inputs:
/// - Deserialized from `_build/web/manifest.json`.
///
/// Output:
/// - Asset kind and web-relative path used for package validation.
///
/// Transformation:
/// - Narrows the manifest entry to fields required by the server so additional
///   build metadata can evolve without changing this command.
#[derive(Debug, Deserialize)]
struct WebPackageAsset {
    kind: String,
    web_relative_path: String,
}

/// Validates one browser web package.
///
/// Inputs:
/// - `web_root`: package root that should contain `manifest.json`.
///
/// Output:
/// - `Ok(())` when the package schema, index file, assets, and handlers are
///   valid.
/// - `Err(String)` with a stable `error[serve_package]` diagnostic otherwise.
///
/// Transformation:
/// - Deserializes the browser manifest, verifies expected schema, resolves
///   manifest-relative paths safely under the package root, checks referenced
///   files, validates dynamic handler metadata, and reuses the project
///   manifest parser for adjacent TLS configuration when present.
pub(crate) fn validate_web_package(web_root: &Path) -> Result<(), String> {
    let manifest_path = web_root.join("manifest.json");
    let manifest = read_web_manifest(web_root).map_err(|message| {
        format!(
            "error[serve_package]: cannot read browser package manifest `{}`: {message}",
            manifest_path.display(),
        )
    })?;
    if manifest.schema != "terlan-web-build-v1" {
        return Err(format!(
            "error[serve_package]: unsupported browser package schema `{}`",
            manifest.schema
        ));
    }

    let index_path = package_relative_path(web_root, &manifest.index).ok_or_else(|| {
        format!(
            "error[serve_package]: unsafe browser package index path `{}`",
            manifest.index
        )
    })?;
    if !index_path.is_file() {
        return Err(format!(
            "error[serve_package]: browser package index `{}` does not exist",
            index_path.display()
        ));
    }

    for asset in manifest.assets {
        validate_asset_kind(&asset.kind)?;
        let asset_path =
            package_relative_path(web_root, &asset.web_relative_path).ok_or_else(|| {
                format!(
                    "error[serve_package]: unsafe browser package asset path `{}`",
                    asset.web_relative_path
                )
            })?;
        if !asset_path.is_file() {
            return Err(format!(
                "error[serve_package]: browser package asset `{}` does not exist",
                asset_path.display()
            ));
        }
    }
    for handler in &manifest.handlers {
        validate_handler(handler)?;
    }
    for websocket in &manifest.websockets {
        validate_websocket(websocket)?;
    }
    for response in &manifest.static_responses {
        validate_static_response(response)?;
    }
    for response in &manifest.file_responses {
        validate_file_response(response)?;
        let response_path = package_relative_path(web_root, &response.path).ok_or_else(|| {
            format!(
                "error[serve_package]: unsafe file response path `{}`",
                response.path
            )
        })?;
        if !response_path.is_file() {
            return Err(format!(
                "error[serve_package]: file response path `{}` does not exist",
                response_path.display()
            ));
        }
    }
    if let Some(handler) = &manifest.error_handler {
        validate_error_handler(handler)?;
    }
    validate_handler_routes(&manifest.handlers)?;
    validate_websocket_routes(&manifest.websockets)?;
    validate_static_response_routes(&manifest.static_responses)?;
    validate_file_response_routes(&manifest.file_responses)?;
    validate_manifest_route_namespace(
        &manifest.handlers,
        &manifest.websockets,
        &manifest.static_responses,
        &manifest.file_responses,
    )?;
    validate_adjacent_project_manifest(web_root)?;

    Ok(())
}

/// Validates project metadata that belongs to a packaged web root.
///
/// Inputs:
/// - `web_root`: package root passed to `terlc serve`.
///
/// Output:
/// - `Ok(())` when no nearby project manifest exists or when it parses.
/// - `Err(String)` when a nearby `terlan.toml` has invalid project metadata.
///
/// Transformation:
/// - Searches the package-local, package-parent, and `_build/web` project-root
///   locations for `terlan.toml`; the first discovered manifest is parsed with
///   the build command's manifest parser so serve-time TLS checks cannot drift
///   from build-time TLS checks.
fn validate_adjacent_project_manifest(web_root: &Path) -> Result<(), String> {
    if let Some(path) = adjacent_project_manifest_path(web_root) {
        let manifest = project_manifest::read_project_manifest(&path).map_err(|message| {
            format!(
                "error[serve_package]: invalid project manifest `{}` for browser package: {message}",
                path.display()
            )
        })?;
        if let Some(project_root) = path.parent() {
            validate_manual_tls_file_references(project_root, &manifest)?;
            compose_check::validate_project_compose(project_root)?;
        }
    }
    Ok(())
}

/// Validates manual TLS file references for a package-adjacent project.
///
/// Inputs:
/// - `project_root`: directory containing the adjacent `terlan.toml`.
/// - `manifest`: parsed project manifest.
///
/// Output:
/// - `Ok(())` when TLS is absent, non-manual, or all manual file references
///   point at existing files.
/// - Stable `error[serve_package]` diagnostic when a manual certificate,
///   private key, or custom CA path is missing or resolves outside the project.
///
/// Transformation:
/// - Keeps build-time TLS shape validation in the project manifest parser and
///   applies serve-time filesystem validation only for local manual TLS
///   references that the runtime will later need to open.
fn validate_manual_tls_file_references(
    project_root: &Path,
    manifest: &project_manifest::ProjectManifest,
) -> Result<(), String> {
    let Some(tls) = &manifest.server_tls else {
        return Ok(());
    };
    if tls.mode != ProjectServerTlsMode::Manual {
        return Ok(());
    }
    for (field, value) in [
        ("cert", tls.cert.as_deref()),
        ("key", tls.key.as_deref()),
        ("ca", tls.ca.as_deref()),
    ] {
        let Some(value) = value else {
            continue;
        };
        validate_manual_tls_file_reference(project_root, field, value)?;
    }
    Ok(())
}

/// Validates one manual TLS file reference.
///
/// Inputs:
/// - `project_root`: directory containing the adjacent `terlan.toml`.
/// - `field`: TLS field name, such as `cert`, `key`, or `ca`.
/// - `value`: project-relative file path from the manifest.
///
/// Output:
/// - `Ok(())` when the path is relative, stays inside the project, and exists
///   as a file.
/// - Stable `error[serve_package]` diagnostic otherwise.
///
/// Transformation:
/// - Resolves project-relative TLS paths without following runtime certificate
///   loading semantics, giving `terlc serve --check` a deterministic local
///   validation boundary before rustls socket serving is implemented.
fn validate_manual_tls_file_reference(
    project_root: &Path,
    field: &str,
    value: &str,
) -> Result<(), String> {
    let relative = Path::new(value);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "error[serve_package]: [server.tls] manual {field} path `{value}` must be project-relative and stay inside the project"
        ));
    }
    let full_path = project_root.join(relative);
    if !full_path.is_file() {
        return Err(format!(
            "error[serve_package]: [server.tls] manual {field} file `{}` does not exist",
            full_path.display()
        ));
    }
    Ok(())
}

/// Finds the adjacent project root for a packaged web directory.
///
/// Inputs:
/// - `web_root`: package root passed to `terlc serve`.
///
/// Output:
/// - Project directory containing the adjacent `terlan.toml`.
/// - `None` when the web package is standalone.
///
/// Transformation:
/// - Reuses the same deterministic manifest search path as package validation
///   so dependency startup cannot drift from `terlc serve --check`.
pub(super) fn adjacent_project_root(web_root: &Path) -> Option<std::path::PathBuf> {
    adjacent_project_manifest_path(web_root).and_then(|path| path.parent().map(Path::to_path_buf))
}

/// Loads adjacent project TLS metadata for a packaged web root.
///
/// Inputs:
/// - `web_root`: package root passed to `terlc serve`.
///
/// Output:
/// - `Ok(Some((project_root, tls)))` when a nearby `terlan.toml` contains
///   `[server.tls]`.
/// - `Ok(None)` when no adjacent manifest exists or TLS is absent.
/// - `Err(String)` when adjacent project metadata is invalid.
///
/// Transformation:
/// - Reuses the build manifest parser and returns the project root beside the
///   parsed TLS config so runtime certificate loading can resolve
///   project-relative manual paths without reimplementing manifest discovery.
pub(super) fn web_package_tls_config(
    web_root: &Path,
) -> Result<Option<(std::path::PathBuf, ProjectServerTls)>, String> {
    let Some(path) = adjacent_project_manifest_path(web_root) else {
        return Ok(None);
    };
    let manifest = project_manifest::read_project_manifest(&path).map_err(|message| {
        format!(
            "error[serve_package]: invalid project manifest `{}` for browser package: {message}",
            path.display()
        )
    })?;
    let Some(tls) = manifest.server_tls else {
        return Ok(None);
    };
    let project_root = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| Path::new(".").to_path_buf());
    Ok(Some((project_root, tls)))
}

/// Finds project metadata related to a packaged web root.
///
/// Inputs:
/// - `web_root`: package root passed to `terlc serve`.
///
/// Output:
/// - First existing `terlan.toml` path in supported package layouts.
/// - `None` when the web package is standalone.
///
/// Transformation:
/// - Checks deterministic candidate locations without requiring a project
///   layout:
///   - package-local `web_root/terlan.toml`
///   - sibling `web_root_parent/terlan.toml`
///   - project-root `project/terlan.toml` for `project/_build/web`.
fn adjacent_project_manifest_path(web_root: &Path) -> Option<std::path::PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(web_root.join("terlan.toml"));
    if let Some(parent) = web_root.parent() {
        candidates.push(parent.join("terlan.toml"));
        if web_root.file_name().is_some_and(|name| name == "web")
            && parent.file_name().is_some_and(|name| name == "_build")
        {
            if let Some(project_root) = parent.parent() {
                candidates.push(project_root.join("terlan.toml"));
            }
        }
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// Reads a browser package manifest.
///
/// Inputs:
/// - `web_root`: package root containing `manifest.json`.
///
/// Output:
/// - Deserialized browser package manifest or a user-facing read/parse error.
///
/// Transformation:
/// - Reads the manifest text and deserializes it into the current server
///   contract, including optional handler entries.
pub(super) fn read_web_manifest(web_root: &Path) -> Result<WebPackageManifest, String> {
    let manifest_path = web_root.join("manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|err| err.to_string())?;
    serde_json::from_str(&manifest_text).map_err(|err| format!("malformed manifest: {err}"))
}

/// Reads the browser package build id for local observability.
///
/// Inputs:
/// - `web_root`: package root containing `manifest.json`.
///
/// Output:
/// - Manifest `build_id` when it can be read.
/// - `"unknown"` when the manifest is unavailable or malformed.
///
/// Transformation:
/// - Keeps request logging best-effort so a manifest read failure does not hide
///   the primary request handling diagnostic.
pub(super) fn manifest_build_id(web_root: &Path) -> String {
    read_web_manifest(web_root)
        .map(|manifest| manifest.build_id)
        .unwrap_or_else(|_| default_build_id())
}

/// Finds a manifest-declared static file for one request path.
///
/// Inputs:
/// - `web_root`: package root containing `manifest.json`.
/// - `request_path`: URL path without query text.
///
/// Output:
/// - Safe package file path when the request targets the manifest index or one
///   declared asset.
/// - `None` when the manifest cannot be read, the path is not declared, or the
///   declared file is unsafe/missing.
///
/// Transformation:
/// - Converts `/` to the manifest index, compares other requests against
///   declared web-relative asset paths, and resolves the selected manifest path
///   through the same package-relative safety boundary used during validation.
pub(super) fn manifest_static_file_for_request(
    web_root: &Path,
    request_path: &str,
) -> Option<std::path::PathBuf> {
    let manifest = read_web_manifest(web_root).ok()?;
    let request_relative = request_path.trim_start_matches('/');
    let manifest_relative = if request_relative.is_empty() {
        Some(manifest.index)
    } else if request_relative == manifest.index {
        Some(manifest.index)
    } else {
        manifest
            .assets
            .into_iter()
            .find(|asset| request_relative == asset.web_relative_path)
            .map(|asset| asset.web_relative_path)
    }?;
    let path = package_relative_path(web_root, &manifest_relative)?;
    path.is_file().then_some(path)
}

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
fn validate_asset_kind(kind: &str) -> Result<(), String> {
    match kind {
        "javascript-module" | "asset-file" | "asset-css" | "asset-markdown" | "static-asset"
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
fn validate_static_response_routes(responses: &[WebPackageStaticResponse]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for response in responses {
        let key = (
            response.method.as_str(),
            crate::commands::web_route::route_ambiguity_key(&response.route)?,
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
fn validate_file_response_routes(responses: &[WebPackageFileResponse]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for response in responses {
        let key = (
            response.method.as_str(),
            crate::commands::web_route::route_ambiguity_key(&response.route)?,
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
fn validate_websocket_routes(websockets: &[WebPackageWebSocket]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for websocket in websockets {
        let key = crate::commands::web_route::route_ambiguity_key(&websocket.route)?;
        if !seen.insert(key) {
            return Err(format!(
                "error[serve_package]: duplicate or ambiguous websocket route `{}`",
                websocket.route
            ));
        }
    }
    Ok(())
}

/// Validates the combined dynamic/static route namespace.
///
/// Inputs:
/// - `handlers`: dynamic BEAM-backed handler routes.
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
fn validate_manifest_route_namespace(
    handlers: &[WebPackageHandler],
    websockets: &[WebPackageWebSocket],
    responses: &[WebPackageStaticResponse],
    file_responses: &[WebPackageFileResponse],
) -> Result<(), String> {
    let mut seen = std::collections::BTreeMap::new();
    for handler in handlers {
        let key = (
            handler.method.as_str(),
            crate::commands::web_route::route_ambiguity_key(&handler.route)?,
        );
        seen.insert(
            key,
            format!("handler route `{}` `{}`", handler.method, handler.route),
        );
    }
    for websocket in websockets {
        let key = (
            "GET",
            crate::commands::web_route::route_ambiguity_key(&websocket.route)?,
        );
        if let Some(existing) = seen.get(&key) {
            return Err(format!(
                "error[serve_package]: websocket route `GET` `{}` conflicts with {existing}",
                websocket.route
            ));
        }
        seen.insert(key, format!("websocket route `GET` `{}`", websocket.route));
    }
    for response in responses {
        let key = (
            response.method.as_str(),
            crate::commands::web_route::route_ambiguity_key(&response.route)?,
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
            crate::commands::web_route::route_ambiguity_key(&response.route)?,
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

#[cfg(test)]
#[path = "manifest_test.rs"]
mod manifest_test;
