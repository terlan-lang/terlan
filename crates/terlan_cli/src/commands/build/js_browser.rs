use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use terlan_syntax::SyntaxImportKind;

use crate::commands::artifacts::SyntaxAssetImportInput;
use crate::commands::emit_js::target_contract::JsTargetContract;

use super::js::JsModuleArtifact;
use super::write_build_file;

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
pub(super) fn write_browser_package(
    js_root: &Path,
    contract: JsTargetContract,
    modules: &[JsModuleArtifact],
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
    if let Some(static_assets) = static_assets {
        copy_manifest_static_assets(&web_root, static_assets, &mut assets, incremental)?;
    }

    write_browser_index(&web_root, &assets, incremental)?;
    write_browser_manifest(&web_root, contract, assets, incremental)
}

/// Copies one emitted JavaScript module into the browser package.
///
/// Inputs:
/// - `js_root`: root JS output directory.
/// - `web_root`: root browser package directory.
/// - `module`: emitted module metadata.
/// - `assets`: manifest asset list to extend.
/// - `incremental`: whether unchanged writes may be skipped.
///
/// Output:
/// - `Ok(())` after the copied JS module exists and has a manifest row.
///
/// Transformation:
/// - Reads the emitted JS artifact, writes it under `_build/web/assets/js`,
///   and records a deterministic fingerprint for release validation.
fn copy_js_module_asset(
    js_root: &Path,
    web_root: &Path,
    module: &JsModuleArtifact,
    assets: &mut Vec<WebAssetArtifact>,
    incremental: bool,
) -> Result<(), String> {
    let source = js_root.join(&module.relative_path);
    let bytes = fs::read(&source).map_err(|err| {
        format!(
            "cannot read JS module {} for browser package: {err}",
            source.display()
        )
    })?;
    let web_relative_path = PathBuf::from("assets")
        .join("js")
        .join(&module.relative_path);
    let destination = web_root.join(&web_relative_path);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "cannot create browser asset directory {}: {err}",
                parent.display()
            )
        })?;
    }
    write_build_file(&destination, &bytes, incremental)?;
    assets.push(WebAssetArtifact {
        module: module.module.clone(),
        kind: "javascript-module".to_string(),
        source_relative_path: module.relative_path.clone(),
        web_relative_path: super::path_to_manifest_string(&web_relative_path),
        fingerprint: super::fingerprint(&bytes),
    });
    Ok(())
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

/// Writes the browser package manifest.
///
/// Inputs:
/// - `web_root`: root browser package directory.
/// - `contract`: selected JS artifact contract.
/// - `assets`: copied asset metadata.
/// - `incremental`: whether unchanged writes may be skipped.
///
/// Output:
/// - `Ok(())` after `_build/web/manifest.json` exists.
///
/// Transformation:
/// - Serializes the web package manifest consumed by `terlc serve`, including
///   an empty handler list until handler discovery is implemented.
fn write_browser_manifest(
    web_root: &Path,
    contract: JsTargetContract,
    assets: Vec<WebAssetArtifact>,
    incremental: bool,
) -> Result<(), String> {
    let manifest = WebBuildManifest {
        schema: "terlan-web-build-v1",
        target_profile: contract.profile_name,
        source_js_manifest: "../js/manifest.json",
        index: "index.html",
        assets,
        handlers: Vec::new(),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("cannot serialize browser package manifest: {err}"))?;
    write_build_file(
        &web_root.join("manifest.json"),
        manifest_json.as_bytes(),
        incremental,
    )
}

