use std::fs;
use std::path::{Path, PathBuf};

use rcgen::generate_simple_self_signed;

use crate::commands::build::project_manifest::{
    ProjectServerTls, ProjectServerTlsMode, ProjectServerTlsProvider,
};
use crate::support::test_fs;

use super::{
    acme_contact_strings, acme_domain_identifiers, acme_runtime_plan, generate_acme_csr,
    issue_acme_certificate_cache, load_acme_account_credentials, pending_http01_challenges,
    runtime_tls_config, store_acme_account_credentials, store_acme_certificate_cache,
    store_acme_http01_challenge,
};

/// Sample serialized ACME credentials accepted by `instant-acme`.
const SAMPLE_ACCOUNT_CREDENTIALS_JSON: &str = r#"{"id":"id","key_pkcs8":"MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgJVWC_QzOTCS5vtsJp2IG-UDc8cdDfeoKtxSZxaznM-mhRANCAAQenCPoGgPFTdPJ7VLLKt56RxPlYT1wNXnHc54PEyBg3LxKaH0-sJkX0mL8LyPEdsfL_Oz4TxHkWLJGrXVtNhfH","directory":"https://acme-v02.api.letsencrypt.org/directory"}"#;

/// Creates an isolated temporary directory for TLS tests.
///
/// Inputs:
/// - `name`: descriptive test directory suffix.
///
/// Output:
/// - Absolute temporary directory path.
///
/// Transformation:
/// - Delegates to the shared test filesystem helper with the serve-TLS
///   namespace.
fn temp_dir(name: &str) -> PathBuf {
    test_fs::temp_path("serve_tls", name)
}

/// Writes a minimal valid web package manifest.
///
/// Inputs:
/// - `web_root`: package directory to create.
///
/// Output:
/// - Filesystem fixture containing `manifest.json` and `index.html`.
///
/// Transformation:
/// - Produces only the fields needed by the TLS boundary, because package
///   schema validation itself is owned by `manifest_test.rs`.
fn write_valid_package(web_root: &Path) {
    fs::create_dir_all(web_root).expect("create web root");
    fs::write(web_root.join("index.html"), "<main>Hello</main>").expect("write index");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "build_id": "tls-test",
  "index": "index.html",
  "assets": []
}"#,
    )
    .expect("write manifest");
}

/// Writes project metadata with a `[server.tls]` table.
///
/// Inputs:
/// - `path`: project manifest path.
/// - `tls`: raw TLS table body.
///
/// Output:
/// - `terlan.toml` fixture next to the package build output.
///
/// Transformation:
/// - Keeps package metadata minimal while varying the TLS configuration shape.
fn write_project_manifest(path: &Path, tls: &str) {
    fs::write(
        path,
        format!(
            r#"[package]
name = "serve_tls_demo"
version = "0.0.0"

[server.tls]
{tls}
"#
        ),
    )
    .expect("write project manifest");
}

fn write_self_signed_cert_pair(dir: &Path) {
    fs::create_dir_all(dir.join("certs")).expect("create cert dir");
    let generated =
        generate_simple_self_signed(vec!["localhost".to_string()]).expect("generate cert");
    fs::write(dir.join("certs/dev.pem"), generated.cert.pem()).expect("write cert fixture");
    fs::write(
        dir.join("certs/dev-key.pem"),
        generated.key_pair.serialize_pem(),
    )
    .expect("write key fixture");
}

/// Builds an auto TLS manifest model for ACME plan tests.
///
/// Inputs:
/// - `domains`: domain names requested for ACME certificates.
/// - `email`: optional ACME account email.
/// - `primary_provider`: optional primary ACME provider.
/// - `fallback_provider`: optional fallback ACME provider.
///
/// Output:
/// - Parsed project TLS model equivalent to `[server.tls] mode = "auto"`.
///
/// Transformation:
/// - Fills non-auto fields with `None` so tests exercise runtime ACME planning
///   rather than manifest parser validation.
fn auto_tls_model(
    domains: Vec<&str>,
    email: Option<&str>,
    primary_provider: Option<ProjectServerTlsProvider>,
    fallback_provider: Option<ProjectServerTlsProvider>,
) -> ProjectServerTls {
    ProjectServerTls {
        mode: ProjectServerTlsMode::Auto,
        domains: domains.into_iter().map(str::to_string).collect(),
        email: email.map(str::to_string),
        primary_provider,
        fallback_provider,
        cert: None,
        key: None,
        passphrase_env: None,
        ca: None,
        server_name: None,
        trust_local: None,
    }
}

