use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-web-deployment-profile-report.json";

const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_SERVE_ARG_ANCHORS: &[&str] = &[
    "DEFAULT_SERVE_HOST",
    "DEFAULT_SERVE_PORT",
    "ServeArgs",
    "host: String",
    "port: u16",
    "terlc serve --host requires a value",
    "terlc serve --port expects a u16 value",
];

const REQUIRED_ROUTER_ANCHORS: &[&str] = &[
    "VmHttpRouteMethod",
    "VmHttpRouteTarget",
    "VmHttpRouter",
    "dispatch(",
    "route(",
    "pub(crate) fn sse(",
    "pub(crate) fn websocket(",
];

const REQUIRED_ROUTER_TEST_ANCHORS: &[&str] = &[
    "VmHttpRouteMethod::Get",
    ".dispatch(VmHttpRouteMethod::Get, \"/health\")",
    ".dispatch(VmHttpRouteMethod::Get, \"/assets/app.js\")",
    ".dispatch(VmHttpRouteMethod::Get, \"/events\")",
    ".dispatch(VmHttpRouteMethod::Get, \"/socket\")",
];

const REQUIRED_TLS_ACME_ANCHORS: &[&str] = &[
    "runtime_tls_config_for_serve",
    "acme_runtime_tls_config_for_serve",
    "acme_http01_challenge",
    "is_acme_http01_token",
    "load_acme_runtime_tls_cache",
];

const REQUIRED_TLS_TEST_ANCHORS: &[&str] = &[
    "runtime_tls_config_for_serve_accepts_auto_tls_certificate_cache",
    "acme_http01_challenge_cache_rejects_invalid_token",
    "hyper_request_handler_serves_acme_http01_challenge_from_auto_tls_cache",
    "vm_stream_request_serves_acme_http01_challenge_without_hyper",
    "vm_stream_request_rejects_invalid_acme_http01_token_without_hyper",
];

