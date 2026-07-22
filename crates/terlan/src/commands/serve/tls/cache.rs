use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
#[cfg(any(feature = "acme-live", test))]
use std::time::SystemTime;

#[cfg(any(feature = "acme-live", test))]
use instant_acme::AccountCredentials;
use serde::{Deserialize, Serialize};

use super::AcmeRuntimePlan;
#[cfg(any(feature = "acme-live", test))]
use super::{is_acme_http01_token, unix_seconds, ACME_RENEWAL_INTERVAL};
#[cfg(any(feature = "acme-live", test))]
use super::{load_certificate_chain, load_private_key};

#[cfg(any(feature = "acme-live", test))]
const ACME_CERTIFICATE_CACHE_SCHEMA_VERSION: u32 = 1;
#[cfg(any(feature = "acme-live", test))]
const ACME_CERTIFICATE_CACHE_FORMAT_VERSION: u32 = 1;
#[cfg(any(feature = "acme-live", test))]
const ACME_CERTIFICATE_CACHE_KEY_ALGORITHM: &str = "ecdsa-p256";
#[cfg(any(feature = "acme-live", test))]
const ACME_CERTIFICATE_CACHE_CHALLENGE_METHOD: &str = "http-01";
#[cfg(any(feature = "acme-live", test))]
const ACME_CERTIFICATE_CACHE_ISSUING_WORKER: &str = "vm-acme-worker";
const ACME_CERTIFICATE_CACHE_MODE_LIVE: &str = "live";
const ACME_CERTIFICATE_CACHE_MODE_STAGING: &str = "staging";

/// ACME certificate cache renewal and provenance metadata.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub(super) struct AcmeCertificateCacheMetadata {
    pub(super) schema_version: u32,
    pub(super) cache_format_version: u32,
    pub(super) domains: Vec<String>,
    pub(super) subject_alternative_names: Vec<String>,
    pub(super) issuer: String,
    pub(super) account_id: String,
    pub(super) key_algorithm: String,
    pub(super) challenge_method: String,
    pub(super) acme_mode: String,
    pub(super) not_before_unix_seconds: u64,
    pub(super) not_after_unix_seconds: u64,
    pub(super) issued_at_unix_seconds: u64,
    pub(super) renew_after_unix_seconds: u64,
    pub(super) issuing_worker_identity: String,
    pub(super) provenance_hash: String,
}

/// Redacted ACME cache details safe for support-bundle replay.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AcmeCacheSupportBundleRedaction {
    pub(super) cache_dir: String,
    pub(super) provenance_fingerprint: String,
    pub(super) diagnostic: String,
}

