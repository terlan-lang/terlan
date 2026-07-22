use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::terlan_quality::{render_failure, QualityResult};

const TYPE_MAPPING_MANIFEST: &str = "std/js/manifests/std_js_type_mapping.json";
const GENERATED_BINDINGS_MANIFEST: &str = "std/js/manifests/std_js_bindings.json";
const SKIPPED_DECLARATIONS_MANIFEST: &str = "std/js/manifests/std_js_skipped.json";
const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

const REQUIRED_TYPE_MAPPING_CATEGORIES: &[&str] = &[
    "primitive",
    "constructor",
    "interface",
    "call-signature",
    "overload",
    "optional",
    "nullable",
    "array",
    "tuple",
    "readonly-collection",
    "map",
    "set",
    "promise",
    "iterable",
    "callback",
    "structural-record",
    "union",
    "this-parameter",
    "index-signature",
    "unsupported-shape",
];

/// Summary produced by the JS type emission contract gate.
///
/// Inputs:
/// - Mapping category count.
/// - Generated output count.
/// - Skipped TypeScript declaration count.
///
/// Output:
/// - Stable success metrics for the quality CLI.
///
/// Transformation:
/// - Keeps the generated JS standard-library type surface tied to explicit
///   mapping, output, and skip manifests instead of relying on file presence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsTypeEmissionContractSummary {
    pub mapping_category_count: usize,
    pub generated_output_count: usize,
    pub skipped_declaration_count: usize,
}

/// Runs the JS type emission contract gate.
///
/// Inputs:
/// - `root`: repository root containing `std/js/manifests`.
///
/// Output:
/// - Success when the JS mapping manifest covers every required TypeScript
///   shape category and generated binding manifests reference real artifacts.
/// - Stable diagnostics for missing mapping categories, stale generated output
///   references, declaration-only generated tests under `@test`, and skipped
///   TypeScript entries without explicit reason/detail metadata.
///
/// Transformation:
/// - Treats generated JS bindings as a typed contract: emitted source,
///   summaries, tests, and unsupported-shape metadata must all agree with the
///   mapping manifest.
pub fn run_js_type_emission_contract(root: &Path) -> QualityResult<JsTypeEmissionContractSummary> {
    let mapping = read_json(root, TYPE_MAPPING_MANIFEST)?;
    let bindings = read_json(root, GENERATED_BINDINGS_MANIFEST)?;
    let skipped = read_json(root, SKIPPED_DECLARATIONS_MANIFEST)?;

    let mut diagnostics = Vec::new();
    let mapping_category_count = check_type_mapping_manifest(&mapping, &mut diagnostics);
    let generated_output_count = check_generated_bindings(root, &bindings, &mut diagnostics);
    let skipped_declaration_count = check_skipped_declarations(&skipped, &mut diagnostics);

    if diagnostics.is_empty() {
        Ok(JsTypeEmissionContractSummary {
            mapping_category_count,
            generated_output_count,
            skipped_declaration_count,
        })
    } else {
        Err(render_failure("js-type-emission-contract", &diagnostics))
    }
}

/// Reads a JSON manifest from the repository.
fn read_json(root: &Path, relative: &str) -> QualityResult<Value> {
    let path = root.join(relative);
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read manifest: {err}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|err| format!("{}: failed to parse JSON manifest: {err}", path.display()))
}

/// Checks the canonical TypeScript-to-Terlan mapping manifest.
fn check_type_mapping_manifest(mapping: &Value, diagnostics: &mut Vec<String>) -> usize {
    if mapping.get("schema").and_then(Value::as_str) != Some("terlan.std.js.type-mapping.v1") {
        diagnostics.push(format!(
            "{TYPE_MAPPING_MANIFEST}: schema must be `terlan.std.js.type-mapping.v1`"
        ));
    }

    let Some(categories) = mapping.get("categories").and_then(Value::as_array) else {
        diagnostics.push(format!(
            "{TYPE_MAPPING_MANIFEST}: missing `categories` array"
        ));
        return 0;
    };

    let mut seen = BTreeSet::new();
    for (index, category) in categories.iter().enumerate() {
        let row = index + 1;
        let Some(id) = nonempty_string(category, "id") else {
            diagnostics.push(format!(
                "{TYPE_MAPPING_MANIFEST}: categories[{row}] missing non-empty `id`"
            ));
            continue;
        };
        if !seen.insert(id.to_string()) {
            diagnostics.push(format!(
                "{TYPE_MAPPING_MANIFEST}: duplicate mapping category `{id}`"
            ));
        }
        if nonempty_string(category, "typescript_shape").is_none() {
            diagnostics.push(format!(
                "{TYPE_MAPPING_MANIFEST}: category `{id}` missing `typescript_shape`"
            ));
        }
        if nonempty_string(category, "terlan_surface").is_none() {
            diagnostics.push(format!(
                "{TYPE_MAPPING_MANIFEST}: category `{id}` missing `terlan_surface`"
            ));
        }
        let status = nonempty_string(category, "status");
        if status.is_none() {
            diagnostics.push(format!(
                "{TYPE_MAPPING_MANIFEST}: category `{id}` missing `status`"
            ));
        }
        if status == Some("unsupported")
            && nonempty_string(category, "unsupported_policy").is_none()
        {
            diagnostics.push(format!(
                "{TYPE_MAPPING_MANIFEST}: unsupported category `{id}` missing `unsupported_policy`"
            ));
        }
    }

    for required in REQUIRED_TYPE_MAPPING_CATEGORIES {
        if !seen.contains(*required) {
            diagnostics.push(format!(
                "{TYPE_MAPPING_MANIFEST}: missing required mapping category `{required}`"
            ));
        }
    }

    categories.len()
}

