use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use terlan_syntax::SyntaxImportKind;

use crate::commands::artifacts::SyntaxAssetImportInput;

use super::super::js::JsModuleArtifact;
use super::super::{fingerprint, path_to_manifest_string, write_build_file};
use super::manifest::WebAssetArtifact;
use super::BrowserStaticAssetConfig;

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
pub(super) fn copy_js_module_asset(
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
        web_relative_path: path_to_manifest_string(&web_relative_path),
        fingerprint: fingerprint(&bytes),
    });
    Ok(())
}

/// Copies non-code assets imported by one JavaScript module.
///
/// Inputs:
/// - `web_root`: root browser package directory.
/// - `module`: emitted JavaScript module with recorded asset imports.
/// - `assets`: manifest asset accumulator updated with copied imports.
/// - `incremental`: whether unchanged writes may be skipped.
///
/// Output:
/// - `Ok(())` after imported assets are copied and registered.
///
/// Transformation:
/// - Copies each source asset into the browser package next to the importing
///   module's output path and records its manifest fingerprint.
pub(super) fn copy_browser_imported_assets(
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
            web_relative_path: path_to_manifest_string(&web_relative_path),
            fingerprint: fingerprint(&import.bytes),
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
pub(super) fn copy_manifest_static_assets(
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
            source_relative_path: path_to_manifest_string(
                &PathBuf::from(&config.source_label).join(relative),
            ),
            web_relative_path: path_to_manifest_string(&web_relative_path),
            fingerprint: fingerprint(&bytes),
        });
    }
    let _ = config.inline_limit;
    Ok(())
}

/// Bundles manifest-declared browser assets through the internal Rsbuild path.
///
/// Inputs:
/// - `web_root`: root `_build/web` output directory.
/// - `config`: resolved static asset directory.
/// - `assets`: browser manifest asset list to extend with generated files.
/// - `incremental`: whether generated config writes may be skipped.
///
/// Output:
/// - `Ok(true)` when an `index.js` entrypoint was bundled.
/// - `Ok(false)` when the asset directory has no bundle entrypoint.
/// - `Err(String)` when Rsbuild is required but unavailable or fails.
///
/// Transformation:
/// - Generates a private Rsbuild config from Terlan's `[web.assets]` contract,
///   runs the project-local Rsbuild CLI, copies non-code static templates, and
///   records the resulting web-root files in the Terlan web manifest. The
///   project never sees or owns Rsbuild/Rspack configuration.
pub(super) fn bundle_manifest_static_assets_with_rsbuild(
    web_root: &Path,
    config: &BrowserStaticAssetConfig,
    assets: &mut Vec<WebAssetArtifact>,
    incremental: bool,
) -> Result<bool, String> {
    let entry = config.source_dir.join("index.js");
    if !entry.is_file() {
        return Ok(false);
    }
    let template = config.source_dir.join("index.html");
    if !template.is_file() {
        return Err(format!(
            "project manifest [web.assets] Rsbuild entrypoint requires {}",
            template.display()
        ));
    }
    let project_root = config.source_dir.parent().ok_or_else(|| {
        format!(
            "cannot determine project root for web asset directory {}",
            config.source_dir.display()
        )
    })?;
    let project_root = if project_root.as_os_str().is_empty() {
        Path::new(".")
    } else {
        project_root
    };
    let project_root = fs::canonicalize(project_root).map_err(|err| {
        format!(
            "cannot resolve project root {} for web asset bundling: {err}",
            project_root.display()
        )
    })?;
    let build_root = web_root.parent().ok_or_else(|| {
        format!(
            "cannot determine build root for browser package {}",
            web_root.display()
        )
    })?;
    fs::create_dir_all(build_root).map_err(|err| {
        format!(
            "cannot create browser build directory {}: {err}",
            build_root.display()
        )
    })?;
    let entry = fs::canonicalize(&entry).map_err(|err| {
        format!(
            "cannot resolve Rsbuild entrypoint {}: {err}",
            entry.display()
        )
    })?;
    let template = fs::canonicalize(&template).map_err(|err| {
        format!(
            "cannot resolve Rsbuild HTML template {}: {err}",
            template.display()
        )
    })?;
    let build_root = fs::canonicalize(build_root).map_err(|err| {
        format!(
            "cannot resolve browser build directory {}: {err}",
            build_root.display()
        )
    })?;
    let web_root = fs::canonicalize(web_root).map_err(|err| {
        format!(
            "cannot resolve browser package directory {}: {err}",
            web_root.display()
        )
    })?;

    let config_path = if let Some(config_path) = config.rsbuild_config.as_ref() {
        if !config_path.is_file() {
            return Err(format!(
                "error[web_rsbuild_config]: configured Rsbuild config does not exist: {}",
                config_path.display()
            ));
        }
        config_path.clone()
    } else {
        let config_path = build_root.join("rsbuild.terlan.generated.mjs");
        let config_text = render_rsbuild_config(&entry, &template, &web_root)?;
        write_build_file(&config_path, config_text.as_bytes(), incremental)?;
        config_path
    };

    let rsbuild = project_root.join("node_modules/.bin/rsbuild");
    if !rsbuild.is_file() {
        return Err(format!(
            "error[web_rsbuild_missing]: Terlan web asset bundling requires project-local @rsbuild/core; run npm install with @rsbuild/core in devDependencies for {}",
            project_root.display()
        ));
    }
    let rsbuild = fs::canonicalize(&rsbuild).map_err(|err| {
        format!(
            "error[web_rsbuild_missing]: cannot resolve Rsbuild binary {}: {err}",
            rsbuild.display()
        )
    })?;
    let config_path = fs::canonicalize(&config_path).map_err(|err| {
        format!(
            "error[web_rsbuild_config]: cannot resolve Rsbuild config {}: {err}",
            config_path.display()
        )
    })?;
    let output = Command::new(&rsbuild)
        .arg("build")
        .arg("--config")
        .arg(&config_path)
        .env("TERLAN_RSB_ENTRY", &entry)
        .env("TERLAN_RSB_TEMPLATE", &template)
        .env("TERLAN_RSB_WEB_ROOT", &web_root)
        .env("TERLAN_RSB_BUILD_ROOT", &build_root)
        .current_dir(project_root)
        .output()
        .map_err(|err| format!("error[web_rsbuild]: failed to start Rsbuild: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "error[web_rsbuild]: Rsbuild failed for {}:\n{}{}",
            config.source_dir.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    copy_manifest_passthrough_assets(&web_root, config, incremental)?;
    register_web_root_assets(&web_root, assets)
}

/// Renders the private Rsbuild config used by `terlc build --target js.browser`.
fn render_rsbuild_config(entry: &Path, template: &Path, web_root: &Path) -> Result<String, String> {
    let build_root = web_root.parent().ok_or_else(|| {
        format!(
            "cannot determine build root for browser package {}",
            web_root.display()
        )
    })?;
    let build_root = serde_json::to_string(build_root)
        .map_err(|err| format!("cannot serialize Rsbuild root path: {err}"))?;
    let entry = serde_json::to_string(&entry)
        .map_err(|err| format!("cannot serialize Rsbuild entry path: {err}"))?;
    let template = serde_json::to_string(&template)
        .map_err(|err| format!("cannot serialize Rsbuild template path: {err}"))?;
    let web_root = serde_json::to_string(&web_root)
        .map_err(|err| format!("cannot serialize Rsbuild output path: {err}"))?;
    Ok(format!(
        "export default {{\n  root: {build_root},\n  source: {{ entry: {{ index: {entry} }} }},\n  html: {{ template: {template} }},\n  output: {{\n    distPath: {{ root: {web_root} }},\n    cleanDistPath: false,\n    assetPrefix: '/',\n    filename: {{ js: 'static/[name].js', css: 'static/[name].css', media: 'static/[name][ext]', font: 'static/[name][ext]', image: 'static/[name][ext]' }}\n  }}\n}};\n"
    ))
}

/// Copies non-code files that Rsbuild should not bundle, such as Angular HTML templates.
fn copy_manifest_passthrough_assets(
    web_root: &Path,
    config: &BrowserStaticAssetConfig,
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
        if !should_copy_passthrough_asset(relative) {
            continue;
        }
        validate_safe_manifest_asset_path(relative, &source)?;
        let destination = web_root.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "cannot create manifest passthrough asset directory {}: {err}",
                    parent.display()
                )
            })?;
        }
        let bytes = fs::read(&source).map_err(|err| {
            format!(
                "cannot read manifest passthrough asset {}: {err}",
                source.display()
            )
        })?;
        write_build_file(&destination, &bytes, incremental)?;
    }
    Ok(())
}

