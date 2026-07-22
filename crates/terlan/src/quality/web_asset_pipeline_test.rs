use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_web_asset_pipeline, validate_asset_graph_entries};

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
            "terlan-web-asset-pipeline-{name}-{}-{unique}",
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
            "crates/terlan/src/commands/build/js_browser/assets.rs",
            r#"
copy_js_module_asset copy_browser_imported_assets copy_manifest_static_assets
manifest_static_asset_files validate_safe_manifest_asset_path
validate_no_case_folded_manifest_asset_collisions to_ascii_lowercase()
browser_import_asset_relative_path files.sort() fingerprint(&bytes)
subresource_integrity
source_map_relative_path source_map_source_label sourceMappingURL
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/build/js_browser/manifest.rs",
            r#"
write_browser_manifest web_build_id terlan-web-build-v1 source_js_manifest
build_id WebAssetArtifact WebSourceSpanArtifact fingerprint(text.as_bytes()) integrity
validate_unique_web_asset_paths error[web_assets]: duplicate browser asset path
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_static.rs",
            r#"
VmHttpStaticAssetTable VmHttpStaticManifestEntry content_type_for_path
cache_control fingerprint DuplicateRoute InvalidAssetPath insert_manifest
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/build/build_test/tests/artifact_test.rs",
            r#"
build_command_emits_browser_web_package_for_js_browser_target
build_command_infers_js_browser_target_from_asset_imports
build_command_emits_manifest_declared_static_assets_for_js_browser_project
build_command_rejects_case_folded_static_asset_collisions_for_js_browser_project
asset-css asset-file asset-markdown static-asset
javascript-source-map
logo with space.txt Logo.txt assets/nested/logo with space.txt fingerprint
integrity sha256-
sourceMappingURL app.js.map
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/build/js_browser_test.rs",
            r#"
write_browser_manifest_rejects_duplicate_web_asset_paths
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
web-asset-pipeline-check: typed-template-render-mode-check
	$(MAKE) browser-package-preflight
	$(MAKE) web-profile-preflight
	$(RUST_TEST) --locked -p terlan --bin terlan-quality web_asset_pipeline_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- web-asset-pipeline
"#;

#[test]
fn web_asset_pipeline_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_web_asset_pipeline(repo.root()).expect("quality check");

    assert_eq!(summary.asset_graph_entry_count, 8);
    assert_eq!(summary.content_type_check_count, 4);
    assert_eq!(summary.cache_header_check_count, 3);
    assert_eq!(summary.rejected_asset_path_count, 4);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-web-asset-pipeline-report-v1"));
    assert!(report.contains("asset-css"));
    assert!(report.contains("manifest-declared path with spaces copied and recorded"));
    assert!(report.contains("case-folded manifest asset collision rejected"));
    assert!(report.contains("duplicate final browser asset path rejected"));
    assert!(report.contains(r#""implemented": true"#));
    assert!(report.contains("browser asset rows carry integrity metadata"));
    assert!(report.contains("browser source maps use package-safe source labels"));
    assert!(report.contains("live-template-protocol-asset.rejectedUntilProtocolHashCompatibility"));
    assert!(report.contains("wasm-asset.rejectedUntilHostedWasmExecution"));
    assert!(
        !report.to_ascii_lowercase().contains("placeholder"),
        "report must not carry placeholder asset graph evidence: {report}"
    );
    assert!(!report.contains("source-map link emission"));
    assert!(!report.contains("source-map leakage detection"));
    assert!(!report.contains("subresource integrity hashes remain rejected"));
    assert!(report.contains("mixed compiler version client asset rejection"));
    assert!(!report.contains("path with spaces adversarial fixture"));
    assert!(!report.contains("case-sensitive path collision rejection"));
    assert!(!report.contains("duplicate filename across asset roots"));
}

#[test]
fn web_asset_pipeline_rejects_placeholder_asset_graph_entries() {
    let diagnostics = validate_asset_graph_entries(&["wasm-asset-placeholder"]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder language")),
        "expected placeholder asset graph diagnostic: {diagnostics:?}"
    );
}

#[test]
fn web_asset_pipeline_rejects_missing_asset_anchor() {
    let repo = TestRepo::new("missing-asset-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/commands/build/js_browser/assets.rs");
    let source = fs::read_to_string(&path).expect("asset source");
    repo.write(
        "crates/terlan/src/commands/build/js_browser/assets.rs",
        &source.replace("validate_safe_manifest_asset_path", ""),
    )
    .expect("rewrite asset source");

    let error = run_web_asset_pipeline(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("validate_safe_manifest_asset_path"));
}

#[test]
fn web_asset_pipeline_rejects_missing_vm_static_anchor() {
    let repo = TestRepo::new("missing-vm-static-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/http_static.rs");
    let source = fs::read_to_string(&path).expect("VM static source");
    repo.write(
        "crates/terlan/src/runtime/vm/http_static.rs",
        &source.replace("DuplicateRoute", ""),
    )
    .expect("rewrite VM static source");

    let error = run_web_asset_pipeline(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("DuplicateRoute"));
}

#[test]
fn web_asset_pipeline_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate-term").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("$(MAKE) web-profile-preflight", ""),
    )
    .expect("rewrite makefile");

    let error = run_web_asset_pipeline(repo.root()).expect_err("gate should fail");

    assert!(error.contains("web-profile-preflight"));
}
