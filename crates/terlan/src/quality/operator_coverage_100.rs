use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::terlan_quality::{render_failure, QualityResult};

const OPERATOR_MATRIX: &str = "docs/compiler/type_spec/operator_coverage_matrix.json";
const REQUIRED_OPERATORS: &[&str] = &[
    "add",
    "sub",
    "mul",
    "slash_div",
    "div_keyword",
    "rem_keyword",
    "eq",
    "not_eq",
    "lt",
    "gt",
    "lte",
    "gte",
    "and_keyword",
    "or_keyword",
    "and_symbols",
    "or_symbols",
    "unary_neg",
    "not_keyword",
    "bang_not",
    "string_concat",
    "pipe_forward",
    "index_access",
    "index_assignment",
    "deprecated_strict_eq",
    "deprecated_slash_not_eq",
    "deprecated_exact_not_eq",
];
const STAGE_FIELDS: &[&str] = &[
    "parse",
    "format",
    "typecheck",
    "core_ir",
    "target_profile",
    "vm",
    "js",
];
const ALLOWED_STAGE_VALUES: &[&str] = &[
    "supported",
    "partial",
    "vm-gated",
    "unsupported",
    "rejected",
    "not-applicable",
    "compatibility-only",
];

/// Summary produced by the operator coverage quality gate.
///
/// Inputs:
/// - Operator rows from the canonical coverage matrix.
/// - Executable Terlan positive test anchors.
/// - Adversarial rejection and source-fragment references.
///
/// Output:
/// - Stable counts for the quality CLI.
///
/// Transformation:
/// - Keeps parser, typechecker, CoreIR, VM, and JS operator support tied to a
///   single executable matrix instead of scattered assumptions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorCoverage100Summary {
    pub operator_count: usize,
    pub positive_test_count: usize,
    pub adversarial_reference_count: usize,
    pub source_fragment_count: usize,
}

/// Runs the operator coverage matrix gate.
///
/// Inputs:
/// - `root`: repository root containing the operator matrix and executable
///   Terlan operator tests.
///
/// Output:
/// - Success when every required operator is classified and supported VM rows
///   have executable `.terl` tests.
/// - Stable diagnostics for missing rows, stale source anchors, or unsupported
///   rows without adversarial coverage.
///
/// Transformation:
/// - Treats operator support as a cross-phase release contract.
pub fn run_operator_coverage_100(root: &Path) -> QualityResult<OperatorCoverage100Summary> {
    let matrix = read_json(root, OPERATOR_MATRIX)?;
    let mut diagnostics = Vec::new();
    let positive_tests = positive_test_names(root, &matrix, &mut diagnostics);

    let (operator_count, positive_test_count, adversarial_reference_count, source_fragment_count) =
        validate_matrix(root, &matrix, &positive_tests, &mut diagnostics);

    if diagnostics.is_empty() {
        Ok(OperatorCoverage100Summary {
            operator_count,
            positive_test_count,
            adversarial_reference_count,
            source_fragment_count,
        })
    } else {
        Err(render_failure("operator-coverage-100", &diagnostics))
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
    let Some(files) = matrix
        .get("positive_test_files")
        .and_then(serde_json::Value::as_array)
    else {
        diagnostics.push(format!(
            "{OPERATOR_MATRIX}: missing `positive_test_files` array"
        ));
        return BTreeSet::new();
    };

    let mut names = BTreeSet::new();
    for file in files {
        let Some(file) = file.as_str().filter(|text| !text.trim().is_empty()) else {
            diagnostics.push(format!(
                "{OPERATOR_MATRIX}: `positive_test_files` contains a non-string entry"
            ));
            continue;
        };
        names.extend(test_names_from_file(root, file, diagnostics));
    }
    names
}

/// Returns annotated test function names from one Terlan test file.
fn test_names_from_file(
    root: &Path,
    positive_test_file: &str,
    diagnostics: &mut Vec<String>,
) -> BTreeSet<String> {
    let path = root.join(positive_test_file);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => {
            diagnostics.push(format!(
                "{positive_test_file}: failed to read positive test file: {err}"
            ));
            return BTreeSet::new();
        }
    };

    let mut names = BTreeSet::new();
    let mut previous_was_test = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "@test" {
            previous_was_test = true;
            continue;
        }
        if previous_was_test && trimmed.starts_with("pub ") {
            if let Some(name) = function_name(trimmed) {
                names.insert(name.to_string());
            } else {
                diagnostics.push(format!(
                    "{positive_test_file}: failed to read @test function name from `{trimmed}`"
                ));
            }
            previous_was_test = false;
        } else if !trimmed.is_empty() && !trimmed.starts_with('@') {
            previous_was_test = false;
        }
    }
    names
}

