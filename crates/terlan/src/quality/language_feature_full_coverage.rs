use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::Value;

use super::support::annotated_public_test_names;
use crate::terlan_quality::{render_failure, QualityResult};

const LANGUAGE_FEATURE_MATRIX: &str =
    "docs/compiler/type_spec/language_feature_coverage_matrix.json";
const REQUIRED_FEATURES: &[&str] = &[
    "module_declaration",
    "value_import",
    "type_import",
    "type_alias",
    "public_function",
    "private_function",
    "generic_function",
    "let_binding",
    "sequencing",
    "lambda_inline",
    "if_expression",
    "case_expression",
    "constructor_call",
    "pattern_matching",
    "list_literal",
    "tuple_literal",
    "map_literal",
    "index_access",
    "operator_expressions",
    "string_receiver",
    "primitive_constructor",
    "struct_declaration",
    "receiver_methods",
    "trait_impl_dispatch",
    "pipe_forward",
    "index_assignment",
    "diagnostics",
];
const STAGE_FIELDS: &[&str] = &[
    "parse",
    "format",
    "typecheck",
    "core_ir",
    "target_profile",
    "vm",
    "js",
    "lsp",
];
const ALLOWED_STAGE_VALUES: &[&str] = &[
    "supported",
    "partial",
    "vm-gated",
    "unsupported",
    "rejected",
    "diagnostic-only",
    "not-applicable",
];

/// Summary produced by the language feature coverage quality gate.
///
/// Inputs:
/// - Feature rows from the canonical language coverage matrix.
/// - Executable Terlan positive test anchors.
/// - Adversarial and implementation source references.
///
/// Output:
/// - Stable counts for the quality CLI.
///
/// Transformation:
/// - Turns the source language surface into an executable release contract
///   instead of relying on parser or typechecker acceptance alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageFeatureCoverage100Summary {
    pub feature_count: usize,
    pub positive_test_count: usize,
    pub adversarial_reference_count: usize,
    pub source_fragment_count: usize,
}

/// Runs the language feature coverage matrix gate.
///
/// Inputs:
/// - `root`: repository root containing the matrix and executable Terlan tests.
///
/// Output:
/// - Success when every required source feature is classified and supported VM
///   rows have executable `.terl` coverage.
/// - Stable diagnostics for missing rows, stale anchors, or unsupported rows
///   without rejection coverage.
///
/// Transformation:
/// - Keeps parser, formatter, typechecker, CoreIR, target-profile, VM, JS, LSP,
///   and diagnostic status synchronized in one matrix.
pub fn run_language_feature_coverage_100(
    root: &Path,
) -> QualityResult<LanguageFeatureCoverage100Summary> {
    let matrix = read_json(root, LANGUAGE_FEATURE_MATRIX)?;
    let mut diagnostics = Vec::new();
    let positive_tests = positive_test_names(root, &matrix, &mut diagnostics);

    let (feature_count, positive_test_count, adversarial_reference_count, source_fragment_count) =
        validate_matrix(root, &matrix, &positive_tests, &mut diagnostics);

    if diagnostics.is_empty() {
        Ok(LanguageFeatureCoverage100Summary {
            feature_count,
            positive_test_count,
            adversarial_reference_count,
            source_fragment_count,
        })
    } else {
        Err(render_failure(
            "language-feature-coverage-100",
            &diagnostics,
        ))
    }
}

/// Reads one JSON manifest from the repository.
fn read_json(root: &Path, relative: &str) -> QualityResult<Value> {
    let path = root.join(relative);
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read JSON: {err}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|err| format!("{}: failed to parse JSON: {err}", path.display()))
}

/// Returns all executable `@test` names declared by matrix positive files.
fn positive_test_names(
    root: &Path,
    matrix: &Value,
    diagnostics: &mut Vec<String>,
) -> BTreeSet<String> {
    let Some(files) = matrix.get("positive_test_files").and_then(Value::as_array) else {
        diagnostics.push(format!(
            "{LANGUAGE_FEATURE_MATRIX}: missing `positive_test_files` array"
        ));
        return BTreeSet::new();
    };

    let mut names = BTreeSet::new();
    for file in files {
        let Some(file) = file.as_str().filter(|text| !text.trim().is_empty()) else {
            diagnostics.push(format!(
                "{LANGUAGE_FEATURE_MATRIX}: `positive_test_files` contains a non-string entry"
            ));
            continue;
        };
        names.extend(annotated_public_test_names(root, file, diagnostics));
    }
    names
}

