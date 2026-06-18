use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::handler::{validate_handler, WebPackageHandler};
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
    index: String,
    assets: Vec<WebPackageAsset>,
    #[serde(default)]
    pub(super) handlers: Vec<WebPackageHandler>,
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
///   files, and validates dynamic handler metadata.
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

    Ok(())
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
        "javascript-module" | "asset-file" | "asset-css" | "asset-markdown" | "static-asset" => {
            Ok(())
        }
        other => Err(format!(
            "error[serve_package]: unsupported browser package asset kind `{other}`"
        )),
    }
}

#[cfg(test)]
#[path = "manifest_test.rs"]
mod manifest_test;
