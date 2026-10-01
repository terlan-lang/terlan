//! ACME cache policy and HTTP-01 serving owned by std.http.
//! Issuer scheduling and project discovery are supplied by the serving host.

use crate::tls_config::{
    Config as ProjectServerTls, Mode as ProjectServerTlsMode, Provider as ProjectServerTlsProvider,
};
use crate::tls_material::{
    load_certificate_chain, load_private_key, rustls_server_config,
    RuntimeConfig as RuntimeTlsConfig,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub mod cache;
mod certificate_validation;
#[cfg(any(feature = "acme-issuer", test))]
pub mod issuer;
use certificate_validation::*;
pub use certificate_validation::{
    acme_runtime_plan, unix_seconds, validate_acme_provider_supported,
};

/// URL path prefix reserved by ACME HTTP-01.
pub const ACME_HTTP01_PATH_PREFIX: &str = "/.well-known/acme-challenge/";

/// Directory under the ACME cache that stores HTTP-01 challenge bodies.
const ACME_HTTP01_CACHE_DIR: &str = "http-01";

/// File under the ACME cache that stores reusable account credentials.
const ACME_ACCOUNT_CREDENTIALS_FILE: &str = "account.json";

/// File under the ACME cache that stores renewal metadata.
const ACME_RENEWAL_METADATA_FILE: &str = "renewal.json";

/// Default renewal window for locally cached ACME certificates.
pub const ACME_RENEWAL_INTERVAL: Duration = Duration::from_secs(60 * 60 * 24 * 60);

/// Maximum tolerated clock skew for ACME cache metadata.
const ACME_METADATA_CLOCK_SKEW: Duration = Duration::from_secs(60 * 5);

/// Planned ACME runtime configuration before certificate issuance.
///
/// Inputs:
/// - Produced from `[server.tls] mode = "auto"` project metadata.
///
/// Output:
/// - Normalized ACME domains, account email, provider selection, provider
///   endpoint, and project-local cache directory.
///
/// Transformation:
/// - Applies the production runtime defaults Terlan promises to users without
///   performing network I/O: Let's Encrypt is the default primary provider,
///   fallback provider metadata is preserved, and certificate state belongs to
///   `.terlan/tls/acme` under the project root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcmeRuntimePlan {
    pub domains: Vec<String>,
    pub email: Option<String>,
    pub primary_provider: ProjectServerTlsProvider,
    pub fallback_provider: Option<ProjectServerTlsProvider>,
    pub directory_url: String,
    pub cache_dir: PathBuf,
    pub certificate_path: PathBuf,
    pub private_key_path: PathBuf,
    pub account_credentials_path: PathBuf,
    pub renewal_metadata_path: PathBuf,
    pub http01_challenge_dir: PathBuf,
}

/// Result of resolving one ACME HTTP-01 request path.
///
/// Inputs:
/// - Produced from a request path and adjacent auto TLS project metadata.
///
/// Output:
/// - Selected challenge body, missing challenge marker, invalid request
///   diagnostic, or no match.
///
/// Transformation:
/// - Keeps ACME challenge serving independent from normal static file and
///   manifest handler routing.
#[derive(Debug, PartialEq, Eq)]
pub enum AcmeHttp01Challenge {
    Found(String),
    Missing,
    Invalid(String),
    NotMatched,
}