/// Returns annotated test function names from one Terlan test file.
/// Validates the top-level feature matrix rows.
fn validate_matrix(
    root: &Path,
    matrix: &Value,
    positive_tests: &BTreeSet<String>,
    diagnostics: &mut Vec<String>,
) -> (usize, usize, usize, usize) {
    if matrix.get("schema").and_then(Value::as_str) != Some("terlan.language-feature-coverage.v1") {
        diagnostics.push(format!(
            "{LANGUAGE_FEATURE_MATRIX}: schema must be `terlan.language-feature-coverage.v1`"
        ));
    }

    let Some(features) = matrix.get("features").and_then(Value::as_array) else {
        diagnostics.push(format!(
            "{LANGUAGE_FEATURE_MATRIX}: missing `features` array"
        ));
        return (0, 0, 0, 0);
    };

    let mut seen = BTreeSet::new();
    let mut positive_count = 0;
    let mut adversarial_count = 0;
    let mut source_fragment_count = 0;
    for (index, feature) in features.iter().enumerate() {
        let row = index + 1;
        let Some(id) = nonempty_string(feature, "id") else {
            diagnostics.push(format!(
                "{LANGUAGE_FEATURE_MATRIX}: features[{row}] missing non-empty `id`"
            ));
            continue;
        };
        if !seen.insert(id.to_string()) {
            diagnostics.push(format!(
                "{LANGUAGE_FEATURE_MATRIX}: duplicate feature `{id}`"
            ));
        }
        validate_required_string(id, feature, "feature", diagnostics);
        validate_stage_fields(id, feature, diagnostics);
        let positives = string_array(feature, "positive_tests", diagnostics, id);
        let adversarials = string_array(feature, "adversarial_tests", diagnostics, id);
        let source_fragments = string_array(feature, "source_fragments", diagnostics, id);
        positive_count += positives.len();
        adversarial_count += adversarials.len();
        source_fragment_count += source_fragments.len();
        validate_positive_tests(id, feature, &positives, positive_tests, diagnostics);
        validate_adversarial_tests(root, id, feature, &positives, &adversarials, diagnostics);
        validate_source_fragments(root, id, &source_fragments, diagnostics);
    }

    for required in REQUIRED_FEATURES {
        if !seen.contains(*required) {
            diagnostics.push(format!(
                "{LANGUAGE_FEATURE_MATRIX}: missing feature `{required}`"
            ));
        }
    }

    (
        features.len(),
        positive_count,
        adversarial_count,
        source_fragment_count,
    )
}

/// Validates a required non-empty string field for one feature row.
fn validate_required_string(id: &str, feature: &Value, field: &str, diagnostics: &mut Vec<String>) {
    if nonempty_string(feature, field).is_none() {
        diagnostics.push(format!(
            "{LANGUAGE_FEATURE_MATRIX}: feature `{id}` missing non-empty `{field}`"
        ));
    }
}

/// Validates all matrix phase fields for one feature row.
fn validate_stage_fields(id: &str, feature: &Value, diagnostics: &mut Vec<String>) {
    for field in STAGE_FIELDS {
        let Some(value) = nonempty_string(feature, field) else {
            diagnostics.push(format!(
                "{LANGUAGE_FEATURE_MATRIX}: feature `{id}` missing `{field}` stage"
            ));
            continue;
        };
        if !ALLOWED_STAGE_VALUES.contains(&value) {
            diagnostics.push(format!(
                "{LANGUAGE_FEATURE_MATRIX}: feature `{id}` has unsupported `{field}` stage `{value}`"
            ));
        }
    }
}

