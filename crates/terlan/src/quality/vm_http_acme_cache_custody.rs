use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-http-acme-cache-custody-report.json";

const REQUIRED_FOUNDATION_ANCHORS: &[(&str, &[&str])] = &[
    (
        "std/http/native/src/acme/cache.rs",
        &[
            "AcmeCertificateCacheMetadata",
            "schema_version",
            "cache_format_version",
            "domains",
            "subject_alternative_names",
            "issuer",
            "account_id",
            "key_algorithm",
            "challenge_method",
            "acme_mode",
            "not_before_unix_seconds",
            "not_after_unix_seconds",
            "issued_at_unix_seconds",
            "renew_after_unix_seconds",
            "issuing_worker_identity",
            "provenance_hash",
            "store_acme_certificate_cache",
            "load_acme_certificate_cache_metadata",
            "store_acme_certificate_cache_metadata",
            "write_cache_file_atomically",
            "persist_cache_file",
            "restrict_private_key_file_permissions",
            "validate_private_key_cache_permissions",
            "validate_acme_certificate_cache_mode",
            "validate_acme_certificate_cache_provenance_hash",
            "AcmeCacheSupportBundleRedaction",
            "redact_acme_cache_support_bundle",
            "validate_acme_cache_paths",
            "validate_acme_key_custody_policy",
        ],
    ),
    (
        "std/http/native/src/acme.rs",
        &[
            "load_acme_runtime_tls_cache",
            "validate_acme_certificate_cache_age",
            "validate_acme_certificate_cache_domains",
            "validate_acme_certificate_cache_validity_window",
            "load_certificate_chain",
            "load_private_key",
            "validate_acme_key_custody_policy",
            "validate_acme_certificate_cache_provenance_hash",
            "validate_acme_certificate_cache_mode",
            "rustls_server_config",
            "validate_acme_provider_supported",
            "acme_runtime_plan",
        ],
    ),
    (
        "std/net/native/src/tls.rs",
        &[
            "parse_certificate_chain",
            "parse_private_key",
            "with_single_cert",
        ],
    ),
];

const REQUIRED_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "std/http/native/src/acme/tests.rs",
        &[
            "acme_cache_support_bundle_redaction_removes_sensitive_material",
            "rejected_replacement_keeps_published_material_and_cleans_staging_files",
            "cache_paths_reject_parent_traversal_before_reads_or_writes",
        ],
    ),
    (
        "crates/terlan/src/commands/serve/tls/acme_runtime/tls_test.rs",
        &[
            "acme_certificate_cache_write_feeds_runtime_tls_config",
            "runtime_tls_config_accepts_auto_tls_certificate_cache",
            "runtime_tls_config_for_serve_accepts_auto_tls_certificate_cache",
            "acme_certificate_cache_metadata_records_typed_provenance_schema",
            "runtime_tls_config_rejects_world_readable_auto_tls_private_key_cache",
            "runtime_tls_config_rejects_staging_mode_auto_tls_certificate_cache",
            "acme_key_custody_policy_rejects_cache_path_escape",
            "runtime_tls_config_rejects_mismatched_auto_tls_certificate_key_pair",
            "runtime_tls_config_rejects_wrong_domain_auto_tls_certificate_cache",
            "runtime_tls_config_rejects_expired_auto_tls_certificate_cache",
            "runtime_tls_config_rejects_tampered_auto_tls_certificate_cache_provenance_hash",
            "runtime_tls_config_rejects_auto_tls_cache_without_renewal_metadata",
            "runtime_tls_config_rejects_malformed_auto_tls_certificate_cache_metadata",
            "runtime_tls_config_rejects_future_dated_auto_tls_certificate_cache_metadata",
            "runtime_tls_config_rejects_stale_auto_tls_certificate_cache",
            "runtime_tls_config_rejects_zerossl_primary_before_cache_loading",
        ],
    ),
    (
        "std/http/native/src/tls_material_test.rs",
        &[
            "manual_propagates_file_and_material_errors_without_accepting_partial_configuration",
            "pem_failures_keep_file_context_and_never_accept_partial_chains",
            "manual_rejects_missing_fields_and_encrypted_key_options_before_io",
        ],
    ),
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-http-acme-cache-custody-check: vm-http-acme-worker-migration-check",
    "vm-http-acme-cache-custody",
];

const CACHE_MANIFEST_FIELDS: &[&str] = &[
    "domain",
    "subject alternative names",
    "issuer",
    "account id",
    "key algorithm",
    "challenge method",
    "ACME mode",
    "not-before",
    "not-after",
    "renewal deadline",
    "cache format version",
    "issuing worker identity",
    "provenance hash",
];

const KEY_CUSTODY_DECISIONS: &[&str] = &[
    "key path scoped to project cache",
    "private key parsed only by maintained PEM/TLS libraries",
    "private key diagnostics redacted",
    "private key cache write is atomic",
    "key cache admission is HTTP-package-owned",
    "encrypted keys fail closed",
    "unsupported keys fail closed",
];