/// Copies source-imported assets into the browser package.
///
/// Inputs:
/// - `web_root`: root `_build/web` output directory.
/// - `module`: JS module artifact carrying syntax asset imports.
/// - `assets`: mutable browser manifest asset list to extend.
/// - `incremental`: whether unchanged writes may be skipped.
///
/// Output:
/// - `Ok(())` after every source-imported asset is copied.
/// - `Err(String)` when an asset directory or file cannot be written.
///
/// Transformation:
/// - Converts validated `import file/css/markdown` syntax metadata into
///   deterministic `_build/web/assets/imports/**` files and records each copy
///   in the browser manifest.
fn copy_browser_imported_assets(
    web_root: &Path,
    module: &JsModuleArtifact,
    assets: &mut Vec<WebAssetArtifact>,
    incremental: bool,
) -> Result<(), String> {
    for import in &module.asset_imports {
        let web_relative_path = browser_import_asset_relative_path(module, import);
        let destination = web_root.join(&web_relative_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "cannot create browser imported asset directory {}: {err}",
                    parent.display()
                )
            })?;
        }
        write_build_file(&destination, &import.bytes, incremental)?;
        assets.push(WebAssetArtifact {
            module: module.module.clone(),
            kind: browser_import_asset_kind(import.kind).to_string(),
            source_relative_path: import.source_path.clone(),
            web_relative_path: super::path_to_manifest_string(&web_relative_path),
            fingerprint: super::fingerprint(&import.bytes),
        });
    }

    Ok(())
}

/// Copies manifest-declared static assets into the browser package.
///
/// Inputs:
/// - `web_root`: root `_build/web` output directory.
/// - `config`: resolved static asset directory and web path prefix.
/// - `assets`: mutable browser manifest asset list to extend.
/// - `incremental`: whether unchanged writes may be skipped.
///
/// Output:
/// - `Ok(())` after every file under the configured asset directory is copied.
/// - `Err(String)` when an asset path is unsafe or cannot be read/written.
///
/// Transformation:
/// - Recursively walks the manifest-declared directory, copies files to the
///   configured browser path prefix, and records `static-asset` manifest rows.
fn copy_manifest_static_assets(
    web_root: &Path,
    config: &BrowserStaticAssetConfig,
    assets: &mut Vec<WebAssetArtifact>,
    incremental: bool,
) -> Result<(), String> {
    let mut files = manifest_static_asset_files(&config.source_dir)?;
    files.sort();
    for source in files {
        let relative = source.strip_prefix(&config.source_dir).map_err(|err| {
            format!(
                "cannot relativize static asset {} against {}: {err}",
                source.display(),
                config.source_dir.display()
            )
        })?;
        validate_safe_manifest_asset_path(relative, &source)?;
        let web_relative_path = config.web_path_prefix.join(relative);
        let destination = web_root.join(&web_relative_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "cannot create manifest static asset directory {}: {err}",
                    parent.display()
                )
            })?;
        }
        let bytes = fs::read(&source).map_err(|err| {
            format!(
                "cannot read manifest static asset {}: {err}",
                source.display()
            )
        })?;
        write_build_file(&destination, &bytes, incremental)?;
        assets.push(WebAssetArtifact {
            module: String::new(),
            kind: "static-asset".to_string(),
            source_relative_path: super::path_to_manifest_string(
                &PathBuf::from(&config.source_label).join(relative),
            ),
            web_relative_path: super::path_to_manifest_string(&web_relative_path),
            fingerprint: super::fingerprint(&bytes),
        });
    }
    let _ = config.inline_limit;
    Ok(())
}

/// Lists regular files under a static asset directory.
///
/// Inputs:
/// - `dir`: manifest-declared asset directory.
///
/// Output:
/// - Regular file paths discovered recursively.
///
/// Transformation:
/// - Performs a deterministic filesystem traversal without following
///   unsupported non-directory, non-file entries.
fn manifest_static_asset_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let mut entries = fs::read_dir(&current)
            .map_err(|err| {
                format!(
                    "cannot read static asset directory {}: {err}",
                    current.display()
                )
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| {
                format!(
                    "cannot read static asset directory {}: {err}",
                    current.display()
                )
            })?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries.into_iter().rev() {
            let path = entry.path();
            let file_type = entry.file_type().map_err(|err| {
                format!("cannot inspect static asset path {}: {err}", path.display())
            })?;
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                files.push(path);
            }
        }
    }
    Ok(files)
}

