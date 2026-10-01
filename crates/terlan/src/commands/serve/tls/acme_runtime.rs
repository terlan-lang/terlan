use std::path::Path;

use crate::commands::serve::manifest::web_package_tls_config;
use terlan_http_native::acme::AcmeRuntimePlan;
use terlan_http_native::tls_runtime;

pub(in crate::commands::serve) use terlan_http_native::acme::AcmeHttp01Challenge;
pub(in crate::commands::serve) use terlan_http_native::tls_material::RuntimeConfig as RuntimeTlsConfig;

#[cfg(test)]
use crate::commands::serve::tls_contract::ProjectServerTls;
#[cfg(test)]
use cache::{
    load_acme_account_credentials, store_acme_account_credentials, store_acme_certificate_cache,
    store_acme_certificate_cache_metadata, store_acme_http01_challenge,
};
#[cfg(test)]
use terlan_http_native::acme::issuer::{
    acme_contact_strings, acme_domain_identifiers, generate_acme_csr, pending_http01_challenges,
};
#[cfg(test)]
use terlan_http_native::acme::{
    acme_runtime_plan, cache, validate_acme_provider_supported, ACME_RENEWAL_INTERVAL,
};

#[cfg(test)]
mod tls_test;

/// The host supplies project discovery; the HTTP package owns TLS mode/cache policy.
pub(in crate::commands::serve) fn runtime_tls_config_for_serve(
    web_root: &Path,
) -> Result<Option<RuntimeTlsConfig>, String> {
    let Some((project_root, tls)) = web_package_tls_config(web_root)? else {
        return Ok(None);
    };
    tls_runtime::load(&project_root, &tls, issue_acme_certificate_cache_for_serve).map(Some)
}

/// Temporary executor adapter until the maintained client's transport uses VM I/O.
#[cfg(feature = "acme-live")]
fn issue_acme_certificate_cache_for_serve(plan: &AcmeRuntimePlan) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            format!("error[serve_tls]: failed to start temporary ACME runtime: {err}")
        })?;
    runtime.block_on(terlan_http_native::acme::issuer::issue_certificate_cache(
        plan,
        None,
        |_| Ok(()),
        tokio::time::sleep,
    ))
}

#[cfg(not(feature = "acme-live"))]
fn issue_acme_certificate_cache_for_serve(_plan: &AcmeRuntimePlan) -> Result<(), String> {
    Err(
        "error[serve_tls]: live ACME issuance requires a compiler build with the `acme-live` feature."
            .to_string(),
    )
}

/// Supplies project discovery to the package-owned HTTP-01 cache reader.
pub(in crate::commands::serve) fn acme_http01_challenge(
    web_root: &Path,
    request_path: &str,
) -> Result<AcmeHttp01Challenge, String> {
    if !request_path.starts_with(terlan_http_native::acme::ACME_HTTP01_PATH_PREFIX) {
        return Ok(AcmeHttp01Challenge::NotMatched);
    }
    let project = web_package_tls_config(web_root)?;
    terlan_http_native::acme::acme_http01_challenge(
        project.as_ref().map(|(root, tls)| (root.as_path(), tls)),
        request_path,
    )
}

#[cfg(test)]
fn runtime_tls_config(web_root: &Path) -> Result<Option<RuntimeTlsConfig>, String> {
    let Some((root, tls)) = web_package_tls_config(web_root)? else {
        return Ok(None);
    };
    tls_runtime::load_cached(&root, &tls).map(Some)
}

#[cfg(test)]
fn acme_runtime_tls_config_with_local_issuer(
    project_root: &Path,
    tls: &ProjectServerTls,
    issuer: impl FnOnce(&AcmeRuntimePlan) -> Result<(), String>,
) -> Result<RuntimeTlsConfig, String> {
    tls_runtime::load(project_root, tls, issuer)
}

#[cfg(test)]
fn issue_acme_certificate_cache_preflight(plan: &AcmeRuntimePlan) -> Result<(), String> {
    validate_acme_provider_supported(plan)
}
