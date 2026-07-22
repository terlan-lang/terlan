use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-http-acme-cache-custody-report.json";

const REQUIRED_FOUNDATION_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/commands/serve/tls/cache.rs",
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
            "rename_cache_file",
            "restrict_private_key_file_permissions",
            "validate_private_key_cache_permissions",
            "validate_acme_certificate_cache_mode",
            "validate_acme_certificate_cache_provenance_hash",
            "AcmeCacheSupportBundleRedaction",
            "redact_acme_cache_support_bundle",
            "AcmeKeyCustodyPolicy",
            "validate_acme_key_custody_policy",
        ],
    ),
    (
        "crates/terlan/src/commands/serve/tls.rs",
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
        "crates/terlan/src/runtime/vm/tls.rs",
        &[
            "load_certificate_chain",
            "load_private_key",
            "with_single_cert",
            "VM TLS manual encrypted private keys are not supported",
        ],
    ),
];

const REQUIRED_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/commands/serve/tls_test.rs",
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
            "acme_cache_support_bundle_redaction_removes_sensitive_material",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/tls_test.rs",
        &[
            "vm_tls_runtime_reports_missing_manual_private_key_file",
            "vm_tls_runtime_reports_malformed_manual_private_key_file",
            "vm_tls_runtime_reports_manual_private_key_without_supported_key",
            "vm_tls_runtime_rejects_manual_encrypted_private_key_plan",
        ],
    ),
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-http-acme-cache-custody-check: vm-http-acme-worker-migration-check",
    "commands::serve::tls::tls_test::acme_certificate_cache_metadata_records_typed_provenance_schema",
    "commands::serve::tls::tls_test::runtime_tls_config_rejects_world_readable_auto_tls_private_key_cache",
    "commands::serve::tls::tls_test::runtime_tls_config_rejects_staging_mode_auto_tls_certificate_cache",
    "commands::serve::tls::tls_test::acme_cache_support_bundle_redaction_removes_sensitive_material",
    "commands::serve::tls::tls_test::acme_key_custody_policy_rejects_cache_path_escape",
    "commands::serve::tls::tls_test::runtime_tls_config_rejects_mismatched_auto_tls_certificate_key_pair",
    "commands::serve::tls::tls_test::runtime_tls_config_rejects_wrong_domain_auto_tls_certificate_cache",
    "commands::serve::tls::tls_test::runtime_tls_config_rejects_expired_auto_tls_certificate_cache",
    "commands::serve::tls::tls_test::runtime_tls_config_rejects_tampered_auto_tls_certificate_cache_provenance_hash",
    "vm_http_acme_cache_custody_test",
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
    "private key handoff remains VM-owned",
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
pub struct VmHttpAcmeCacheCustodySummary {
    pub cache_manifest_field_count: usize,
    pub key_custody_decision_count: usize,
    pub rejected_cache_fixture_count: usize,
    pub rejected_custody_path_count: usize,
    pub report_path: PathBuf,
}

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
            "rustls-pemfile parses certificate and private-key PEM",
            "rustls validates certificate/private-key pairing for server config",
            "rustls-webpki validates cached ACME certificate DNS identity",
            "rustls-webpki parses cached ACME certificate not-before/not-after",
            "rcgen remains limited to local deterministic certificate fixtures",
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
    let text = fs::read_to_string(root.join(relative))
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
mod vm_http_acme_cache_custody_test;
