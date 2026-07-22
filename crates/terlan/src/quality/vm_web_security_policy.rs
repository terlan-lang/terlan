use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-web-security-policy-report.json";

const REQUIRED_COOKIE_STD_ANCHORS: &[&str] = &[
    "pub type SameSite = Lax | Strict | None.",
    "pub struct Options",
    "same_site: Option[SameSite]",
    "set_header_options",
    "set_header_with_options",
    "delete_header",
];

const REQUIRED_RESPONSE_STD_ANCHORS: &[&str] = &[
    "pub struct SecurityHeaders",
    "pub default_security_headers(): SecurityHeaders",
    "pub production_security_headers(): SecurityHeaders",
    "pub redirect(location: String, status: Int = 302): Response",
    "pub (mut response: Response) with_header",
    "pub (mut response: Response) security_headers",
    "pub (mut response: Response) with_security_headers",
    "pub (mut response: Response) set_cookie_header",
    "pub (mut response: Response) cookie_with_options",
    "pub (mut response: Response) with_cookies",
    "Strict-Transport-Security",
];

const REQUIRED_RESPONSE_TEST_ANCHORS: &[&str] = &[
    "response_cookie_helpers_record_metadata",
    "response_chainable_metadata_helpers_record_metadata",
    "response_security_headers_record_typed_policy_metadata",
    "response_production_security_headers_record_hsts_metadata",
    "response_security_policy_markers_typecheck",
    "sample_cookie_with_options",
    "sample_delete_cookie",
    "sample_redirect",
];

const REQUIRED_SESSION_ANCHORS: &[&str] = &[
    "{SESSION_COOKIE_NAME}={session_id}; Path=/; HttpOnly; SameSite=Lax",
    "SESSION_COOKIE_NAME",
    "expire(runtime",
    "runtime.rotate(session)",
];

const REQUIRED_HANDLER_ANCHORS: &[&str] = &[
    "validate_response_header",
    "Response.redirect expects String",
    "unsupported cookie SameSite value",
    "Set-Cookie",
    "Location",
];

const REQUIRED_HTTP_LIMIT_ANCHORS: &[&str] = &[
    "VM HTTP request exceeded 64 KiB header limit",
    "VM HTTP request exceeded 1 MiB body limit",
    "VM HTTP response exceeded 64 KiB header limit",
    "VM HTTP response exceeded 1 MiB body limit",
    "httparse",
];

const REQUIRED_TLS_ANCHORS: &[&str] = &[
    "RuntimeTlsConfig",
    "ProjectServerTlsMode::Manual",
    "ProjectServerTlsMode::Internal",
    "ProjectServerTlsMode::Auto",
    "instant_acme::LetsEncrypt::Production.url()",
    "is_acme_http01_token",
];

const REQUIRED_SECURITY_TEST_ANCHORS: &[&str] = &[
    "cookie_full_options_header_serializes_attributes",
    "session_get_and_response_threading_execute",
    "build_http_response_rejects_invalid_http_metadata",
    "vm_http_rejects_oversized_request_headers",
    "vm_http_rejects_oversized_response_headers",
    "runtime_tls_config_accepts_internal_local_tls",
    "acme_runtime_plan_defaults_to_lets_encrypt_production",
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-web-security-policy-check: web-asset-pipeline-check",
    "$(MAKE) http-tls-check",
    "$(MAKE) native-boundary-http-cookie-check",
    "vm_web_security_policy_test",
    "vm-web-security-policy",
];

const ROUTE_POLICY_MATRIX: &[&str] = &[
    "dynamic handler response headers validated through http::HeaderName/HeaderValue",
    "redirect metadata validated as Location header",
    "static/file response bodies constrained by VM HTTP body limit",
    "session actor cookie defaults include HttpOnly and SameSite=Lax",
    "ACME HTTP-01 route token names are path-safe before cache lookup",
];

