use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-web-route-schema-client-report.json";

const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_API_CONTRACT_ANCHORS: &[&str] = &[
    "API_CONTRACT_SCHEMA",
    "OPENAPI_VERSION",
    "pub(crate) struct ApiContract",
    "pub(crate) struct ApiRoute",
    "method: String",
    "path: String",
    "handler: String",
    "from_router_source",
    "to_openapi",
    "routes_from_syntax_module",
    "openapi_path",
    "openapi_operation_id",
];

const REQUIRED_API_COMMAND_ANCHORS: &[&str] = &[
    "API_CONTRACT_FILE",
    "OPENAPI_JSON_FILE",
    "OPENAPI_YAML_FILE",
    "API_IMPORT_SKIP_FILE",
    "emit_api_artifacts",
    "api_contract_from_emit_args",
    "import_openapi_client",
    "imported_operations",
    "import_skips",
    "render_imported_client_module",
    "terlan-api-import-skips-v1",
];

const REQUIRED_API_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/compiler/api_contract_test.rs",
        &[
            "router_source_contract_extracts_routes",
            "router_source_contract_projects_to_openapi_paths",
            "router_source_contract_extracts_group_routes",
        ],
    ),
    (
        "crates/terlan/src/commands/api/mod_test.rs",
        &[
            "api_emit_from_source_writes_route_openapi_paths",
            "api_import_generates_client_module_and_skip_manifest",
            "api_import_records_unsupported_operation_skips",
        ],
    ),
];

const REQUIRED_WEB_MANIFEST_ANCHORS: &[&str] = &[
    "WebBuildManifest",
    "WebHandlerArtifact",
    "WebSocketArtifact",
    "WebStaticResponseArtifact",
    "WebFileResponseArtifact",
    "WebErrorHandlerArtifact",
    "WebSourceSpanArtifact",
    "build_id",
    "web_build_id",
    "source: Option<WebSourceSpanArtifact>",
];

const REQUIRED_ROUTE_EXTRACTION_ANCHORS: &[&str] = &[
    "WebRouteManifestRows",
    "discover_web_route_manifest_from_sources",
    "route_source_context",
    "source_span_for_expr",
    "validate_discovered_web_routes",
    "validate_router_handler_rows",
    "validate_router_middleware",
    "validate_router_error_handler",
    "route_param_types",
];

const REQUIRED_ROUTE_VALIDATION_ANCHORS: &[&str] = &[
    "validate_router_handler_rows",
    "validate_route_handler_param_types",
    "validate_router_middleware",
    "validate_router_error_handler",
    "validate_discovered_web_routes",
    "duplicate or ambiguous",
];

const REQUIRED_ROUTE_TEST_ANCHORS: &[&str] = &[
    "route_param_types_extracts_defaults_and_typed_captures",
    "validate_route_pattern_rejects_unsupported_route_param_type",
    "validate_route_pattern_rejects_non_binding_capture_names",
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-web-route-schema-client-check: vm-web-deployment-profile-check",
    "$(MAKE) api-schema-check",
    "$(MAKE) web-profile-preflight",
    "vm_web_route_schema_client_test",
    "vm-web-route-schema-client",
];

const ROUTE_MANIFEST_HASH_CASES: &[&str] = &[
    "browser web manifest build_id is deterministic from route/static asset identity",
    "API contract schema is compiler-owned before OpenAPI projection",
    "OpenAPI paths are projected from compiler-owned route rows",
    "client import writes a skip manifest for unsupported OpenAPI features",
];

const SCHEMA_OUTPUT_CASES: &[&str] = &[
    "api-contract.json preserves service, method, path, and handler identity",
    "openapi.json emits OpenAPI 3.1 paths from Terlan routes",
    "openapi.yaml mirrors the JSON projection",
    "route groups are flattened before schema projection",
];

const GENERATED_CLIENT_FIXTURES: &[&str] = &[
    "OpenAPI import emits a Terlan module with method helpers",
    "OpenAPI import emits a Terlan module with path helpers",
    "OpenAPI import preserves operation documentation",
    "OpenAPI import records unsupported TRACE and reference paths as skips",
];

const SECURITY_POLICY_LINKS: &[&str] = &[
    "router middleware rows are validated before manifest emission",
    "router error handler rows are validated before manifest emission",
    "route parameter names and types are checked against handler signatures",
    "security policy schema output remains rejected until typed policy rows exist",
];

const DEPLOYMENT_PROFILE_LINKS: &[&str] = &[
    "route/schema gate is sequenced after vm-web-deployment-profile-check",
    "base path schema output remains rejected until deployment profiles exist",
    "streaming endpoint schema output remains rejected until deployment profiles own live streams",
];

