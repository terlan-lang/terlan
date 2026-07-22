use std::collections::BTreeMap;
use std::path::Path;

use serde_json::json;

use super::ts_dom_module_mapping::{DomModuleMapping, DomSkippedDeclaration};
use super::ts_generated_artifact::GeneratedBindingFileHash;
use super::ts_input_manifest::TsInputManifest;

/// Renders the generated binding manifest.
pub(super) fn render_binding_manifest(
    manifest: &TsInputManifest,
    manifest_path: &Path,
    mapping: &DomModuleMapping,
    generated_file_hashes: &[GeneratedBindingFileHash],
) -> Result<String, String> {
    let outputs = mapping
        .modules
        .iter()
        .map(|module| {
            json!({
                "module": module.module_path,
                "source": module.source_path,
                "interface": module.interface_path,
                "summary": module.summary_path,
                "test": module.test_path,
            })
        })
        .collect::<Vec<_>>();
    let generated_files = generated_file_hashes
        .iter()
        .map(|file| {
            json!({
                "path": file.path,
                "sha256": file.sha256,
            })
        })
        .collect::<Vec<_>>();
    let skipped = skipped_manifest_entries(&mapping.skipped);
    let inputs = manifest
        .inputs
        .iter()
        .map(|input| {
            json!({
                "package": manifest.source_package.name,
                "package_version": manifest.source_package.version,
                "path": input.path,
                "sha256": input.sha256,
                "kind": input.kind,
                "namespace": input.namespace,
            })
        })
        .collect::<Vec<_>>();

    serde_json::to_string_pretty(&json!({
        "schema": "terlan.std.js.bindings.v1",
        "generator": manifest.generator.name,
        "generator_version": manifest.generator.version,
        "generator_profile": manifest.generator.profile,
        "input_manifest": manifest_path.display().to_string(),
        "target_profile": manifest.target_profile,
        "inputs": inputs,
        "outputs": outputs,
        "generated_files": generated_files,
        "skipped_manifest": "std/js/manifests/std_js_skipped.json",
        "skipped": skipped,
    }))
    .map(|json| format!("{json}\n"))
    .map_err(|err| format!("ts_bindgen.binding_manifest_render_failed: {err}"))
}

/// Renders the skipped-declarations manifest.
pub(super) fn render_skipped_manifest(
    manifest: &TsInputManifest,
    manifest_path: &Path,
    mapping: &DomModuleMapping,
) -> Result<String, String> {
    let skipped = skipped_manifest_entries(&mapping.skipped);
    serde_json::to_string_pretty(&json!({
        "schema": "terlan.std.js.skipped-declarations.v1",
        "generator": manifest.generator.name,
        "generator_version": manifest.generator.version,
        "input_manifest": manifest_path.display().to_string(),
        "target_profile": manifest.target_profile,
        "skipped": skipped,
    }))
    .map(|json| format!("{json}\n"))
    .map_err(|err| format!("ts_bindgen.skipped_manifest_render_failed: {err}"))
}

/// Converts skipped declarations into JSON with unique source labels.
fn skipped_manifest_entries(skipped: &[DomSkippedDeclaration]) -> Vec<serde_json::Value> {
    let mut seen = BTreeMap::new();
    skipped
        .iter()
        .map(|entry| {
            let source = unique_skipped_source(&entry.source, &mut seen);
            skipped_manifest_entry(entry, source)
        })
        .collect()
}

/// Returns a deterministic unique skipped source label.
fn unique_skipped_source(source: &str, seen: &mut BTreeMap<String, usize>) -> String {
    let count = seen.entry(source.to_string()).or_insert(0);
    *count += 1;
    if *count == 1 {
        source.to_string()
    } else {
        format!("{source}#{}", *count)
    }
}

/// Converts a skipped declaration into JSON.
fn skipped_manifest_entry(skipped: &DomSkippedDeclaration, source: String) -> serde_json::Value {
    json!({
        "source": source,
        "reason": skipped.reason,
        "detail": skipped.detail,
    })
}