/// Builds parsed sample ACME account credentials.
///
/// Inputs:
/// - Static JSON copied from the `instant-acme` credential shape.
///
/// Output:
/// - Opaque `instant-acme` account credentials value.
///
/// Transformation:
/// - Exercises the same serde boundary the runtime uses for stored account
///   caches without exposing private credential fields.
fn sample_account_credentials() -> instant_acme::AccountCredentials {
    serde_json::from_str(SAMPLE_ACCOUNT_CREDENTIALS_JSON).expect("sample account credentials")
}

/// Verifies plain HTTP packages do not configure TLS.
///
/// Inputs:
/// - A package without adjacent `[server.tls]` configuration.
///
/// Output:
/// - Test passes when runtime TLS config returns `None`.
///
/// Transformation:
/// - Locks the default local serve behavior as plain HTTP unless project
///   metadata explicitly opts into TLS.
#[test]
fn runtime_tls_config_returns_none_for_plain_http_package() {
    let dir = temp_dir("plain");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);

    assert!(runtime_tls_config(&web_root)
        .expect("plain package should pass TLS boundary")
        .is_none());

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies invalid manual TLS files fail before listener binding.
///
/// Inputs:
/// - A package with adjacent manual `[server.tls]` project metadata pointing
///   to non-PEM fixture files.
///
/// Output:
/// - Test passes when the stable `error[serve_tls]` diagnostic is returned.
///
/// Transformation:
/// - Exercises live rustls config loading without binding a network listener.
#[test]
fn runtime_tls_config_rejects_invalid_manual_tls_files() {
    let dir = temp_dir("configured");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    fs::create_dir_all(dir.join("certs")).expect("create cert dir");
    fs::write(dir.join("certs/dev.pem"), "dev cert").expect("write cert fixture");
    fs::write(dir.join("certs/dev-key.pem"), "dev key").expect("write key fixture");
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "manual"
cert = "certs/dev.pem"
key = "certs/dev-key.pem""#,
    );

    let message = match runtime_tls_config(&web_root) {
        Ok(_) => panic!("TLS config should fail"),
        Err(message) => message,
    };

    assert!(message.starts_with("error[serve_tls]: TLS certificate"));
    assert!(message.contains("did not contain any PEM certificates"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies manual TLS builds a rustls server configuration.
///
/// Inputs:
/// - A package with adjacent manual `[server.tls]` project metadata and PEM
///   certificate/key files.
///
/// Output:
/// - Test passes when runtime TLS configuration is present.
///
/// Transformation:
/// - Covers maintained PEM parsing and rustls config construction without
///   starting the long-running listener.
#[test]
fn runtime_tls_config_accepts_manual_certificate_tls() {
    let dir = temp_dir("manual");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_self_signed_cert_pair(&dir);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "manual"
cert = "certs/dev.pem"
key = "certs/dev-key.pem""#,
    );

    assert!(runtime_tls_config(&web_root)
        .expect("manual TLS config should load")
        .is_some());

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies internal TLS builds an in-memory local certificate.
///
/// Inputs:
/// - A package with adjacent internal `[server.tls]` project metadata.
///
/// Output:
/// - Test passes when runtime TLS configuration is present.
///
/// Transformation:
/// - Covers the local CA/self-signed runtime branch without external files.
#[test]
fn runtime_tls_config_accepts_internal_local_tls() {
    let dir = temp_dir("internal");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "internal"
server_name = "localhost""#,
    );

    assert!(runtime_tls_config(&web_root)
        .expect("internal TLS config should load")
        .is_some());

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies automatic TLS defaults to Let's Encrypt production.
///
/// Inputs:
/// - Auto TLS model with domains and no explicit provider.
///
/// Output:
/// - Test passes when the ACME plan selects Let's Encrypt production and the
///   project-local certificate cache directory.
///
/// Transformation:
/// - Locks the intended out-of-the-box production default without contacting
///   the ACME network.
#[test]
fn acme_runtime_plan_defaults_to_lets_encrypt_production() {
    let dir = temp_dir("auto_plan_default");
    let tls = auto_tls_model(vec!["example.com"], Some("admin@example.com"), None, None);

    let plan = acme_runtime_plan(&dir, &tls);

    assert_eq!(plan.domains, vec!["example.com".to_string()]);
    assert_eq!(plan.email, Some("admin@example.com".to_string()));
    assert_eq!(plan.primary_provider, ProjectServerTlsProvider::LetsEncrypt);
    assert_eq!(plan.fallback_provider, None);
    assert!(plan.directory_url.contains("letsencrypt"));
    assert_eq!(plan.cache_dir, dir.join(".terlan/tls/acme"));
    assert_eq!(
        plan.certificate_path,
        dir.join(".terlan/tls/acme/fullchain.pem")
    );
    assert_eq!(
        plan.private_key_path,
        dir.join(".terlan/tls/acme/privkey.pem")
    );
    assert_eq!(
        plan.account_credentials_path,
        dir.join(".terlan/tls/acme/account.json")
    );
    assert_eq!(
        plan.http01_challenge_dir,
        dir.join(".terlan/tls/acme/http-01")
    );
}

/// Verifies automatic TLS preserves fallback provider metadata.
///
/// Inputs:
/// - Auto TLS model with explicit Let's Encrypt primary and ZeroSSL fallback.
///
/// Output:
/// - Test passes when the ACME plan keeps both provider choices distinct.
///
/// Transformation:
/// - Ensures future issuance can try the documented fallback provider without
///   weakening the default primary provider.
#[test]
fn acme_runtime_plan_preserves_zerossl_fallback_provider() {
    let dir = temp_dir("auto_plan_fallback");
    let tls = auto_tls_model(
        vec!["example.com", "www.example.com"],
        Some("admin@example.com"),
        Some(ProjectServerTlsProvider::LetsEncrypt),
        Some(ProjectServerTlsProvider::ZeroSsl),
    );

    let plan = acme_runtime_plan(&dir, &tls);

    assert_eq!(
        plan.domains,
        vec!["example.com".to_string(), "www.example.com".to_string()]
    );
    assert_eq!(plan.primary_provider, ProjectServerTlsProvider::LetsEncrypt);
    assert_eq!(
        plan.fallback_provider,
        Some(ProjectServerTlsProvider::ZeroSsl)
    );
    assert!(plan.directory_url.contains("letsencrypt"));
}

/// Verifies ACME domain conversion preserves manifest domains.
///
/// Inputs:
/// - Auto TLS domain strings from a runtime plan.
///
/// Output:
/// - Test passes when domains become ACME DNS identifiers.
///
/// Transformation:
/// - Covers the local preflight boundary before domains are passed to
///   `instant-acme`.
#[test]
fn acme_domain_identifiers_preserve_dns_names() {
    let identifiers =
        acme_domain_identifiers(&["example.com".to_string(), "www.example.com".to_string()])
            .expect("domain identifiers");

    assert_eq!(
        serde_json::to_value(&identifiers).expect("identifier json"),
        serde_json::json!([
            {"type": "dns", "value": "example.com"},
            {"type": "dns", "value": "www.example.com"}
        ])
    );
}

/// Verifies ACME domain conversion rejects empty input.
///
/// Inputs:
/// - Empty and whitespace-only domain lists.
///
/// Output:
/// - Test passes when stable `error[serve_tls]` diagnostics are returned.
///
/// Transformation:
/// - Keeps malformed domain state out of the maintained ACME client.
#[test]
fn acme_domain_identifiers_reject_empty_domains() {
    let message = acme_domain_identifiers(&[]).expect_err("empty domains should fail");
    assert_eq!(
        message,
        "error[serve_tls]: automatic ACME TLS requires at least one domain"
    );

    let message =
        acme_domain_identifiers(&["  ".to_string()]).expect_err("blank domain should fail");
    assert_eq!(
        message,
        "error[serve_tls]: automatic ACME TLS domain cannot be empty"
    );
}

/// Verifies ACME account contact URI generation.
///
/// Inputs:
/// - Optional manifest email address.
///
/// Output:
/// - Test passes when present email becomes one `mailto:` contact and absent
///   email becomes no contact entries.
///
/// Transformation:
/// - Covers the contact slice passed into `instant-acme::NewAccount`.
#[test]
fn acme_contact_strings_wrap_optional_email() {
    assert_eq!(
        acme_contact_strings(Some(" admin@example.com ")),
        vec!["mailto:admin@example.com".to_string()]
    );
    assert!(acme_contact_strings(Some("   ")).is_empty());
    assert!(acme_contact_strings(None).is_empty());
}

/// Verifies pending HTTP-01 challenge selection.
///
/// Inputs:
/// - ACME authorization fixture containing a pending HTTP-01 challenge.
///
/// Output:
/// - Test passes when the selector returns the expected identifier, token, and
///   challenge URL.
///
/// Transformation:
/// - Locks Terlan auto TLS to HTTP-01 without contacting an ACME server.
#[test]
fn pending_http01_challenges_select_pending_http_challenges() {
    let authorization = serde_json::from_str::<instant_acme::Authorization>(
        r#"{
          "status": "pending",
          "identifier": {"type": "dns", "value": "www.example.org"},
          "challenges": [
            {
              "type": "dns-01",
              "url": "https://example.com/acme/chall/dns",
              "status": "pending",
              "token": "dns_token"
            },
            {
              "type": "http-01",
              "url": "https://example.com/acme/chall/http",
              "status": "pending",
              "token": "http_token"
            }
          ]
        }"#,
    )
    .expect("authorization fixture");

    let authorizations = [authorization];
    let selected = pending_http01_challenges(&authorizations).expect("http01 challenge");

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].challenge.token, "http_token");
    assert_eq!(
        selected[0].challenge.url,
        "https://example.com/acme/chall/http"
    );
}

