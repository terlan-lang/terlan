use super::*;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use webpki::{EndEntityCert, Error as WebPkiError, KeyUsage};

/// Builds the production-shaped ACME runtime plan.
///
/// Inputs:
/// - `project_root`: directory containing `terlan.toml`.
/// - `tls`: parsed auto `[server.tls]` configuration.
///
/// Output:
/// - ACME runtime plan with defaulted provider and project-local cache path.
///
/// Transformation:
/// - Defaults the primary provider to Let's Encrypt, maps provider metadata to
///   the selected ACME directory, and reserves deterministic certificate/key
///   paths without issuing certificates or opening network connections.
pub fn acme_runtime_plan(project_root: &Path, tls: &ProjectServerTls) -> AcmeRuntimePlan {
    let primary_provider = tls
        .primary_provider
        .unwrap_or(ProjectServerTlsProvider::LetsEncrypt);
    let cache_dir = project_root.join(".terlan/tls/acme");
    AcmeRuntimePlan {
        domains: tls.domains.clone(),
        email: tls.email.clone(),
        primary_provider,
        fallback_provider: tls.fallback_provider,
        directory_url: acme_directory_url(primary_provider).to_string(),
        certificate_path: cache_dir.join("fullchain.pem"),
        private_key_path: cache_dir.join("privkey.pem"),
        account_credentials_path: cache_dir.join(ACME_ACCOUNT_CREDENTIALS_FILE),
        renewal_metadata_path: cache_dir.join(ACME_RENEWAL_METADATA_FILE),
        http01_challenge_dir: cache_dir.join(ACME_HTTP01_CACHE_DIR),
        cache_dir,
    }
}

/// Validates that an ACME provider can be used by the local runtime.
///
/// Inputs:
/// - `plan`: normalized automatic TLS runtime plan.
///
/// Output:
/// - `Ok(())` when the selected primary provider has all required account
///   machinery implemented.
/// - Stable `error[serve_tls]` diagnostic when provider-specific requirements
///   are not supported yet.
///
/// Transformation:
/// - Applies provider policy before either cached certificate loading or live
///   issuance so unsupported providers cannot be activated through stale local
///   state.
pub fn validate_acme_provider_supported(plan: &AcmeRuntimePlan) -> Result<(), String> {
    if plan.primary_provider == ProjectServerTlsProvider::ZeroSsl {
        return Err(
            "error[serve_tls]: ZeroSSL automatic issuance requires external account binding support"
                .to_string(),
        );
    }
    Ok(())
}

/// Validates cached ACME certificate renewal metadata.
///
/// Inputs:
/// - `plan`: normalized automatic TLS runtime plan.
/// - `now`: current clock value supplied by runtime or tests.
///
/// Output:
/// - `Ok(())` when the cache is still within its renewal window.
/// - Stable `error[serve_tls]` diagnostics for missing, malformed, stale, or
///   future-dated renewal metadata.
///
/// Transformation:
/// - Keeps cached automatic TLS material from being loaded forever after first
///   issuance. The local cache remains deterministic, while live renewal can
///   use the same metadata boundary before certificate expiry.
pub fn validate_acme_certificate_cache_age(
    plan: &AcmeRuntimePlan,
    now: SystemTime,
) -> Result<(), String> {
    let metadata = cache::load_acme_certificate_cache_metadata(plan)?;
    cache::validate_acme_certificate_cache_mode(plan, &metadata)?;
    let now = unix_seconds(now)?;
    if metadata.issued_at_unix_seconds > now.saturating_add(ACME_METADATA_CLOCK_SKEW.as_secs()) {
        return Err(format!(
            "error[serve_tls]: ACME certificate cache metadata `{}` is dated in the future",
            plan.renewal_metadata_path.display()
        ));
    }
    if now >= metadata.renew_after_unix_seconds {
        return Err(format!(
            "error[serve_tls]: automatic ACME TLS cache for domains [{}] requires renewal; renew_after={} now={now}",
            plan.domains.join(", "),
            metadata.renew_after_unix_seconds
        ));
    }
    cache::validate_acme_certificate_cache_provenance_hash(plan, &metadata)?;
    Ok(())
}

