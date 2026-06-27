use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use instant_acme::{
    Account, AccountCredentials, Authorization, AuthorizationStatus, Challenge, ChallengeType,
    Identifier, NewAccount, NewOrder, OrderStatus,
};
use rcgen::{generate_simple_self_signed, CertificateParams, DistinguishedName, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;
use tokio::time::sleep;

use crate::commands::build::project_manifest::{
    ProjectServerTls, ProjectServerTlsMode, ProjectServerTlsProvider,
};

use super::manifest::web_package_tls_config;

/// URL path prefix reserved by ACME HTTP-01.
const ACME_HTTP01_PATH_PREFIX: &str = "/.well-known/acme-challenge/";

/// Directory under the ACME cache that stores HTTP-01 challenge bodies.
const ACME_HTTP01_CACHE_DIR: &str = "http-01";

/// File under the ACME cache that stores reusable account credentials.
const ACME_ACCOUNT_CREDENTIALS_FILE: &str = "account.json";

/// Maximum order-state refresh attempts while waiting for ACME readiness.
const ACME_READY_MAX_POLLS: u8 = 5;

/// Initial delay between ACME order-state refresh attempts.
const ACME_READY_INITIAL_DELAY: Duration = Duration::from_millis(250);

/// Delay between ACME certificate fetch attempts after finalization.
const ACME_CERTIFICATE_POLL_DELAY: Duration = Duration::from_secs(1);

/// Maximum certificate fetch attempts after ACME finalization.
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
pub(super) struct RuntimeTlsConfig {
    pub(super) server_config: Arc<ServerConfig>,
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
pub(super) enum AcmeHttp01Challenge {
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
///   helpers, and constructs the `rustls::ServerConfig` consumed by the Tokio
///   accept loop.
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
fn acme_runtime_tls_config(
    project_root: &Path,
    tls: &ProjectServerTls,
) -> Result<RuntimeTlsConfig, String> {
    let plan = acme_runtime_plan(project_root, tls);
    if plan.certificate_path.is_file() && plan.private_key_path.is_file() {
        let certificates = load_certificate_chain(&plan.certificate_path)?;
        let private_key = load_private_key(&plan.private_key_path)?;
        let server_config = rustls_server_config(certificates, private_key)?;
        return Ok(RuntimeTlsConfig {
            server_config: Arc::new(server_config),
        });
    }
    Err(acme_issuance_required_message(&plan, project_root))
}

/// Renders the automatic TLS cache-miss diagnostic.
///
/// Inputs:
/// - `plan`: normalized ACME runtime plan.
/// - `project_root`: directory containing `terlan.toml`.
///
/// Output:
/// - Stable `error[serve_tls]` diagnostic explaining which cache files are
///   missing and which provider will be used by live issuance.
///
/// Transformation:
/// - Keeps the not-yet-orchestrated live issuance boundary linked to the auto
///   TLS path while preserving the current fail-closed behavior when no local
///   certificate cache exists.
fn acme_issuance_required_message(plan: &AcmeRuntimePlan, project_root: &Path) -> String {
    let _live_issuer = issue_acme_certificate_cache;
    let domains = if plan.domains.is_empty() {
        "<none>".to_string()
    } else {
        plan.domains.join(", ")
    };
    format!(
        "error[serve_tls]: automatic ACME TLS for domains [{domains}] has no local certificate cache yet; primary provider `{}` uses `{}` and cache `{}`; expected certificate `{}` and key `{}`; project `{}` should use mode `manual` or `internal` until issuance populates the cache",
        tls_provider_name(plan.primary_provider),
        plan.directory_url,
        plan.cache_dir.display(),
        plan.certificate_path.display(),
        plan.private_key_path.display(),
        project_root.display()
    )
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
async fn issue_acme_certificate_cache(plan: &AcmeRuntimePlan) -> Result<(), String> {
    if plan.primary_provider == ProjectServerTlsProvider::ZeroSsl {
        return Err(
            "error[serve_tls]: ZeroSSL automatic issuance requires external account binding support"
                .to_string(),
        );
    }
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
    for selected in pending_challenges {
        let key_authorization = order.key_authorization(selected.challenge);
        store_acme_http01_challenge(plan, &selected.challenge.token, key_authorization.as_str())?;
        challenge_urls.push(selected.challenge.url.clone());
    }
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
    store_acme_certificate_cache(plan, &certificate_pem, &private_key_pem)
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
        sleep(delay).await;
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
async fn wait_for_acme_certificate(order: &mut instant_acme::Order) -> Result<String, String> {
    for attempt in 0..ACME_CERTIFICATE_MAX_POLLS {
        match order
            .certificate()
            .await
            .map_err(acme_error("failed to fetch ACME certificate"))?
        {
            Some(certificate_pem) => return Ok(certificate_pem),
            None if attempt + 1 < ACME_CERTIFICATE_MAX_POLLS => {
                sleep(ACME_CERTIFICATE_POLL_DELAY).await;
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
fn acme_error(context: &'static str) -> impl FnOnce(instant_acme::Error) -> String {
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
fn acme_runtime_plan(project_root: &Path, tls: &ProjectServerTls) -> AcmeRuntimePlan {
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
        http01_challenge_dir: cache_dir.join(ACME_HTTP01_CACHE_DIR),
        cache_dir,
    }
}

/// Loads cached ACME account credentials.
///
/// Inputs:
/// - `plan`: normalized ACME runtime plan.
///
/// Output:
/// - `Ok(None)` when no account cache exists.
/// - `Ok(Some(AccountCredentials))` when the account cache is valid JSON.
/// - Stable `error[serve_tls]` diagnostic when the cache cannot be read or
///   decoded.
///
/// Transformation:
/// - Deserializes the opaque `instant-acme` account credential payload from
///   the deterministic project-local cache file without interpreting its
///   private fields.
fn load_acme_account_credentials(
    plan: &AcmeRuntimePlan,
) -> Result<Option<AccountCredentials>, String> {
    let contents = match fs::read_to_string(&plan.account_credentials_path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(format!(
                "error[serve_tls]: failed to read ACME account credentials `{}`: {err}",
                plan.account_credentials_path.display()
            ));
        }
    };
    serde_json::from_str::<AccountCredentials>(&contents)
        .map(Some)
        .map_err(|err| {
            format!(
                "error[serve_tls]: failed to parse ACME account credentials `{}`: {err}",
                plan.account_credentials_path.display()
            )
        })
}

/// Stores ACME account credentials.
///
/// Inputs:
/// - `plan`: normalized ACME runtime plan.
/// - `credentials`: opaque credentials returned by `instant-acme`.
///
/// Output:
/// - `Ok(())` when credentials are durably written to the project cache.
///
/// Transformation:
/// - Serializes through `serde_json` and writes through a temporary file before
///   renaming into place, so interrupted issuance does not leave partial
///   account JSON at the runtime path.
fn store_acme_account_credentials(
    plan: &AcmeRuntimePlan,
    credentials: &AccountCredentials,
) -> Result<(), String> {
    let contents = serde_json::to_string_pretty(credentials).map_err(|err| {
        format!("error[serve_tls]: failed to serialize ACME account credentials: {err}")
    })?;
    write_cache_file_atomically(&plan.account_credentials_path, contents.as_bytes())
}

/// Stores one ACME HTTP-01 challenge response.
///
/// Inputs:
/// - `plan`: normalized ACME runtime plan.
/// - `token`: ACME HTTP-01 challenge token.
/// - `key_authorization`: ACME key authorization body generated by
///   `instant-acme`.
///
/// Output:
/// - Path to the written challenge response file.
///
/// Transformation:
/// - Validates the token against the same URL-safe filename policy used by the
///   request handler, then writes the response under
///   `.terlan/tls/acme/http-01`.
fn store_acme_http01_challenge(
    plan: &AcmeRuntimePlan,
    token: &str,
    key_authorization: &str,
) -> Result<PathBuf, String> {
    if !is_acme_http01_token(token) {
        return Err(format!(
            "error[serve_tls]: ACME HTTP-01 token `{token}` is invalid"
        ));
    }
    let path = plan.http01_challenge_dir.join(token);
    write_cache_file_atomically(&path, key_authorization.as_bytes())?;
    Ok(path)
}

/// Stores issued ACME certificate material.
///
/// Inputs:
/// - `plan`: normalized ACME runtime plan.
/// - `certificate_pem`: issued certificate chain PEM.
/// - `private_key_pem`: private key PEM used by the CSR.
///
/// Output:
/// - `Ok(())` when both runtime cache files are written and parseable by
///   `rustls`.
///
/// Transformation:
/// - Writes certificate/key to temporary paths, validates them through the same
///   PEM/rustls path used by serving, then atomically renames them into the
///   deterministic ACME cache.
fn store_acme_certificate_cache(
    plan: &AcmeRuntimePlan,
    certificate_pem: &str,
    private_key_pem: &str,
) -> Result<(), String> {
    let cert_temp = temporary_cache_path(&plan.certificate_path);
    let key_temp = temporary_cache_path(&plan.private_key_path);
    write_cache_file_atomically(&cert_temp, certificate_pem.as_bytes())?;
    write_cache_file_atomically(&key_temp, private_key_pem.as_bytes())?;
    let certificates = load_certificate_chain(&cert_temp);
    let private_key = load_private_key(&key_temp);
    match (certificates, private_key) {
        (Ok(_), Ok(_)) => {
            rename_cache_file(&cert_temp, &plan.certificate_path)?;
            rename_cache_file(&key_temp, &plan.private_key_path)?;
            Ok(())
        }
        (Err(message), _) | (_, Err(message)) => {
            let _ = fs::remove_file(&cert_temp);
            let _ = fs::remove_file(&key_temp);
            Err(message)
        }
    }
}

/// Writes one cache file through a temporary path.
///
/// Inputs:
/// - `path`: final cache file path.
/// - `contents`: bytes to write.
///
/// Output:
/// - `Ok(())` when the file exists at `path`.
///
/// Transformation:
/// - Creates the parent directory, writes sibling temporary content, and then
///   renames over the final path to avoid partial cache files.
fn write_cache_file_atomically(path: &Path, contents: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "error[serve_tls]: failed to create ACME cache directory `{}`: {err}",
                parent.display()
            )
        })?;
    }
    let temp_path = temporary_cache_path(path);
    fs::write(&temp_path, contents).map_err(|err| {
        format!(
            "error[serve_tls]: failed to write ACME cache file `{}`: {err}",
            temp_path.display()
        )
    })?;
    rename_cache_file(&temp_path, path)
}

/// Builds a sibling temporary path for one ACME cache file.
///
/// Inputs:
/// - `path`: final cache path.
///
/// Output:
/// - Temporary path with a process-local suffix.
///
/// Transformation:
/// - Keeps temporary files next to the destination so rename stays on the same
///   filesystem.
fn temporary_cache_path(path: &Path) -> PathBuf {
    let suffix = format!("tmp-{}", std::process::id());
    let extension = path.extension().and_then(|extension| extension.to_str());
    let temporary_extension = extension
        .map(|extension| format!("{extension}.{suffix}"))
        .unwrap_or(suffix);
    path.with_extension(temporary_extension)
}

/// Renames a temporary ACME cache file into place.
///
/// Inputs:
/// - `source`: temporary file path.
/// - `target`: final cache path.
///
/// Output:
/// - `Ok(())` when the target has replaced any previous cache file.
///
/// Transformation:
/// - Delegates to `std::fs::rename` and converts IO errors to stable serve TLS
///   diagnostics.
fn rename_cache_file(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target).map_err(|err| {
        format!(
            "error[serve_tls]: failed to move ACME cache file `{}` to `{}`: {err}",
            source.display(),
            target.display()
        )
    })
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
pub(super) fn acme_http01_challenge(
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
fn is_acme_http01_token(token: &str) -> bool {
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
fn acme_directory_url(provider: ProjectServerTlsProvider) -> &'static str {
    match provider {
        ProjectServerTlsProvider::LetsEncrypt => instant_acme::LetsEncrypt::Production.url(),
        ProjectServerTlsProvider::ZeroSsl => "https://acme.zerossl.com/v2/DV90",
    }
}

/// Returns the manifest spelling for an ACME provider.
///
/// Inputs:
/// - `provider`: parsed ACME provider.
///
/// Output:
/// - Stable lower-case provider name for diagnostics and tests.
///
/// Transformation:
/// - Keeps runtime diagnostics aligned with `terlan.toml` provider spelling.
fn tls_provider_name(provider: ProjectServerTlsProvider) -> &'static str {
    match provider {
        ProjectServerTlsProvider::LetsEncrypt => "letsencrypt",
        ProjectServerTlsProvider::ZeroSsl => "zerossl",
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
fn load_certificate_chain(path: &Path) -> Result<Vec<CertificateDer<'static>>, String> {
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
fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, String> {
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
fn rustls_server_config(
    certificates: Vec<CertificateDer<'static>>,
    private_key: PrivateKeyDer<'static>,
) -> Result<ServerConfig, String> {
    ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|err| format!("error[serve_tls]: failed to select TLS protocol versions: {err}"))?
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .map_err(|err| format!("error[serve_tls]: failed to build TLS server config: {err}"))
}

/// Resolves a project-relative TLS path.
///
/// Inputs:
/// - `project_root`: directory containing `terlan.toml`.
/// - `value`: project-relative manifest path.
///
/// Output:
/// - Joined path used by runtime certificate loading.
///
/// Transformation:
/// - Exists primarily for tests and future stricter normalization; manifest
///   validation already rejects absolute or escaping paths before runtime load.
#[allow(dead_code)]
fn project_tls_path(project_root: &Path, value: &str) -> PathBuf {
    project_root.join(value)
}

#[cfg(test)]
#[path = "tls_test.rs"]
mod tls_test;
