use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(feature = "acme-live")]
use instant_acme::{Account, NewAccount, NewOrder, OrderStatus};
#[cfg(any(feature = "acme-live", test))]
use instant_acme::{Authorization, AuthorizationStatus, Challenge, ChallengeType, Identifier};
use rcgen::generate_simple_self_signed;
#[cfg(any(feature = "acme-live", test))]
use rcgen::{CertificateParams, DistinguishedName, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::ServerConfig;
use webpki::{EndEntityCert, Error as WebPkiError, KeyUsage};

use crate::commands::build::project_manifest::{
    ProjectServerTls, ProjectServerTlsMode, ProjectServerTlsProvider,
};
use crate::runtime::vm::acme_worker::{
    VmAcmeMode, VmAcmeWorkerExecutionLane, VmAcmeWorkerHandle, VmAcmeWorkerRequest,
    VmAcmeWorkerRuntime,
};
use crate::runtime::vm::process::VmProcessId;

use crate::commands::serve::manifest::web_package_tls_config;

mod cache;
mod certificate_validation;

pub(in crate::commands::serve) use certificate_validation::acme_http01_challenge;
use certificate_validation::*;

#[cfg(test)]
mod tls_test;

#[cfg(test)]
use cache::store_acme_certificate_cache_metadata;
#[cfg(any(feature = "acme-live", test))]
use cache::{
    load_acme_account_credentials, store_acme_account_credentials, store_acme_certificate_cache,
    store_acme_http01_challenge,
};

/// URL path prefix reserved by ACME HTTP-01.
const ACME_HTTP01_PATH_PREFIX: &str = "/.well-known/acme-challenge/";

/// Directory under the ACME cache that stores HTTP-01 challenge bodies.
const ACME_HTTP01_CACHE_DIR: &str = "http-01";

/// File under the ACME cache that stores reusable account credentials.
const ACME_ACCOUNT_CREDENTIALS_FILE: &str = "account.json";

/// File under the ACME cache that stores renewal metadata.
const ACME_RENEWAL_METADATA_FILE: &str = "renewal.json";

/// Default renewal window for locally cached ACME certificates.
#[cfg(any(feature = "acme-live", test))]
const ACME_RENEWAL_INTERVAL: Duration = Duration::from_secs(60 * 60 * 24 * 60);

/// Maximum tolerated clock skew for ACME cache metadata.
const ACME_METADATA_CLOCK_SKEW: Duration = Duration::from_secs(60 * 5);

/// Maximum order-state refresh attempts while waiting for ACME readiness.
#[cfg(feature = "acme-live")]
const ACME_READY_MAX_POLLS: u8 = 5;

/// Initial delay between ACME order-state refresh attempts.
#[cfg(feature = "acme-live")]
const ACME_READY_INITIAL_DELAY: Duration = Duration::from_millis(250);

/// Delay between ACME certificate fetch attempts after finalization.
#[cfg(feature = "acme-live")]
const ACME_CERTIFICATE_POLL_DELAY: Duration = Duration::from_secs(1);

/// Maximum certificate fetch attempts after ACME finalization.
#[cfg(feature = "acme-live")]
const ACME_CERTIFICATE_MAX_POLLS: u8 = 10;

/// Resolved TLS runtime configuration for one served package.
///
/// Inputs:
/// - Produced from adjacent project metadata and runtime certificate loading.
///
/// Output:
/// - `rustls` server configuration plus the user-facing URL scheme.
///
/// Transformation:
/// - Keeps HTTP/HTTPS protocol choice outside the request handler so routing
///   behavior remains shared between plain and TLS listeners.
#[derive(Clone)]
pub(in crate::commands::serve) struct RuntimeTlsConfig {
    pub(in crate::commands::serve) server_config: Arc<ServerConfig>,
}

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
struct AcmeRuntimePlan {
    domains: Vec<String>,
    email: Option<String>,
    primary_provider: ProjectServerTlsProvider,
    fallback_provider: Option<ProjectServerTlsProvider>,
    directory_url: String,
    cache_dir: PathBuf,
    certificate_path: PathBuf,
    private_key_path: PathBuf,
    account_credentials_path: PathBuf,
    renewal_metadata_path: PathBuf,
    http01_challenge_dir: PathBuf,
}

/// Pending ACME HTTP-01 challenge selected for one authorization.
///
/// Inputs:
/// - Produced from ACME authorization data returned by `instant-acme`.
///
/// Output:
/// - Selected HTTP-01 challenge reference.
///
/// Transformation:
/// - Keeps challenge selection separate from challenge readiness and CSR
///   finalization so the non-network policy can be tested deterministically.
#[cfg(any(feature = "acme-live", test))]
struct PendingHttp01Challenge<'a> {
    challenge: &'a Challenge,
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
pub(in crate::commands::serve) enum AcmeHttp01Challenge {
    Found(String),
    Missing,
    Invalid(String),
    NotMatched,
}

/// Loads optional live TLS serving configuration for a package.
///
/// Inputs:
/// - `web_root`: validated web package root passed to `terlc serve`.
///
/// Output:
/// - `Ok(None)` when no adjacent project manifest configures TLS.
/// - `Ok(Some(_))` for manual certificate or internal self-signed TLS modes.
/// - Stable `error[serve_tls]` diagnostic when runtime TLS configuration fails.
///
/// Transformation:
/// - Reuses serve manifest discovery, resolves project-relative certificate
///   paths, parses PEM certificates through maintained `rustls-pemfile`
///   helpers, and constructs the `rustls::ServerConfig` consumed by the
///   runtime accept loop.
#[cfg(test)]
pub(super) fn runtime_tls_config(web_root: &Path) -> Result<Option<RuntimeTlsConfig>, String> {
    let Some((project_root, tls)) = web_package_tls_config(web_root)? else {
        return Ok(None);
    };
    match tls.mode {
        ProjectServerTlsMode::Manual => manual_runtime_tls_config(&project_root, &tls),
        ProjectServerTlsMode::Internal => internal_runtime_tls_config(&tls),
        ProjectServerTlsMode::Auto => acme_runtime_tls_config(&project_root, &tls),
    }
    .map(Some)
}

/// Loads live TLS configuration for normal `terlc serve` startup.
///
/// Inputs:
/// - `web_root`: validated web package root passed to `terlc serve`.
///
/// Output:
/// - Runtime TLS config for manual/internal/cached-auto TLS.
/// - Auto TLS cache misses start a VM-owned ACME worker before the maintained
///   ACME client is allowed to populate the cache.
///
/// Transformation:
/// - Keeps cached startup deterministic while moving live issuance lifecycle
///   visibility into the VM worker contract used by production renewal.
pub(in crate::commands::serve) fn runtime_tls_config_for_serve(
    web_root: &Path,
) -> Result<Option<RuntimeTlsConfig>, String> {
    let Some((project_root, tls)) = web_package_tls_config(web_root)? else {
        return Ok(None);
    };
    match tls.mode {
        ProjectServerTlsMode::Manual => manual_runtime_tls_config(&project_root, &tls).map(Some),
        ProjectServerTlsMode::Internal => internal_runtime_tls_config(&tls).map(Some),
        ProjectServerTlsMode::Auto => {
            acme_runtime_tls_config_for_serve(&project_root, &tls).map(Some)
        }
    }
}

/// Builds a manual certificate rustls configuration.
///
/// Inputs:
/// - `project_root`: directory containing `terlan.toml`.
/// - `tls`: parsed manual `[server.tls]` configuration.
///
/// Output:
/// - Runtime TLS config using the configured certificate chain and private key.
///
/// Transformation:
/// - Resolves already-validated project-relative cert/key paths, loads PEM
///   bytes, and delegates protocol setup to `rustls`.
fn manual_runtime_tls_config(
    project_root: &Path,
    tls: &ProjectServerTls,
) -> Result<RuntimeTlsConfig, String> {
    if tls.passphrase_env.is_some() {
        return Err(
            "error[serve_tls]: encrypted manual TLS keys are not supported by the local runtime yet"
                .to_string(),
        );
    }
    let cert = tls.cert.as_deref().ok_or_else(|| {
        "error[serve_tls]: manual TLS runtime requires a certificate path".to_string()
    })?;
    let key = tls
        .key
        .as_deref()
        .ok_or_else(|| "error[serve_tls]: manual TLS runtime requires a key path".to_string())?;
    let certificates = load_certificate_chain(&project_root.join(cert))?;
    let private_key = load_private_key(&project_root.join(key))?;
    let server_config = rustls_server_config(certificates, private_key)?;
    Ok(RuntimeTlsConfig {
        server_config: Arc::new(server_config),
    })
}

/// Builds an internal/local self-signed rustls configuration.
///
/// Inputs:
/// - `tls`: parsed internal `[server.tls]` configuration.
///
/// Output:
/// - Runtime TLS config with an in-memory self-signed certificate.
///
/// Transformation:
/// - Uses maintained `rcgen` certificate generation so local HTTPS serving can
///   run without user-managed certificate files or public ACME.
fn internal_runtime_tls_config(tls: &ProjectServerTls) -> Result<RuntimeTlsConfig, String> {
    let server_name = tls.server_name.as_deref().unwrap_or("localhost");
    let subject_alt_names = vec![server_name.to_string()];
    let generated = generate_simple_self_signed(subject_alt_names).map_err(|err| {
        format!("error[serve_tls]: failed to generate internal certificate: {err}")
    })?;
    let cert_der = generated.cert.der().as_ref().to_vec();
    let key_der = generated.key_pair.serialize_der();
    let server_config = rustls_server_config(
        vec![CertificateDer::from(cert_der)],
        PrivateKeyDer::from(PrivatePkcs8KeyDer::from(key_der)),
    )?;
    Ok(RuntimeTlsConfig {
        server_config: Arc::new(server_config),
    })
}

/// Builds an automatic ACME rustls configuration.
///
/// Inputs:
/// - `project_root`: directory containing `terlan.toml`.
/// - `tls`: parsed auto `[server.tls]` configuration.
///
/// Output:
/// - Runtime TLS config when the ACME certificate cache exists.
/// - Stable runtime diagnostic until ACME issuance creates that cache.
///
/// Transformation:
/// - Loads the deterministic project-local ACME cache into `rustls` when
///   present. If issuance has not populated the cache yet, the runtime fails
///   closed instead of silently serving plaintext.
#[cfg(test)]
fn acme_runtime_tls_config(
    project_root: &Path,
    tls: &ProjectServerTls,
) -> Result<RuntimeTlsConfig, String> {
    let plan = acme_runtime_plan(project_root, tls);
    validate_acme_provider_supported(&plan)?;
    if let Some(config) = load_acme_runtime_tls_cache(&plan)? {
        return Ok(config);
    }
    Err(acme_issuance_required_message(&plan, project_root))
}

/// Builds auto TLS configuration for serve startup, issuing when opted in.
///
/// Inputs:
/// - `project_root`: directory containing `terlan.toml`.
/// - `tls`: parsed auto `[server.tls]` configuration.
///
/// Output:
/// - Runtime TLS config from a valid local cache or newly issued certificate.
/// - Stable fail-closed diagnostic when cache is absent and live ACME is not
///   explicitly enabled.
///
/// Transformation:
/// - Reuses deterministic cache loading before creating any async runtime,
///   then calls the maintained `instant-acme` issuer only when the operator
///   explicitly opts into the public network path.
fn acme_runtime_tls_config_for_serve(
    project_root: &Path,
    tls: &ProjectServerTls,
) -> Result<RuntimeTlsConfig, String> {
    let plan = acme_runtime_plan(project_root, tls);
    validate_acme_provider_supported(&plan)?;
    if let Some(config) = load_acme_runtime_tls_cache(&plan)? {
        return Ok(config);
    }
    issue_acme_certificate_cache_for_serve(&plan)?;
    load_acme_runtime_tls_cache(&plan)?.ok_or_else(|| acme_issuer_missing_cache_message(&plan))
}

/// Issues an ACME certificate cache entry for serve startup.
///
/// Inputs:
/// - `plan`: normalized automatic TLS runtime plan.
///
/// Output:
/// - `Ok(())` after the live ACME issuer populates the cache.
/// - Stable runtime creation or issuer diagnostics otherwise.
///
/// Transformation:
/// - Runs the maintained async ACME client inside the temporary Tokio runtime
///   only when the compiler was built with the explicit `acme-live` feature,
///   after the VM ACME worker accepts the live lane request.
#[cfg(feature = "acme-live")]
fn issue_acme_certificate_cache_for_serve(plan: &AcmeRuntimePlan) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            format!("error[serve_tls]: failed to start temporary ACME runtime: {err}")
        })?;
    runtime.block_on(issue_acme_certificate_cache(plan))
}

