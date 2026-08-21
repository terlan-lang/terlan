//! Runtime-owned TLS manifest contract shared with project builds.

#[cfg(all(feature = "serve-runtime-bin", not(test)))]
use std::path::Path;
#[cfg(all(feature = "serve-runtime-bin", not(test)))]
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectServerTls {
    pub(crate) mode: ProjectServerTlsMode,
    pub(crate) domains: Vec<String>,
    pub(crate) email: Option<String>,
    pub(crate) primary_provider: Option<ProjectServerTlsProvider>,
    pub(crate) fallback_provider: Option<ProjectServerTlsProvider>,
    pub(crate) cert: Option<String>,
    pub(crate) key: Option<String>,
    pub(crate) passphrase_env: Option<String>,
    pub(crate) ca: Option<String>,
    pub(crate) server_name: Option<String>,
    pub(crate) trust_local: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectServerTlsMode {
    Auto,
    Manual,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectServerTlsProvider {
    LetsEncrypt,
    ZeroSsl,
}

#[cfg(all(feature = "serve-runtime-bin", not(test)))]
#[derive(serde::Deserialize, Default)]
struct RuntimeProjectManifest {
    #[serde(default)]
    server: RuntimeServerManifest,
}

#[cfg(all(feature = "serve-runtime-bin", not(test)))]
#[derive(serde::Deserialize, Default)]
struct RuntimeServerManifest {
    tls: Option<RuntimeServerTls>,
}

#[cfg(all(feature = "serve-runtime-bin", not(test)))]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeServerTls {
    mode: Option<String>,
    domains: Option<Vec<String>>,
    email: Option<String>,
    primary_provider: Option<String>,
    fallback_provider: Option<String>,
    cert: Option<String>,
    key: Option<String>,
    passphrase_env: Option<String>,
    ca: Option<String>,
    server_name: Option<String>,
    trust_local: Option<bool>,
}

#[cfg(all(feature = "serve-runtime-bin", not(test)))]
pub(crate) fn read_runtime_server_tls(
    path: &Path,
) -> Result<Option<ProjectServerTls>, BoundaryError> {
    let source = std::fs::read_to_string(path).map_err(|error| {
        tls_contract_error(path, format!("cannot read project manifest: {error}"))
    })?;
    let manifest: RuntimeProjectManifest = basic_toml::from_str(&source).map_err(|error| {
        tls_contract_error(path, format!("cannot parse runtime TLS metadata: {error}"))
    })?;
    manifest
        .server
        .tls
        .map(|tls| validate_runtime_tls(tls, path))
        .transpose()
}

#[cfg(all(feature = "serve-runtime-bin", not(test)))]
fn validate_runtime_tls(
    tls: RuntimeServerTls,
    path: &Path,
) -> Result<ProjectServerTls, BoundaryError> {
    for (field, value) in [
        ("email", tls.email.as_deref()),
        ("cert", tls.cert.as_deref()),
        ("key", tls.key.as_deref()),
        ("passphrase_env", tls.passphrase_env.as_deref()),
        ("ca", tls.ca.as_deref()),
        ("server_name", tls.server_name.as_deref()),
    ] {
        if matches!(value, Some(value) if value.trim().is_empty()) {
            return Err(tls_contract_error(
                path,
                format!("project manifest [server.tls] {field} cannot be empty"),
            ));
        }
    }
    if tls
        .domains
        .as_deref()
        .is_some_and(|domains| domains.iter().any(|domain| domain.trim().is_empty()))
    {
        return Err(tls_contract_error(
            path,
            "project manifest [server.tls] domains cannot contain empty entries",
        ));
    }
    let mode = tls
        .mode
        .as_deref()
        .ok_or_else(|| tls_contract_error(path, "project manifest [server.tls] requires mode"))
        .and_then(|value| parse_mode(value, path))?;
    let primary_provider = tls
        .primary_provider
        .as_deref()
        .map(|value| parse_provider(value, path))
        .transpose()?;
    let fallback_provider = tls
        .fallback_provider
        .as_deref()
        .map(|value| parse_provider(value, path))
        .transpose()?;
    match mode {
        ProjectServerTlsMode::Auto => {
            if tls.domains.as_ref().is_none_or(Vec::is_empty) {
                return Err(tls_contract_error(
                    path,
                    "project manifest [server.tls] mode auto requires domains",
                ));
            }
            if tls.cert.is_some()
                || tls.key.is_some()
                || tls.passphrase_env.is_some()
                || tls.ca.is_some()
                || tls.server_name.is_some()
                || tls.trust_local.is_some()
            {
                return Err(tls_contract_error(
                    path,
                    "project manifest [server.tls] mode auto cannot set manual or internal TLS fields",
                ));
            }
        }
        ProjectServerTlsMode::Manual => {
            if tls.cert.is_none() || tls.key.is_none() {
                return Err(tls_contract_error(
                    path,
                    "project manifest [server.tls] mode manual requires cert and key",
                ));
            }
            if primary_provider.is_some() || fallback_provider.is_some() {
                return Err(tls_contract_error(
                    path,
                    "project manifest [server.tls] mode manual cannot set ACME providers",
                ));
            }
        }
        ProjectServerTlsMode::Internal => {
            if tls.domains.is_some()
                || tls.email.is_some()
                || primary_provider.is_some()
                || fallback_provider.is_some()
                || tls.cert.is_some()
                || tls.key.is_some()
                || tls.passphrase_env.is_some()
                || tls.ca.is_some()
            {
                return Err(tls_contract_error(
                    path,
                    "project manifest [server.tls] mode internal cannot set public or manual TLS fields",
                ));
            }
        }
    }
    Ok(ProjectServerTls {
        mode,
        domains: tls.domains.unwrap_or_default(),
        email: tls.email,
        primary_provider,
        fallback_provider,
        cert: tls.cert,
        key: tls.key,
        passphrase_env: tls.passphrase_env,
        ca: tls.ca,
        server_name: tls.server_name,
        trust_local: tls.trust_local,
    })
}

#[cfg(all(feature = "serve-runtime-bin", not(test)))]
fn parse_mode(value: &str, path: &Path) -> Result<ProjectServerTlsMode, BoundaryError> {
    match value {
        "auto" => Ok(ProjectServerTlsMode::Auto),
        "manual" => Ok(ProjectServerTlsMode::Manual),
        "internal" => Ok(ProjectServerTlsMode::Internal),
        other => Err(tls_contract_error(
            path,
            format!(
                "unsupported [server.tls] mode `{other}`; supported modes: auto, manual, internal"
            ),
        )),
    }
}

#[cfg(all(feature = "serve-runtime-bin", not(test)))]
fn parse_provider(value: &str, path: &Path) -> Result<ProjectServerTlsProvider, BoundaryError> {
    match value {
        "letsencrypt" => Ok(ProjectServerTlsProvider::LetsEncrypt),
        "zerossl" => Ok(ProjectServerTlsProvider::ZeroSsl),
        other => Err(tls_contract_error(
            path,
            format!(
                "unsupported [server.tls] provider `{other}`; supported providers: letsencrypt, zerossl"
            ),
        )),
    }
}

#[cfg(all(feature = "serve-runtime-bin", not(test)))]
fn tls_contract_error(path: &Path, message: impl Into<String>) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::CommandExecution,
        "read runtime TLS contract",
        format!("{}: {}", path.display(), message.into()),
    )
}