/// Validates positive test anchors for one matrix row.
fn validate_positive_tests(
    id: &str,
    feature: &Value,
    positives: &[String],
    positive_tests: &BTreeSet<String>,
    diagnostics: &mut Vec<String>,
) {
    if nonempty_string(feature, "vm") == Some("supported") && positives.is_empty() {
        diagnostics.push(format!(
            "{LANGUAGE_FEATURE_MATRIX}: VM-supported feature `{id}` must list positive tests"
        ));
    }
    for test in positives {
        if !positive_tests.contains(test) {
            diagnostics.push(format!(
                "{LANGUAGE_FEATURE_MATRIX}: feature `{id}` references missing positive test `{test}`"
            ));
        }
    }
}

/// Validates adversarial references for partial, unsupported, or diagnostic rows.
fn validate_adversarial_tests(
    root: &Path,
    id: &str,
    feature: &Value,
    positives: &[String],
    adversarials: &[String],
    diagnostics: &mut Vec<String>,
) {
    let vm_stage = nonempty_string(feature, "vm").unwrap_or("");
    let diagnostic = feature
        .get("diagnostic")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty());
    if positives.is_empty() && vm_stage != "supported" {
        if adversarials.is_empty() {
            diagnostics.push(format!(
                "{LANGUAGE_FEATURE_MATRIX}: non-supported feature `{id}` must list adversarial tests"
            ));
        }
        if diagnostic.is_none() {
            diagnostics.push(format!(
                "{LANGUAGE_FEATURE_MATRIX}: non-supported feature `{id}` must declare a diagnostic code"
            ));
        }
    }
    for reference in adversarials {
        validate_reference(root, id, "adversarial", reference, diagnostics);
    }
}

/// Validates source fragments for one feature row.
fn validate_source_fragments(
    root: &Path,
    id: &str,
    fragments: &[String],
    diagnostics: &mut Vec<String>,
) {
    if fragments.is_empty() {
        diagnostics.push(format!(
            "{LANGUAGE_FEATURE_MATRIX}: feature `{id}` must list source fragments"
        ));
    }
    for reference in fragments {
        validate_reference(root, id, "source fragment", reference, diagnostics);
    }
}

/// Validates that a `path::needle` reference points at committed source text.
fn validate_reference(
    root: &Path,
    id: &str,
    kind: &str,
    reference: &str,
    diagnostics: &mut Vec<String>,
) {
    let Some((path_text, needle)) = reference.split_once("::") else {
        diagnostics.push(format!(
            "{LANGUAGE_FEATURE_MATRIX}: feature `{id}` {kind} reference `{reference}` must use `path::needle`"
        ));
        return;
    };
    if needle.trim().is_empty() {
        diagnostics.push(format!(
            "{LANGUAGE_FEATURE_MATRIX}: feature `{id}` {kind} reference `{reference}` has an empty needle"
        ));
        return;
    }
    let path = root.join(path_text);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => {
            diagnostics.push(format!(
                "{LANGUAGE_FEATURE_MATRIX}: feature `{id}` {kind} reference `{reference}` could not be read: {err}"
            ));
            return;
        }
    };
    if !text.contains(needle) {
        diagnostics.push(format!(
            "{LANGUAGE_FEATURE_MATRIX}: feature `{id}` {kind} reference `{reference}` does not match current source"
        ));
    }
}

/// Reads a non-empty string field.
fn nonempty_string<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
}

/// Reads a string array field and reports malformed entries.
fn string_array(
    value: &Value,
    field: &str,
    diagnostics: &mut Vec<String>,
    feature_id: &str,
) -> Vec<String> {
    let Some(items) = value.get(field).and_then(Value::as_array) else {
        diagnostics.push(format!(
            "{LANGUAGE_FEATURE_MATRIX}: feature `{feature_id}` missing `{field}` array"
        ));
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        if let Some(text) = item.as_str().filter(|text| !text.trim().is_empty()) {
            out.push(text.to_string());
        } else {
            diagnostics.push(format!(
                "{LANGUAGE_FEATURE_MATRIX}: feature `{feature_id}` has non-string `{field}` entry"
            ));
        }
    }
    out
}

#[cfg(test)]
#[path = "language_feature_full_coverage_test.rs"]
#[cfg(test)]
mod language_feature_full_coverage_test;