/// Reports that live ACME support is not compiled into this binary.
///
/// Inputs:
/// - `_plan`: normalized automatic TLS runtime plan.
///
/// Output:
/// - Stable feature-required diagnostic.
///
/// Transformation:
/// - Keeps automatic TLS cache validation available in default builds while
///   making public-network ACME issuance an explicit build feature.
#[cfg(not(feature = "acme-live"))]
fn issue_acme_certificate_cache_for_serve(plan: &AcmeRuntimePlan) -> Result<(), String> {
    let mut worker_runtime = VmAcmeWorkerRuntime::new();
    start_live_acme_worker_for_serve(plan, &mut worker_runtime)?;
    Err(
        "error[serve_tls]: live ACME issuance requires a compiler build with the `acme-live` feature."
            .to_string(),
    )
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
fn load_acme_runtime_tls_cache(plan: &AcmeRuntimePlan) -> Result<Option<RuntimeTlsConfig>, String> {
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

/// Builds runtime TLS config after a deterministic local issuer handoff.
///
/// Inputs:
/// - `project_root`: directory containing `terlan.toml`.
/// - `tls`: parsed auto `[server.tls]` configuration.
/// - `issuer`: local issuer boundary that must populate the ACME cache.
///
/// Output:
/// - Runtime TLS config loaded from the issuer-populated cache.
/// - Stable `error[serve_tls]` diagnostic when the issuer fails or does not
///   write certificate material.
///
/// Transformation:
/// - Gives tests a local/mocked ACME boundary with the same cache handoff as
///   the production `instant-acme` path, without depending on public DNS or
///   Let's Encrypt network success.
#[cfg(test)]
fn acme_runtime_tls_config_with_local_issuer(
    project_root: &Path,
    tls: &ProjectServerTls,
    issuer: impl FnOnce(&AcmeRuntimePlan) -> Result<(), String>,
) -> Result<RuntimeTlsConfig, String> {
    let plan = acme_runtime_plan(project_root, tls);
    validate_acme_provider_supported(&plan)?;
    if let Some(config) = load_acme_runtime_tls_cache(&plan)? {
        return Ok(config);
    }
    issuer(&plan)?;
    load_acme_runtime_tls_cache(&plan)?.ok_or_else(|| acme_issuer_missing_cache_message(&plan))
}

/// Renders the diagnostic for issuer success without cache material.
///
/// Inputs:
/// - `plan`: normalized automatic TLS runtime plan.
///
/// Output:
/// - Stable `error[serve_tls]` diagnostic.
///
/// Transformation:
/// - Shares the same failure message between local mocked issuance tests and
///   live ACME startup.
fn acme_issuer_missing_cache_message(plan: &AcmeRuntimePlan) -> String {
    format!(
        "error[serve_tls]: ACME issuer completed without writing certificate cache `{}` and `{}`",
        plan.certificate_path.display(),
        plan.private_key_path.display()
    )
}

/// Renders the automatic TLS cache-miss diagnostic for deterministic tests.
///
/// Inputs:
/// - `plan`: normalized ACME runtime plan.
/// - `project_root`: directory containing `terlan.toml`.
///
/// Output:
/// - Stable `error[serve_tls]` diagnostic explaining which cache files are
///   missing and which provider would be used by live issuance.
///
/// Transformation:
/// - Keeps test-time cache-miss diagnostics linked to the VM ACME worker
///   boundary while preserving fail-closed behavior when no local certificate
///   cache exists.
#[cfg(test)]
fn acme_issuance_required_message(plan: &AcmeRuntimePlan, project_root: &Path) -> String {
    format!(
        "error[serve_tls]: automatic ACME TLS for domains [{}] has no local certificate cache yet; primary provider `{}` uses `{}` and cache `{}`; expected certificate `{}` and key `{}`; project `{}` should use mode `manual` or `internal` until issuance populates the cache",
        acme_domain_list(plan),
        tls_provider_name(plan.primary_provider),
        plan.directory_url,
        plan.cache_dir.display(),
        plan.certificate_path.display(),
        plan.private_key_path.display(),
        project_root.display()
    )
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

/// Issues an ACME certificate into the deterministic local cache.
///
/// Inputs:
/// - `plan`: normalized automatic TLS runtime plan.
///
/// Output:
/// - `Ok(())` when account credentials, HTTP-01 challenges, certificate chain,
///   and private key have been written into `.terlan/tls/acme`.
///
/// Transformation:
/// - Uses the maintained `instant-acme` client for account/order/challenge
///   protocol work, `rcgen` for CSR/key generation, the existing HTTP-01 cache
///   writer for challenge bodies, and the existing certificate-cache writer for
///   the final rustls handoff.
#[cfg(feature = "acme-live")]
async fn issue_acme_certificate_cache(plan: &AcmeRuntimePlan) -> Result<(), String> {
    issue_acme_certificate_cache_preflight(plan)?;
    let mut worker_runtime = VmAcmeWorkerRuntime::new();
    let worker = start_live_acme_worker_for_serve(plan, &mut worker_runtime)?;
    let account = load_or_create_acme_account(plan).await?;
    let identifiers = acme_domain_identifiers(&plan.domains)?;
    let mut order = account
        .new_order(&NewOrder {
            identifiers: &identifiers,
        })
        .await
        .map_err(acme_error("failed to create ACME order"))?;

    let authorizations = order
        .authorizations()
        .await
        .map_err(acme_error("failed to fetch ACME authorizations"))?;
    let pending_challenges = pending_http01_challenges(&authorizations)?;
    let mut challenge_urls = Vec::with_capacity(pending_challenges.len());
    for (index, selected) in pending_challenges.into_iter().enumerate() {
        let key_authorization = order.key_authorization(selected.challenge);
        store_acme_http01_challenge(plan, &selected.challenge.token, key_authorization.as_str())?;
        if index == 0 {
            worker_runtime
                .prepare_http01_challenge(
                    worker,
                    &selected.challenge.token,
                    key_authorization.as_str(),
                )
                .map_err(acme_worker_error)?;
        }
        challenge_urls.push(selected.challenge.url.clone());
    }
    worker_runtime
        .start_issuance(worker)
        .map_err(acme_worker_error)?;
    for challenge_url in challenge_urls {
        order
            .set_challenge_ready(&challenge_url)
            .await
            .map_err(acme_error("failed to mark ACME HTTP-01 challenge ready"))?;
    }

    wait_for_acme_order_ready(&mut order).await?;
    let (csr_der, private_key_pem) = generate_acme_csr(&plan.domains)?;
    order
        .finalize(&csr_der)
        .await
        .map_err(acme_error("failed to finalize ACME order"))?;
    let certificate_pem = wait_for_acme_certificate(&mut order).await?;
    worker_runtime
        .begin_cache_write(worker, unix_seconds(SystemTime::now())?.max(1))
        .map_err(acme_worker_error)?;
    store_acme_certificate_cache(plan, &certificate_pem, &private_key_pem)?;
    worker_runtime
        .complete_worker(worker)
        .map_err(acme_worker_error)?;
    Ok(())
}

/// Starts the VM-owned ACME worker used by serve auto TLS issuance.
///
/// Inputs:
/// - `plan`: normalized automatic TLS runtime plan.
/// - `worker_runtime`: VM ACME worker registry owned by the serve startup flow.
///
/// Output:
/// - VM worker handle for the live issuance lane.
///
/// Transformation:
/// - Converts serve TLS metadata into the same typed ACME worker request used
///   by deterministic fixtures, avoiding a separate environment-gated live
///   path.
fn start_live_acme_worker_for_serve(
    plan: &AcmeRuntimePlan,
    worker_runtime: &mut VmAcmeWorkerRuntime,
) -> Result<VmAcmeWorkerHandle, String> {
    let primary_domain = plan
        .domains
        .first()
        .map(String::as_str)
        .unwrap_or_default()
        .trim();
    let account_id = plan
        .email
        .as_deref()
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .unwrap_or("acme-account");
    let request = VmAcmeWorkerRequest::new(
        primary_domain,
        account_id,
        plan.cache_dir.display().to_string(),
        VmAcmeMode::Live,
    );
    worker_runtime
        .start_worker_for_lane(
            VmProcessId::system_runtime_worker(),
            request,
            VmAcmeWorkerExecutionLane::Live {
                directory_url: plan.directory_url.clone(),
            },
        )
        .map_err(acme_worker_error)
}

fn acme_worker_error(error: String) -> String {
    format!("error[serve_tls]: VM ACME worker rejected live issuance: {error}")
}

/// Loads or creates the ACME account for one runtime plan.
///
/// Inputs:
/// - `plan`: normalized automatic TLS runtime plan.
///
/// Output:
/// - Restored or newly-created `instant_acme::Account`.
///
/// Transformation:
/// - Reuses cached account credentials when present. Otherwise creates a new
///   account with Let's Encrypt terms accepted, then durably stores returned
///   credentials before the order flow proceeds.
#[cfg(feature = "acme-live")]
async fn load_or_create_acme_account(plan: &AcmeRuntimePlan) -> Result<Account, String> {
    if let Some(credentials) = load_acme_account_credentials(plan)? {
        return Account::from_credentials(credentials)
            .await
            .map_err(acme_error("failed to restore ACME account"));
    }
    let contact_strings = acme_contact_strings(plan.email.as_deref());
    let contact_refs: Vec<&str> = contact_strings.iter().map(String::as_str).collect();
    let new_account = NewAccount {
        contact: &contact_refs,
        terms_of_service_agreed: true,
        only_return_existing: false,
    };
    let (account, credentials) = Account::create(&new_account, &plan.directory_url, None)
        .await
        .map_err(acme_error("failed to create ACME account"))?;
    store_acme_account_credentials(plan, &credentials)?;
    Ok(account)
}

/// Converts configured domains to ACME DNS identifiers.
///
/// Inputs:
/// - `domains`: configured auto-TLS domain names.
///
/// Output:
/// - Non-empty ACME DNS identifier list.
///
/// Transformation:
/// - Rejects empty or whitespace-only names before they reach the ACME client
///   and otherwise preserves domain spelling for the CA.
#[cfg(any(feature = "acme-live", test))]
fn acme_domain_identifiers(domains: &[String]) -> Result<Vec<Identifier>, String> {
    if domains.is_empty() {
        return Err(
            "error[serve_tls]: automatic ACME TLS requires at least one domain".to_string(),
        );
    }
    domains
        .iter()
        .map(|domain| {
            let domain = domain.trim();
            if domain.is_empty() {
                Err("error[serve_tls]: automatic ACME TLS domain cannot be empty".to_string())
            } else {
                Ok(Identifier::Dns(domain.to_string()))
            }
        })
        .collect()
}

/// Builds ACME contact URIs from optional manifest email.
///
/// Inputs:
/// - `email`: optional manifest email address.
///
/// Output:
/// - Empty contact list or one `mailto:` contact URI.
///
/// Transformation:
/// - Keeps account creation compatible with ACME contact URI requirements while
///   leaving email validation to the CA.
#[cfg(any(feature = "acme-live", test))]
fn acme_contact_strings(email: Option<&str>) -> Vec<String> {
    email
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .map(|email| format!("mailto:{email}"))
        .into_iter()
        .collect()
}

/// Selects pending HTTP-01 challenges from ACME authorizations.
///
/// Inputs:
/// - `authorizations`: ACME authorization records returned by `instant-acme`.
///
/// Output:
/// - Pending HTTP-01 challenge references for each authorization that still
///   requires validation.
///
/// Transformation:
/// - Skips already-valid authorizations, rejects invalid terminal states, and
///   requires HTTP-01 availability so Terlan's automatic TLS mode remains tied
///   to the challenge route it knows how to serve.
#[cfg(any(feature = "acme-live", test))]
fn pending_http01_challenges(
    authorizations: &[Authorization],
) -> Result<Vec<PendingHttp01Challenge<'_>>, String> {
    let mut selected = Vec::new();
    for authorization in authorizations {
        let Identifier::Dns(identifier) = &authorization.identifier;
        match authorization.status {
            AuthorizationStatus::Valid => continue,
            AuthorizationStatus::Pending => {
                let challenge = authorization
                    .challenges
                    .iter()
                    .find(|challenge| challenge.r#type == ChallengeType::Http01)
                    .ok_or_else(|| {
                        format!(
                            "error[serve_tls]: ACME authorization for `{identifier}` did not offer HTTP-01"
                        )
                    })?;
                selected.push(PendingHttp01Challenge { challenge });
            }
            status => {
                return Err(format!(
                    "error[serve_tls]: ACME authorization for `{identifier}` is not usable: {status:?}"
                ));
            }
        }
    }
    Ok(selected)
}

/// Waits for an ACME order to become ready.
///
/// Inputs:
/// - `order`: in-flight ACME order after challenges were marked ready.
///
/// Output:
/// - `Ok(())` when the order reaches `ready`.
///
/// Transformation:
/// - Polls the CA with bounded exponential backoff and converts timeout or
///   invalid states to stable TLS diagnostics.
#[cfg(feature = "acme-live")]
async fn wait_for_acme_order_ready(order: &mut instant_acme::Order) -> Result<(), String> {
    let mut delay = ACME_READY_INITIAL_DELAY;
    for attempt in 0..ACME_READY_MAX_POLLS {
        match order.state().status {
            OrderStatus::Ready => return Ok(()),
            OrderStatus::Invalid => {
                return Err("error[serve_tls]: ACME order became invalid".to_string());
            }
            _ => {}
        }
        std::thread::sleep(delay);
        let state = order
            .refresh()
            .await
            .map_err(acme_error("failed to refresh ACME order"))?;
        if state.status == OrderStatus::Ready {
            return Ok(());
        }
        delay *= 2;
        if attempt + 1 == ACME_READY_MAX_POLLS {
            return Err(format!(
                "error[serve_tls]: ACME order did not become ready after {} polls; last status: {:?}",
                ACME_READY_MAX_POLLS, state.status
            ));
        }
    }
    Err("error[serve_tls]: ACME order did not become ready".to_string())
}

/// Generates CSR bytes and private key PEM for an ACME certificate.
///
/// Inputs:
/// - `domains`: domain names requested in the ACME order.
///
/// Output:
/// - DER-encoded CSR and PEM-encoded private key.
///
/// Transformation:
/// - Delegates certificate request and key generation to `rcgen`, using the
///   same subject alternative names as the ACME order identifiers.
#[cfg(any(feature = "acme-live", test))]
fn generate_acme_csr(domains: &[String]) -> Result<(Vec<u8>, String), String> {
    let mut params = CertificateParams::new(domains.to_vec())
        .map_err(|err| format!("error[serve_tls]: failed to create ACME CSR parameters: {err}"))?;
    params.distinguished_name = DistinguishedName::new();
    let private_key = KeyPair::generate()
        .map_err(|err| format!("error[serve_tls]: failed to generate ACME private key: {err}"))?;
    let csr = params
        .serialize_request(&private_key)
        .map_err(|err| format!("error[serve_tls]: failed to serialize ACME CSR: {err}"))?;
    Ok((csr.der().as_ref().to_vec(), private_key.serialize_pem()))
}