/// Returns whether a source asset should be copied beside the Rsbuild output.
fn should_copy_passthrough_asset(relative: &Path) -> bool {
    if relative == Path::new("index.html") {
        return false;
    }
    !matches!(
        relative
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("js") | Some("css")
    )
}

/// Registers all generated browser package files as manifest assets.
fn register_web_root_assets(
    web_root: &Path,
    assets: &mut Vec<WebAssetArtifact>,
) -> Result<bool, String> {
    let mut registered_paths = assets
        .iter()
        .map(|asset| asset.web_relative_path.clone())
        .collect::<BTreeSet<_>>();
    let mut files = manifest_static_asset_files(web_root)?;
    files.sort();
    for source in files {
        let relative = source.strip_prefix(web_root).map_err(|err| {
            format!(
                "cannot relativize generated web asset {} against {}: {err}",
                source.display(),
                web_root.display()
            )
        })?;
        if relative == Path::new("manifest.json") {
            continue;
        }
        validate_safe_manifest_asset_path(relative, &source)?;
        let web_relative_path = path_to_manifest_string(relative);
        if !registered_paths.insert(web_relative_path.clone()) {
            continue;
        }
        let bytes = fs::read(&source).map_err(|err| {
            format!(
                "cannot read generated web asset {}: {err}",
                source.display()
            )
        })?;
        assets.push(WebAssetArtifact {
            module: String::new(),
            kind: generated_web_asset_kind(relative).to_string(),
            source_relative_path: path_to_manifest_string(relative),
            web_relative_path,
            fingerprint: fingerprint(&bytes),
        });
    }
    Ok(true)
}

/// Classifies a generated web-root file for the package manifest.
fn generated_web_asset_kind(relative: &Path) -> &'static str {
    match relative
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some("js") => "javascript-module",
        Some("css") => "css",
        _ => "static-asset",
    }
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
        fingerprint(&import.bytes),
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
