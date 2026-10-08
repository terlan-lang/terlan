use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_vm_web_route_schema_client, validate_no_placeholder_report_entries};

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
            "terlan-vm-web-route-schema-client-{name}-{}-{unique}",
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
            "std/http/native/src/api_contract.rs",
            r#"
API_CONTRACT_SCHEMA OPENAPI_VERSION pub struct ApiContract
pub struct ApiRoute method: String path: String handler: String
from_routes to_openapi openapi_path
openapi_operation_id
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/api/source_contract.rs",
            "from_router_source routes_from_syntax_module ApiContract::from_routes",
        )?;
        self.write(
            "crates/terlan/src/commands/api/mod.rs",
            r#"
API_CONTRACT_FILE OPENAPI_JSON_FILE OPENAPI_YAML_FILE API_IMPORT_SKIP_FILE
emit_api_artifacts api_contract_from_emit_args import_openapi_client
imported_operations import_skips render_imported_client_module
terlan-api-import-skips-v1
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/api/source_contract_test.rs",
            r#"
router_source_contract_extracts_routes
router_source_contract_projects_to_openapi_paths
router_source_contract_extracts_group_routes
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/api/mod_test.rs",
            r#"
api_emit_from_source_writes_route_openapi_paths
api_import_generates_client_module_and_skip_manifest
api_import_records_unsupported_operation_skips
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/build/js_browser/manifest.rs",
            r#"
WebBuildManifest WebHandlerArtifact WebSocketArtifact WebStaticResponseArtifact
WebFileResponseArtifact WebErrorHandlerArtifact WebSourceSpanArtifact
build_id web_build_id pub(super) use terlan_http_native::manifest
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/build/js_browser/routes.rs",
            r#"
WebRouteManifestRows discover_web_route_manifest_from_sources
route_source_context source_span_for_expr validate_discovered_web_routes
validate_router_handler_rows
route_param_types
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/build/js_browser/routes/validation.rs",
            r#"
validate_router_handler_rows validate_route_handler_param_types
validate_discovered_web_routes terlan_http_native::manifest::validate_route_namespace
"#,
        )?;
        self.write(
            "std/http/native/src/route_pattern_test.rs",
            r#"
route_param_types_extracts_defaults_and_typed_captures
validate_route_pattern_rejects_unsupported_route_param_type
validate_route_pattern_rejects_non_binding_capture_names
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
vm-web-route-schema-client-check: \
	vm-web-deployment-profile-check \
	api-schema-check \
	web-profile-preflight
	$(TERLAN_QUALITY) vm-web-route-schema-client
"#;

#[test]
fn vm_web_route_schema_client_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_web_route_schema_client(repo.root()).expect("quality check");

    assert_eq!(summary.route_manifest_hash_case_count, 4);
    assert_eq!(summary.schema_output_case_count, 4);
    assert_eq!(summary.generated_client_fixture_count, 4);
    assert_eq!(summary.rejected_schema_client_path_count, 10);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-web-route-schema-client-report-v1"));
    assert!(report.contains("OpenAPI import emits a Terlan module with method helpers"));
    assert!(report.contains("route/schema gate is sequenced after"));
    assert!(report.contains("typed request body schemas"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_web_route_schema_client_rejects_missing_api_contract_anchor() {
    let repo = TestRepo::new("missing-api-contract").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("std/http/native/src/api_contract.rs");
    let source = fs::read_to_string(&path).expect("api contract source");
    repo.write(
        "std/http/native/src/api_contract.rs",
        &source.replace("openapi_operation_id", ""),
    )
    .expect("rewrite API contract source");

    let error = run_vm_web_route_schema_client(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("openapi_operation_id"));
}

#[test]
fn vm_web_route_schema_client_rejects_missing_source_span_anchor() {
    let repo = TestRepo::new("missing-source-span").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/commands/build/js_browser/manifest.rs");
    let source = fs::read_to_string(&path).expect("manifest source");
    repo.write(
        "crates/terlan/src/commands/build/js_browser/manifest.rs",
        &source.replace("WebSourceSpanArtifact", ""),
    )
    .expect("rewrite manifest source");

    let error = run_vm_web_route_schema_client(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("WebSourceSpanArtifact"));
}

#[test]
fn vm_web_route_schema_client_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    for term in [
        "vm-web-deployment-profile-check",
        "api-schema-check",
        "web-profile-preflight",
        "$(TERLAN_QUALITY) vm-web-route-schema-client",
    ] {
        let changed = COMPLETE_MAKEFILE.replace(term, "");
        // A detached reference elsewhere must not satisfy a missing prerequisite.
        repo.write("Makefile", &format!("{changed}\n# {term}\n"))
            .expect("rewrite makefile");
        let error = run_vm_web_route_schema_client(repo.root()).expect_err("gate should fail");
        assert!(error.contains(term));
    }
}

#[test]
fn vm_web_route_schema_client_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries(
        "generated client fixtures",
        &["generated client placeholder fixture"],
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected placeholder report entry diagnostic: {diagnostics:?}"
    );
}
