use std::path::Path;

use super::model::{
    ProjectServerTls, ProjectServerTlsMode, ProjectServerTlsProvider, ProjectWebAssets,
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

/// Incremental parser state for optional `[server.tls]`.
///
/// Inputs:
/// - Filled while scanning manifest key/value assignments.
///
/// Output:
/// - Optional `ProjectServerTls` after validation.
///
/// Transformation:
/// - Distinguishes an absent TLS section from a present but incomplete section
///   and applies mode-specific required-field checks.
#[derive(Debug, Default)]
pub(super) struct ProjectServerTlsBuilder {
    pub(super) mode: Option<ProjectServerTlsMode>,
    pub(super) domains: Option<Vec<String>>,
    pub(super) email: Option<String>,
    pub(super) primary_provider: Option<ProjectServerTlsProvider>,
    pub(super) fallback_provider: Option<ProjectServerTlsProvider>,
    pub(super) cert: Option<String>,
    pub(super) key: Option<String>,
    pub(super) passphrase_env: Option<String>,
    pub(super) ca: Option<String>,
    pub(super) server_name: Option<String>,
    pub(super) trust_local: Option<bool>,
}

impl ProjectServerTlsBuilder {
    /// Finalizes parsed server TLS configuration.
    ///
    /// Inputs:
    /// - `self`: accumulated optional section values.
    /// - `path`: manifest path used in diagnostics.
    ///
    /// Output:
    /// - `Ok(None)` when `[server.tls]` was absent.
    /// - `Ok(Some(ProjectServerTls))` when the section is complete.
    /// - `Err(String)` when the section is incomplete or invalid for its mode.
    ///
    /// Transformation:
    /// - Requires `mode` once any TLS key appears, rejects empty scalar/list
    ///   values, and validates only the current declarative contract without
    ///   performing certificate or network work.
    pub(super) fn finish(self, path: &Path) -> Result<Option<ProjectServerTls>, String> {
        if !self.has_any_key() {
            return Ok(None);
        }
        self.validate_non_empty_values(path)?;
        let mode = self.mode.ok_or_else(|| {
            format!(
                "{}: project manifest [server.tls] requires mode",
                path.display()
            )
        })?;
        match mode {
            ProjectServerTlsMode::Auto => self.validate_auto_mode(path)?,
            ProjectServerTlsMode::Manual => self.validate_manual_mode(path)?,
            ProjectServerTlsMode::Internal => self.validate_internal_mode(path)?,
        }
        Ok(Some(ProjectServerTls {
            mode,
            domains: self.domains.unwrap_or_default(),
            email: self.email,
            primary_provider: self.primary_provider,
            fallback_provider: self.fallback_provider,
            cert: self.cert,
            key: self.key,
            passphrase_env: self.passphrase_env,
            ca: self.ca,
            server_name: self.server_name,
            trust_local: self.trust_local,
        }))
    }

    /// Returns whether the TLS section appeared with at least one key.
    ///
    /// Inputs:
    /// - `self`: accumulated TLS builder.
    ///
    /// Output:
    /// - `true` when any TLS field has been set.
    ///
    /// Transformation:
    /// - Collapses optional fields into one section-presence check so absent
    ///   config stays distinct from incomplete config.
    fn has_any_key(&self) -> bool {
        self.mode.is_some()
            || self.domains.is_some()
            || self.email.is_some()
            || self.primary_provider.is_some()
            || self.fallback_provider.is_some()
            || self.cert.is_some()
            || self.key.is_some()
            || self.passphrase_env.is_some()
            || self.ca.is_some()
            || self.server_name.is_some()
            || self.trust_local.is_some()
    }

    /// Validates non-empty TLS scalar and list values.
    ///
    /// Inputs:
    /// - `self`: accumulated TLS builder.
    /// - `path`: manifest path used in diagnostics.
    ///
    /// Output:
    /// - `Ok(())` when all present string-like fields are non-empty.
    /// - `Err(String)` when a field cannot be meaningful runtime metadata.
    ///
    /// Transformation:
    /// - Rejects blank values before mode-specific validation so later runtime
    ///   code can assume parsed paths, domains, and names are intentional.
    fn validate_non_empty_values(&self, path: &Path) -> Result<(), String> {
        for (field, value) in [
            ("email", self.email.as_deref()),
            ("cert", self.cert.as_deref()),
            ("key", self.key.as_deref()),
            ("passphrase_env", self.passphrase_env.as_deref()),
            ("ca", self.ca.as_deref()),
            ("server_name", self.server_name.as_deref()),
        ] {
            if matches!(value, Some(value) if value.trim().is_empty()) {
                return Err(format!(
                    "{}: project manifest [server.tls] {} cannot be empty",
                    path.display(),
                    field
                ));
            }
        }
        if let Some(domains) = self.domains.as_deref() {
            if domains.iter().any(|domain| domain.trim().is_empty()) {
                return Err(format!(
                    "{}: project manifest [server.tls] domains cannot contain empty entries",
                    path.display()
                ));
            }
        }
        Ok(())
    }

    /// Validates automatic ACME TLS mode.
    ///
    /// Inputs:
    /// - `self`: accumulated TLS builder.
    /// - `path`: manifest path used in diagnostics.
    ///
    /// Output:
    /// - `Ok(())` when auto mode has domains and no manual/internal-only
    ///   fields.
    ///
    /// Transformation:
    /// - Enforces the declarative ACME mode boundary while leaving provider
    ///   selection optional for the future runtime defaulting layer.
    fn validate_auto_mode(&self, path: &Path) -> Result<(), String> {
        if self.domains.as_ref().map_or(true, Vec::is_empty) {
            return Err(format!(
                "{}: project manifest [server.tls] mode auto requires domains",
                path.display()
            ));
        }
        if self.cert.is_some()
            || self.key.is_some()
            || self.passphrase_env.is_some()
            || self.ca.is_some()
            || self.server_name.is_some()
            || self.trust_local.is_some()
        {
            return Err(format!(
                "{}: project manifest [server.tls] mode auto cannot set manual or internal TLS fields",
                path.display()
            ));
        }
        Ok(())
    }

    /// Validates manual certificate TLS mode.
    ///
    /// Inputs:
    /// - `self`: accumulated TLS builder.
    /// - `path`: manifest path used in diagnostics.
    ///
    /// Output:
    /// - `Ok(())` when manual mode has certificate and key paths.
    ///
    /// Transformation:
    /// - Requires explicit cert/key fields and rejects ACME-only provider fields
    ///   before runtime certificate loading exists.
    fn validate_manual_mode(&self, path: &Path) -> Result<(), String> {
        if self.cert.is_none() || self.key.is_none() {
            return Err(format!(
                "{}: project manifest [server.tls] mode manual requires cert and key",
                path.display()
            ));
        }
        if self.primary_provider.is_some() || self.fallback_provider.is_some() {
            return Err(format!(
                "{}: project manifest [server.tls] mode manual cannot set ACME providers",
                path.display()
            ));
        }
        Ok(())
    }

    /// Validates internal development CA TLS mode.
    ///
    /// Inputs:
    /// - `self`: accumulated TLS builder.
    /// - `path`: manifest path used in diagnostics.
    ///
    /// Output:
    /// - `Ok(())` when internal mode does not mix public/manual TLS fields.
    ///
    /// Transformation:
    /// - Keeps development-only certificate management separate from ACME and
    ///   manual certificate configuration.
    fn validate_internal_mode(&self, path: &Path) -> Result<(), String> {
        if self.domains.is_some()
            || self.email.is_some()
            || self.primary_provider.is_some()
            || self.fallback_provider.is_some()
            || self.cert.is_some()
            || self.key.is_some()
            || self.passphrase_env.is_some()
            || self.ca.is_some()
        {
            return Err(format!(
                "{}: project manifest [server.tls] mode internal cannot set public or manual TLS fields",
                path.display()
            ));
        }
        Ok(())
    }
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
    match parsed.as_str() {
        "auto" => Ok(ProjectServerTlsMode::Auto),
        "manual" => Ok(ProjectServerTlsMode::Manual),
        "internal" => Ok(ProjectServerTlsMode::Internal),
        other => Err(format!(
            "{}:{}: unsupported [server.tls] mode `{}`; supported modes: auto, manual, internal",
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
    match parsed.as_str() {
        "letsencrypt" => Ok(ProjectServerTlsProvider::LetsEncrypt),
        "zerossl" => Ok(ProjectServerTlsProvider::ZeroSsl),
        other => Err(format!(
            "{}:{}: unsupported [server.tls] provider `{}`; supported providers: letsencrypt, zerossl",
            path.display(),
            line_no,
            other
        )),
    }
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
