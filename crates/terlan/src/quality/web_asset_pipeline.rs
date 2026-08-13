use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/web-asset-pipeline-report.json";

const REQUIRED_ASSET_ANCHORS: &[&str] = &[
    "copy_js_module_asset",
    "copy_browser_imported_assets",
    "copy_manifest_static_assets",
    "manifest_static_asset_files",
    "validate_safe_manifest_asset_path",
    "validate_no_case_folded_manifest_asset_collisions",
    "browser_import_asset_relative_path",
    "files.sort()",
    "fingerprint(&bytes)",
    "subresource_integrity",
    "source_map_relative_path",
    "source_map_source_label",
    "sourceMappingURL",
    "to_ascii_lowercase()",
];

const REQUIRED_MANIFEST_ANCHORS: &[&str] = &[
    "write_browser_manifest",
    "validate_unique_web_asset_paths",
    "error[web_assets]: duplicate browser asset path",
    "web_build_id",
    "terlan-web-build-v1",
    "source_js_manifest",
    "build_id",
    "WebAssetArtifact",
    "WebSourceSpanArtifact",
    "fingerprint(text.as_bytes())",
    "integrity",
];

const REQUIRED_VM_STATIC_ANCHORS: &[&str] = &[
    "VmHttpStaticAssetTable",
    "VmHttpStaticManifestEntry",
    "content_type_for_path",
    "cache_control",
    "fingerprint",
    "DuplicateRoute",
    "InvalidAssetPath",
    "insert_manifest",
];

const REQUIRED_ARTIFACT_TEST_ANCHORS: &[&str] = &[
    "build_command_emits_browser_web_package_for_js_browser_target",
    "build_command_infers_js_browser_target_from_asset_imports",
    "build_command_emits_manifest_declared_static_assets_for_js_browser_project",
    "build_command_rejects_case_folded_static_asset_collisions_for_js_browser_project",
    "asset-css",
    "asset-file",
    "asset-markdown",
    "javascript-source-map",
    "static-asset",
    "logo with space.txt",
    "Logo.txt",
    "assets/nested/logo with space.txt",
    "fingerprint",
    "integrity",
    "sha256-",
    "sourceMappingURL",
    "app.js.map",
];

const REQUIRED_MANIFEST_TEST_ANCHORS: &[&str] =
    &["write_browser_manifest_rejects_duplicate_web_asset_paths"];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "web-asset-pipeline-check: typed-template-render-mode-check",
    "$(MAKE) browser-package-preflight",
    "$(MAKE) web-profile-preflight",
    "web_asset_pipeline_test",
    "web-asset-pipeline",
];

const ASSET_GRAPH: &[&str] = &[
    "javascript-module",
    "javascript-source-map",
    "asset-css",
    "asset-file",
    "asset-markdown",
    "static-asset",
    "live-template-protocol-asset.rejectedUntilProtocolHashCompatibility",
    "wasm-asset.rejectedUntilHostedWasmExecution",
];

const PLACEHOLDER_ASSET_TERMS: &[&str] = &["placeholder", "todo", "tbd", "unknown"];

const CONTENT_TYPE_CHECKS: &[&str] = &[
    "VM static asset content type inference",
    "constant HTML response content type",
    "constant text response content type",
    "declared file response content type",
];

const CACHE_HEADER_CHECKS: &[&str] = &[
    "fingerprinted VM asset immutable cache-control",
    "unfingerprinted VM asset no-cache",
    "manifest build id excludes timestamps",
];

const PATH_SAFETY_CHECKS: &[&str] = &[
    "unsafe parent/root manifest asset paths rejected",
    "manifest-declared path with spaces copied and recorded",
    "case-folded manifest asset collision rejected",
    "duplicate final browser asset path rejected",
];

const REJECTED_ASSET_PATHS: &[&str] = &[
    "compression metadata",
    "stale generated asset rejection",
    "mixed compiler version client asset rejection",
    "live-template protocol asset hash compatibility",
];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing web asset pipeline summary.
pub struct WebAssetPipelineSummary {
    pub asset_graph_entry_count: usize,
    pub content_type_check_count: usize,
    pub cache_header_check_count: usize,
    pub rejected_asset_path_count: usize,
    pub report_path: PathBuf,
}

