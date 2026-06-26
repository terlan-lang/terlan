use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::emit_js::target_contract::JsTargetContract;

use super::js::JsModuleArtifact;
use super::write_build_file;

mod assets;
mod manifest;
mod routes;

use assets::{
    bundle_manifest_static_assets_with_rsbuild, copy_browser_imported_assets, copy_js_module_asset,
    copy_manifest_static_assets,
};
use manifest::{write_browser_manifest, WebAssetArtifact};
use routes::{discover_web_error_handler_from_sources, discover_web_route_manifest_from_sources};

/// Manifest-declared static assets for a browser package.
///
/// Inputs:
/// - Produced from parsed `[web.assets]` project metadata.
///
/// Output:
/// - Browser packaging input used to copy static files into `_build/web`.
///
/// Transformation:
/// - Keeps Terlan's TOML asset contract separate from any hidden
///   Oxc/Rsbuild/Rspack translation layer.
#[derive(Debug, Clone)]
pub(super) struct BrowserStaticAssetConfig {
    pub(super) source_dir: PathBuf,
    pub(super) source_label: String,
    pub(super) web_path_prefix: PathBuf,
    pub(super) inline_limit: Option<u64>,
    pub(super) rsbuild_config: Option<PathBuf>,
}

/// Source module used only for web route-manifest discovery.
///
/// Inputs:
/// - Created from a Terlan source module that declares HTTP router functions.
///
/// Output:
/// - Minimal route-source metadata used by `_build/web/manifest.json`
///   extraction.
///
/// Transformation:
/// - Separates server-side route metadata from browser JavaScript artifacts so
///   HTTP handler modules do not need to pass through the JS backend.
pub(super) struct WebRouteSourceArtifact {
    pub(super) module: String,
    pub(super) source_path: String,
}

impl WebRouteSourceArtifact {
    /// Builds a route-source artifact from an emitted JS module artifact.
    ///
    /// Inputs:
    /// - `module`: emitted JavaScript module artifact that also owns route
    ///   source metadata.
    ///
    /// Output:
    /// - Route-source artifact with only the fields needed by route discovery.
    ///
    /// Transformation:
    /// - Drops browser asset fields so manifest routing stays independent from
    ///   the JavaScript package copy step.
    #[allow(dead_code)]
    fn from_js_module(module: &JsModuleArtifact) -> Self {
        Self {
            module: module.module.clone(),
            source_path: module.source_path.clone(),
        }
    }
}

/// Writes the deterministic browser package artifact for a JS browser build.
///
/// Inputs:
/// - `js_root`: root JS output directory containing emitted modules.
/// - `contract`: selected JS artifact contract.
/// - `modules`: emitted JS module artifacts from the build manifest.
/// - `static_assets`: optional manifest-declared static asset directory.
/// - `incremental`: whether unchanged writes may be skipped.
///
/// Output:
/// - `Ok(())` after `_build/web/index.html`, copied JS assets, and
///   `_build/web/manifest.json` exist.
/// - `Err(String)` for missing JS modules, serialization, or filesystem
///   failures.
///
/// Transformation:
/// - Copies Oxc-validated JS modules from `_build/js/modules/**` into
///   `_build/web/assets/js/modules/**`, emits a minimal module-script HTML
///   shell, and records the package in a browser manifest for later `terlc
///   serve` and release checks.
#[allow(dead_code)]
pub(super) fn write_browser_package(
    js_root: &Path,
    contract: JsTargetContract,
    modules: &[JsModuleArtifact],
    static_assets: Option<&BrowserStaticAssetConfig>,
    incremental: bool,
) -> Result<(), String> {
    let route_sources = modules
        .iter()
        .map(WebRouteSourceArtifact::from_js_module)
        .collect::<Vec<_>>();
    write_browser_package_with_route_sources(
        js_root,
        contract,
        modules,
        &route_sources,
        static_assets,
        incremental,
    )
}

