use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::json;

use super::ts_dom_module_mapping::{DomMemberPlan, DomModuleMapping, DomSkippedDeclaration};
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
            let mut members = module.members.iter().map(|member| match member {
                DomMemberPlan::Property(property) => json!({
                    "name": property.terlan_name, "js_name": property.js_name,
                    "kind": "property", "return_type": property.terlan_type, "arity": 1,
                    "undefined_only": property.undefined_only,
                }),
                DomMemberPlan::Method(method) => json!({
                    "name": method.terlan_name, "js_name": method.js_name, "kind": "method",
                    "return_type": method.return_type, "arity": method.params.len() + 1,
                    "undefined_only": method.undefined_only,
                    "parameters": method.params.iter().map(|parameter| json!({
                        "optional": parameter.optional, "rest": parameter.rest, "type": parameter.terlan_type,
                    })).collect::<Vec<_>>(),
                }),
            }).collect::<Vec<_>>();
            if let Some(methods) = module.callback_members() {
                members.push(json!({
                    "name": "with_members", "js_name": "", "kind": "mixin",
                    "return_type": "BindingTarget", "arity": methods.len() + 1,
                    "js_names": methods.iter().map(|method| &method.js_name).collect::<Vec<_>>(),
                    "parameters": methods.iter().map(|method| json!({
                        "optional": false, "type": format!("({}) -> {}",
                            method.params.iter().map(|parameter| parameter.terlan_type.as_str()).collect::<Vec<_>>().join(", "), method.return_type),
                    })).collect::<Vec<_>>(),
                }));
            }
            if manifest.generator.profile == "typescript-declarations" {
                members.extend(super::ts_record_helpers::operations(module).into_iter().map(|operation| operation.manifest()));
            }
            json!({
                "module": module.module_path,
                "typescript_type": if manifest.generator.profile == "typescript-declarations"
                    && manifest.inputs.len() == 1 && !module.source_interface.contains('.') {
                    Some(json!({"source": manifest.inputs[0].path, "name": module.source_interface}))
                } else { None },
                "source": module.source_path,
                "interface": module.interface_path,
                "summary": module.summary_path,
                "test": module.test_path,
                "members": members,
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
    let mut used = skipped
        .iter()
        .map(|entry| entry.source.clone())
        .collect::<BTreeSet<_>>();
    skipped
        .iter()
        .map(|entry| {
            let source = unique_skipped_source(&entry.source, &mut seen, &mut used);
            skipped_manifest_entry(entry, source)
        })
        .collect()
}

/// Returns a deterministic unique skipped source label.
fn unique_skipped_source(
    source: &str,
    seen: &mut BTreeMap<String, usize>,
    used: &mut BTreeSet<String>,
) -> String {
    let count = seen.entry(source.to_string()).or_insert(0);
    *count += 1;
    if *count == 1 {
        return source.to_string();
    }
    loop {
        let candidate = format!("{source}#{}", *count);
        if used.insert(candidate.clone()) {
            return candidate;
        }
        *count += 1;
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

#[cfg(test)]
#[path = "ts_dom_manifest_test.rs"]
mod tests;