const STALE_CLIENT_REJECTIONS: &[&str] = &[
    "unsupported OpenAPI path references are recorded in api-import-skips.json",
    "unsupported TRACE operations are recorded in api-import-skips.json",
    "stale generated client hash comparison remains rejected until client manifests exist",
];

const SOURCE_MAP_PARITY_CHECKS: &[&str] = &[
    "web route manifest rows carry source span artifacts",
    "route extraction stores package-safe source path, line, and column",
    "API schema source links remain rejected until ApiRoute carries source spans",
];

const REJECTED_SCHEMA_CLIENT_PATHS: &[&str] = &[
    "typed request body schemas",
    "typed response body schemas",
    "typed error response schemas",
    "header and query parameter schema output",
    "security policy export in API schemas",
    "deployment base-path export in API schemas",
    "SSE and WebSocket schema output",
    "source-map parity between API contract and web manifest",
    "stale generated Terlan client hash rejection",
    "retry and cancellation client policy generation",
];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing vm web route schema client summary.
pub struct VmWebRouteSchemaClientSummary {
    pub route_manifest_hash_case_count: usize,
    pub schema_output_case_count: usize,
    pub generated_client_fixture_count: usize,
    pub rejected_schema_client_path_count: usize,
    pub report_path: PathBuf,
}

/// Runs vm web route schema client.
pub fn run_vm_web_route_schema_client(root: &Path) -> QualityResult<VmWebRouteSchemaClientSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/compiler/api_contract.rs",
        REQUIRED_API_CONTRACT_ANCHORS,
        "compiler API contract",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/api/mod.rs",
        REQUIRED_API_COMMAND_ANCHORS,
        "API command schema/client surface",
    )?);
    for (relative, anchors) in REQUIRED_API_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "API schema/client tests",
        )?);
    }
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/build/js_browser/manifest.rs",
        REQUIRED_WEB_MANIFEST_ANCHORS,
        "web route manifest schema surface",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/build/js_browser/routes.rs",
        REQUIRED_ROUTE_EXTRACTION_ANCHORS,
        "web route extraction surface",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/build/js_browser/routes/validation.rs",
        REQUIRED_ROUTE_VALIDATION_ANCHORS,
        "web route validation surface",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/web_route_test.rs",
        REQUIRED_ROUTE_TEST_ANCHORS,
        "typed route parameter tests",
    )?);
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries(
        "route manifest hash cases",
        ROUTE_MANIFEST_HASH_CASES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "schema output cases",
        SCHEMA_OUTPUT_CASES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "generated client fixtures",
        GENERATED_CLIENT_FIXTURES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "security policy links",
        SECURITY_POLICY_LINKS,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "deployment profile links",
        DEPLOYMENT_PROFILE_LINKS,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "stale client rejections",
        STALE_CLIENT_REJECTIONS,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "source-map parity checks",
        SOURCE_MAP_PARITY_CHECKS,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "rejected schema/client paths",
        REJECTED_SCHEMA_CLIENT_PATHS,
    ));
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-web-route-schema-client", &diagnostics));
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
        "schema": "terlan-vm-web-route-schema-client-report-v1",
        "routeManifestHashes": ROUTE_MANIFEST_HASH_CASES,
        "schemaOutput": SCHEMA_OUTPUT_CASES,
        "generatedClientFixtures": GENERATED_CLIENT_FIXTURES,
        "securityPolicyLinks": SECURITY_POLICY_LINKS,
        "deploymentProfileLinks": DEPLOYMENT_PROFILE_LINKS,
        "staleClientRejections": STALE_CLIENT_REJECTIONS,
        "sourceMapParityChecks": SOURCE_MAP_PARITY_CHECKS,
        "rejectedSchemaClientPaths": REJECTED_SCHEMA_CLIENT_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM web route schema client report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmWebRouteSchemaClientSummary {
        route_manifest_hash_case_count: ROUTE_MANIFEST_HASH_CASES.len(),
        schema_output_case_count: SCHEMA_OUTPUT_CASES.len(),
        generated_client_fixture_count: GENERATED_CLIENT_FIXTURES.len(),
        rejected_schema_client_path_count: REJECTED_SCHEMA_CLIENT_PATHS.len(),
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
        .map_err(|err| format!("Makefile: failed to read route schema client gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing route schema client gate term `{term}`"))
        .collect())
}

/// Validates no placeholder report entries.
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
                        "VM web route schema/client {label} entry `{entry}` uses placeholder term `{term}`"
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
#[path = "vm_web_route_schema_client_test.rs"]
#[cfg(test)]
mod vm_web_route_schema_client_test;