/// Verifies already-valid ACME authorizations do not require challenges.
///
/// Inputs:
/// - ACME authorization fixture marked valid.
///
/// Output:
/// - Test passes when no pending challenges are returned.
///
/// Transformation:
/// - Allows ACME account/order reuse when the CA has already validated a
///   domain authorization.
#[test]
fn pending_http01_challenges_skip_valid_authorizations() {
    let authorization = serde_json::from_str::<instant_acme::Authorization>(
        r#"{
          "status": "valid",
          "identifier": {"type": "dns", "value": "www.example.org"},
          "challenges": []
        }"#,
    )
    .expect("authorization fixture");

    assert!(pending_http01_challenges(&[authorization])
        .expect("valid authorization")
        .is_empty());
}

/// Verifies missing HTTP-01 challenges produce stable diagnostics.
///
/// Inputs:
/// - Pending ACME authorization fixture with only DNS-01.
///
/// Output:
/// - Test passes when a stable `error[serve_tls]` diagnostic is returned.
///
/// Transformation:
/// - Prevents automatic TLS from silently selecting a challenge type Terlan
///   does not serve.
#[test]
fn pending_http01_challenges_reject_missing_http01() {
    let authorization = serde_json::from_str::<instant_acme::Authorization>(
        r#"{
          "status": "pending",
          "identifier": {"type": "dns", "value": "www.example.org"},
          "challenges": [
            {
              "type": "dns-01",
              "url": "https://example.com/acme/chall/dns",
              "status": "pending",
              "token": "dns_token"
            }
          ]
        }"#,
    )
    .expect("authorization fixture");

    let authorizations = [authorization];
    let message = match pending_http01_challenges(&authorizations) {
        Ok(_) => panic!("missing HTTP-01 should fail"),
        Err(message) => message,
    };

    assert_eq!(
        message,
        "error[serve_tls]: ACME authorization for `www.example.org` did not offer HTTP-01"
    );
}