/// Checks generated binding output references.
fn check_generated_bindings(root: &Path, bindings: &Value, diagnostics: &mut Vec<String>) -> usize {
    let Some(outputs) = bindings.get("outputs").and_then(Value::as_array) else {
        diagnostics.push(format!(
            "{GENERATED_BINDINGS_MANIFEST}: missing `outputs` array"
        ));
        return 0;
    };

    for (index, output) in outputs.iter().enumerate() {
        let row = index + 1;
        let Some(module) = nonempty_string(output, "module") else {
            diagnostics.push(format!(
                "{GENERATED_BINDINGS_MANIFEST}: outputs[{row}] missing `module`"
            ));
            continue;
        };
        check_output_file(
            root,
            output,
            "source",
            row,
            diagnostics,
            |path, text, diagnostics| {
                if !text.contains(&format!("module {module}.")) {
                    diagnostics.push(format!(
                        "{}: generated source does not declare `module {module}.`",
                        path.display()
                    ));
                }
            },
        );
        check_output_file(root, output, "summary", row, diagnostics, |_, _, _| {});
        check_output_file(
            root,
            output,
            "test",
            row,
            diagnostics,
            |path, text, diagnostics| {
                let contract = generated_contract_function(text);
                if contract.is_none() {
                    diagnostics.push(format!(
                        "{}: generated test is missing a generated surface contract function",
                        path.display()
                    ));
                }
                if contract.is_some_and(|name| has_annotated_test_function(text, name)) {
                    diagnostics.push(format!(
                        "{}: generated surface contract must not be annotated with `@test`",
                        path.display()
                    ));
                }
            },
        );
    }

    outputs.len()
}

/// Checks one generated output file reference.
fn check_output_file<F>(
    root: &Path,
    output: &Value,
    field: &str,
    row: usize,
    diagnostics: &mut Vec<String>,
    validate: F,
) where
    F: FnOnce(&Path, &str, &mut Vec<String>),
{
    let Some(relative) = nonempty_string(output, field) else {
        diagnostics.push(format!(
            "{GENERATED_BINDINGS_MANIFEST}: outputs[{row}] missing `{field}`"
        ));
        return;
    };
    let path = root.join(relative);
    let Ok(text) = fs::read_to_string(&path) else {
        diagnostics.push(format!(
            "{GENERATED_BINDINGS_MANIFEST}: outputs[{row}] `{field}` references missing `{relative}`"
        ));
        return;
    };
    validate(&path, &text, diagnostics);
}

/// Checks unsupported/lossy TypeScript declaration metadata.
fn check_skipped_declarations(skipped: &Value, diagnostics: &mut Vec<String>) -> usize {
    if skipped.get("schema").and_then(Value::as_str)
        != Some("terlan.std.js.skipped-declarations.v1")
    {
        diagnostics.push(format!(
            "{SKIPPED_DECLARATIONS_MANIFEST}: schema must be `terlan.std.js.skipped-declarations.v1`"
        ));
    }

    let Some(rows) = skipped.get("skipped").and_then(Value::as_array) else {
        diagnostics.push(format!(
            "{SKIPPED_DECLARATIONS_MANIFEST}: missing `skipped` array"
        ));
        return 0;
    };
    for (index, row) in rows.iter().enumerate() {
        let row_no = index + 1;
        if nonempty_string(row, "source").is_none() {
            diagnostics.push(format!(
                "{SKIPPED_DECLARATIONS_MANIFEST}: skipped[{row_no}] missing `source`"
            ));
        }
        let reason = nonempty_string(row, "reason");
        if reason.is_none() {
            diagnostics.push(format!(
                "{SKIPPED_DECLARATIONS_MANIFEST}: skipped[{row_no}] missing `reason`"
            ));
        } else if !reason.unwrap_or_default().starts_with("ts_bindgen.") {
            diagnostics.push(format!(
                "{SKIPPED_DECLARATIONS_MANIFEST}: skipped[{row_no}] reason must start with `ts_bindgen.`"
            ));
        }
        if nonempty_string(row, "detail").is_none() {
            diagnostics.push(format!(
                "{SKIPPED_DECLARATIONS_MANIFEST}: skipped[{row_no}] missing `detail`"
            ));
        }
    }
    rows.len()
}

/// Returns one non-empty string field.
fn nonempty_string<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| meaningful_contract_text(text))
}

/// Returns whether text carries real contract metadata.
fn meaningful_contract_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lowered = trimmed.to_ascii_lowercase();
    !PLACEHOLDER_TERMS
        .iter()
        .any(|placeholder| lowered.contains(placeholder))
}

/// Returns whether a file contains an `@test`-annotated public function.
fn has_annotated_test_function(text: &str, function_name: &str) -> bool {
    let mut pending_test = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "@test" {
            pending_test = true;
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if pending_test && starts_public_function(trimmed, function_name) {
            return true;
        }
        if pending_test {
            pending_test = false;
        }
    }
    false
}

/// Returns whether a file contains a public function.
fn has_public_function(text: &str, function_name: &str) -> bool {
    text.lines()
        .map(str::trim)
        .any(|line| starts_public_function(line, function_name))
}

/// Returns the generated contract function name used by one test artifact.
fn generated_contract_function(text: &str) -> Option<&'static str> {
    [
        "generated_surface_contract",
        "generated_binding_surface_contract",
    ]
    .into_iter()
    .find(|name| has_public_function(text, name))
}

/// Returns whether a trimmed line starts `pub function_name(`.
fn starts_public_function(line: &str, function_name: &str) -> bool {
    line.strip_prefix("pub ").is_some_and(|tail| {
        tail.starts_with(function_name) && tail[function_name.len()..].starts_with('(')
    })
}

#[cfg(test)]
#[path = "js_type_emission_contract_test.rs"]
mod js_type_emission_contract_test;