/// Writes a browser package with explicit server route-source inputs.
///
/// Inputs:
/// - `js_root`: root JS output directory containing emitted browser modules.
/// - `contract`: selected JavaScript target contract.
/// - `modules`: emitted JS module artifacts copied into the browser package.
/// - `route_sources`: server or browser Terlan sources used for route metadata.
/// - `static_assets`: optional manifest-declared static asset directory.
/// - `incremental`: whether unchanged writes may be skipped.
///
/// Output:
/// - `Ok(())` after browser package files and manifest exist.
/// - `Err(String)` for route extraction, serialization, or filesystem failures.
///
/// Transformation:
/// - Copies only browser JS artifacts while extracting HTTP routes from the
///   separate route-source list, allowing BEAM-backed handlers to shape the web
///   manifest without being emitted as JavaScript.
pub(super) fn write_browser_package_with_route_sources(
    js_root: &Path,
    contract: JsTargetContract,
    modules: &[JsModuleArtifact],
    route_sources: &[WebRouteSourceArtifact],
    static_assets: Option<&BrowserStaticAssetConfig>,
    incremental: bool,
) -> Result<(), String> {
    let build_root = js_root.parent().ok_or_else(|| {
        format!(
            "cannot determine build root for JS output directory {}",
            js_root.display()
        )
    })?;
    let web_root = build_root.join("web");
    fs::create_dir_all(&web_root).map_err(|err| {
        format!(
            "cannot create browser package directory {}: {err}",
            web_root.display()
        )
    })?;

    let mut assets = Vec::new();
    for module in modules {
        copy_js_module_asset(js_root, &web_root, module, &mut assets, incremental)?;
        copy_browser_imported_assets(&web_root, module, &mut assets, incremental)?;
    }
    let mut has_static_asset_entrypoint = false;
    if let Some(static_assets) = static_assets {
        if bundle_manifest_static_assets_with_rsbuild(
            &web_root,
            static_assets,
            &mut assets,
            incremental,
        )? {
            has_static_asset_entrypoint = true;
        } else {
            copy_manifest_static_assets(&web_root, static_assets, &mut assets, incremental)?;
        }
    }

    if !has_static_asset_entrypoint {
        write_browser_index(&web_root, &assets, incremental)?;
    }
    let route_manifest = discover_web_route_manifest_from_sources(route_sources)?;
    let error_handler = discover_web_error_handler_from_sources(route_sources)?;
    write_browser_manifest(
        &web_root,
        contract,
        assets,
        route_manifest.handlers,
        route_manifest.websockets,
        route_manifest.static_responses,
        route_manifest.file_responses,
        error_handler,
        incremental,
    )
}

/// Writes the browser package HTML entrypoint.
///
/// Inputs:
/// - `web_root`: root browser package directory.
/// - `assets`: browser manifest assets used to generate module script tags.
/// - `incremental`: whether unchanged writes may be skipped.
///
/// Output:
/// - `Ok(())` after `index.html` exists.
///
/// Transformation:
/// - Emits a minimal deterministic HTML shell that loads every copied asset as
///   a module script. The local server owns live reload injection at serve time.
fn write_browser_index(
    web_root: &Path,
    assets: &[WebAssetArtifact],
    incremental: bool,
) -> Result<(), String> {
    let script_tags = assets
        .iter()
        .map(|asset| {
            format!(
                r#"    <script type="module" src="./{}"></script>"#,
                asset.web_relative_path
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let index_html = format!(
        "<!doctype html>\n<html>\n  <head>\n    <meta charset=\"utf-8\">\n    <title>Terlan</title>\n  </head>\n  <body>\n{}\n  </body>\n</html>\n",
        script_tags
    );
    write_build_file(
        &web_root.join("index.html"),
        index_html.as_bytes(),
        incremental,
    )
}

#[cfg(test)]
#[path = "js_browser_test.rs"]
mod js_browser_test;