/// Extracts a function name from a `pub name(...` declaration line.
fn function_name(line: &str) -> Option<&str> {
    let after_pub = line.strip_prefix("pub ")?;
    let name_end = after_pub.find('(')?;
    let name = &after_pub[..name_end];
    (!name.is_empty()).then_some(name)
}

/// Validates the top-level operator matrix rows.
fn validate_matrix(
    root: &Path,
    matrix: &Value,
    positive_tests: &BTreeSet<String>,
    diagnostics: &mut Vec<String>,
) -> (usize, usize, usize, usize) {
    if matrix.get("schema").and_then(Value::as_str) != Some("terlan.operator-coverage.v1") {
        diagnostics.push(format!(
            "{OPERATOR_MATRIX}: schema must be `terlan.operator-coverage.v1`"
        ));
    }

    let Some(operators) = matrix.get("operators").and_then(Value::as_array) else {
        diagnostics.push(format!("{OPERATOR_MATRIX}: missing `operators` array"));
        return (0, 0, 0, 0);
    };

    let mut seen = BTreeSet::new();
    let mut positive_count = 0;
    let mut adversarial_count = 0;
    let mut source_fragment_count = 0;
    for (index, operator) in operators.iter().enumerate() {
        let row = index + 1;
        let Some(id) = nonempty_string(operator, "id") else {
            diagnostics.push(format!(
                "{OPERATOR_MATRIX}: operators[{row}] missing non-empty `id`"
            ));
            continue;
        };
        if !seen.insert(id.to_string()) {
            diagnostics.push(format!("{OPERATOR_MATRIX}: duplicate operator `{id}`"));
        }
        validate_required_string(id, operator, "spelling", diagnostics);
        validate_required_string(id, operator, "kind", diagnostics);
        validate_stage_fields(id, operator, diagnostics);
        let positives = string_array(operator, "positive_tests", diagnostics, id);
        let adversarials = string_array(operator, "adversarial_tests", diagnostics, id);
        let source_fragments = string_array(operator, "source_fragments", diagnostics, id);
        positive_count += positives.len();
        adversarial_count += adversarials.len();
        source_fragment_count += source_fragments.len();
        validate_positive_tests(id, operator, &positives, positive_tests, diagnostics);
        validate_adversarial_tests(root, id, operator, &positives, &adversarials, diagnostics);
        validate_source_fragments(root, id, &source_fragments, diagnostics);
    }

    for required in REQUIRED_OPERATORS {
        if !seen.contains(*required) {
            diagnostics.push(format!("{OPERATOR_MATRIX}: missing operator `{required}`"));
        }
    }

    (
        operators.len(),
        positive_count,
        adversarial_count,
        source_fragment_count,
    )
}

/// Validates a required non-empty string field for one operator row.
fn validate_required_string(
    id: &str,
    operator: &Value,
    field: &str,
    diagnostics: &mut Vec<String>,
) {
    if nonempty_string(operator, field).is_none() {
        diagnostics.push(format!(
            "{OPERATOR_MATRIX}: operator `{id}` missing non-empty `{field}`"
        ));
    }
}

