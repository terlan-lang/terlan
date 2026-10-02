use std::path::Path;

use super::model::{
    ProjectServerProfile, ProjectServerTls, ProjectServerTlsMode, ProjectServerTlsProvider,
    ProjectWebAssets,
};
use super::strings::parse_string;

/// Incremental parser state for optional `[web.assets]`.
///
/// Inputs:
/// - Filled while scanning manifest key/value assignments.
///
/// Output:
/// - Optional `ProjectWebAssets` after validation.
///
/// Transformation:
/// - Distinguishes an absent section from a present but incomplete section so
///   users get a precise diagnostic when they start configuring web assets.
#[derive(Debug, Default)]
pub(super) struct ProjectWebAssetsBuilder {
    pub(super) directory: Option<String>,
    pub(super) public_path: Option<String>,
    pub(super) inline_limit: Option<u64>,
    pub(super) rsbuild_config: Option<String>,
}

impl ProjectWebAssetsBuilder {
    /// Finalizes parsed web asset configuration.
    ///
    /// Inputs:
    /// - `self`: accumulated optional section values.
    /// - `path`: manifest path used in diagnostics.
    ///
    /// Output:
    /// - `Ok(None)` when `[web.assets]` was absent.
    /// - `Ok(Some(ProjectWebAssets))` when the section is complete.
    /// - `Err(String)` when the section is incomplete or invalid.
    ///
    /// Transformation:
    /// - Requires `directory` when any web asset key is present and rejects
    ///   empty path-like values before browser packaging consumes them.
    pub(super) fn finish(self, path: &Path) -> Result<Option<ProjectWebAssets>, String> {
        let has_any_key = self.directory.is_some()
            || self.public_path.is_some()
            || self.inline_limit.is_some()
            || self.rsbuild_config.is_some();
        if !has_any_key {
            return Ok(None);
        }
        let directory = self.directory.ok_or_else(|| {
            format!(
                "{}: project manifest [web.assets] requires directory",
                path.display()
            )
        })?;
        if directory.trim().is_empty() {
            return Err(format!(
                "{}: project manifest [web.assets] directory cannot be empty",
                path.display()
            ));
        }
        if let Some(public_path) = self.public_path.as_deref() {
            if public_path.trim().is_empty() {
                return Err(format!(
                    "{}: project manifest [web.assets] public_path cannot be empty",
                    path.display()
                ));
            }
        }
        if let Some(rsbuild_config) = self.rsbuild_config.as_deref() {
            if rsbuild_config.trim().is_empty() {
                return Err(format!(
                    "{}: project manifest [web.assets] rsbuild_config cannot be empty",
                    path.display()
                ));
            }
        }
        Ok(Some(ProjectWebAssets {
            directory,
            public_path: self.public_path,
            inline_limit: self.inline_limit,
            rsbuild_config: self.rsbuild_config,
        }))
    }
}

pub(super) use terlan_http_native::tls_config::Settings as ProjectServerTlsBuilder;

/// Rejects development-only TLS configuration for a production server profile.
pub(super) fn validate_server_profile_defaults(
    path: &Path,
    server_profile: Option<ProjectServerProfile>,
    server_tls: Option<&ProjectServerTls>,
) -> Result<(), String> {
    if matches!(server_profile, Some(ProjectServerProfile::Production))
        && matches!(
            server_tls.map(|tls| tls.mode),
            Some(ProjectServerTlsMode::Internal)
        )
    {
        return Err(format!(
            "{}: project manifest [server] profile production cannot use [server.tls] mode internal",
            path.display()
        ));
    }
    Ok(())
}

/// Parses a supported server TLS mode.
///
/// Inputs:
/// - `value`: trimmed manifest value text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Supported TLS mode.
///
/// Transformation:
/// - Parses a manifest string and admits only the current public TLS modes.
pub(super) fn parse_server_tls_mode(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectServerTlsMode, String> {
    let parsed = parse_string(value, path, line_no)?;
    parsed
        .parse()
        .map_err(|error| format!("{}:{line_no}: {error}", path.display()))
}

/// Parses a supported server deployment profile.
///
/// Inputs:
/// - `value`: trimmed manifest value text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Supported server profile.
///
/// Transformation:
/// - Parses a manifest string and admits only explicit lifecycle profiles so
///   production safety checks are typed before runtime configuration exists.
pub(super) fn parse_server_profile(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectServerProfile, String> {
    let parsed = parse_string(value, path, line_no)?;
    match parsed.as_str() {
        "development" => Ok(ProjectServerProfile::Development),
        "test" => Ok(ProjectServerProfile::Test),
        "staging" => Ok(ProjectServerProfile::Staging),
        "production" => Ok(ProjectServerProfile::Production),
        other => Err(format!(
            "{}:{}: unsupported [server] profile `{}`; supported profiles: development, test, staging, production",
            path.display(),
            line_no,
            other
        )),
    }
}

/// Parses a supported server TLS ACME provider.
///
/// Inputs:
/// - `value`: trimmed manifest value text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Supported TLS provider.
///
/// Transformation:
/// - Parses a manifest string and admits only provider names documented for
///   0.0.5 automatic TLS configuration.
pub(super) fn parse_server_tls_provider(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectServerTlsProvider, String> {
    let parsed = parse_string(value, path, line_no)?;
    parsed
        .parse()
        .map_err(|error| format!("{}:{line_no}: {error}", path.display()))
}

/// Parses a non-negative unsigned integer manifest value.
///
/// Inputs:
/// - `value`: trimmed manifest value text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Parsed `u64` value.
///
/// Transformation:
/// - Accepts plain ASCII decimal digits only so user-authored TOML config stays
///   predictable and does not inherit target-tool numeric syntax variants.
pub(super) fn parse_non_negative_u64(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<u64, String> {
    if value.is_empty() || !value.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(format!(
            "{}:{}: project manifest value must be a non-negative integer",
            path.display(),
            line_no
        ));
    }
    value.parse::<u64>().map_err(|err| {
        format!(
            "{}:{}: project manifest integer value is out of range: {err}",
            path.display(),
            line_no
        )
    })
}

/// Parses a boolean manifest value.
///
/// Inputs:
/// - `value`: trimmed manifest value text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Parsed boolean value for typed manifest configuration.
///
/// Transformation:
/// - Accepts only lowercase TOML-style `true` and `false` so boolean fields
///   remain predictable for hand-authored project configuration.
pub(super) fn parse_bool(value: &str, path: &Path, line_no: usize) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!(
            "{}:{}: project manifest value must be true or false",
            path.display(),
            line_no
        )),
    }
}
