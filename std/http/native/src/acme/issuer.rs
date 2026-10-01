//! Optional maintained ACME issuer. The caller owns scheduling and worker lifecycle.
use super::cache::{
    load_acme_account_credentials, store_acme_account_credentials, store_acme_certificate_cache,
    store_acme_http01_challenge,
};
use super::{validate_acme_provider_supported, AcmeRuntimePlan};
use instant_acme::{
    Account, Authorization, AuthorizationStatus, Challenge, ChallengeType, Identifier, NewAccount,
    NewOrder, OrderStatus,
};
use rcgen::{CertificateParams, DistinguishedName, KeyPair};
use std::{future::Future, time::Duration};

const ACME_READY_MAX_POLLS: u8 = 5;
const ACME_READY_INITIAL_DELAY: Duration = Duration::from_millis(250);
const ACME_CERTIFICATE_POLL_DELAY: Duration = Duration::from_secs(1);
const ACME_CERTIFICATE_MAX_POLLS: u8 = 10;

#[derive(Debug, PartialEq, Eq)]
pub enum IssuanceEvent<'a> {
    ChallengePrepared {
        token: &'a str,
        key_authorization: &'a str,
    },
    Issuing,
    WritingCache,
    Complete,
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
pub struct PendingHttp01Challenge<'a> {
    pub challenge: &'a Challenge,
}

/// Executes maintained ACME protocol work without selecting an executor or VM worker.
/// Dropping the future stops further protocol progress; challenge cache cleanup
/// remains a separate lifecycle responsibility.
pub async fn issue_certificate_cache<F: Future<Output = ()>>(
    plan: &AcmeRuntimePlan,
    http: Option<Box<dyn instant_acme::HttpClient>>,
    mut observe: impl FnMut(IssuanceEvent<'_>) -> Result<(), String>,
    mut delay_for: impl FnMut(Duration) -> F,
) -> Result<(), String> {
    validate_acme_provider_supported(plan)?;
    let identifiers = acme_domain_identifiers(&plan.domains)?;
    super::cache::validate_acme_cache_paths(plan)?;
    let account = load_or_create_acme_account(plan, http).await?;
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
        observe(IssuanceEvent::ChallengePrepared {
            token: &selected.challenge.token,
            key_authorization: key_authorization.as_str(),
        })?;
        challenge_urls.push(selected.challenge.url.clone());
    }
    observe(IssuanceEvent::Issuing)?;
    for challenge_url in challenge_urls {
        order
            .set_challenge_ready(&challenge_url)
            .await
            .map_err(acme_error("failed to mark ACME HTTP-01 challenge ready"))?;
    }

    wait_for_acme_order_ready(&mut order, &mut delay_for).await?;
    let (csr_der, private_key_pem) = generate_acme_csr(&plan.domains)?;
    order
        .finalize(&csr_der)
        .await
        .map_err(acme_error("failed to finalize ACME order"))?;
    let certificate_pem = wait_for_acme_certificate(&mut order, &mut delay_for).await?;
    observe(IssuanceEvent::WritingCache)?;
    store_acme_certificate_cache(plan, &certificate_pem, &private_key_pem)?;
    observe(IssuanceEvent::Complete)
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
async fn load_or_create_acme_account(
    plan: &AcmeRuntimePlan,
    http: Option<Box<dyn instant_acme::HttpClient>>,
) -> Result<Account, String> {
    if let Some(credentials) = load_acme_account_credentials(plan)? {
        return match http {
            Some(http) => Account::from_credentials_and_http(credentials, http).await,
            None => Account::from_credentials(credentials).await,
        }
        .map_err(acme_error("failed to restore ACME account"));
    }
    let contact_strings = acme_contact_strings(plan.email.as_deref());
    let contact_refs: Vec<&str> = contact_strings.iter().map(String::as_str).collect();
    let new_account = NewAccount {
        contact: &contact_refs,
        terms_of_service_agreed: true,
        only_return_existing: false,
    };
    let (account, credentials) = match http {
        Some(http) => {
            Account::create_with_http(&new_account, &plan.directory_url, None, http).await
        }
        None => Account::create(&new_account, &plan.directory_url, None).await,
    }
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
pub fn acme_domain_identifiers(domains: &[String]) -> Result<Vec<Identifier>, String> {
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
pub fn acme_contact_strings(email: Option<&str>) -> Vec<String> {
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
pub fn pending_http01_challenges(
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
pub fn generate_acme_csr(domains: &[String]) -> Result<(Vec<u8>, String), String> {
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
async fn wait_for_acme_order_ready<F: Future<Output = ()>>(
    order: &mut instant_acme::Order,
    delay_for: &mut impl FnMut(Duration) -> F,
) -> Result<(), String> {
    let mut delay = ACME_READY_INITIAL_DELAY;
    let mut polls = 0;
    loop {
        match order.state().status {
            OrderStatus::Ready => return Ok(()),
            OrderStatus::Invalid => {
                return Err("error[serve_tls]: ACME order became invalid".to_string());
            }
            _ => {}
        }
        if polls == ACME_READY_MAX_POLLS {
            return Err(format!(
                "error[serve_tls]: ACME order did not become ready after {} polls; last status: {:?}",
                ACME_READY_MAX_POLLS, order.state().status
            ));
        }
        delay_for(delay).await;
        order
            .refresh()
            .await
            .map_err(acme_error("failed to refresh ACME order"))?;
        polls += 1;
        delay *= 2;
    }
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
async fn wait_for_acme_certificate<F: Future<Output = ()>>(
    order: &mut instant_acme::Order,
    delay_for: &mut impl FnMut(Duration) -> F,
) -> Result<String, String> {
    for attempt in 0..ACME_CERTIFICATE_MAX_POLLS {
        match order
            .certificate()
            .await
            .map_err(acme_error("failed to fetch ACME certificate"))?
        {
            Some(certificate_pem) => return Ok(certificate_pem),
            None if attempt + 1 < ACME_CERTIFICATE_MAX_POLLS => {
                delay_for(ACME_CERTIFICATE_POLL_DELAY).await;
            }
            None => {}
        }
    }
    Err(format!(
        "error[serve_tls]: ACME certificate was not available after {} polls",
        ACME_CERTIFICATE_MAX_POLLS
    ))
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

#[cfg(test)]
#[path = "issuer_test.rs"]
mod tests;