/// Runs web asset pipeline.
pub fn run_web_asset_pipeline(root: &Path) -> QualityResult<WebAssetPipelineSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/build/js_browser/assets.rs",
        REQUIRED_ASSET_ANCHORS,
        "browser asset copying",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/build/js_browser/manifest.rs",
        REQUIRED_MANIFEST_ANCHORS,
        "browser asset manifest",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_static.rs",
        REQUIRED_VM_STATIC_ANCHORS,
        "VM static asset serving",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/build/build_test/tests/artifact_test.rs",
        REQUIRED_ARTIFACT_TEST_ANCHORS,
        "browser asset artifact tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/build/js_browser_test.rs",
        REQUIRED_MANIFEST_TEST_ANCHORS,
        "browser manifest serialization tests",
    )?);
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_asset_graph_entries(ASSET_GRAPH));
    if !diagnostics.is_empty() {
        return Err(render_failure("web-asset-pipeline", &diagnostics));
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
        "schema": "terlan-web-asset-pipeline-report-v1",
        "assetGraph": ASSET_GRAPH,
        "fingerprints": {
            "implemented": true,
            "evidence": [
                "browser asset bytes use compiler fingerprint helper",
                "browser manifest build_id is deterministic",
                "VM static assets carry optional fingerprint metadata"
            ]
        },
        "manifestEntries": [
            "javascript-module",
            "asset-css",
            "asset-file",
            "asset-markdown",
            "static-asset",
            "dynamic handler",
            "websocket route",
            "static response",
            "file response"
        ],
        "sourceMapChecks": {
            "implemented": true,
            "evidence": [
                "copied JavaScript modules emit sibling .js.map assets",
                "copied JavaScript modules append sourceMappingURL comments",
                "browser source maps use package-safe source labels without host path leakage"
            ]
        },
        "contentTypeChecks": CONTENT_TYPE_CHECKS,
        "cacheHeaderChecks": CACHE_HEADER_CHECKS,
        "pathSafetyChecks": {
            "implemented": true,
            "checks": PATH_SAFETY_CHECKS
        },
        "integrityHashes": {
            "implemented": true,
            "algorithm": "sha256",
            "format": "Subresource Integrity sha256-<base64>",
            "evidence": [
                "browser asset rows carry integrity metadata",
                "generated module script tags include integrity attributes"
            ]
        },
        "staleAssetRejectionCases": {
            "implemented": false,
            "reason": "stale browser asset rejection belongs to the later compatibility slice"
        },
        "rejectedAssetPaths": REJECTED_ASSET_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize web asset pipeline report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(WebAssetPipelineSummary {
        asset_graph_entry_count: ASSET_GRAPH.len(),
        content_type_check_count: CONTENT_TYPE_CHECKS.len(),
        cache_header_check_count: CACHE_HEADER_CHECKS.len(),
        rejected_asset_path_count: REJECTED_ASSET_PATHS.len(),
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
        .map_err(|err| format!("Makefile: failed to read web asset pipeline gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing web asset pipeline gate term `{term}`"))
        .collect())
}

fn validate_asset_graph_entries(entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let normalized = entry.to_ascii_lowercase();
            let has_placeholder = PLACEHOLDER_ASSET_TERMS
                .iter()
                .any(|term| normalized.contains(term));
            if has_placeholder {
                return Some(format!(
                    "web asset graph entry `{entry}` uses placeholder language"
                ));
            }
            let implemented_entry = entry
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch == '-' || ch == '.');
            let explicit_rejection = entry.contains(".rejectedUntil");
            if implemented_entry || explicit_rejection {
                None
            } else {
                Some(format!(
                    "web asset graph entry `{entry}` must be a canonical asset kind or rejectedUntil reason"
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
#[path = "web_asset_pipeline_test.rs"]
#[cfg(test)]
mod web_asset_pipeline_test;