/// Verifies ACME CSR generation returns runtime-usable key material.
///
/// Inputs:
/// - Domain list for one certificate order.
///
/// Output:
/// - Test passes when CSR bytes are non-empty and private key PEM parses
///   through the certificate-cache writer.
///
/// Transformation:
/// - Covers the maintained `rcgen` boundary used before ACME finalization.
#[test]
fn generate_acme_csr_returns_der_and_private_key_pem() {
    let dir = temp_dir("auto_csr");
    let tls = auto_tls_model(vec!["example.test"], Some("admin@example.test"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
    let (csr_der, private_key_pem) =
        generate_acme_csr(&["example.test".to_string()]).expect("generate csr");
    let generated =
        generate_simple_self_signed(vec!["example.test".to_string()]).expect("generate cert");

    assert!(!csr_der.is_empty());
    assert!(private_key_pem.contains("PRIVATE KEY"));
    store_acme_certificate_cache(&plan, &generated.cert.pem(), &private_key_pem)
        .expect("generated private key should be runtime-parseable");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies unsupported ACME providers fail before network issuance.
///
/// Inputs:
/// - Auto TLS runtime plan using ZeroSSL as the primary provider.
///
/// Output:
/// - Test passes when the issuer returns a stable provider diagnostic.
///
/// Transformation:
/// - Exercises the production issuer entry point without contacting an ACME
///   server, preserving the rule that unsupported provider-specific account
///   requirements must fail before partial cache writes.
#[tokio::test]
async fn issue_acme_certificate_cache_rejects_zerossl_before_network() {
    let dir = temp_dir("auto_issue_zerossl");
    let tls = auto_tls_model(
        vec!["example.test"],
        Some("admin@example.test"),
        Some(ProjectServerTlsProvider::ZeroSsl),
        None,
    );
    let plan = acme_runtime_plan(&dir, &tls);

    let message = issue_acme_certificate_cache(&plan)
        .await
        .expect_err("ZeroSSL issuance should fail before network");

    assert_eq!(
        message,
        "error[serve_tls]: ZeroSSL automatic issuance requires external account binding support"
    );
    assert!(!plan.cache_dir.exists());
}

/// Verifies ACME account credentials are stored in the project cache.
///
/// Inputs:
/// - Parsed sample `instant-acme` account credentials.
///
/// Output:
/// - Test passes when the credentials round-trip through the deterministic
///   account cache file.
///
/// Transformation:
/// - Covers the durable account identity cache needed by automatic renewal.
#[test]
fn acme_account_credentials_round_trip_through_cache() {
    let dir = temp_dir("auto_account_cache");
    let tls = auto_tls_model(vec!["example.com"], Some("admin@example.com"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
    let credentials = sample_account_credentials();

    assert!(load_acme_account_credentials(&plan)
        .expect("missing account cache should be ok")
        .is_none());
    store_acme_account_credentials(&plan, &credentials).expect("store account credentials");
    let loaded = load_acme_account_credentials(&plan)
        .expect("load account credentials")
        .expect("stored account credentials");

    let expected_json = serde_json::to_value(&credentials).expect("expected json");
    let loaded_json = serde_json::to_value(&loaded).expect("loaded json");
    assert_eq!(loaded_json, expected_json);

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies ACME account cache parse failures are stable.
///
/// Inputs:
/// - Invalid JSON at the account cache path.
///
/// Output:
/// - Test passes when a stable `error[serve_tls]` diagnostic is returned.
///
/// Transformation:
/// - Prevents corrupt account cache state from being silently ignored during
///   automatic renewal.
#[test]
fn acme_account_credentials_cache_reports_invalid_json() {
    let dir = temp_dir("auto_account_invalid");
    let tls = auto_tls_model(vec!["example.com"], Some("admin@example.com"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);
    fs::create_dir_all(plan.account_credentials_path.parent().expect("parent"))
        .expect("create cache dir");
    fs::write(&plan.account_credentials_path, "not json").expect("write invalid credentials");

    let message = match load_acme_account_credentials(&plan) {
        Ok(_) => panic!("invalid account credentials should fail"),
        Err(message) => message,
    };

    assert!(message.starts_with("error[serve_tls]: failed to parse ACME account credentials"));
    assert!(message.contains("account.json"));

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies ACME HTTP-01 challenge responses are stored safely.
///
/// Inputs:
/// - Valid token and key authorization response.
///
/// Output:
/// - Test passes when the response is written under the HTTP-01 challenge dir.
///
/// Transformation:
/// - Covers the file that the HTTP handler serves for Let’s Encrypt challenge
///   probes.
#[test]
fn acme_http01_challenge_cache_writes_valid_token() {
    let dir = temp_dir("auto_http01_write");
    let tls = auto_tls_model(vec!["example.com"], Some("admin@example.com"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);

    let path = store_acme_http01_challenge(&plan, "token_123", "token_123.thumbprint")
        .expect("store challenge");

    assert_eq!(path, plan.http01_challenge_dir.join("token_123"));
    assert_eq!(
        fs::read_to_string(path).expect("challenge body"),
        "token_123.thumbprint"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies unsafe ACME HTTP-01 challenge tokens are rejected before writing.
///
/// Inputs:
/// - Token containing a dot.
///
/// Output:
/// - Test passes when no cache file is written.
///
/// Transformation:
/// - Shares the same token validation policy as the request handler.
#[test]
fn acme_http01_challenge_cache_rejects_invalid_token() {
    let dir = temp_dir("auto_http01_invalid");
    let tls = auto_tls_model(vec!["example.com"], Some("admin@example.com"), None, None);
    let plan = acme_runtime_plan(&dir, &tls);

    let message = store_acme_http01_challenge(&plan, "bad.token", "value")
        .expect_err("invalid token should fail");

    assert!(message.contains("ACME HTTP-01 token `bad.token` is invalid"));
    assert!(!plan.http01_challenge_dir.exists());
}

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
fn acme_certificate_cache_write_feeds_runtime_tls_config() {
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
    assert!(runtime_tls_config(&web_root)
        .expect("auto TLS cache should load")
        .is_some());

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
fn runtime_tls_config_accepts_auto_tls_certificate_cache() {
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
    let cache_dir = dir.join(".terlan/tls/acme");
    fs::create_dir_all(&cache_dir).expect("create acme cache");
    fs::write(cache_dir.join("fullchain.pem"), generated.cert.pem()).expect("write cached cert");
    fs::write(
        cache_dir.join("privkey.pem"),
        generated.key_pair.serialize_pem(),
    )
    .expect("write cached key");

    assert!(runtime_tls_config(&web_root)
        .expect("auto TLS cache should load")
        .is_some());

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
fn runtime_tls_config_rejects_auto_tls_without_certificate_cache() {
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