const MIDDLEWARE_COMPOSITION: &[&str] = &[
    "route",
    "middleware.rejectedUntilTypedComposition",
    "template.rejectedUntilTypedSecurityComposition",
    "static-asset",
    "stateful-session-actor",
    "live-template-stream.rejectedUntilCommandAuthorizationPolicy",
    "environment-tls-config",
];

const PLACEHOLDER_POLICY_TERMS: &[&str] = &["placeholder", "todo", "tbd", "unknown"];

const REJECTED_REQUEST_FIXTURES: &[&str] = &[
    "CSRF replay",
    "missing SameSite",
    "insecure cookie flags in production",
    "CORS wildcard credential leak",
    "CSP bypass",
    "header injection",
    "redirect injection",
    "oversized uploads",
    "stale live-template command token",
    "mixed dev/prod security config",
];

const HEADER_SNAPSHOTS: &[&str] = &[
    "Location redirect header",
    "Set-Cookie metadata header",
    "Content-Type response header",
    "Content-Length response header",
    "Cache-Control static response header",
    "X-Content-Type-Options static response header",
    "X-Frame-Options secure response header",
    "Referrer-Policy secure response header",
    "Strict-Transport-Security production response header",
];

const COOKIE_SNAPSHOTS: &[&str] = &[
    "session cookie Path=/",
    "session cookie HttpOnly",
    "session cookie SameSite=Lax",
    "typed SameSite=Strict option",
    "typed Secure option",
    "typed Max-Age/Expires option",
];

const LIVE_TEMPLATE_AUTHORIZATION_CHECKS: &[&str] = &[
    "stream protocol event shape documented",
    "client command token authorization rejected until typed policy exists",
    "stale command token fixture rejected until typed policy exists",
];

const ENVIRONMENT_CONFIG_DECISIONS: &[&str] = &[
    "manual TLS loads project-relative certificate paths",
    "internal TLS uses generated local certificate",
    "auto TLS defaults to Let's Encrypt production",
    "ACME live issuance requires explicit compiler feature/env enablement",
    "auto TLS challenge token rejects path-sensitive characters",
];

