use super::*;

/// Waits for an issued ACME certificate chain.
///
/// Inputs:
/// - `order`: finalized ACME order.
///
/// Output:
/// - PEM certificate chain returned by the CA.
///
/// Transformation:
/// - Polls the maintained ACME client certificate endpoint with a bounded
///   retry loop and returns a stable diagnostic if issuance does not complete.
#[cfg(feature = "acme-live")]
pub(super) async fn wait_for_acme_certificate(
    order: &mut instant_acme::Order,
) -> Result<String, String> {
    for attempt in 0..ACME_CERTIFICATE_MAX_POLLS {
        match order
            .certificate()
            .await
            .map_err(acme_error("failed to fetch ACME certificate"))?
        {
            Some(certificate_pem) => return Ok(certificate_pem),
            None if attempt + 1 < ACME_CERTIFICATE_MAX_POLLS => {
                std::thread::sleep(ACME_CERTIFICATE_POLL_DELAY);
            }
            None => {
                return Err(format!(
                    "error[serve_tls]: ACME certificate was not available after {} polls",
                    ACME_CERTIFICATE_MAX_POLLS
                ));
            }
        }
    }
    Err("error[serve_tls]: ACME certificate was not available".to_string())
}

/// Converts an `instant-acme` error into a stable TLS diagnostic closure.
///
/// Inputs:
/// - `context`: operation-specific diagnostic prefix.
///
/// Output:
/// - Closure suitable for `Result::map_err`.
///
/// Transformation:
/// - Preserves the maintained client's error text while keeping Terlan's
///   user-facing error code stable.
#[cfg(feature = "acme-live")]
pub(super) fn acme_error(context: &'static str) -> impl FnOnce(instant_acme::Error) -> String {
    move |err| format!("error[serve_tls]: {context}: {err}")
}

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
pub(super) fn acme_runtime_plan(project_root: &Path, tls: &ProjectServerTls) -> AcmeRuntimePlan {
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

/// Validates ACME issuance before any network or cache writes.
///
/// Inputs:
/// - `plan`: normalized automatic TLS runtime plan.
///
/// Output:
/// - `Ok(())` when live issuance may proceed.
/// - Stable `error[serve_tls]` diagnostic when provider-specific requirements
///   are not supported yet.
///
/// Transformation:
/// - Runs synchronous policy checks shared by the live issuer and tests so
///   pre-network failures do not require an async test harness.
#[cfg(any(feature = "acme-live", test))]
pub(super) fn issue_acme_certificate_cache_preflight(plan: &AcmeRuntimePlan) -> Result<(), String> {
    validate_acme_provider_supported(plan)
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
pub(super) fn validate_acme_provider_supported(plan: &AcmeRuntimePlan) -> Result<(), String> {
    if plan.primary_provider == ProjectServerTlsProvider::ZeroSsl {
        return Err(
            "error[serve_tls]: ZeroSSL automatic issuance requires external account binding support"
                .to_string(),
        );
    }
    Ok(())
}

/// Returns the manifest spelling of an ACME provider for diagnostics.
#[cfg(test)]
pub(super) fn tls_provider_name(provider: ProjectServerTlsProvider) -> &'static str {
    match provider {
        ProjectServerTlsProvider::LetsEncrypt => "letsencrypt",
        ProjectServerTlsProvider::ZeroSsl => "zerossl",
    }
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
pub(super) fn validate_acme_certificate_cache_age(
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
pub(super) fn validate_acme_certificate_cache_domains(
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
pub(super) fn validate_acme_certificate_cache_validity_window(
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

pub(super) fn webpki_unix_time(time: SystemTime) -> Result<UnixTime, String> {
    time.duration_since(UNIX_EPOCH)
        .map(UnixTime::since_unix_epoch)
        .map_err(|err| format!("error[serve_tls]: system clock is before Unix epoch: {err}"))
}

/// Returns Unix seconds for a system clock value.
pub(super) fn unix_seconds(time: SystemTime) -> Result<u64, String> {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|err| format!("error[serve_tls]: system clock is before Unix epoch: {err}"))
}

/// Resolves an ACME HTTP-01 challenge response from the project cache.
///
/// Inputs:
/// - `web_root`: validated web package root passed to `terlc serve`.
/// - `request_path`: URL path from the incoming HTTP request.
///
/// Output:
/// - `Found` with the challenge body when the project has auto TLS enabled and
///   the token file exists.
/// - `Missing` when the ACME challenge path is requested but the token file has
///   not been written yet.
/// - `Invalid` when the requested token is not an ACME-safe token.
/// - `NotMatched` when the request is not an ACME challenge route or the
///   project does not use auto TLS.
///
/// Transformation:
/// - Locates adjacent project TLS metadata, verifies auto mode, validates the
///   token as path-safe base64url-like text, and reads only from the
///   deterministic `.terlan/tls/acme/http-01` cache directory.
pub(in crate::commands::serve) fn acme_http01_challenge(
    web_root: &Path,
    request_path: &str,
) -> Result<AcmeHttp01Challenge, String> {
    let Some(token) = request_path.strip_prefix(ACME_HTTP01_PATH_PREFIX) else {
        return Ok(AcmeHttp01Challenge::NotMatched);
    };
    let Some((project_root, tls)) = web_package_tls_config(web_root)? else {
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
    let plan = acme_runtime_plan(&project_root, &tls);
    let challenge_path = plan.cache_dir.join(ACME_HTTP01_CACHE_DIR).join(token);
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
pub(super) fn is_acme_http01_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
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
/// - Uses the maintained `instant-acme` provider constant for Let's Encrypt.
///   ZeroSSL remains provider metadata until the issuance layer owns its
///   account and external account binding requirements.
pub(super) fn acme_directory_url(provider: ProjectServerTlsProvider) -> &'static str {
    match provider {
        ProjectServerTlsProvider::LetsEncrypt => instant_acme::LetsEncrypt::Production.url(),
        ProjectServerTlsProvider::ZeroSsl => "https://acme.zerossl.com/v2/DV90",
    }
}

/// Loads a PEM certificate chain.
///
/// Inputs:
/// - `path`: certificate chain file path.
///
/// Output:
/// - Non-empty DER certificate chain for rustls.
///
/// Transformation:
/// - Delegates PEM parsing to `rustls-pemfile` and converts parse/IO failures
///   into stable serve diagnostics.
pub(super) fn load_certificate_chain(path: &Path) -> Result<Vec<CertificateDer<'static>>, String> {
    let file = fs::File::open(path).map_err(|err| {
        format!(
            "error[serve_tls]: failed to open TLS certificate `{}`: {err}",
            path.display()
        )
    })?;
    let certificates = rustls_pemfile::certs(&mut BufReader::new(file))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            format!(
                "error[serve_tls]: failed to parse TLS certificate `{}`: {err}",
                path.display()
            )
        })?;
    if certificates.is_empty() {
        return Err(format!(
            "error[serve_tls]: TLS certificate `{}` did not contain any PEM certificates",
            path.display()
        ));
    }
    Ok(certificates)
}

/// Loads one PEM private key.
///
/// Inputs:
/// - `path`: private key file path.
///
/// Output:
/// - DER private key for rustls.
///
/// Transformation:
/// - Accepts the first supported PKCS#8, PKCS#1, or SEC1 key returned by
///   `rustls-pemfile`, preserving encrypted-key rejection as a user-facing
///   runtime diagnostic.
pub(super) fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, String> {
    let file = fs::File::open(path).map_err(|err| {
        format!(
            "error[serve_tls]: failed to open TLS private key `{}`: {err}",
            path.display()
        )
    })?;
    rustls_pemfile::private_key(&mut BufReader::new(file))
        .map_err(|err| {
            format!(
                "error[serve_tls]: failed to parse TLS private key `{}`: {err}",
                path.display()
            )
        })?
        .ok_or_else(|| {
            format!(
                "error[serve_tls]: TLS private key `{}` did not contain a supported unencrypted PEM key",
                path.display()
            )
        })
}

/// Builds a rustls server config.
///
/// Inputs:
/// - `certificates`: server certificate chain.
/// - `private_key`: server private key.
///
/// Output:
/// - Rustls server configuration with safe defaults and no client auth.
///
/// Transformation:
/// - Centralizes the rustls builder call so manual and internal modes share the
///   same protocol policy.
pub(super) fn rustls_server_config(
    certificates: Vec<CertificateDer<'static>>,
    private_key: PrivateKeyDer<'static>,
) -> Result<ServerConfig, String> {
    let mut config =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|err| {
                format!("error[serve_tls]: failed to select TLS protocol versions: {err}")
            })?
            .with_no_client_auth()
            .with_single_cert(certificates, private_key)
            .map_err(|err| format!("error[serve_tls]: failed to build TLS server config: {err}"))?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}