/// Validates one manifest static asset relative path.
///
/// Inputs:
/// - `relative`: path under the configured asset directory.
/// - `source`: full source path used in diagnostics.
///
/// Output:
/// - `Ok(())` when the path can be copied under `_build/web`.
/// - `Err(String)` when the path contains parent, root, or prefix components.
///
/// Transformation:
/// - Rejects unsafe path components before writing the browser package copy.
fn validate_safe_manifest_asset_path(relative: &Path, source: &Path) -> Result<(), String> {
    if relative.components().any(|component| {
        !matches!(
            component,
            std::path::Component::Normal(_) | std::path::Component::CurDir
        )
    }) {
        return Err(format!(
            "manifest static asset path is not safe for browser packaging: {}",
            source.display()
        ));
    }
    Ok(())
}

/// Builds the browser package path for one source-imported asset.
///
/// Inputs:
/// - `module`: emitted JS module that declared the asset import.
/// - `import`: validated asset import metadata.
///
/// Output:
/// - Relative path under `_build/web`.
///
/// Transformation:
/// - Namespaces assets by Terlan module, uses the import alias as the readable
///   file stem, preserves the source file extension when present, and appends a
///   content fingerprint to avoid collisions.
fn browser_import_asset_relative_path(
    module: &JsModuleArtifact,
    import: &SyntaxAssetImportInput,
) -> PathBuf {
    let module_dir = module
        .module
        .split('.')
        .map(sanitize_browser_asset_segment)
        .fold(PathBuf::from("assets").join("imports"), |path, segment| {
            path.join(segment)
        });
    let extension = import
        .resolved_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{}", sanitize_browser_asset_segment(extension)))
        .unwrap_or_default();
    let stem = sanitize_browser_asset_segment(&import.alias);
    module_dir.join(format!(
        "{}-{:016x}{}",
        stem,
        super::fingerprint(&import.bytes),
        extension
    ))
}

/// Converts an imported asset kind into browser manifest text.
///
/// Inputs:
/// - `kind`: syntax import kind from the formal parser output.
///
/// Output:
/// - Stable browser manifest kind string.
///
/// Transformation:
/// - Maps source grammar import categories onto release-facing package asset
///   categories without exposing parser enum names.
fn browser_import_asset_kind(kind: SyntaxImportKind) -> &'static str {
    match kind {
        SyntaxImportKind::File => "asset-file",
        SyntaxImportKind::Css => "asset-css",
        SyntaxImportKind::Markdown => "asset-markdown",
        SyntaxImportKind::Module => "asset-module",
    }
}

/// Sanitizes one browser asset path segment.
///
/// Inputs:
/// - `segment`: module name, alias, or extension text.
///
/// Output:
/// - Non-empty ASCII path segment safe for generated artifacts.
///
/// Transformation:
/// - Keeps alphanumeric characters, `_`, and `-`, converts all other
///   characters to `_`, and uses `asset` if sanitization removes everything.
fn sanitize_browser_asset_segment(segment: &str) -> String {
    let sanitized = segment
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "asset".to_string()
    } else {
        sanitized
    }
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
/// - Records the source JS manifest, HTML entrypoint, target profile, and
///   copied asset list without embedding JavaScript source text.
#[derive(Debug, Serialize)]
struct WebBuildManifest {
    schema: &'static str,
    target_profile: &'static str,
    source_js_manifest: &'static str,
    index: &'static str,
    assets: Vec<WebAssetArtifact>,
    handlers: Vec<WebHandlerArtifact>,
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
struct WebAssetArtifact {
    module: String,
    kind: String,
    source_relative_path: String,
    web_relative_path: String,
    fingerprint: u64,
}

/// Browser dynamic handler manifest entry.
///
/// Inputs:
/// - Future route metadata generated from Terlan HTTP handler declarations.
///
/// Output:
/// - Serializable handler entry inside the browser package manifest.
///
/// Transformation:
/// - Reserves the manifest field in 0.0.4 while current JS browser builds emit
///   an empty handler list until BEAM handler discovery is implemented.
#[derive(Debug, Serialize)]
struct WebHandlerArtifact {
    method: String,
    route: String,
    module: String,
    function: String,
    arity: usize,
}