/// Builds runtime TLS config from the deterministic ACME certificate cache.
///
/// Inputs:
/// - `plan`: normalized automatic TLS runtime plan.
///
/// Output:
/// - `Ok(Some(_))` when both certificate and key cache files exist and are
///   fresh enough to serve.
/// - `Ok(None)` when the cache has not been populated yet.
/// - Stable `error[serve_tls]` diagnostic for partial, stale, malformed, or
///   unreadable cache state.
///
/// Transformation:
/// - Keeps cache validation reusable by both normal startup and deterministic
///   issuer-handoff tests without opening the ACME network.
pub fn load_acme_runtime_tls_cache(
    plan: &AcmeRuntimePlan,
) -> Result<Option<RuntimeTlsConfig>, String> {
    cache::validate_acme_cache_paths(plan)?;
    let certificate_exists = plan.certificate_path.is_file();
    let private_key_exists = plan.private_key_path.is_file();
    match (certificate_exists, private_key_exists) {
        (false, false) => Ok(None),
        (true, true) => {
            let now = SystemTime::now();
            validate_acme_certificate_cache_age(plan, now)?;
            let certificates = load_certificate_chain(&plan.certificate_path)?;
            validate_acme_certificate_cache_domains(plan, &certificates)?;
            validate_acme_certificate_cache_validity_window(plan, &certificates, now)?;
            cache::validate_acme_key_custody_policy(plan)?;
            let private_key = load_private_key(&plan.private_key_path)?;
            let server_config = rustls_server_config(certificates, private_key)?;
            Ok(Some(RuntimeTlsConfig {
                server_config: Arc::new(server_config),
            }))
        }
        _ => Err(format!(
            "error[serve_tls]: automatic ACME TLS cache for domains [{}] is incomplete; expected certificate `{}` and key `{}`",
            acme_domain_list(plan),
            plan.certificate_path.display(),
            plan.private_key_path.display()
        )),
    }
}

/// Formats ACME domains for diagnostics.
///
/// Inputs:
/// - `plan`: normalized automatic TLS runtime plan.
///
/// Output:
/// - Comma-separated domain list, or `<none>` for malformed empty domain
///   state.
///
/// Transformation:
/// - Centralizes diagnostic rendering so cache-miss and partial-cache errors
///   identify the same target domain set.
fn acme_domain_list(plan: &AcmeRuntimePlan) -> String {
    if plan.domains.is_empty() {
        "<none>".to_string()
    } else {
        plan.domains.join(", ")
    }
}

/// Reads an HTTP-01 challenge only for an auto-TLS project and a safe token.
pub fn acme_http01_challenge(
    project: Option<(&Path, &ProjectServerTls)>,
    request_path: &str,
) -> Result<AcmeHttp01Challenge, String> {
    let Some(token) = request_path.strip_prefix(ACME_HTTP01_PATH_PREFIX) else {
        return Ok(AcmeHttp01Challenge::NotMatched);
    };
    let Some((project_root, tls)) = project else {
        return Ok(AcmeHttp01Challenge::NotMatched);
    };
    if tls.mode != ProjectServerTlsMode::Auto {
        return Ok(AcmeHttp01Challenge::NotMatched);
    }
    if !is_acme_http01_token(token) {
        return Ok(AcmeHttp01Challenge::Invalid(format!(
            "error[serve_tls]: ACME HTTP-01 token `{token}` is invalid"
        )));
    }
    let plan = acme_runtime_plan(project_root, tls);
    let challenge_path = plan.cache_dir.join(ACME_HTTP01_CACHE_DIR).join(token);
    cache::validate_acme_cache_path("HTTP-01", &plan.cache_dir, &challenge_path)?;
    match fs::read_to_string(&challenge_path) {
        Ok(body) => Ok(AcmeHttp01Challenge::Found(body)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(AcmeHttp01Challenge::Missing),
        Err(err) => Err(format!(
            "error[serve_tls]: failed to read ACME HTTP-01 challenge `{}`: {err}",
            challenge_path.display()
        )),
    }
}

/// Returns whether a request path segment is a valid ACME HTTP-01 token.
///
/// Inputs:
/// - `token`: raw path suffix after `/.well-known/acme-challenge/`.
///
/// Output:
/// - `true` when the token is non-empty and contains only URL-safe token
///   characters accepted by ACME HTTP-01.
///
/// Transformation:
/// - Rejects path separators, empty values, dots, escapes, and other
///   filesystem-sensitive characters before the token is used as a file name.
pub(crate) fn is_acme_http01_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

#[cfg(test)]
mod tests;