const REJECTED_POLICY_PATHS: &[&str] = &[
    "typed CSRF token issuance and replay checks",
    "typed CORS policy composition",
    "typed CSP policy composition",
    "typed redirect allow-list policy",
    "typed upload limit policy",
    "typed live-template command authorization",
    "editor hover policy explanation",
    "generated docs policy matrix",
    "support bundle policy export",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmWebSecurityPolicySummary {
    pub route_policy_count: usize,
    pub rejected_request_fixture_count: usize,
    pub rejected_policy_path_count: usize,
    pub report_path: PathBuf,
}

pub fn run_vm_web_security_policy(root: &Path) -> QualityResult<VmWebSecurityPolicySummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "std/http/Cookies.terl",
        REQUIRED_COOKIE_STD_ANCHORS,
        "typed cookie policy stdlib surface",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/http/Response.terl",
        REQUIRED_RESPONSE_STD_ANCHORS,
        "typed response security helpers",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/http/ResponseTest.terl",
        REQUIRED_RESPONSE_TEST_ANCHORS,
        "response security stdlib tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session.rs",
        REQUIRED_SESSION_ANCHORS,
        "VM session cookie policy",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/handler/response_bridge.rs",
        REQUIRED_HANDLER_ANCHORS,
        "VM handler response validation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http.rs",
        REQUIRED_HTTP_LIMIT_ANCHORS,
        "VM HTTP parser and body limits",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/tls.rs",
        REQUIRED_TLS_ANCHORS,
        "TLS and ACME policy boundary",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/http/CookiesTest.terl",
        &["cookie_full_options_header_serializes_attributes"],
        "typed cookie tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/http/SessionTest.terl",
        &["session_get_and_response_threading_execute"],
        "typed session tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/serve_test.rs",
        &["VM HTTP request exceeded 1 MiB body limit"],
        "serve security tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/serve_test/package_validation_test.rs",
        &["build_http_response_rejects_invalid_http_metadata"],
        "serve package security tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_test.rs",
        &[
            "vm_http_rejects_oversized_request_headers",
            "vm_http_rejects_oversized_response_headers",
        ],
        "VM HTTP adversarial limit tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/tls_test.rs",
        &[
            "runtime_tls_config_accepts_internal_local_tls",
            "acme_runtime_plan_defaults_to_lets_encrypt_production",
        ],
        "TLS security tests",
    )?);
    diagnostics.extend(validate_required_security_test_terms(root)?);
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_policy_surface_entries(MIDDLEWARE_COMPOSITION));
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-web-security-policy", &diagnostics));
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
        "schema": "terlan-vm-web-security-policy-report-v1",
        "routePolicyMatrix": ROUTE_POLICY_MATRIX,
        "middlewareComposition": {
            "implemented": false,
            "surfaces": MIDDLEWARE_COMPOSITION,
            "reason": "security policy is observable for current concrete surfaces, but cross-route/middleware/template composition is not typed yet"
        },
        "rejectedRequestFixtures": REJECTED_REQUEST_FIXTURES,
        "headerSnapshots": HEADER_SNAPSHOTS,
        "cookieSnapshots": COOKIE_SNAPSHOTS,
        "liveTemplateCommandAuthorizationChecks": {
            "implemented": false,
            "checks": LIVE_TEMPLATE_AUTHORIZATION_CHECKS
        },
        "environmentConfigDecisions": ENVIRONMENT_CONFIG_DECISIONS,
        "rejectedPolicyPaths": REJECTED_POLICY_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM web security policy report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmWebSecurityPolicySummary {
        route_policy_count: ROUTE_POLICY_MATRIX.len(),
        rejected_request_fixture_count: REJECTED_REQUEST_FIXTURES.len(),
        rejected_policy_path_count: REJECTED_POLICY_PATHS.len(),
        report_path,
    })
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

fn validate_required_security_test_terms(root: &Path) -> QualityResult<Vec<String>> {
    let files = [
        "std/http/CookiesTest.terl",
        "std/http/SessionTest.terl",
        "crates/terlan/src/commands/serve/serve_test.rs",
        "crates/terlan/src/commands/serve/serve_test/package_validation_test.rs",
        "crates/terlan/src/runtime/vm/http_test.rs",
        "crates/terlan/src/commands/serve/tls_test.rs",
    ];
    let mut combined = String::new();
    for file in files {
        combined.push_str(&fs::read_to_string(root.join(file)).map_err(|err| {
            format!("{file}: failed to read combined security test inventory: {err}")
        })?);
        combined.push('\n');
    }
    Ok(REQUIRED_SECURITY_TEST_ANCHORS
        .iter()
        .filter(|term| !combined.contains(**term))
        .map(|term| format!("security tests: missing required adversarial anchor `{term}`"))
        .collect())
}

fn validate_makefile(root: &Path) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join("Makefile"))
        .map_err(|err| format!("Makefile: failed to read VM web security policy gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing VM web security policy gate term `{term}`"))
        .collect())
}

fn validate_policy_surface_entries(entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let normalized = entry.to_ascii_lowercase();
            let has_placeholder = PLACEHOLDER_POLICY_TERMS
                .iter()
                .any(|term| normalized.contains(term));
            if has_placeholder {
                return Some(format!(
                    "VM web security policy surface `{entry}` uses placeholder language"
                ));
            }
            let implemented_surface = entry
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch == '-' || ch == '.');
            let explicit_rejection = entry.contains(".rejectedUntil");
            if implemented_surface || explicit_rejection {
                None
            } else {
                Some(format!(
                    "VM web security policy surface `{entry}` must be a canonical surface or rejectedUntil reason"
                ))
            }
        })
        .collect()
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
#[path = "vm_web_security_policy_test.rs"]
mod vm_web_security_policy_test;
