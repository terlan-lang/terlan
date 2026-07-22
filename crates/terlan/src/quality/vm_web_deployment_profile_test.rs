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
            "crates/terlan/src/runtime/vm/http_router.rs",
            r#"
VmHttpRouteMethod VmHttpRouteTarget VmHttpRouter
dispatch( route( pub(crate) fn sse( pub(crate) fn websocket(
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_router_test.rs",
            r#"
VmHttpRouteMethod::Get
.dispatch(VmHttpRouteMethod::Get, "/health")
.dispatch(VmHttpRouteMethod::Get, "/assets/app.js")
.dispatch(VmHttpRouteMethod::Get, "/events")
.dispatch(VmHttpRouteMethod::Get, "/socket")
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls.rs",
            r#"
runtime_tls_config_for_serve acme_runtime_tls_config_for_serve
acme_http01_challenge is_acme_http01_token load_acme_runtime_tls_cache
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls_test.rs",
            r#"
runtime_tls_config_for_serve_accepts_auto_tls_certificate_cache
acme_http01_challenge_cache_rejects_invalid_token
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/serve_test.rs",
            r#"
hyper_request_handler_serves_acme_http01_challenge_from_auto_tls_cache
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
            "crates/terlan/src/commands/serve/handler/response_bridge.rs",
            r#"
validate_response_header Location Set-Cookie unsupported cookie SameSite value
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_session.rs",
            r#"
Path=/; HttpOnly; SameSite=Lax
"#,
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
            "crates/terlan/src/runtime/vm/sse.rs",
            r#"
VmSseEndpointPlan VmSseStream flush_next
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/websocket.rs",
            r#"
build_websocket_upgrade_response serialize_websocket_upgrade_response
VmWebSocketEndpointPlan
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/websocket.rs",
            r#"
websocket_upgrade_state WebSocketUpgradeState
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
        self.write("Makefile", COMPLETE_MAKEFILE)
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const COMPLETE_MAKEFILE: &str = r#"
vm-web-deployment-profile-check: vm-web-lifecycle-health-check
	$(MAKE) http-router-check
	$(MAKE) http-tls-check
	$(MAKE) native-boundary-http-cookie-check
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_web_deployment_profile_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-web-deployment-profile
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
    let path = repo.root().join("crates/terlan/src/commands/serve/tls.rs");
    let source = fs::read_to_string(&path).expect("tls source");
    repo.write(
        "crates/terlan/src/commands/serve/tls.rs",
        &source.replace("is_acme_http01_token", ""),
    )
    .expect("rewrite tls source");

    let error = run_vm_web_deployment_profile(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("is_acme_http01_token"));
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
fn vm_web_deployment_profile_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("$(MAKE) native-boundary-http-cookie-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_web_deployment_profile(repo.root()).expect_err("gate should fail");

    assert!(error.contains("native-boundary-http-cookie-check"));
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
