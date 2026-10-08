use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_vm_web_deployment_profile, validate_no_placeholder_report_entries};

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
            "terlan-vm-web-deployment-profile-{name}-{}-{unique}",
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
            "crates/terlan/src/commands/serve/args.rs",
            r#"
DEFAULT_SERVE_HOST DEFAULT_SERVE_PORT ServeArgs host: String port: u16
terlc serve --host requires a value
terlc serve --port expects a u16 value
"#,
        )?;
        self.write(
            "std/http/native/src/routing.rs",
            r#"
pub enum RouteMethod pub enum RouteTarget pub struct Router
pub fn dispatch( pub fn route( SseEndpoint( WebSocketEndpoint(
"#,
        )?;
        self.write(
            "std/http/Router.terl",
            "pub (router: Router) sse(\npub (router: Router) websocket(",
        )?;
        self.write(
            "std/http/native/src/routing/tests.rs",
            r#"
deployment_routes_preserve_handler_and_channel_targets
RouteMethod::Get "/health" "/assets/app.js" "/events" "/socket"
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls/acme_runtime.rs",
            r#"
runtime_tls_config_for_serve tls_runtime::load(
load_acme_runtime_tls_cache
"#,
        )?;
        self.write(
            "std/http/native/src/acme.rs",
            "acme_http01_challenge is_acme_http01_token",
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls/acme_runtime/tls_test/cache_custody.rs",
            "runtime_tls_config_for_serve_accepts_auto_tls_certificate_cache",
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls/acme_runtime/tls_test/tls_and_acme_fixtures.rs",
            "acme_http01_challenge_cache_rejects_invalid_token",
        )?;
        self.write(
            "crates/terlan/src/commands/serve/serve_test/static_fallbacks.rs",
            "hyper_request_handler_serves_acme_http01_challenge_from_auto_tls_cache",
        )?;
        self.write(
            "crates/terlan/src/commands/serve/serve_test/upgrades_and_acme.rs",
            r#"
vm_stream_request_serves_acme_http01_challenge_without_hyper
vm_stream_request_rejects_invalid_acme_http01_token_without_hyper
"#,
        )?;
        self.write(
            "std/http/Response.terl",
            r#"
pub redirect(location: String, status: Int = 302): Response
Location with_header Set-Cookie cookie_with_options
"#,
        )?;
        self.write(
            "std/http/Cookies.terl",
            r#"
SameSite http_only: Bool secure: Bool same_site_to_string
"#,
        )?;
        self.write(
            "std/http/native/src/response_headers.rs",
            "validate_response_header HeaderName::from_bytes HeaderValue::from_str content-length",
        )?;
        self.write(
            "std/http/native/src/bindings.rs",
            "dispatch.http.cookie.invalid_same_site CookieSameSite::Lax CookieSameSite::Strict CookieSameSite::None",
        )?;
        self.write("std/http/Session.terl", "set_header_with_options(")?;
        self.write(
            "std/http/SessionTest.terl",
            "; HttpOnly; SameSite=Lax; Path=/",
        )?;
        self.write(
            "std/http/Sse.terl",
            r#"
endpoint_with_keep_alive max_pending_events keep_alive_ms
"#,
        )?;
        self.write(
            "std/http/WebSocket.terl",
            r#"
endpoint max_pending_frames max_frame_bytes
"#,
        )?;
        self.write(
            "std/http/native/src/sse_session.rs",
            r#"
pub struct SseSession<C> crate::encode_event( pub fn flush_next(
"#,
        )?;
        self.write(
            "std/http/native/src/websocket.rs",
            r#"
pub fn upgrade_response tungstenite::handshake::derive_accept_key
"#,
        )?;
        self.write(
            "std/http/native/src/websocket/session.rs",
            "pub struct Session<C> pub fn enqueue_inbound( pub fn next_inbound(",
        )?;
        self.write(
            "std/http/native/src/websocket/handshake.rs",
            r#"
pub fn opening_handshake tungstenite::handshake::server::create_response_with_body
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/build/js_browser/manifest.rs",
            r#"
WebBuildManifest WebAssetArtifact web_relative_path fingerprint
source_js_manifest index.html
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/manifest_test.rs",
            r#"
healthcheck: "Location" validate_web_package_accepts_static_responses
validate_web_package_accepts_static_response_headers
"#,
        )?;
        self.write("std/http/native/src/tls_runtime.rs", "pub fn load( validate_acme_provider_supported load_acme_runtime_tls_cache issuer(&plan)?")?;
        self.write("Makefile", COMPLETE_MAKEFILE)
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const COMPLETE_MAKEFILE: &str = r#"
vm-web-deployment-profile-check: \
	vm-web-lifecycle-health-check \
	http-router-check \
	http-tls-check \
	native-boundary-http-cookie-check
	$(TERLAN_QUALITY) vm-web-deployment-profile
"#;

#[test]
fn vm_web_deployment_profile_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_web_deployment_profile(repo.root()).expect("quality check");

    assert_eq!(summary.profile_matrix_count, 8);
    assert_eq!(summary.proxy_fixture_count, 5);
    assert_eq!(summary.upgrade_case_count, 4);
    assert_eq!(summary.rejected_deployment_path_count, 10);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-web-deployment-profile-report-v1"));
    assert!(report.contains("Forwarded header is never trusted"));
    assert!(report.contains("ACME HTTP-01 token syntax is validated"));
    assert!(report.contains("static asset CDN URL generation"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_web_deployment_profile_rejects_missing_acme_anchor() {
    let repo = TestRepo::new("missing-acme").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let relative = "std/http/native/src/acme.rs";
    let path = repo.root().join(relative);
    let source = fs::read_to_string(&path).expect("tls source");
    repo.write(relative, &source.replace("is_acme_http01_token", ""))
        .expect("rewrite tls source");

    let error = run_vm_web_deployment_profile(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("is_acme_http01_token"));
}

#[test]
fn vm_web_deployment_profile_requires_package_owned_header_and_cookie_validation() {
    for (path, anchor) in [
        (
            "std/http/native/src/response_headers.rs",
            "HeaderValue::from_str",
        ),
        (
            "std/http/native/src/bindings.rs",
            "dispatch.http.cookie.invalid_same_site",
        ),
    ] {
        let repo = TestRepo::new("package-response-boundary").unwrap();
        repo.write_complete_fixture().unwrap();
        let source = fs::read_to_string(repo.root().join(path)).unwrap();
        repo.write(path, &source.replace(anchor, "")).unwrap();
        repo.write(
            "crates/terlan/src/commands/serve/handler/response_bridge.rs",
            anchor,
        )
        .unwrap();
        let error = run_vm_web_deployment_profile(repo.root())
            .expect_err("compiler fallback is not package ownership");
        assert!(error.contains(anchor));
    }
}

#[test]
fn vm_web_deployment_profile_rejects_missing_secure_cookie_anchor() {
    let repo = TestRepo::new("missing-cookie").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("std/http/Cookies.terl");
    let source = fs::read_to_string(&path).expect("cookie source");
    repo.write("std/http/Cookies.terl", &source.replace("secure: Bool", ""))
        .expect("rewrite cookie source");

    let error = run_vm_web_deployment_profile(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("secure: Bool"));
}

#[test]
fn vm_web_deployment_profile_rejects_legacy_router_and_codec_evidence() {
    for (owner, legacy, anchor) in [
        (
            "std/http/native/src/routing.rs",
            "crates/terlan/src/runtime/vm/http_router.rs",
            "pub fn dispatch(",
        ),
        (
            "std/http/native/src/routing/tests.rs",
            "crates/terlan/src/runtime/vm/http_router_test.rs",
            "deployment_routes_preserve_handler_and_channel_targets",
        ),
        (
            "std/http/Router.terl",
            "crates/terlan/src/runtime/vm/http_router.rs",
            "pub (router: Router) sse(",
        ),
        (
            "std/http/native/src/websocket.rs",
            "crates/terlan/src/runtime/vm/websocket.rs",
            "tungstenite::handshake::derive_accept_key",
        ),
        (
            "std/http/native/src/sse_session.rs",
            "crates/terlan/src/runtime/vm/sse.rs",
            "pub struct SseSession<C>",
        ),
        (
            "std/http/native/src/websocket/session.rs",
            "crates/terlan/src/runtime/vm/websocket.rs",
            "pub struct Session<C>",
        ),
    ] {
        let repo = TestRepo::new("package-deployment-owner").unwrap();
        repo.write_complete_fixture().unwrap();
        let source = fs::read_to_string(repo.root().join(owner)).unwrap();
        repo.write(owner, &source.replace(anchor, "")).unwrap();
        repo.write(legacy, anchor).unwrap();
        let error = run_vm_web_deployment_profile(repo.root()).unwrap_err();
        assert!(error.contains(owner) && error.contains(anchor), "{error}");
    }
}

#[test]
fn vm_web_deployment_profile_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("native-boundary-http-cookie-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_web_deployment_profile(repo.root()).expect_err("gate should fail");

    assert!(error.contains("native-boundary-http-cookie-check"));
}

#[test]
fn vm_web_deployment_profile_requires_its_own_prerequisites_and_recipe() {
    let repo = TestRepo::new("wrong-target").unwrap();
    repo.write_complete_fixture().unwrap();
    let unrelated = COMPLETE_MAKEFILE.replace("vm-web-deployment-profile-check:", "unrelated:");
    repo.write("Makefile", &unrelated).unwrap();
    let error = run_vm_web_deployment_profile(repo.root()).unwrap_err();
    assert!(error.contains("must declare prerequisite `http-router-check`"));
    assert!(error.contains("must run `$(TERLAN_QUALITY) vm-web-deployment-profile`"));

    let inline = COMPLETE_MAKEFILE.replace("\\\n\t", " ");
    repo.write("Makefile", &inline).unwrap();
    run_vm_web_deployment_profile(repo.root()).unwrap();

    repo.write(
        "Makefile",
        &inline.replace(
            "\t$(TERLAN_QUALITY) vm-web-deployment-profile",
            "\t# $(TERLAN_QUALITY) vm-web-deployment-profile",
        ),
    )
    .unwrap();
    assert!(run_vm_web_deployment_profile(repo.root())
        .unwrap_err()
        .contains("must run"));
}

#[test]
fn vm_web_deployment_profile_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries(
        "profile matrix",
        &["reverse proxy placeholder profile"],
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected placeholder report entry diagnostic: {diagnostics:?}"
    );
}