/// Validates all matrix phase fields for one operator row.
fn validate_stage_fields(id: &str, operator: &Value, diagnostics: &mut Vec<String>) {
    for field in STAGE_FIELDS {
        let Some(value) = nonempty_string(operator, field) else {
            diagnostics.push(format!(
                "{OPERATOR_MATRIX}: operator `{id}` missing `{field}` stage"
            ));
            continue;
        };
        if !ALLOWED_STAGE_VALUES.contains(&value) {
            diagnostics.push(format!(
                "{OPERATOR_MATRIX}: operator `{id}` has unsupported `{field}` stage `{value}`"
            ));
        }
    }
}

/// Validates positive test anchors for one matrix row.
fn validate_positive_tests(
    id: &str,
    operator: &Value,
    positives: &[String],
    positive_tests: &BTreeSet<String>,
    diagnostics: &mut Vec<String>,
) {
    if nonempty_string(operator, "vm") == Some("supported") && positives.is_empty() {
        diagnostics.push(format!(
            "{OPERATOR_MATRIX}: VM-supported operator `{id}` must list positive tests"
        ));
    }
    for test in positives {
        if !positive_tests.contains(test) {
            diagnostics.push(format!(
                "{OPERATOR_MATRIX}: operator `{id}` references missing positive test `{test}`"
            ));
        }
    }
}

/// Validates adversarial references for unsupported or rejected operator rows.
fn validate_adversarial_tests(
    root: &Path,
    id: &str,
    operator: &Value,
    positives: &[String],
    adversarials: &[String],
    diagnostics: &mut Vec<String>,
) {
    let vm_stage = nonempty_string(operator, "vm").unwrap_or("");
    let parse_stage = nonempty_string(operator, "parse").unwrap_or("");
    let diagnostic = operator
        .get("diagnostic")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty());
    if positives.is_empty() && (vm_stage != "supported" || parse_stage == "rejected") {
        if adversarials.is_empty() {
            diagnostics.push(format!(
                "{OPERATOR_MATRIX}: unsupported operator `{id}` must list adversarial tests"
            ));
        }
        if diagnostic.is_none() {
            diagnostics.push(format!(
                "{OPERATOR_MATRIX}: unsupported operator `{id}` must declare a diagnostic code"
            ));
        }
    }
    for reference in adversarials {
        validate_reference(root, id, "adversarial", reference, diagnostics);
    }
}

/// Validates source fragments for one operator row.
fn validate_source_fragments(
    root: &Path,
    id: &str,
    fragments: &[String],
    diagnostics: &mut Vec<String>,
) {
    if fragments.is_empty() {
        diagnostics.push(format!(
            "{OPERATOR_MATRIX}: operator `{id}` must list source fragments"
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
            "{OPERATOR_MATRIX}: operator `{id}` {kind} reference `{reference}` must use `path::needle`"
        ));
        return;
    };
    if needle.trim().is_empty() {
        diagnostics.push(format!(
            "{OPERATOR_MATRIX}: operator `{id}` {kind} reference `{reference}` has an empty needle"
        ));
        return;
    }
    let path = root.join(path_text);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => {
            diagnostics.push(format!(
                "{OPERATOR_MATRIX}: operator `{id}` {kind} reference `{reference}` could not be read: {err}"
            ));
            return;
        }
    };
    if !text.contains(needle) {
        diagnostics.push(format!(
            "{OPERATOR_MATRIX}: operator `{id}` {kind} reference `{reference}` does not match current source"
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
    operator_id: &str,
) -> Vec<String> {
    let Some(items) = value.get(field).and_then(Value::as_array) else {
        diagnostics.push(format!(
            "{OPERATOR_MATRIX}: operator `{operator_id}` missing `{field}` array"
        ));
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        if let Some(text) = item.as_str().filter(|text| !text.trim().is_empty()) {
            out.push(text.to_string());
        } else {
            diagnostics.push(format!(
                "{OPERATOR_MATRIX}: operator `{operator_id}` has non-string `{field}` entry"
            ));
        }
    }
    out
}

#[cfg(test)]
#[path = "operator_coverage_100_test.rs"]
mod operator_coverage_100_test;