const REQUIRED_RESPONSE_ANCHORS: &[(&str, &[&str])] = &[
    (
        "std/http/Response.terl",
        &[
            "pub redirect(location: String, status: Int = 302): Response",
            "Location",
            "with_header",
            "Set-Cookie",
            "cookie_with_options",
        ],
    ),
    (
        "std/http/Cookies.terl",
        &[
            "SameSite",
            "http_only: Bool",
            "secure: Bool",
            "same_site_to_string",
        ],
    ),
    (
        "crates/terlan/src/commands/serve/handler/response_bridge.rs",
        &[
            "validate_response_header",
            "Location",
            "Set-Cookie",
            "unsupported cookie SameSite value",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/http_session.rs",
        &["Path=/; HttpOnly; SameSite=Lax"],
    ),
];

const REQUIRED_STREAM_ANCHORS: &[(&str, &[&str])] = &[
    (
        "std/http/Sse.terl",
        &[
            "endpoint_with_keep_alive",
            "max_pending_events",
            "keep_alive_ms",
        ],
    ),
    (
        "std/http/WebSocket.terl",
        &["endpoint", "max_pending_frames", "max_frame_bytes"],
    ),
    (
        "crates/terlan/src/runtime/vm/sse.rs",
        &["VmSseEndpointPlan", "VmSseStream", "flush_next"],
    ),
    (
        "crates/terlan/src/runtime/vm/websocket.rs",
        &[
            "build_websocket_upgrade_response",
            "serialize_websocket_upgrade_response",
            "VmWebSocketEndpointPlan",
        ],
    ),
    (
        "crates/terlan/src/commands/serve/websocket.rs",
        &["websocket_upgrade_state", "WebSocketUpgradeState"],
    ),
];

const REQUIRED_ASSET_ANCHORS: &[&str] = &[
    "WebBuildManifest",
    "WebAssetArtifact",
    "web_relative_path",
    "fingerprint",
    "source_js_manifest",
    "index.html",
];

const REQUIRED_MANIFEST_TEST_ANCHORS: &[&str] = &[
    "healthcheck:",
    "\"Location\"",
    "validate_web_package_accepts_static_responses",
    "validate_web_package_accepts_static_response_headers",
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-web-deployment-profile-check: vm-web-lifecycle-health-check",
    "$(MAKE) http-router-check",
    "$(MAKE) http-tls-check",
    "$(MAKE) native-boundary-http-cookie-check",
    "vm_web_deployment_profile_test",
    "vm-web-deployment-profile",
];

const PROFILE_MATRIX: &[&str] = &[
    "local development: direct loopback bind with explicit host/port",
    "container: direct bind plus Compose healthcheck validation",
    "bare metal: direct listener plus VM TLS material when configured",
    "reverse proxy: rejected until trusted proxy profile exists",
    "TLS terminated proxy: rejected until scheme/host reconstruction exists",
    "VM terminated TLS: runtime TLS cache and ACME readiness are validated",
    "static asset CDN: rejected until asset URL profile exists",
    "ACME production: HTTP-01 challenge route is reserved and token-validated",
];

const PROXY_FIXTURES: &[&str] = &[
    "Forwarded header is never trusted without a typed trusted-proxy profile",
    "X-Forwarded-Host is never trusted without a typed trusted-proxy profile",
    "X-Forwarded-Proto is never trusted without a typed trusted-proxy profile",
    "base-path rewriting is rejected until the profile owns route reconstruction",
    "spoofed proxy headers remain observable as rejected deployment paths",
];

const HEADER_TRUST_DECISIONS: &[&str] = &[
    "bind host and port come from explicit serve arguments",
    "Host is request metadata, not deployment authority, until profiles exist",
    "Forwarded and X-Forwarded-* require a trusted proxy list before use",
    "redirect authority cannot be reconstructed from untrusted proxy headers",
    "secure-cookie inference cannot depend on untrusted proxy headers",
];

const URL_RECONSTRUCTION_CASES: &[&str] = &[
    "direct HTTP: scheme derives from listener TLS state",
    "VM TLS: scheme is https when runtime TLS config is active",
    "reverse proxy: rejected until trusted proxy identity is configured",
    "base path: rejected until deployment profile records the path prefix",
    "CDN assets: rejected until manifest carries deployment URL policy",
];

const COOKIE_DECISIONS: &[&str] = &[
    "Set-Cookie headers are validated at the response bridge",
    "SameSite supports lax, strict, and none through typed std.http.Cookies",
    "session cookies are HttpOnly and SameSite=Lax by default",
    "Secure inference under TLS termination is rejected until profiles exist",
];

const UPGRADE_CASES: &[&str] = &[
    "WebSocket upgrade state is classified before static fallback",
    "VM stream WebSocket handshake is serialized by VM WebSocket runtime",
    "SSE endpoints carry explicit queue and keep-alive policy",
    "reverse-proxy WebSocket/SSE upgrades are rejected until profile-owned",
];

const HEALTH_ENDPOINT_CASES: &[&str] = &[
    "route manifest can declare health handlers",
    "VM router dispatches a /health route",
    "Compose dependency healthcheck is validated by the lifecycle gate",
    "public readiness/liveness exposure remains rejected until profiled",
];

const ACME_ROUTING_CASES: &[&str] = &[
    "ACME HTTP-01 token syntax is validated",
    "cached challenges are served before static fallback",
    "HEAD and GET challenge paths are covered by serve tests",
    "application-route conflict policy remains rejected until profiled",
];

const REJECTED_DEPLOYMENT_PATHS: &[&str] = &[
    "trusted proxy list with CIDR and named network support",
    "Forwarded and X-Forwarded-* scheme/host reconstruction",
    "base-path route rewriting for reverse proxies",
    "absolute redirect generation behind TLS-terminated proxies",
    "automatic Secure cookie inference behind TLS-terminated proxies",
    "static asset CDN URL generation and stale-CDN rejection",
    "health endpoint exposure policy for public vs private interfaces",
    "reverse-proxy WebSocket upgrade semantics",
    "reverse-proxy SSE buffering and heartbeat policy",
    "ACME challenge conflict policy with application routes",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmWebDeploymentProfileSummary {
    pub profile_matrix_count: usize,
    pub proxy_fixture_count: usize,
    pub upgrade_case_count: usize,
    pub rejected_deployment_path_count: usize,
    pub report_path: PathBuf,
}

pub fn run_vm_web_deployment_profile(root: &Path) -> QualityResult<VmWebDeploymentProfileSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/args.rs",
        REQUIRED_SERVE_ARG_ANCHORS,
        "serve deployment arguments",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_router.rs",
        REQUIRED_ROUTER_ANCHORS,
        "VM route deployment surface",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_router_test.rs",
        REQUIRED_ROUTER_TEST_ANCHORS,
        "VM route deployment tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/tls.rs",
        REQUIRED_TLS_ACME_ANCHORS,
        "TLS and ACME deployment surface",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/tls_test.rs",
        &REQUIRED_TLS_TEST_ANCHORS[..2],
        "TLS and ACME deployment tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/serve_test.rs",
        &REQUIRED_TLS_TEST_ANCHORS[2..],
        "serve ACME routing tests",
    )?);
    for (relative, anchors) in REQUIRED_RESPONSE_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "response security and URL surface",
        )?);
    }
    for (relative, anchors) in REQUIRED_STREAM_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "live upgrade deployment surface",
        )?);
    }
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/build/js_browser/manifest.rs",
        REQUIRED_ASSET_ANCHORS,
        "web asset deployment manifest",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/manifest_test.rs",
        REQUIRED_MANIFEST_TEST_ANCHORS,
        "web package manifest deployment tests",
    )?);
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries(
        "profile matrix",
        PROFILE_MATRIX,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "proxy fixtures",
        PROXY_FIXTURES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "header trust decisions",
        HEADER_TRUST_DECISIONS,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "URL reconstruction cases",
        URL_RECONSTRUCTION_CASES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "cookie decisions",
        COOKIE_DECISIONS,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "upgrade cases",
        UPGRADE_CASES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "health endpoint cases",
        HEALTH_ENDPOINT_CASES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "ACME routing cases",
        ACME_ROUTING_CASES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "rejected deployment paths",
        REJECTED_DEPLOYMENT_PATHS,
    ));
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-web-deployment-profile", &diagnostics));
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
        "schema": "terlan-vm-web-deployment-profile-report-v1",
        "profileMatrix": PROFILE_MATRIX,
        "proxyFixtures": PROXY_FIXTURES,
        "headerTrustDecisions": HEADER_TRUST_DECISIONS,
        "urlReconstructionCases": URL_RECONSTRUCTION_CASES,
        "cookieDecisions": COOKIE_DECISIONS,
        "upgradeCases": UPGRADE_CASES,
        "healthEndpointCases": HEALTH_ENDPOINT_CASES,
        "acmeRoutingCases": ACME_ROUTING_CASES,
        "rejectedDeploymentPaths": REJECTED_DEPLOYMENT_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM web deployment profile report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmWebDeploymentProfileSummary {
        profile_matrix_count: PROFILE_MATRIX.len(),
        proxy_fixture_count: PROXY_FIXTURES.len(),
        upgrade_case_count: UPGRADE_CASES.len(),
        rejected_deployment_path_count: REJECTED_DEPLOYMENT_PATHS.len(),
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

fn validate_makefile(root: &Path) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join("Makefile"))
        .map_err(|err| format!("Makefile: failed to read VM web deployment gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing VM web deployment gate term `{term}`"))
        .collect())
}

pub fn validate_no_placeholder_report_entries(label: &str, entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let normalized = entry.to_ascii_lowercase();
            PLACEHOLDER_REPORT_TERMS
                .iter()
                .find(|term| normalized.contains(**term))
                .map(|term| {
                    format!(
                        "VM web deployment profile {label} entry `{entry}` uses placeholder term `{term}`"
                    )
                })
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
#[path = "vm_web_deployment_profile_test.rs"]
mod vm_web_deployment_profile_test;