const PERMISSION_CHECKS: &[&str] = &[
    "cache directory exists",
    "certificate path exists",
    "private key path exists",
    "renewal metadata exists",
    "world-readable private key rejected",
    "partial cache rejected",
];

const PROVENANCE_VALIDATION_TRACES: &[&str] = &[
    "domain match",
    "SAN match",
    "issuer policy",
    "certificate validity window",
    "key/certificate pairing",
    "provenance hash",
    "schema version",
    "staging/live mode",
];

const REJECTED_CACHE_FIXTURES: &[&str] = &[
    "mismatched key/certificate pair",
    "copied staging certificate in live mode",
    "wrong domain",
    "expired certificate",
    "future not-before",
    "corrupt cache metadata",
    "weak permissions",
    "partial write",
    "support-bundle secret leak",
];

const RENEWAL_ELIGIBILITY: &[&str] = &[
    "fresh cache eligible for TLS startup",
    "stale cache requires renewal",
    "future-dated metadata rejected",
    "malformed metadata rejected",
    "missing renewal metadata rejected",
];

const REDACTION_OUTCOMES: &[&str] = &[
    "diagnostics contain paths but no PEM bytes",
    "telemetry emits key custody decision only",
    "support bundles expose provenance hash only",
    "reports never include private key material",
    "cache write failures redact attempted contents",
];

const REJECTED_CUSTODY_PATHS: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing vm http acme cache custody summary.
pub struct VmHttpAcmeCacheCustodySummary {
    pub cache_manifest_field_count: usize,
    pub key_custody_decision_count: usize,
    pub rejected_cache_fixture_count: usize,
    pub rejected_custody_path_count: usize,
    pub report_path: PathBuf,
}

/// Runs vm http acme cache custody.
pub fn run_vm_http_acme_cache_custody(root: &Path) -> QualityResult<VmHttpAcmeCacheCustodySummary> {
    let mut diagnostics = Vec::new();
    for (relative, anchors) in REQUIRED_FOUNDATION_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM HTTP ACME cache custody foundation",
        )?);
    }
    for (relative, anchors) in REQUIRED_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM HTTP ACME cache custody fixture coverage",
        )?);
    }
    diagnostics.extend(validate_makefile(root)?);
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-http-acme-cache-custody", &diagnostics));
    }

    let report_path = root.join(REPORT_PATH);
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan-vm-http-acme-cache-custody-report-v1",
        "cacheManifests": CACHE_MANIFEST_FIELDS,
        "keyCustodyDecisions": KEY_CUSTODY_DECISIONS,
        "permissionChecks": PERMISSION_CHECKS,
        "provenanceValidationTraces": PROVENANCE_VALIDATION_TRACES,
        "rejectedCacheFixtures": REJECTED_CACHE_FIXTURES,
        "renewalEligibility": RENEWAL_ELIGIBILITY,
        "redactionOutcomes": REDACTION_OUTCOMES,
        "maintainedCrateBoundaries": [
            "rustls-pki-types parses certificate and private-key PEM",
            "rustls validates certificate/private-key pairing for server config",
            "rustls-webpki validates cached ACME certificate DNS identity",
            "rustls-webpki parses cached ACME certificate not-before/not-after",
            "rcgen supplies internal certificates and ACME CSR material",
            "serde_json serializes renewal metadata",
            "instant_acme owns account credential payloads"
        ],
        "rejectedCustodyPaths": REJECTED_CUSTODY_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM HTTP ACME cache custody report: {err}"))?;
    if contains_private_key_marker(&report_text) {
        return Err("vm-http-acme-cache-custody: report contains private key material".to_string());
    }
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmHttpAcmeCacheCustodySummary {
        cache_manifest_field_count: CACHE_MANIFEST_FIELDS.len(),
        key_custody_decision_count: KEY_CUSTODY_DECISIONS.len(),
        rejected_cache_fixture_count: REJECTED_CACHE_FIXTURES.len(),
        rejected_custody_path_count: REJECTED_CUSTODY_PATHS.len(),
        report_path,
    })
}

fn contains_private_key_marker(text: &str) -> bool {
    [
        "BEGIN PRIVATE KEY",
        "BEGIN RSA PRIVATE KEY",
        "BEGIN EC PRIVATE KEY",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn validate_required_terms(
    root: &Path,
    relative: &str,
    terms: &[&str],
    label: &str,
) -> QualityResult<Vec<String>> {
    let text = super::vm_http_acme_worker::read_split_source(root, relative)
        .map_err(|err| format!("{relative}: failed to read {label}: {err}"))?;
    Ok(terms
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("{relative}: missing {label} anchor `{term}`"))
        .collect())
}

fn validate_makefile(root: &Path) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join("Makefile"))
        .map_err(|err| format!("Makefile: failed to read VM HTTP ACME custody gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing VM HTTP ACME custody gate term `{term}`"))
        .collect())
}

fn render_failure(label: &str, diagnostics: &[String]) -> String {
    let mut message = format!("[{label}] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_http_acme_cache_custody_test.rs"]
#[cfg(test)]
mod vm_http_acme_cache_custody_test;
