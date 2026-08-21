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
    pub(super) angular_ts: bool,
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
    pub(super) manifest_path: Option<String>,
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
    #[cfg(test)]
    fn from_js_module(module: &JsModuleArtifact) -> Self {
        Self {
            module: module.module.clone(),
            source_path: module.source_path.clone(),
            manifest_path: None,
        }
    }
}

/// Writes a self-contained route package for a native Terlan VM service.
///
/// The package retains the root Terlan sources because the compiler-free serve
/// runtime uses their checksums to bind persisted AOT handler generations to
/// the exact source that produced them. No JavaScript artifact is emitted.
pub(super) fn write_vm_service_package(
    project_dir: &Path,
    build_root: &Path,
    source_roots: &[String],
    route_sources: &[WebRouteSourceArtifact],
    incremental: bool,
) -> Result<PathBuf, String> {
    let web_root = build_root.join("web");
    let staging_root = build_root.join("web.staging");
    remove_generated_web_root(&staging_root)?;
    fs::create_dir_all(&staging_root).map_err(|error| {
        format!(
            "cannot create VM service package directory {}: {error}",
            staging_root.display()
        )
    })?;
    copy_regular_file(
        &project_dir.join(super::TERLAN_PROJECT_MANIFEST_FILE),
        &staging_root.join(super::TERLAN_PROJECT_MANIFEST_FILE),
    )?;
    for source_root in source_roots {
        copy_source_tree(
            &project_dir.join(source_root),
            &staging_root.join(source_root),
        )?;
    }
    write_build_file(
        &staging_root.join("index.html"),
        b"<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>Terlan service</title></head><body><main>Terlan service</main></body></html>\n",
        incremental,
    )?;
    let routes = discover_web_route_manifest_from_sources(route_sources)?;
    let error_handler = discover_web_error_handler_from_sources(route_sources)?;
    manifest::write_vm_service_manifest(&staging_root, routes, error_handler, incremental)?;
    crate::commands::serve::prewarm_dynamic_handler_sources(&staging_root)?;
    remove_transient_vm_service_build_state(&staging_root)?;
    remove_generated_web_root(&web_root)?;
    fs::rename(&staging_root, &web_root).map_err(|error| {
        format!(
            "cannot publish VM service package {}: {error}",
            web_root.display()
        )
    })?;
    Ok(web_root)
}

fn remove_transient_vm_service_build_state(web_root: &Path) -> Result<(), String> {
    let terlan_root = web_root.join(".terlan");
    for transient in ["serve-build", "serve-compiler"] {
        remove_generated_web_root(&terlan_root.join(transient))?;
    }
    let serve_aot = terlan_root.join("serve-aot");
    for transient in ["units", "vm"] {
        remove_generated_web_root(&serve_aot.join(transient))?;
    }
    let native_aot = serve_aot.join("native-aot");
    if native_aot.is_dir() {
        prune_native_aot_directory(&native_aot)?;
    }
    Ok(())
}

fn prune_native_aot_directory(directory: &Path) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| {
            format!(
                "cannot inspect serve AOT cache {}: {error}",
                directory.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            format!(
                "cannot inspect serve AOT cache {}: {error}",
                directory.display()
            )
        })?;
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!("cannot inspect serve AOT entry {}: {error}", path.display())
        })?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "serve AOT cache cannot contain symlink {}",
                path.display()
            ));
        }
        if metadata.is_dir() {
            prune_native_aot_directory(&path)?;
            if fs::read_dir(&path)
                .map_err(|error| {
                    format!("cannot inspect serve AOT entry {}: {error}", path.display())
                })?
                .next()
                .is_none()
            {
                fs::remove_dir(&path).map_err(|error| {
                    format!(
                        "cannot remove serve AOT directory {}: {error}",
                        path.display()
                    )
                })?;
            }
        } else if metadata.is_file()
            && path.extension().and_then(|extension| extension.to_str()) != Some("tvm")
        {
            fs::remove_file(&path).map_err(|error| {
                format!(
                    "cannot remove serve AOT build file {}: {error}",
                    path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn remove_generated_web_root(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "refusing to replace generated web package symlink {}",
            path.display()
        )),
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)
            .map_err(|error| format!("cannot remove {}: {error}", path.display())),
        Ok(_) => fs::remove_file(path)
            .map_err(|error| format!("cannot remove {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect {}: {error}", path.display())),
    }
}

fn copy_source_tree(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("cannot inspect source tree {}: {error}", source.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "VM service source tree cannot contain symlink {}",
            source.display()
        ));
    }
    if metadata.is_file() {
        return copy_regular_file(source, destination);
    }
    if !metadata.is_dir() {
        return Err(format!(
            "VM service source tree contains unsupported entry {}",
            source.display()
        ));
    }
    fs::create_dir_all(destination).map_err(|error| {
        format!(
            "cannot create VM service source directory {}: {error}",
            destination.display()
        )
    })?;
    let mut entries = fs::read_dir(source)
        .map_err(|error| format!("cannot inspect source tree {}: {error}", source.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot inspect source tree {}: {error}", source.display()))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        copy_source_tree(&entry.path(), &destination.join(entry.file_name()))?;
    }
    Ok(())
}

fn copy_regular_file(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("cannot inspect source file {}: {error}", source.display()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(format!(
            "VM service package requires a regular source file: {}",
            source.display()
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "cannot create VM service source directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::copy(source, destination).map_err(|error| {
        format!(
            "cannot copy VM service source {} to {}: {error}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
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
#[cfg(test)]
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
///   separate route-source list, allowing VM handler routes to shape the web
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
        route_manifest,
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
        .filter(|asset| asset.kind == "javascript-module")
        .map(|asset| {
            format!(
                r#"    <script type="module" src="./{}" integrity="{}"></script>"#,
                asset.web_relative_path, asset.integrity
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
#[cfg(test)]
mod js_browser_test;
