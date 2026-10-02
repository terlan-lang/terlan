use super::cache::*;
use super::*;
use crate::tls_config::{Mode, Settings};
use rcgen::generate_simple_self_signed;

fn tls() -> ProjectServerTls {
    Settings {
        mode: Some(Mode::Auto),
        domains: Some(vec!["example.test".into()]),
        email: Some("admin@example.test".into()),
        ..Settings::default()
    }
    .validate()
    .unwrap()
}

#[test]
fn cache_roundtrip_reuses_http_tls_setup_and_rejects_partial_or_stale_material() {
    let directory = tempfile::tempdir().unwrap();
    let plan = acme_runtime_plan(directory.path(), &tls());
    assert!(load_acme_runtime_tls_cache(&plan).unwrap().is_none());
    let generated = generate_simple_self_signed(plan.domains.clone()).unwrap();
    store_acme_certificate_cache(
        &plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )
    .unwrap();
    let runtime = load_acme_runtime_tls_cache(&plan).unwrap().unwrap();
    assert_eq!(
        runtime.server_config.alpn_protocols,
        [b"h2".to_vec(), b"http/1.1".to_vec()]
    );
    store_acme_certificate_cache_metadata(&plan, UNIX_EPOCH).unwrap();
    assert!(load_acme_runtime_tls_cache(&plan)
        .err()
        .unwrap()
        .to_string()
        .contains("requires renewal"));
    fs::remove_file(&plan.private_key_path).unwrap();
    assert!(load_acme_runtime_tls_cache(&plan)
        .err()
        .unwrap()
        .to_string()
        .contains("incomplete"));
}

#[test]
fn rejected_replacement_keeps_published_material_and_cleans_staging_files() {
    let directory = tempfile::tempdir().unwrap();
    let plan = acme_runtime_plan(directory.path(), &tls());
    let generated = generate_simple_self_signed(plan.domains.clone()).unwrap();
    let other = generate_simple_self_signed(plan.domains.clone()).unwrap();
    store_acme_certificate_cache(
        &plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )
    .unwrap();
    let paths = [
        &plan.certificate_path,
        &plan.private_key_path,
        &plan.renewal_metadata_path,
    ];
    let original = paths.map(|path| fs::read(path).unwrap());
    for (cert, key) in [
        ("not PEM".to_string(), generated.key_pair.serialize_pem()),
        (generated.cert.pem(), "not PEM".to_string()),
        (generated.cert.pem(), other.key_pair.serialize_pem()),
    ] {
        assert!(store_acme_certificate_cache(&plan, &cert, &key).is_err());
        assert_eq!(paths.map(|path| fs::read(path).unwrap()), original);
        assert_eq!(fs::read_dir(&plan.cache_dir).unwrap().count(), 3);
    }
    assert!(load_acme_runtime_tls_cache(&plan).unwrap().is_some());
}

#[test]
fn http01_lookup_preserves_mode_admission_and_rejects_unsafe_tokens() {
    let directory = tempfile::tempdir().unwrap();
    let config = tls();
    let project = Some((directory.path(), &config));
    let plan = acme_runtime_plan(directory.path(), &config);
    let route = |token: &str| format!("{ACME_HTTP01_PATH_PREFIX}{token}");
    assert_eq!(
        acme_http01_challenge(project, "/index").unwrap(),
        AcmeHttp01Challenge::NotMatched
    );
    assert_eq!(
        acme_http01_challenge(None, &route("safe")).unwrap(),
        AcmeHttp01Challenge::NotMatched
    );
    let mut manual = config.clone();
    manual.mode = Mode::Manual;
    assert_eq!(
        acme_http01_challenge(Some((directory.path(), &manual)), &route("safe")).unwrap(),
        AcmeHttp01Challenge::NotMatched
    );
    assert_eq!(
        acme_http01_challenge(project, &route("safe")).unwrap(),
        AcmeHttp01Challenge::Missing
    );
    for token in [
        "",
        ".",
        "..",
        "../secret",
        "%2fsecret",
        "a/b",
        "a\\b",
        "a?b",
        "a\0b",
        "\u{e9}",
    ] {
        assert!(matches!(
            acme_http01_challenge(project, &route(token)).unwrap(),
            AcmeHttp01Challenge::Invalid(_)
        ));
        assert!(store_acme_http01_challenge(&plan, token, "secret").is_err());
    }
    store_acme_http01_challenge(&plan, "safe_123-ABC", "safe.thumbprint").unwrap();
    assert_eq!(
        acme_http01_challenge(project, &route("safe_123-ABC")).unwrap(),
        AcmeHttp01Challenge::Found("safe.thumbprint".into())
    );
    fs::create_dir(plan.http01_challenge_dir.join("directory")).unwrap();
    assert!(acme_http01_challenge(project, &route("directory"))
        .unwrap_err()
        .to_string()
        .contains("failed to read ACME HTTP-01"));
}