/// VM-owned ACME key custody policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AcmeKeyCustodyPolicy {
    pub(super) cache_dir: PathBuf,
    pub(super) certificate_path: PathBuf,
    pub(super) private_key_path: PathBuf,
    pub(super) renewal_metadata_path: PathBuf,
    pub(super) account_credentials_path: PathBuf,
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
#[cfg(any(feature = "acme-live", test))]
pub(super) fn load_acme_account_credentials(
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
#[cfg(any(feature = "acme-live", test))]
pub(super) fn store_acme_account_credentials(
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
#[cfg(any(feature = "acme-live", test))]
pub(super) fn store_acme_http01_challenge(
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
#[cfg(any(feature = "acme-live", test))]
pub(super) fn store_acme_certificate_cache(
    plan: &AcmeRuntimePlan,
    certificate_pem: &str,
    private_key_pem: &str,
) -> Result<(), String> {
    let cert_temp = temporary_cache_path(&plan.certificate_path);
    let key_temp = temporary_cache_path(&plan.private_key_path);
    write_cache_file_atomically(&cert_temp, certificate_pem.as_bytes())?;
    write_cache_file_atomically(&key_temp, private_key_pem.as_bytes())?;
    restrict_private_key_file_permissions(&key_temp)?;
    let certificates = load_certificate_chain(&cert_temp);
    let private_key = load_private_key(&key_temp);
    match (certificates, private_key) {
        (Ok(_), Ok(_)) => {
            rename_cache_file(&cert_temp, &plan.certificate_path)?;
            rename_cache_file(&key_temp, &plan.private_key_path)?;
            store_acme_certificate_cache_metadata(plan, SystemTime::now())?;
            Ok(())
        }
        (Err(message), _) | (_, Err(message)) => {
            let _ = fs::remove_file(&cert_temp);
            let _ = fs::remove_file(&key_temp);
            Err(message)
        }
    }
}

/// Validates cached ACME mode provenance against the configured endpoint.
pub(super) fn validate_acme_certificate_cache_mode(
    plan: &AcmeRuntimePlan,
    metadata: &AcmeCertificateCacheMetadata,
) -> Result<(), String> {
    let expected = acme_metadata_mode(plan);
    if metadata.acme_mode == expected {
        Ok(())
    } else {
        Err(format!(
            "error[serve_tls]: ACME certificate cache metadata `{}` was issued for mode `{}` but runtime requires `{expected}`",
            plan.renewal_metadata_path.display(),
            metadata.acme_mode
        ))
    }
}

/// Validates the stored ACME cache provenance hash.
pub(super) fn validate_acme_certificate_cache_provenance_hash(
    plan: &AcmeRuntimePlan,
    metadata: &AcmeCertificateCacheMetadata,
) -> Result<(), String> {
    let expected = acme_cache_provenance_fingerprint(metadata);
    if metadata.provenance_hash == expected {
        Ok(())
    } else {
        Err(format!(
            "error[serve_tls]: ACME certificate cache metadata `{}` failed provenance hash validation",
            plan.renewal_metadata_path.display()
        ))
    }
}

/// Validates VM-owned ACME cache custody before TLS startup.
pub(super) fn validate_acme_key_custody_policy(plan: &AcmeRuntimePlan) -> Result<(), String> {
    let policy = AcmeKeyCustodyPolicy {
        cache_dir: plan.cache_dir.clone(),
        certificate_path: plan.certificate_path.clone(),
        private_key_path: plan.private_key_path.clone(),
        renewal_metadata_path: plan.renewal_metadata_path.clone(),
        account_credentials_path: plan.account_credentials_path.clone(),
    };
    validate_acme_cache_path("certificate", &policy.cache_dir, &policy.certificate_path)?;
    validate_acme_cache_path("private key", &policy.cache_dir, &policy.private_key_path)?;
    validate_acme_cache_path(
        "renewal metadata",
        &policy.cache_dir,
        &policy.renewal_metadata_path,
    )?;
    validate_acme_cache_path(
        "account credentials",
        &policy.cache_dir,
        &policy.account_credentials_path,
    )?;
    validate_private_key_cache_permissions(&policy.private_key_path)
}

/// Builds ACME cache metadata safe for support bundles.
///
/// Inputs:
/// - `plan`: normalized ACME runtime plan.
/// - `metadata`: typed cache provenance metadata.
/// - `diagnostic`: runtime diagnostic that may contain sensitive cache data.
///
/// Output:
/// - Redacted cache directory, stable provenance fingerprint, and sanitized
///   diagnostic text.
///
/// Transformation:
/// - Keeps support bundles useful for replay while preventing account ids,
///   cache keys, or private-key PEM markers from leaving the VM-owned ACME
///   cache boundary.
#[cfg(test)]
pub(super) fn redact_acme_cache_support_bundle(
    plan: &AcmeRuntimePlan,
    metadata: &AcmeCertificateCacheMetadata,
    diagnostic: &str,
) -> AcmeCacheSupportBundleRedaction {
    AcmeCacheSupportBundleRedaction {
        cache_dir: plan.cache_dir.display().to_string(),
        provenance_fingerprint: acme_cache_provenance_fingerprint(metadata),
        diagnostic: redact_acme_cache_support_bundle_diagnostic(metadata, diagnostic),
    }
}

/// Restricts a private key cache file to owner-only access where supported.
#[cfg(any(feature = "acme-live", test))]
pub(super) fn restrict_private_key_file_permissions(path: &Path) -> Result<(), String> {
    restrict_private_key_file_permissions_impl(path)
}

#[cfg(all(unix, any(feature = "acme-live", test)))]
fn restrict_private_key_file_permissions_impl(path: &Path) -> Result<(), String> {
    let mut permissions = fs::metadata(path)
        .map_err(|err| {
            format!(
                "error[serve_tls]: failed to inspect ACME private key cache `{}`: {err}",
                path.display()
            )
        })?
        .permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions).map_err(|err| {
        format!(
            "error[serve_tls]: failed to restrict ACME private key cache `{}`: {err}",
            path.display()
        )
    })
}

#[cfg(all(not(unix), any(feature = "acme-live", test)))]
fn restrict_private_key_file_permissions_impl(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Validates a private key cache file is not group/world accessible.
pub(super) fn validate_private_key_cache_permissions(path: &Path) -> Result<(), String> {
    validate_private_key_cache_permissions_impl(path)
}

#[cfg(unix)]
fn validate_private_key_cache_permissions_impl(path: &Path) -> Result<(), String> {
    let mode = fs::metadata(path)
        .map_err(|err| {
            format!(
                "error[serve_tls]: failed to inspect ACME private key cache `{}`: {err}",
                path.display()
            )
        })?
        .permissions()
        .mode()
        & 0o777;
    if mode & 0o077 == 0 {
        Ok(())
    } else {
        Err(format!(
            "error[serve_tls]: ACME private key cache `{}` must not be group/world accessible; mode {:03o}",
            path.display(),
            mode
        ))
    }
}

#[cfg(not(unix))]
fn validate_private_key_cache_permissions_impl(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Loads ACME certificate cache renewal metadata.
///
/// Inputs:
/// - `plan`: normalized ACME runtime plan.
///
/// Output:
/// - Parsed renewal metadata.
/// - Stable diagnostics for missing or malformed metadata.
///
/// Transformation:
/// - Keeps auto TLS cache age validation tied to a deterministic JSON file
///   beside the issued certificate material.
pub(super) fn load_acme_certificate_cache_metadata(
    plan: &AcmeRuntimePlan,
) -> Result<AcmeCertificateCacheMetadata, String> {
    let contents = fs::read_to_string(&plan.renewal_metadata_path).map_err(|err| {
        format!(
            "error[serve_tls]: failed to read ACME certificate cache metadata `{}`: {err}",
            plan.renewal_metadata_path.display()
        )
    })?;
    serde_json::from_str(&contents).map_err(|err| {
        format!(
            "error[serve_tls]: failed to parse ACME certificate cache metadata `{}`: {err}",
            plan.renewal_metadata_path.display()
        )
    })
}

/// Stores ACME certificate cache renewal metadata.
///
/// Inputs:
/// - `plan`: normalized ACME runtime plan.
/// - `issued_at`: issuance time recorded by the runtime.
///
/// Output:
/// - `Ok(())` when renewal metadata is durably written.
///
/// Transformation:
/// - Writes JSON through the same atomic cache-file path as certificate
///   material so runtime cache loading can fail closed when metadata is stale
///   or missing.
#[cfg(any(feature = "acme-live", test))]
pub(super) fn store_acme_certificate_cache_metadata(
    plan: &AcmeRuntimePlan,
    issued_at: SystemTime,
) -> Result<(), String> {
    let issued_at_unix_seconds = unix_seconds(issued_at)?;
    let mut metadata = AcmeCertificateCacheMetadata {
        schema_version: ACME_CERTIFICATE_CACHE_SCHEMA_VERSION,
        cache_format_version: ACME_CERTIFICATE_CACHE_FORMAT_VERSION,
        domains: plan.domains.clone(),
        subject_alternative_names: plan.domains.clone(),
        issuer: acme_metadata_issuer(plan).to_string(),
        account_id: acme_metadata_account_id(plan).to_string(),
        key_algorithm: ACME_CERTIFICATE_CACHE_KEY_ALGORITHM.to_string(),
        challenge_method: ACME_CERTIFICATE_CACHE_CHALLENGE_METHOD.to_string(),
        acme_mode: acme_metadata_mode(plan).to_string(),
        not_before_unix_seconds: issued_at_unix_seconds,
        not_after_unix_seconds: issued_at_unix_seconds + ACME_RENEWAL_INTERVAL.as_secs(),
        issued_at_unix_seconds,
        renew_after_unix_seconds: issued_at_unix_seconds + ACME_RENEWAL_INTERVAL.as_secs(),
        issuing_worker_identity: ACME_CERTIFICATE_CACHE_ISSUING_WORKER.to_string(),
        provenance_hash: String::new(),
    };
    metadata.provenance_hash = acme_cache_provenance_fingerprint(&metadata);
    let contents = serde_json::to_vec_pretty(&metadata).map_err(|err| {
        format!("error[serve_tls]: failed to serialize ACME certificate cache metadata: {err}")
    })?;
    write_cache_file_atomically(&plan.renewal_metadata_path, &contents)
}

#[cfg(any(feature = "acme-live", test))]
fn acme_metadata_issuer(plan: &AcmeRuntimePlan) -> &'static str {
    match plan.primary_provider {
        super::ProjectServerTlsProvider::LetsEncrypt => "letsencrypt",
        super::ProjectServerTlsProvider::ZeroSsl => "zerossl",
    }
}

#[cfg(any(feature = "acme-live", test))]
fn acme_metadata_account_id(plan: &AcmeRuntimePlan) -> &str {
    plan.email
        .as_deref()
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .unwrap_or("acme-account")
}

fn acme_metadata_mode(plan: &AcmeRuntimePlan) -> &'static str {
    if plan.directory_url.contains("staging") {
        ACME_CERTIFICATE_CACHE_MODE_STAGING
    } else {
        ACME_CERTIFICATE_CACHE_MODE_LIVE
    }
}

#[cfg(test)]
fn redact_acme_cache_support_bundle_diagnostic(
    metadata: &AcmeCertificateCacheMetadata,
    diagnostic: &str,
) -> String {
    if contains_private_key_material(diagnostic) {
        return "<redacted acme private key material>".to_string();
    }
    diagnostic
        .replace(&metadata.account_id, "<redacted-acme-account>")
        .replace(&metadata.issuing_worker_identity, "<redacted-acme-worker>")
}

#[cfg(test)]
fn contains_private_key_material(text: &str) -> bool {
    [
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn validate_acme_cache_path(label: &str, cache_dir: &Path, path: &Path) -> Result<(), String> {
    if path.starts_with(cache_dir) {
        Ok(())
    } else {
        Err(format!(
            "error[serve_tls]: ACME {label} cache path `{}` escapes VM-owned cache directory `{}`",
            path.display(),
            cache_dir.display()
        ))
    }
}

fn acme_cache_provenance_fingerprint(metadata: &AcmeCertificateCacheMetadata) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for field in acme_cache_provenance_fields(metadata) {
        for byte in field.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn acme_cache_provenance_fields(metadata: &AcmeCertificateCacheMetadata) -> Vec<String> {
    vec![
        metadata.schema_version.to_string(),
        metadata.cache_format_version.to_string(),
        metadata.domains.join(","),
        metadata.subject_alternative_names.join(","),
        metadata.issuer.clone(),
        metadata.account_id.clone(),
        metadata.key_algorithm.clone(),
        metadata.challenge_method.clone(),
        metadata.acme_mode.clone(),
        metadata.not_before_unix_seconds.to_string(),
        metadata.not_after_unix_seconds.to_string(),
        metadata.issued_at_unix_seconds.to_string(),
        metadata.renew_after_unix_seconds.to_string(),
        metadata.issuing_worker_identity.clone(),
    ]
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
#[cfg(any(feature = "acme-live", test))]
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
#[cfg(any(feature = "acme-live", test))]
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
#[cfg(any(feature = "acme-live", test))]
fn rename_cache_file(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target).map_err(|err| {
        format!(
            "error[serve_tls]: failed to move ACME cache file `{}` to `{}`: {err}",
            source.display(),
            target.display()
        )
    })
}
