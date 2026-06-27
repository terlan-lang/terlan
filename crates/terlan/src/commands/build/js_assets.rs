use std::path::{Path, PathBuf};

use super::js_browser::BrowserStaticAssetConfig;
use super::project_manifest::ProjectWebAssets;

/// Converts parsed manifest web asset metadata into browser package input.
///
/// Inputs:
/// - `project_dir`: directory containing `terlan.toml`.
/// - `assets`: parsed `[web.assets]` metadata.
///
/// Output:
/// - `Ok(BrowserStaticAssetConfig)` for a valid existing asset directory.
/// - `Err(String)` when the configured source directory is missing or the
///   public path cannot become a safe web-relative path.
///
/// Transformation:
/// - Resolves the user-facing manifest directory against the project root and
///   normalizes the public path into the browser package path prefix.
pub(super) fn browser_static_assets_from_manifest(
    project_dir: &Path,
    assets: &ProjectWebAssets,
) -> Result<BrowserStaticAssetConfig, String> {
    let source_dir = project_dir.join(&assets.directory);
    if !source_dir.is_dir() {
        return Err(format!(
            "project manifest [web.assets] directory does not exist: {}",
            source_dir.display()
        ));
    }
    let web_path_prefix = web_asset_public_path_prefix(assets.public_path.as_deref())?;
    Ok(BrowserStaticAssetConfig {
        source_dir,
        source_label: assets.directory.clone(),
        web_path_prefix,
        inline_limit: assets.inline_limit,
        rsbuild_config: assets
            .rsbuild_config
            .as_ref()
            .map(|config| project_dir.join(config)),
    })
}

/// Normalizes a public asset path into a package-relative output path.
///
/// Inputs:
/// - `public_path`: optional manifest `public_path` value.
///
/// Output:
/// - Safe relative path under `_build/web`.
/// - `Err(String)` when the value is empty after trimming or contains unsafe
///   path components.
///
/// Transformation:
/// - Defaults to `assets`, strips a leading slash for URL-style paths, and
///   rejects absolute, parent, or empty output paths.
fn web_asset_public_path_prefix(public_path: Option<&str>) -> Result<PathBuf, String> {
    let raw = public_path.unwrap_or("assets").trim();
    let trimmed = raw.trim_start_matches('/');
    if trimmed.is_empty() {
        return Err("project manifest [web.assets] public_path cannot resolve to web root".into());
    }
    let path = PathBuf::from(trimmed);
    if path.components().any(|component| {
        !matches!(
            component,
            std::path::Component::Normal(_) | std::path::Component::CurDir
        )
    }) {
        return Err(format!(
            "project manifest [web.assets] public_path `{raw}` must be a safe relative web path"
        ));
    }
    Ok(path)
}