/// Validates cached ACME certificate DNS identity against configured domains.
///
/// Inputs:
/// - `plan`: normalized automatic TLS runtime plan.
/// - `certificates`: parsed certificate chain loaded from the ACME cache.
///
/// Output:
/// - `Ok(())` when the leaf certificate is valid for every configured domain.
/// - Stable `error[serve_tls]` diagnostic for malformed or wrong-domain cache
///   material.
///
/// Transformation:
/// - Delegates X.509 leaf parsing and SAN/CN DNS matching to maintained
///   `rustls-webpki` before any cached ACME key is loaded.
pub fn validate_acme_certificate_cache_domains(
    plan: &AcmeRuntimePlan,
    certificates: &[CertificateDer<'static>],
) -> Result<(), String> {
    let leaf = certificates.first().ok_or_else(|| {
        format!(
            "error[serve_tls]: automatic ACME TLS cache for domains [{}] has no leaf certificate",
            acme_domain_list(plan)
        )
    })?;
    let certificate = EndEntityCert::try_from(leaf).map_err(|err| {
        format!(
            "error[serve_tls]: failed to parse cached ACME certificate `{}` for domain validation: {err}",
            plan.certificate_path.display()
        )
    })?;
    for domain in &plan.domains {
        let server_name = ServerName::try_from(domain.clone()).map_err(|err| {
            format!(
                "error[serve_tls]: configured ACME domain `{domain}` is not a valid DNS server name: {err}"
            )
        })?;
        certificate
            .verify_is_valid_for_subject_name(&server_name)
            .map_err(|err| {
                format!(
                    "error[serve_tls]: cached ACME certificate `{}` is not valid for configured domain `{domain}`: {err}",
                    plan.certificate_path.display()
                )
            })?;
    }
    Ok(())
}

/// Validates cached ACME certificate not-before/not-after against runtime time.
///
/// Inputs:
/// - `plan`: normalized automatic TLS runtime plan.
/// - `certificates`: parsed certificate chain loaded from the ACME cache.
/// - `now`: runtime clock shared with cache metadata validation.
///
/// Output:
/// - `Ok(())` when webpki does not report a certificate validity-window error.
/// - Stable `error[serve_tls]` diagnostic when the leaf certificate is expired,
///   not yet valid, or has malformed validity timestamps.
///
/// Transformation:
/// - Reuses `rustls-webpki` path validation only for issuer-independent leaf
///   validity checks, without turning local cache validation into trust-store
///   verification.
pub fn validate_acme_certificate_cache_validity_window(
    plan: &AcmeRuntimePlan,
    certificates: &[CertificateDer<'static>],
    now: SystemTime,
) -> Result<(), String> {
    let leaf = certificates.first().ok_or_else(|| {
        format!(
            "error[serve_tls]: automatic ACME TLS cache for domains [{}] has no leaf certificate",
            acme_domain_list(plan)
        )
    })?;
    let certificate = EndEntityCert::try_from(leaf).map_err(|err| {
        format!(
            "error[serve_tls]: failed to parse cached ACME certificate `{}` for validity-window validation: {err}",
            plan.certificate_path.display()
        )
    })?;
    let validation_time = webpki_unix_time(now)?;
    let provider = rustls::crypto::ring::default_provider();
    let result = certificate.verify_for_usage(
        provider.signature_verification_algorithms.all,
        &[],
        &[],
        validation_time,
        KeyUsage::server_auth(),
        None,
        None,
    );
    match result {
        Ok(_) | Err(WebPkiError::UnknownIssuer) => Ok(()),
        Err(
            err @ (WebPkiError::CertExpired { .. }
            | WebPkiError::CertNotValidYet { .. }
            | WebPkiError::InvalidCertValidity
            | WebPkiError::BadDerTime),
        ) => Err(format!(
            "error[serve_tls]: cached ACME certificate `{}` failed validity-window validation: {err}",
            plan.certificate_path.display()
        )),
        Err(_) => Ok(()),
    }
}

pub fn webpki_unix_time(time: SystemTime) -> Result<UnixTime, String> {
    time.duration_since(UNIX_EPOCH)
        .map(UnixTime::since_unix_epoch)
        .map_err(|err| format!("error[serve_tls]: system clock is before Unix epoch: {err}"))
}

/// Returns Unix seconds for a system clock value.
pub fn unix_seconds(time: SystemTime) -> Result<u64, String> {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|err| format!("error[serve_tls]: system clock is before Unix epoch: {err}"))
}

/// Returns the ACME directory URL for a provider.
///
/// Inputs:
/// - `provider`: parsed ACME provider.
///
/// Output:
/// - Provider directory URL used by future certificate issuance.
///
/// Transformation:
/// - Keeps directory discovery independent of the issuer's async client.
///   ZeroSSL remains provider metadata until the issuance layer owns its
///   account and external account binding requirements.
pub fn acme_directory_url(provider: ProjectServerTlsProvider) -> &'static str {
    match provider {
        ProjectServerTlsProvider::LetsEncrypt => "https://acme-v02.api.letsencrypt.org/directory",
        ProjectServerTlsProvider::ZeroSsl => "https://acme.zerossl.com/v2/DV90",
    }
}
