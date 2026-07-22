use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_vm_web_security_policy, validate_policy_surface_entries};

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
            "terlan-vm-web-security-policy-{name}-{}-{unique}",
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
            "std/http/Cookies.terl",
            r#"
pub type SameSite = Lax | Strict | None.
pub struct Options { same_site: Option[SameSite] }.
set_header_options set_header_with_options delete_header
"#,
        )?;
        self.write(
            "std/http/Response.terl",
            r#"
pub struct SecurityHeaders
pub default_security_headers(): SecurityHeaders
pub production_security_headers(): SecurityHeaders
pub redirect(location: String, status: Int = 302): Response
pub (mut response: Response) with_header
pub (mut response: Response) security_headers
pub (mut response: Response) with_security_headers
pub (mut response: Response) set_cookie_header
pub (mut response: Response) cookie_with_options
pub (mut response: Response) with_cookies
Strict-Transport-Security
"#,
        )?;
        self.write(
            "std/http/ResponseTest.terl",
            r#"
response_cookie_helpers_record_metadata
response_chainable_metadata_helpers_record_metadata
response_security_headers_record_typed_policy_metadata
response_production_security_headers_record_hsts_metadata
response_security_policy_markers_typecheck
sample_cookie_with_options sample_delete_cookie sample_redirect
"#,
        )?;
        self.write(
            "std/http/CookiesTest.terl",
            "cookie_full_options_header_serializes_attributes",
        )?;
        self.write(
            "std/http/SessionTest.terl",
            "session_get_and_response_threading_execute",
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_session.rs",
            r#"
{SESSION_COOKIE_NAME}={session_id}; Path=/; HttpOnly; SameSite=Lax
SESSION_COOKIE_NAME expire(runtime runtime.rotate(session)
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/handler/response_bridge.rs",
            r#"
validate_response_header Response.redirect expects String
unsupported cookie SameSite value Set-Cookie Location
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http.rs",
            r#"
VM HTTP request exceeded 64 KiB header limit
VM HTTP request exceeded 1 MiB body limit
VM HTTP response exceeded 64 KiB header limit
VM HTTP response exceeded 1 MiB body limit
httparse
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls.rs",
            r#"
RuntimeTlsConfig ProjectServerTlsMode::Manual ProjectServerTlsMode::Internal
ProjectServerTlsMode::Auto instant_acme::LetsEncrypt::Production.url()
is_acme_http01_token
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/serve_test.rs",
            r#"
VM HTTP request exceeded 1 MiB body limit
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/serve_test/package_validation_test.rs",
            "build_http_response_rejects_invalid_http_metadata\n",
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_test.rs",
            r#"
vm_http_rejects_oversized_request_headers
vm_http_rejects_oversized_response_headers
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls_test.rs",
            r#"
runtime_tls_config_accepts_internal_local_tls
acme_runtime_plan_defaults_to_lets_encrypt_production
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
vm-web-security-policy-check: web-asset-pipeline-check
	$(MAKE) http-tls-check
	$(MAKE) native-boundary-http-cookie-check
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_web_security_policy_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-web-security-policy
"#;

#[test]
fn vm_web_security_policy_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_web_security_policy(repo.root()).expect("quality check");

    assert_eq!(summary.route_policy_count, 5);
    assert_eq!(summary.rejected_request_fixture_count, 10);
    assert_eq!(summary.rejected_policy_path_count, 9);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-web-security-policy-report-v1"));
    assert!(report.contains("routePolicyMatrix"));
    assert!(report.contains("typed CSRF token issuance and replay checks"));
    assert!(!report.contains("typed HSTS/secure-header policy composition"));
    assert!(report.contains("Strict-Transport-Security production response header"));
    assert!(report.contains("client command token authorization rejected"));
    assert!(report.contains("middleware.rejectedUntilTypedComposition"));
    assert!(report.contains("template.rejectedUntilTypedSecurityComposition"));
    assert!(report.contains("live-template-stream.rejectedUntilCommandAuthorizationPolicy"));
    assert!(
        !report.to_ascii_lowercase().contains("placeholder"),
        "report must not carry placeholder policy surface evidence: {report}"
    );
}

#[test]
fn vm_web_security_policy_rejects_placeholder_policy_surfaces() {
    let diagnostics = validate_policy_surface_entries(&["middleware-placeholder"]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder language")),
        "expected placeholder policy surface diagnostic: {diagnostics:?}"
    );
}

#[test]
fn vm_web_security_policy_rejects_missing_cookie_anchor() {
    let repo = TestRepo::new("missing-cookie-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("std/http/Cookies.terl");
    let source = fs::read_to_string(&path).expect("cookie source");
    repo.write(
        "std/http/Cookies.terl",
        &source.replace("same_site: Option[SameSite]", ""),
    )
    .expect("rewrite cookie source");

    let error = run_vm_web_security_policy(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("same_site: Option[SameSite]"));
}

#[test]
fn vm_web_security_policy_rejects_missing_http_limit_anchor() {
    let repo = TestRepo::new("missing-http-limit-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/runtime/vm/http.rs");
    let source = fs::read_to_string(&path).expect("http source");
    repo.write(
        "crates/terlan/src/runtime/vm/http.rs",
        &source.replace("VM HTTP response exceeded 1 MiB body limit", ""),
    )
    .expect("rewrite http source");

    let error = run_vm_web_security_policy(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("VM HTTP response exceeded 1 MiB body limit"));
}

#[test]
fn vm_web_security_policy_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate-term").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("$(MAKE) native-boundary-http-cookie-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_web_security_policy(repo.root()).expect_err("gate should fail");

    assert!(error.contains("native-boundary-http-cookie-check"));
}
