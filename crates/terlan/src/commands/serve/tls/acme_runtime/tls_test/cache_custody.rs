use super::*;

/// Verifies issued ACME certificate material is cached for runtime loading.
///
/// Inputs:
/// - Generated local certificate/key PEM used as issued-material fixture.
///
/// Output:
/// - Test passes when the certificate cache files exist and `runtime_tls_config`
///   loads them.
///
/// Transformation:
/// - Covers the handoff from future ACME issuance to the already-implemented
///   `rustls` serving path.
#[test]
pub(super) fn acme_certificate_cache_write_feeds_runtime_tls_config() {
    let dir = temp_dir("auto_cert_store");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
    let generated =
        generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");

    store_acme_certificate_cache(
        &plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )
    .expect("store cert cache");

    assert!(plan.certificate_path.is_file());
    assert!(plan.private_key_path.is_file());
    assert!(plan.renewal_metadata_path.is_file());
    #[cfg(unix)]
    {
        let mode = fs::metadata(&plan.private_key_path)
            .expect("key metadata")
            .permissions();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(mode.mode() & 0o077, 0);
    }
    assert!(runtime_tls_config(&web_root)
        .expect("auto TLS cache should load")
        .is_some());

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies issued ACME cache metadata carries typed provenance fields.
///
/// Inputs:
/// - Generated local certificate/key PEM used as issued-material fixture.
/// - Auto TLS metadata with domain and account email.
///
/// Output:
/// - Test passes when `renewal.json` records typed cache provenance alongside
///   renewal timestamps.
///
/// Transformation:
/// - Locks the cache manifest schema before deeper certificate/SAN/key-pairing
///   validation is wired into TLS startup.
#[test]
pub(super) fn acme_certificate_cache_metadata_records_typed_provenance_schema() {
    let dir = temp_dir("auto_cert_metadata_provenance");
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
    let generated =
        generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");

    store_acme_certificate_cache(
        &plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )
    .expect("store cert cache");
    let metadata = load_acme_certificate_cache_metadata(&plan).expect("load metadata");

    assert_eq!(metadata.schema_version, 1);
    assert_eq!(metadata.cache_format_version, 1);
    assert_eq!(metadata.domains, vec!["example.test".to_string()]);
    assert_eq!(
        metadata.subject_alternative_names,
        vec!["example.test".to_string()]
    );
    assert_eq!(metadata.issuer, "letsencrypt");
    assert_eq!(metadata.account_id, "admin@example.test");
    assert_eq!(metadata.key_algorithm, "ecdsa-p256");
    assert_eq!(metadata.challenge_method, "http-01");
    assert_eq!(metadata.acme_mode, "live");
    assert_eq!(metadata.issuing_worker_identity, "vm-acme-worker");
    assert_eq!(
        metadata.not_before_unix_seconds,
        metadata.issued_at_unix_seconds
    );
    assert_eq!(
        metadata.not_after_unix_seconds,
        metadata.renew_after_unix_seconds
    );
    assert_eq!(metadata.provenance_hash.len(), 16);
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies ACME cache support-bundle metadata is redacted.
///
/// Inputs:
/// - Generated local certificate/key PEM used as issued-material fixture.
/// - Auto TLS metadata containing an account id.
/// - A diagnostic containing account id, worker identity, and private-key PEM
///   marker text.
///
/// Output:
/// - Test passes when support-bundle details include a stable provenance
///   fingerprint but no account id, worker id, or private-key marker.
///
/// Transformation:
/// - Locks the ACME cache custody rule that support-bundle replay must expose
///   replayable provenance without leaking certificate private material.
#[test]
pub(super) fn acme_cache_support_bundle_redaction_removes_sensitive_material() {
    let dir = temp_dir("auto_cert_support_bundle_redaction");
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
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
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies auto TLS rejects unsafe private-key cache permissions.
///
/// Inputs:
/// - Auto TLS cache with valid certificate/key material.
/// - Private key cache file changed to group/world-readable permissions on
///   Unix platforms.
///
/// Output:
/// - Test passes when TLS startup rejects the unsafe key before loading it.
///
/// Transformation:
/// - Locks the runtime permission check that prevents copied or manually edited
///   ACME key caches from being served with weak file modes.
#[test]
pub(super) fn runtime_tls_config_rejects_world_readable_auto_tls_private_key_cache() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("auto_cache_world_readable_key");
        let web_root = dir.join("_build/web");
        write_valid_package(&web_root);
        write_project_manifest(
            &dir.join("terlan.toml"),
            r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
        );
        let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
        let plan = acme_runtime_plan(&dir, &tls);
        let generated =
            generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");
        store_acme_certificate_cache(
            &plan,
            &generated.cert.pem(),
            &generated.key_pair.serialize_pem(),
        )
        .expect("write cached certificate");
        fs::set_permissions(&plan.private_key_path, fs::Permissions::from_mode(0o644))
            .expect("weaken key permissions");

        let message = match runtime_tls_config(&web_root) {
            Ok(_) => panic!("weak key mode should fail"),
            Err(message) => message,
        };

        assert!(message.contains("ACME private key cache"));
        assert!(message.contains("must not be group/world accessible"));
        assert!(message.contains("644"));
        fs::remove_dir_all(dir).expect("cleanup");
    }
}

/// Verifies ACME key custody rejects cache path escapes.
///
/// Inputs:
/// - Auto TLS runtime plan for a project-local ACME cache.
/// - Mutated private-key cache path outside the ACME cache directory.
///
/// Output:
/// - Test passes when the VM-owned custody policy rejects the escaped path.
///
/// Transformation:
/// - Prevents TLS startup from accepting certificate/key material outside the
///   deterministic ACME cache boundary.
#[test]
pub(super) fn acme_key_custody_policy_rejects_cache_path_escape() {
    let dir = temp_dir("auto_cache_key_custody_escape");
    fs::create_dir_all(&dir).expect("create temp project");
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let mut plan = acme_runtime_plan(&dir, &tls);
    let escaped_key = dir.join("outside-privkey.pem");
    fs::write(&escaped_key, "not a real key").expect("write escaped key");
    plan.private_key_path = escaped_key;

    let message = validate_acme_key_custody_policy(&plan)
        .expect_err("escaped key path should fail custody policy");

    assert!(message.contains("ACME private key cache path"));
    assert!(message.contains("escapes VM-owned cache directory"));
    assert!(message.contains(".terlan"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies auto TLS rejects mismatched certificate/private-key cache pairs.
///
/// Inputs:
/// - Auto TLS cache assembled by copying one certificate and another
///   unrelated private key into the ACME cache.
/// - Valid renewal metadata and owner-only key permissions.
///
/// Output:
/// - Test passes when TLS startup rejects the cache before returning a server
///   config.
///
/// Transformation:
/// - Locks the maintained rustls pairing validation for copied or corrupted
///   ACME cache material.
#[test]
pub(super) fn runtime_tls_config_rejects_mismatched_auto_tls_certificate_key_pair() {
    let dir = temp_dir("auto_cache_mismatched_key_pair");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
    fs::create_dir_all(&plan.cache_dir).expect("create acme cache");
    let certificate_pair =
        generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");
    let unrelated_key_pair =
        generate_simple_self_signed(vec!["other.example".to_string()]).expect("generate key");
    fs::write(&plan.certificate_path, certificate_pair.cert.pem()).expect("write cached cert");
    fs::write(
        &plan.private_key_path,
        unrelated_key_pair.key_pair.serialize_pem(),
    )
    .expect("write mismatched key");
    restrict_private_key_file_permissions(&plan.private_key_path).expect("restrict key");
    store_acme_certificate_cache_metadata(&plan, SystemTime::now()).expect("write metadata");

    let message = match runtime_tls_config(&web_root) {
        Ok(_) => panic!("mismatched key pair should fail"),
        Err(message) => message,
    };

    assert!(message.contains("failed to build TLS server config"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies auto TLS rejects a cache whose certificate does not cover the domain.
///
/// Inputs:
/// - Auto TLS cache metadata for `example.test`.
/// - Certificate/key material issued for `other.example`.
///
/// Output:
/// - Test passes when TLS startup rejects the wrong-domain certificate before
///   returning a runtime TLS config.
///
/// Transformation:
/// - Locks maintained `rustls-webpki` SAN/CN validation for cached ACME
///   certificate identity.
#[test]
pub(super) fn runtime_tls_config_rejects_wrong_domain_auto_tls_certificate_cache() {
    let dir = temp_dir("auto_cache_wrong_domain_certificate");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
    let generated =
        generate_simple_self_signed(vec!["other.example".to_string()]).expect("generate cert");
    store_acme_certificate_cache(
        &plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )
    .expect("write cached certificate");

    let message = match runtime_tls_config(&web_root) {
        Ok(_) => panic!("wrong-domain cached certificate should fail"),
        Err(message) => message,
    };

    assert!(message.contains("not valid for configured domain `example.test`"));
    assert!(message.contains("fullchain.pem"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies auto TLS rejects a cache whose certificate validity has expired.
///
/// Inputs:
/// - Fresh ACME cache metadata for `example.test`.
/// - Certificate/key material with an expired X.509 not-after timestamp.
///
/// Output:
/// - Test passes when TLS startup rejects the expired certificate before
///   returning a runtime TLS config.
///
/// Transformation:
/// - Locks maintained `rustls-webpki` not-before/not-after validation for
///   cached ACME certificate material.
#[test]
pub(super) fn runtime_tls_config_rejects_expired_auto_tls_certificate_cache() {
    let dir = temp_dir("auto_cache_expired_certificate");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
    let mut params =
        CertificateParams::new(vec!["example.test".to_string()]).expect("certificate params");
    params.not_before = date_time_ymd(2020, 1, 1);
    params.not_after = date_time_ymd(2020, 1, 2);
    let key_pair = KeyPair::generate().expect("generate key");
    let certificate = params.self_signed(&key_pair).expect("self signed cert");
    store_acme_certificate_cache(&plan, &certificate.pem(), &key_pair.serialize_pem())
        .expect("write cached certificate");

    let message = match runtime_tls_config(&web_root) {
        Ok(_) => panic!("expired cached certificate should fail"),
        Err(message) => message,
    };

    assert!(message.contains("failed validity-window validation"));
    assert!(message.contains("fullchain.pem"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies auto TLS rejects a cache issued for the wrong ACME mode.
///
/// Inputs:
/// - Auto TLS cache with valid certificate/key material.
/// - Renewal metadata mutated from live mode to staging mode.
///
/// Output:
/// - Test passes when TLS startup rejects the staging/live mismatch before
///   serving the cache.
///
/// Transformation:
/// - Prevents copied staging cache metadata from being trusted by production
///   live ACME configuration.
#[test]
pub(super) fn runtime_tls_config_rejects_staging_mode_auto_tls_certificate_cache() {
    let dir = temp_dir("auto_cache_staging_mode");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
    let generated =
        generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");
    store_acme_certificate_cache(
        &plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )
    .expect("write cached certificate");
    let mut metadata = load_acme_certificate_cache_metadata(&plan).expect("load metadata");
    metadata.acme_mode = "staging".to_string();
    let contents = serde_json::to_vec_pretty(&metadata).expect("serialize metadata");
    fs::write(&plan.renewal_metadata_path, contents).expect("write staging metadata");

    let message = match runtime_tls_config(&web_root) {
        Ok(_) => panic!("staging cache should fail for live config"),
        Err(message) => message,
    };

    assert!(message.contains("was issued for mode `staging`"));
    assert!(message.contains("runtime requires `live`"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies auto TLS rejects tampered ACME cache metadata provenance.
///
/// Inputs:
/// - Valid auto TLS cache metadata written by the runtime.
/// - Metadata account id changed without recomputing the stored provenance
///   hash.
///
/// Output:
/// - Test passes when TLS startup rejects the tampered metadata before serving
///   cached certificate material.
///
/// Transformation:
/// - Locks provenance hash validation for copied or edited ACME cache metadata.
#[test]
pub(super) fn runtime_tls_config_rejects_tampered_auto_tls_certificate_cache_provenance_hash() {
    let dir = temp_dir("auto_cache_tampered_provenance_hash");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
    let generated =
        generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");
    store_acme_certificate_cache(
        &plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )
    .expect("write cached certificate");
    let mut metadata = load_acme_certificate_cache_metadata(&plan).expect("load metadata");
    let original_hash = metadata.provenance_hash.clone();
    metadata.account_id = "attacker@example.test".to_string();
    assert_eq!(metadata.provenance_hash, original_hash);
    let contents = serde_json::to_vec_pretty(&metadata).expect("serialize metadata");
    fs::write(&plan.renewal_metadata_path, contents).expect("write tampered metadata");

    let message = match runtime_tls_config(&web_root) {
        Ok(_) => panic!("tampered provenance hash should fail"),
        Err(message) => message,
    };

    assert!(message.contains("failed provenance hash validation"));
    assert!(message.contains("renewal.json"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies auto TLS loads cached ACME certificate material.
///
/// Inputs:
/// - A package with adjacent auto `[server.tls]` project metadata and
///   deterministic ACME cache files.
///
/// Output:
/// - Test passes when runtime TLS configuration is present.
///
/// Transformation:
/// - Covers the production runtime shape expected after the ACME issuance
///   layer writes `fullchain.pem` and `privkey.pem`.
#[test]
pub(super) fn runtime_tls_config_accepts_auto_tls_certificate_cache() {
    let dir = temp_dir("auto_cache");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let generated =
        generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);

    store_acme_certificate_cache(
        &plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )
    .expect("write cached certificate");

    assert!(runtime_tls_config(&web_root)
        .expect("auto TLS cache should load")
        .is_some());

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies serve startup loads cached ACME material before live issuance.
///
/// Inputs:
/// - A package with adjacent auto `[server.tls]` metadata and a populated
///   ACME certificate cache.
///
/// Output:
/// - Test passes when the serve-facing TLS config boundary returns a runtime
///   config from cache.
///
/// Transformation:
/// - Guards the cache-first path so ordinary cached auto TLS startup does not
///   need to enter the temporary live ACME issuance boundary.
#[test]
pub(super) fn runtime_tls_config_for_serve_accepts_auto_tls_certificate_cache() {
    let dir = temp_dir("auto_cache_for_serve");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let generated =
        generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);

    store_acme_certificate_cache(
        &plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )
    .expect("write cached certificate");

    assert!(runtime_tls_config_for_serve(&web_root)
        .expect("serve auto TLS cache should load")
        .is_some());

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies a deterministic local issuer can populate the auto TLS cache.
///
/// Inputs:
/// - Auto TLS project metadata with no existing certificate cache.
/// - Local issuer closure that writes a challenge response and certificate
///   cache.
///
/// Output:
/// - Runtime TLS config loaded from the issuer-populated cache.
///
/// Transformation:
/// - Exercises the same cache handoff expected from the maintained ACME issuer
///   without depending on public DNS, Let's Encrypt, or network timing.
#[test]
pub(super) fn acme_runtime_tls_config_accepts_local_mock_issuer_cache_handoff() {
    let dir = temp_dir("auto_cache_mock_issuer");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);

    let config = acme_runtime_tls_config_with_local_issuer(&dir, &tls, |plan| {
        store_acme_http01_challenge(plan, "token_123", "token_123.thumbprint")?;
        let generated =
            generate_simple_self_signed(vec!["example.test".to_string()]).map_err(|err| {
                format!("error[serve_tls]: failed to generate mock ACME certificate: {err}")
            })?;
        store_acme_certificate_cache(
            plan,
            &generated.cert.pem(),
            &generated.key_pair.serialize_pem(),
        )
    })
    .expect("local issuer should populate cache");

    assert_eq!(Arc::strong_count(&config.server_config), 1);
    assert!(dir.join(".terlan/tls/acme/http-01/token_123").is_file());
    assert!(runtime_tls_config(&web_root)
        .expect("cached auto TLS should load after issuer")
        .is_some());
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies a deterministic issuer must actually populate certificate files.
///
/// Inputs:
/// - Auto TLS project metadata with no existing certificate cache.
/// - Local issuer closure that returns success without writing cache files.
///
/// Output:
/// - Stable `error[serve_tls]` diagnostic naming the missing cache handoff.
///
/// Transformation:
/// - Prevents mocked or live issuance paths from reporting success while
///   leaving the serving runtime without certificate material.
#[test]
pub(super) fn acme_runtime_tls_config_rejects_local_mock_issuer_without_cache_handoff() {
    let dir = temp_dir("auto_cache_mock_issuer_empty");
    fs::create_dir_all(&dir).expect("create temp project");
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);

    let message = match acme_runtime_tls_config_with_local_issuer(&dir, &tls, |_| Ok(())) {
        Ok(_) => panic!("issuer success without cache should fail"),
        Err(message) => message,
    };

    assert!(message.starts_with("error[serve_tls]: ACME issuer completed without writing"));
    assert!(message.contains("fullchain.pem"));
    assert!(message.contains("privkey.pem"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies auto TLS rejects certificate caches without renewal metadata.
///
/// Inputs:
/// - Auto TLS cache containing certificate and key files but no
///   `renewal.json`.
///
/// Output:
/// - Stable `error[serve_tls]` diagnostic naming missing cache metadata.
///
/// Transformation:
/// - Prevents auto TLS from loading unbounded certificate caches that cannot
///   be renewed before expiry.
#[test]
pub(super) fn runtime_tls_config_rejects_auto_tls_cache_without_renewal_metadata() {
    let dir = temp_dir("auto_cache_missing_metadata");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let generated =
        generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");
    let cache_dir = dir.join(".terlan/tls/acme");
    fs::create_dir_all(&cache_dir).expect("create acme cache");
    fs::write(cache_dir.join("fullchain.pem"), generated.cert.pem()).expect("write cached cert");
    fs::write(
        cache_dir.join("privkey.pem"),
        generated.key_pair.serialize_pem(),
    )
    .expect("write cached key");

    let message = match runtime_tls_config(&web_root) {
        Ok(_) => panic!("metadata-less cache should fail"),
        Err(message) => message,
    };

    assert!(message.starts_with("error[serve_tls]: failed to read ACME certificate cache metadata"));
    assert!(message.contains("renewal.json"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies malformed auto TLS cache metadata is rejected before serving.
///
/// Inputs:
/// - Auto TLS cache containing certificate/key material.
/// - Invalid `renewal.json` content.
///
/// Output:
/// - Stable `error[serve_tls]` diagnostic naming malformed cache metadata.
///
/// Transformation:
/// - Ensures local cache corruption fails closed before `rustls` configuration
///   is returned to the live listener.
#[test]
pub(super) fn runtime_tls_config_rejects_malformed_auto_tls_certificate_cache_metadata() {
    let dir = temp_dir("auto_cache_malformed_metadata");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
    let generated =
        generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");
    store_acme_certificate_cache(
        &plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )
    .expect("write cached certificate");
    fs::write(&plan.renewal_metadata_path, "{not valid json").expect("write malformed metadata");

    let message = match runtime_tls_config(&web_root) {
        Ok(_) => panic!("malformed cache metadata should fail"),
        Err(message) => message,
    };

    assert!(
        message.starts_with("error[serve_tls]: failed to parse ACME certificate cache metadata")
    );
    assert!(message.contains("renewal.json"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies future-dated auto TLS cache metadata is rejected before serving.
///
/// Inputs:
/// - Auto TLS cache with certificate/key material.
/// - Renewal metadata whose issue timestamp is beyond tolerated clock skew.
///
/// Output:
/// - Stable `error[serve_tls]` diagnostic naming future-dated metadata.
///
/// Transformation:
/// - Prevents accidentally or maliciously future-dated cache metadata from
///   extending automatic certificate lifetime.
#[test]
pub(super) fn runtime_tls_config_rejects_future_dated_auto_tls_certificate_cache_metadata() {
    let dir = temp_dir("auto_cache_future_metadata");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
    let generated =
        generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");
    store_acme_certificate_cache(
        &plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )
    .expect("write cached certificate");
    store_acme_certificate_cache_metadata(&plan, SystemTime::now() + Duration::from_secs(3600))
        .expect("write future metadata");

    let message = match runtime_tls_config(&web_root) {
        Ok(_) => panic!("future-dated cache metadata should fail"),
        Err(message) => message,
    };

    assert!(message.starts_with("error[serve_tls]: ACME certificate cache metadata"));
    assert!(message.contains("renewal.json"));
    assert!(message.contains("dated in the future"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies stale auto TLS cache metadata is rejected before serving.
///
/// Inputs:
/// - Auto TLS cache with certificate/key material and expired renewal metadata.
///
/// Output:
/// - Stable `error[serve_tls]` diagnostic requiring renewal.
///
/// Transformation:
/// - Ensures cached automatic certificates are not served past the configured
///   renewal boundary.
#[test]
pub(super) fn runtime_tls_config_rejects_stale_auto_tls_certificate_cache() {
    let dir = temp_dir("auto_cache_stale_metadata");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
    let generated =
        generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");
    store_acme_certificate_cache(
        &plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )
    .expect("write cached certificate");
    store_acme_certificate_cache_metadata(
        &plan,
        SystemTime::now() - ACME_RENEWAL_INTERVAL - Duration::from_secs(1),
    )
    .expect("write stale metadata");

    let message = match runtime_tls_config(&web_root) {
        Ok(_) => panic!("stale cache should fail"),
        Err(message) => message,
    };

    assert!(message.starts_with("error[serve_tls]: automatic ACME TLS cache"));
    assert!(message.contains("requires renewal"));
    assert!(message.contains("example.test"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies auto TLS reaches the ACME runtime boundary.
///
/// Inputs:
/// - A package with adjacent auto `[server.tls]` project metadata.
///
/// Output:
/// - Test passes when the stable local-cache diagnostic is returned.
///
/// Transformation:
/// - Keeps ACME mode explicit at runtime until certificate issuance/cache
///   storage is implemented.
#[test]
pub(super) fn runtime_tls_config_rejects_auto_tls_without_certificate_cache() {
    let dir = temp_dir("auto");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );

    let message = match runtime_tls_config(&web_root) {
        Ok(_) => panic!("auto TLS should require a certificate cache"),
        Err(message) => message,
    };

    assert!(message.starts_with("error[serve_tls]: automatic ACME TLS"));
    assert!(message.contains("example.test"));
    assert!(message.contains("primary provider `letsencrypt`"));
    assert!(message.contains(".terlan/tls/acme"));
    assert!(message.contains("fullchain.pem"));
    assert!(message.contains("privkey.pem"));
    assert!(message.contains("mode `manual` or `internal`"));
    fs::remove_dir_all(dir).expect("cleanup");
}