#[test]
fn cache_paths_reject_parent_traversal_before_reads_or_writes() {
    let directory = tempfile::tempdir().unwrap();
    let plan = acme_runtime_plan(directory.path(), &tls());
    for field in 0..5 {
        let mut invalid = plan.clone();
        let path = invalid.cache_dir.join("../outside");
        match field {
            0 => invalid.certificate_path = path,
            1 => invalid.private_key_path = path,
            2 => invalid.renewal_metadata_path = path,
            3 => invalid.account_credentials_path = path,
            _ => invalid.http01_challenge_dir = path,
        }
        assert!(load_acme_runtime_tls_cache(&invalid)
            .err()
            .unwrap()
            .to_string()
            .contains("escapes package-owned"));
        assert!(
            store_acme_account_credentials(&invalid, &serde_json::json!({"key": "secret"}))
                .is_err()
        );
        assert!(!invalid.cache_dir.exists());
    }
}

#[cfg(unix)]
#[test]
fn cache_custody_rejects_symlink_escapes_and_writes_secrets_owner_only() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let directory = tempfile::tempdir().unwrap();
    let config = tls();
    let plan = acme_runtime_plan(directory.path(), &config);
    store_acme_account_credentials(&plan, &serde_json::json!({"key": "secret"})).unwrap();
    assert_eq!(
        fs::metadata(&plan.account_credentials_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let outside = directory.path().join("outside");
    fs::write(&outside, "do not disclose").unwrap();
    symlink(&outside, &plan.private_key_path).unwrap();
    assert!(load_acme_runtime_tls_cache(&plan)
        .err()
        .unwrap()
        .to_string()
        .contains("escapes package-owned"));
    fs::remove_file(&plan.private_key_path).unwrap();
    fs::create_dir(&plan.http01_challenge_dir).unwrap();
    symlink(&outside, plan.http01_challenge_dir.join("safe")).unwrap();
    let error = acme_http01_challenge(
        Some((directory.path(), &config)),
        &format!("{ACME_HTTP01_PATH_PREFIX}safe"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("escapes package-owned"));
    assert!(!error.to_string().contains("do not disclose"));
}

#[test]
fn account_storage_roundtrips_opaque_serde_payloads_and_rejects_malformed_files() {
    let directory = tempfile::tempdir().unwrap();
    let plan = acme_runtime_plan(directory.path(), &tls());
    assert!(load_acme_account_credentials::<serde_json::Value>(&plan)
        .unwrap()
        .is_none());
    let credentials = serde_json::json!({"issuer": "maintained", "key": [1, 2, 3]});
    store_acme_account_credentials(&plan, &credentials).unwrap();
    assert_eq!(
        load_acme_account_credentials::<serde_json::Value>(&plan).unwrap(),
        Some(credentials)
    );
    fs::write(&plan.account_credentials_path, "{").unwrap();
    assert!(load_acme_account_credentials::<serde_json::Value>(&plan)
        .unwrap_err()
        .to_string()
        .contains("failed to parse"));
    fs::remove_file(&plan.account_credentials_path).unwrap();
    fs::create_dir(&plan.account_credentials_path).unwrap();
    assert!(load_acme_account_credentials::<serde_json::Value>(&plan)
        .unwrap_err()
        .to_string()
        .contains("failed to read"));
}

#[test]
fn acme_cache_support_bundle_redaction_removes_sensitive_material() {
    let directory = tempfile::tempdir().unwrap();
    let dir = directory.path();
    let tls = tls();
    let plan = acme_runtime_plan(dir, &tls);
    let generated =
        generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");

    store_acme_certificate_cache(
        &plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )
    .expect("store cert cache");
    let metadata = load_acme_certificate_cache_metadata(&plan).expect("load metadata");
    let diagnostic = format!(
        "account={} worker={} key=-----BEGIN PRIVATE KEY-----",
        metadata.account_id, metadata.issuing_worker_identity,
    );

    let redacted = redact_acme_cache_support_bundle(&plan, &metadata, &diagnostic);
    let replay = format!("{redacted:?}");

    assert_eq!(redacted.cache_dir, plan.cache_dir.display().to_string());
    assert_eq!(redacted.provenance_fingerprint.len(), 16);
    assert_eq!(
        redacted.provenance_fingerprint,
        redact_acme_cache_support_bundle(&plan, &metadata, "stable").provenance_fingerprint
    );
    assert!(replay.contains("redacted acme private key material"));
    assert!(!replay.contains("admin@example.test"));
    assert!(!replay.contains("vm-acme-worker"));
    assert!(!replay.contains("BEGIN PRIVATE KEY"));
}
