use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{contains_private_key_marker, run_vm_http_acme_cache_custody};

struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-vm-http-acme-cache-custody-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, text: &str) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)
    }

    fn write_complete_fixture(&self) -> io::Result<()> {
        self.write(
            "crates/terlan/src/commands/serve/tls/cache.rs",
            r#"
AcmeCertificateCacheMetadata schema_version cache_format_version domains
subject_alternative_names issuer account_id key_algorithm challenge_method
acme_mode not_before_unix_seconds not_after_unix_seconds
issued_at_unix_seconds renew_after_unix_seconds issuing_worker_identity
provenance_hash
store_acme_certificate_cache load_acme_certificate_cache_metadata
store_acme_certificate_cache_metadata write_cache_file_atomically rename_cache_file
restrict_private_key_file_permissions validate_private_key_cache_permissions
validate_acme_certificate_cache_mode
validate_acme_certificate_cache_provenance_hash
AcmeCacheSupportBundleRedaction redact_acme_cache_support_bundle
AcmeKeyCustodyPolicy validate_acme_key_custody_policy
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls.rs",
            r#"
load_acme_runtime_tls_cache validate_acme_certificate_cache_age
validate_acme_certificate_cache_domains
validate_acme_certificate_cache_validity_window
load_certificate_chain load_private_key rustls_server_config
validate_acme_key_custody_policy validate_acme_certificate_cache_mode
validate_acme_certificate_cache_provenance_hash
validate_acme_provider_supported acme_runtime_plan
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/tls.rs",
            r#"
load_certificate_chain load_private_key with_single_cert
VM TLS manual encrypted private keys are not supported
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls_test.rs",
            r#"
acme_certificate_cache_write_feeds_runtime_tls_config
runtime_tls_config_accepts_auto_tls_certificate_cache
runtime_tls_config_for_serve_accepts_auto_tls_certificate_cache
acme_certificate_cache_metadata_records_typed_provenance_schema
runtime_tls_config_rejects_world_readable_auto_tls_private_key_cache
runtime_tls_config_rejects_staging_mode_auto_tls_certificate_cache
acme_key_custody_policy_rejects_cache_path_escape
runtime_tls_config_rejects_mismatched_auto_tls_certificate_key_pair
runtime_tls_config_rejects_wrong_domain_auto_tls_certificate_cache
runtime_tls_config_rejects_expired_auto_tls_certificate_cache
runtime_tls_config_rejects_tampered_auto_tls_certificate_cache_provenance_hash
runtime_tls_config_rejects_auto_tls_cache_without_renewal_metadata
runtime_tls_config_rejects_malformed_auto_tls_certificate_cache_metadata
runtime_tls_config_rejects_future_dated_auto_tls_certificate_cache_metadata
runtime_tls_config_rejects_stale_auto_tls_certificate_cache
runtime_tls_config_rejects_zerossl_primary_before_cache_loading
acme_cache_support_bundle_redaction_removes_sensitive_material
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/tls_test.rs",
            r#"
vm_tls_runtime_reports_missing_manual_private_key_file
vm_tls_runtime_reports_malformed_manual_private_key_file
vm_tls_runtime_reports_manual_private_key_without_supported_key
vm_tls_runtime_rejects_manual_encrypted_private_key_plan
"#,
        )?;
        self.write("Makefile", COMPLETE_MAKEFILE)
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const COMPLETE_MAKEFILE: &str = r#"
vm-http-acme-cache-custody-check: vm-http-acme-worker-migration-check
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::serve::tls::tls_test::acme_certificate_cache_metadata_records_typed_provenance_schema -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::serve::tls::tls_test::runtime_tls_config_rejects_world_readable_auto_tls_private_key_cache -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::serve::tls::tls_test::runtime_tls_config_rejects_staging_mode_auto_tls_certificate_cache -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::serve::tls::tls_test::acme_cache_support_bundle_redaction_removes_sensitive_material -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::serve::tls::tls_test::acme_key_custody_policy_rejects_cache_path_escape -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::serve::tls::tls_test::runtime_tls_config_rejects_mismatched_auto_tls_certificate_key_pair -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::serve::tls::tls_test::runtime_tls_config_rejects_wrong_domain_auto_tls_certificate_cache -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::serve::tls::tls_test::runtime_tls_config_rejects_expired_auto_tls_certificate_cache -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::serve::tls::tls_test::runtime_tls_config_rejects_tampered_auto_tls_certificate_cache_provenance_hash -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_http_acme_cache_custody_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-acme-cache-custody
"#;

#[test]
fn vm_http_acme_cache_custody_writes_redacted_report_for_current_foundation() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_http_acme_cache_custody(repo.root()).expect("quality check");

    assert_eq!(summary.cache_manifest_field_count, 13);
    assert_eq!(summary.key_custody_decision_count, 7);
    assert_eq!(summary.rejected_cache_fixture_count, 9);
    assert_eq!(summary.rejected_custody_path_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-http-acme-cache-custody-report-v1"));
    assert!(report.contains("reports never include private key material"));
    assert!(report.contains("rustls-webpki validates cached ACME certificate DNS identity"));
    assert!(report.contains("rustls-webpki parses cached ACME certificate not-before/not-after"));
    assert!(!report.contains("SAN and domain validation against parsed certificates"));
    assert!(
        !report.contains("not-before and not-after parsing through maintained certificate parser")
    );
    assert!(!report.contains("support-bundle ACME redaction integration"));
    assert!(!report.contains("VM-owned key custody policy checks"));
    assert!(!report.contains("key/certificate pairing validation before TLS startup"));
    assert!(!report.contains("provenance hash validation"));
    assert!(!report.contains("typed ACME certificate provenance schema"));
    assert!(!contains_private_key_marker(&report));
}

#[test]
fn vm_http_acme_cache_custody_rejects_missing_atomic_write_anchor() {
    let repo = TestRepo::new("missing-atomic").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/commands/serve/tls/cache.rs");
    let source = fs::read_to_string(&path).expect("cache source");
    repo.write(
        "crates/terlan/src/commands/serve/tls/cache.rs",
        &source.replace("write_cache_file_atomically", ""),
    )
    .expect("rewrite cache source");

    let error = run_vm_http_acme_cache_custody(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("write_cache_file_atomically"));
}

#[test]
fn vm_http_acme_cache_custody_rejects_missing_stale_cache_fixture() {
    let repo = TestRepo::new("missing-stale-fixture").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/commands/serve/tls_test.rs");
    let source = fs::read_to_string(&path).expect("tls test source");
    repo.write(
        "crates/terlan/src/commands/serve/tls_test.rs",
        &source.replace(
            "runtime_tls_config_rejects_stale_auto_tls_certificate_cache",
            "",
        ),
    )
    .expect("rewrite tls test source");

    let error = run_vm_http_acme_cache_custody(repo.root()).expect_err("fixture should fail");

    assert!(error.contains("runtime_tls_config_rejects_stale_auto_tls_certificate_cache"));
}

#[test]
fn vm_http_acme_cache_custody_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("vm_http_acme_cache_custody_test", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_http_acme_cache_custody(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm_http_acme_cache_custody_test"));
}

#[test]
fn vm_http_acme_cache_custody_detects_private_key_markers() {
    assert!(contains_private_key_marker("-----BEGIN PRIVATE KEY-----"));
    assert!(contains_private_key_marker(
        "-----BEGIN RSA PRIVATE KEY-----"
    ));
    assert!(!contains_private_key_marker(
        "private key diagnostics redacted"
    ));
}
