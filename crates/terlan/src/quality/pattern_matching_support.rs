use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::terlan_quality::{render_failure, QualityResult};

const PATTERN_MATRIX: &str = "docs/compiler/type_spec/pattern_matching_support_matrix.json";
const TREE_SITTER_CORPUS: &str = "tree-sitter-terlan/test/corpus/basic.txt";
const SHAPE_TREE_SITTER_CORPUS: &str = "tree-sitter-terlan/test/corpus/shape_synonyms.txt";
const STRING_PATTERN_DOCS: &[&str] = &["README.md", "docs/grammar/README.md"];
const INFERRED_STRING_CAPTURE_EXAMPLE: &str = "\"assets/${bucket}/${file}.txt\"";
const TYPED_STRING_CAPTURE_EXAMPLE: &str = "\"GET /users/${id: Int}\"";
const REQUIRED_FAMILIES: &[&str] = &[
    "wildcard",
    "variable_binding",
    "integer_literal",
    "float_literal",
    "string_literal",
    "string_pattern_long_tail",
    "bool_literal",
    "atom_alias",
    "unit_constructor",
    "tuple",
    "exact_list",
    "list_cons",
    "keyed_map",
    "constructor",
    "case_guard",
    "let_destructuring",
    "lambda_parameter",
    "function_parameter",
    "function_head_pattern_parameter",
    "record_struct",
    "bare_match_expression",
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
    "vm-supported",
    "proof-required",
    "rejected",
    "unsupported",
    "not-applicable",
];
const LONG_TAIL_CONTEXTS: &[&str] = &[
    "route",
    "path",
    "function_head",
    "lambda",
    "shape",
    "template",
];
const LONG_TAIL_STAGE_FIELDS: &[&str] = &[
    "parse",
    "typecheck",
    "core_ir",
    "vm",
    "js",
    "tree_sitter",
    "lsp",
    "stdlib_test",
];
const ALLOWED_LONG_TAIL_STAGE_VALUES: &[&str] = &[
    "supported",
    "syntax-only",
    "diagnostic-only",
    "blocked",
    "unsupported",
    "not-applicable",
];
const SHAPE_SYNONYM_CONTEXTS: &[&str] = &[
    "local",
    "exported_imported",
    "guarded",
    "nested",
    "route",
    "function_head",
    "wildcard_fallback",
    "tooling",
];
const SHAPE_SYNONYM_STAGE_FIELDS: &[&str] = &[
    "parse",
    "format",
    "typecheck",
    "core_ir",
    "vm",
    "js",
    "tree_sitter",
    "lsp",
    "docs",
];
const ALLOWED_SHAPE_SYNONYM_STAGE_VALUES: &[&str] = &[
    "supported",
    "partial",
    "diagnostic-only",
    "unsupported",
    "not-applicable",
];

/// Summary produced by the pattern matching support gate.
///
/// Inputs:
/// - Pattern family count from the canonical matrix.
/// - Positive executable Terlan test anchor count.
/// - Adversarial rejection reference count.
///
/// Output:
/// - Stable success metrics for the quality CLI.
///
/// Transformation:
/// - Keeps the language pattern surface, executable VM tests, and unsupported
///   diagnostics tied to one manifest-backed contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternMatchingSupportSummary {
    pub family_count: usize,
    pub long_tail_context_count: usize,
    pub shape_synonym_context_count: usize,
    pub positive_test_count: usize,
    pub adversarial_test_count: usize,
}

/// Runs the pattern matching support matrix gate.
///
/// Inputs:
/// - `root`: repository root containing the matrix and Terlan pattern tests.
///
/// Output:
/// - Success when every required pattern family is classified and covered.
/// - Stable diagnostics for stale rows, unsupported rows without rejection
///   references, or positive test names missing from the executable anchor.
///
/// Transformation:
/// - Treats pattern support as a cross-phase contract instead of relying on
///   parser acceptance as a proxy for VM support.
pub fn run_pattern_matching_support(root: &Path) -> QualityResult<PatternMatchingSupportSummary> {
    let matrix = read_json(root, PATTERN_MATRIX)?;
    let mut diagnostics = Vec::new();
    let positive_test_files = positive_test_files(&matrix, &mut diagnostics);
    let positive_tests = positive_test_names(root, &positive_test_files, &mut diagnostics);
    validate_string_pattern_docs(root, &mut diagnostics);

    let (family_count, long_tail_context_count, positive_test_count, adversarial_test_count) =
        validate_matrix(root, &matrix, &positive_tests, &mut diagnostics);
    let (shape_synonym_context_count, shape_positive_count, shape_adversarial_count) =
        validate_shape_synonym_contexts(root, &matrix, &mut diagnostics);

    if diagnostics.is_empty() {
        Ok(PatternMatchingSupportSummary {
            family_count,
            long_tail_context_count,
            shape_synonym_context_count,
            positive_test_count: positive_test_count + shape_positive_count,
            adversarial_test_count: adversarial_test_count + shape_adversarial_count,
        })
    } else {
        Err(render_failure("pattern-matching-support", &diagnostics))
    }
}

/// Validates the cross-surface acceptance and adversarial matrix for shape aliases.
fn validate_shape_synonym_contexts(
    root: &Path,
    matrix: &Value,
    diagnostics: &mut Vec<String>,
) -> (usize, usize, usize) {
    let Some(contexts) = matrix
        .get("shape_synonyms")
        .and_then(|value| value.get("contexts"))
        .and_then(Value::as_array)
    else {
        diagnostics.push(format!(
            "{PATTERN_MATRIX}: missing `shape_synonyms.contexts` array"
        ));
        return (0, 0, 0);
    };

    let mut seen = BTreeSet::new();
    let mut positive_count = 0;
    let mut adversarial_count = 0;
    for (index, context) in contexts.iter().enumerate() {
        let row = index + 1;
        let Some(id) = nonempty_string(context, "id") else {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: shape-synonym context[{row}] missing non-empty `id`"
            ));
            continue;
        };
        if !seen.insert(id.to_string()) {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: duplicate shape-synonym context `{id}`"
            ));
        }
        let mut unsupported = false;
        for field in SHAPE_SYNONYM_STAGE_FIELDS {
            let Some(value) = nonempty_string(context, field) else {
                diagnostics.push(format!(
                    "{PATTERN_MATRIX}: shape-synonym context `{id}` missing `{field}` stage"
                ));
                continue;
            };
            if !ALLOWED_SHAPE_SYNONYM_STAGE_VALUES.contains(&value) {
                diagnostics.push(format!(
                    "{PATTERN_MATRIX}: shape-synonym context `{id}` has unsupported `{field}` stage `{value}`"
                ));
            }
            unsupported |= matches!(value, "unsupported" | "diagnostic-only");
        }

        let positives = string_array(context, "positive_tests", diagnostics, id);
        let adversarials = string_array(context, "adversarial_tests", diagnostics, id);
        positive_count += positives.len();
        adversarial_count += adversarials.len();
        if positives.is_empty() {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: shape-synonym context `{id}` must list positive evidence"
            ));
        }
        if adversarials.is_empty() {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: shape-synonym context `{id}` must list adversarial evidence"
            ));
        }
        for reference in &positives {
            validate_test_reference(root, id, reference, "positive", diagnostics);
        }
        for reference in &adversarials {
            validate_test_reference(root, id, reference, "adversarial", diagnostics);
        }
        if nonempty_string(context, "tree_sitter") == Some("supported")
            && !positives
                .iter()
                .any(|reference| reference.starts_with(SHAPE_TREE_SITTER_CORPUS))
        {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: shape-synonym context `{id}` claims Tree-sitter support without a shape corpus anchor"
            ));
        }
        if nonempty_string(context, "docs") == Some("supported")
            && !positives
                .iter()
                .any(|reference| reference.starts_with("docs/"))
        {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: shape-synonym context `{id}` claims docs support without a docs anchor"
            ));
        }
        if unsupported && nonempty_string(context, "diagnostic").is_none() {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: shape-synonym context `{id}` has an unsupported stage without a diagnostic code"
            ));
        }
    }

    for required in SHAPE_SYNONYM_CONTEXTS {
        if !seen.contains(*required) {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: missing shape-synonym context `{required}`"
            ));
        }
    }
    (contexts.len(), positive_count, adversarial_count)
}

/// Verifies both public language references retain typed and inferred captures.
fn validate_string_pattern_docs(root: &Path, diagnostics: &mut Vec<String>) {
    for relative in STRING_PATTERN_DOCS {
        let path = root.join(relative);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                diagnostics.push(format!(
                    "{relative}: failed to read string-pattern documentation: {err}"
                ));
                continue;
            }
        };
        for (form, example) in [
            ("inferred", INFERRED_STRING_CAPTURE_EXAMPLE),
            ("typed", TYPED_STRING_CAPTURE_EXAMPLE),
        ] {
            if !text.contains(example) {
                diagnostics.push(format!(
                    "{relative}: missing canonical {form} string-capture example `{example}`"
                ));
            }
        }
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

/// Validates the top-level pattern matrix rows.
fn validate_matrix(
    root: &Path,
    matrix: &Value,
    positive_tests: &BTreeSet<String>,
    diagnostics: &mut Vec<String>,
) -> (usize, usize, usize, usize) {
    if matrix.get("schema").and_then(Value::as_str) != Some("terlan.pattern-matching-support.v1") {
        diagnostics.push(format!(
            "{PATTERN_MATRIX}: schema must be `terlan.pattern-matching-support.v1`"
        ));
    }

    let Some(families) = matrix.get("families").and_then(Value::as_array) else {
        diagnostics.push(format!("{PATTERN_MATRIX}: missing `families` array"));
        return (0, 0, 0, 0);
    };

    let mut seen = BTreeSet::new();
    let mut positive_count = 0;
    let mut adversarial_count = 0;
    let mut long_tail_context_count = 0;
    for (index, family) in families.iter().enumerate() {
        let row = index + 1;
        let Some(id) = nonempty_string(family, "id") else {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: families[{row}] missing non-empty `id`"
            ));
            continue;
        };
        if !seen.insert(id.to_string()) {
            diagnostics.push(format!("{PATTERN_MATRIX}: duplicate family `{id}`"));
        }
        validate_stage_fields(id, family, diagnostics);
        let positives = string_array(family, "positive_tests", diagnostics, id);
        let adversarials = string_array(family, "adversarial_tests", diagnostics, id);
        positive_count += positives.len();
        adversarial_count += adversarials.len();
        validate_positive_tests(root, id, family, &positives, positive_tests, diagnostics);
        validate_adversarial_tests(root, id, family, &positives, &adversarials, diagnostics);
        validate_function_head_js_contract(id, family, &adversarials, diagnostics);
        if id == "string_pattern_long_tail" {
            long_tail_context_count =
                validate_long_tail_contexts(root, family, positive_tests, diagnostics);
        }
    }

    for required in REQUIRED_FAMILIES {
        if !seen.contains(*required) {
            diagnostics.push(format!("{PATTERN_MATRIX}: missing family `{required}`"));
        }
    }

    (
        families.len(),
        long_tail_context_count,
        positive_count,
        adversarial_count,
    )
}

/// Validates the executable context matrix for long-tail string captures.
fn validate_long_tail_contexts(
    root: &Path,
    family: &Value,
    positive_tests: &BTreeSet<String>,
    diagnostics: &mut Vec<String>,
) -> usize {
    for field in ["typecheck", "core_ir", "vm"] {
        if nonempty_string(family, field) != Some("partial") {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: family `string_pattern_long_tail` must keep `{field}` stage `partial` until every context is executable"
            ));
        }
    }

    let Some(contexts) = family.get("contexts").and_then(Value::as_array) else {
        diagnostics.push(format!(
            "{PATTERN_MATRIX}: family `string_pattern_long_tail` missing `contexts` array"
        ));
        return 0;
    };

    let mut seen = BTreeSet::new();
    for (index, context) in contexts.iter().enumerate() {
        let row = index + 1;
        let Some(id) = nonempty_string(context, "id") else {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: string-pattern context[{row}] missing non-empty `id`"
            ));
            continue;
        };
        if !seen.insert(id.to_string()) {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: duplicate string-pattern context `{id}`"
            ));
        }
        for field in LONG_TAIL_STAGE_FIELDS {
            let Some(value) = nonempty_string(context, field) else {
                diagnostics.push(format!(
                    "{PATTERN_MATRIX}: string-pattern context `{id}` missing `{field}` stage"
                ));
                continue;
            };
            if !ALLOWED_LONG_TAIL_STAGE_VALUES.contains(&value) {
                diagnostics.push(format!(
                    "{PATTERN_MATRIX}: string-pattern context `{id}` has unsupported `{field}` stage `{value}`"
                ));
            }
        }

        let positives = string_array(context, "positive_tests", diagnostics, id);
        let adversarials = string_array(context, "adversarial_tests", diagnostics, id);
        if positives.is_empty() {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: string-pattern context `{id}` must list positive evidence"
            ));
        }
        validate_positive_tests(root, id, context, &positives, positive_tests, diagnostics);
        if nonempty_string(context, "tree_sitter") == Some("supported")
            && !positives
                .iter()
                .any(|reference| reference.starts_with(&format!("{TREE_SITTER_CORPUS}::")))
        {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: string-pattern context `{id}` claims Tree-sitter support without a corpus anchor"
            ));
        }
        for reference in &adversarials {
            validate_test_reference(root, id, reference, "adversarial", diagnostics);
        }

        let blocked = ["typecheck", "core_ir", "vm"]
            .iter()
            .any(|field| nonempty_string(context, field) == Some("blocked"));
        if blocked {
            if nonempty_string(context, "diagnostic").is_none() {
                diagnostics.push(format!(
                    "{PATTERN_MATRIX}: blocked string-pattern context `{id}` must declare a diagnostic code"
                ));
            }
            if adversarials.is_empty() {
                diagnostics.push(format!(
                    "{PATTERN_MATRIX}: blocked string-pattern context `{id}` must list adversarial evidence"
                ));
            }
        }
    }

    for required in LONG_TAIL_CONTEXTS {
        if !seen.contains(*required) {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: missing string-pattern context `{required}`"
            ));
        }
    }
    contexts.len()
}

/// Validates all matrix phase fields for one pattern family.
fn validate_stage_fields(id: &str, family: &Value, diagnostics: &mut Vec<String>) {
    for field in STAGE_FIELDS {
        let Some(value) = nonempty_string(family, field) else {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: family `{id}` missing `{field}` stage"
            ));
            continue;
        };
        if !ALLOWED_STAGE_VALUES.contains(&value) {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: family `{id}` has unsupported `{field}` stage `{value}`"
            ));
        }
    }
}

/// Returns the Terlan files that contain positive executable anchors.
fn positive_test_files(matrix: &Value, diagnostics: &mut Vec<String>) -> Vec<String> {
    if let Some(files) = matrix.get("positive_test_files").and_then(Value::as_array) {
        let mut out = Vec::new();
        for item in files {
            match item.as_str().filter(|text| !text.trim().is_empty()) {
                Some(path) => out.push(path.to_string()),
                None => diagnostics.push(format!(
                    "{PATTERN_MATRIX}: `positive_test_files` contains a non-string entry"
                )),
            }
        }
        if out.is_empty() {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: `positive_test_files` must list at least one file"
            ));
        }
        return out;
    }

    let fallback = nonempty_string(matrix, "positive_test_file")
        .unwrap_or("tests/pattern/PatternMatchingTest.terl");
    vec![fallback.to_string()]
}

/// Returns annotated test function names from the positive Terlan anchors.
fn positive_test_names(
    root: &Path,
    positive_test_files: &[String],
    diagnostics: &mut Vec<String>,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for positive_test_file in positive_test_files {
        names.extend(positive_test_names_from_file(
            root,
            positive_test_file,
            diagnostics,
        ));
    }
    names
}

/// Returns annotated test function names from one positive Terlan anchor.
fn positive_test_names_from_file(
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

/// Validates positive test anchors for one matrix row.
fn validate_positive_tests(
    root: &Path,
    id: &str,
    family: &Value,
    positives: &[String],
    positive_tests: &BTreeSet<String>,
    diagnostics: &mut Vec<String>,
) {
    if nonempty_string(family, "vm") == Some("supported") && positives.is_empty() {
        diagnostics.push(format!(
            "{PATTERN_MATRIX}: supported family `{id}` must list positive tests"
        ));
    }
    for test in positives {
        if test.contains("::") {
            validate_test_reference(root, id, test, "positive", diagnostics);
            continue;
        }
        if !positive_tests.contains(test) {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: family `{id}` references missing positive test `{test}`"
            ));
        }
    }
}

/// Validates the cross-target function-head pattern contract.
fn validate_function_head_js_contract(
    id: &str,
    family: &Value,
    adversarials: &[String],
    diagnostics: &mut Vec<String>,
) {
    if id != "function_head_pattern_parameter" {
        return;
    }

    let js_stage = nonempty_string(family, "js").unwrap_or("");
    if js_stage != "unsupported" {
        diagnostics.push(format!(
            "{PATTERN_MATRIX}: family `function_head_pattern_parameter` must keep JS stage `unsupported` until JS lowering is proven"
        ));
    }

    let diagnostic = family.get("diagnostic").and_then(Value::as_str);
    if diagnostic != Some("target_profile_unsupported") {
        diagnostics.push(format!(
            "{PATTERN_MATRIX}: family `function_head_pattern_parameter` must declare diagnostic `target_profile_unsupported`"
        ));
    }

    if !adversarials.iter().any(|reference| {
        reference.contains("build_command_rejects_function_head_pattern_for_js_target")
    }) {
        diagnostics.push(format!(
            "{PATTERN_MATRIX}: family `function_head_pattern_parameter` must reference the JS target rejection anchor"
        ));
    }
}

/// Validates adversarial references for unsupported matrix rows.
fn validate_adversarial_tests(
    root: &Path,
    id: &str,
    family: &Value,
    positives: &[String],
    adversarials: &[String],
    diagnostics: &mut Vec<String>,
) {
    let vm_stage = nonempty_string(family, "vm").unwrap_or("");
    let diagnostic = family.get("diagnostic").and_then(Value::as_str);
    if positives.is_empty() && vm_stage != "supported" {
        if adversarials.is_empty() {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: unsupported family `{id}` must list adversarial tests"
            ));
        }
        if diagnostic.is_none() {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: unsupported family `{id}` must declare a diagnostic code"
            ));
        }
    }
    for reference in adversarials {
        validate_test_reference(root, id, reference, "adversarial", diagnostics);
    }
}

/// Validates that a test reference points at a committed test file.
fn validate_test_reference(
    root: &Path,
    id: &str,
    reference: &str,
    kind: &str,
    diagnostics: &mut Vec<String>,
) {
    let path_text = reference
        .split_once("::")
        .map_or(reference, |(path, _)| path);
    let path = root.join(path_text);
    if !path.is_file() {
        diagnostics.push(format!(
            "{PATTERN_MATRIX}: family `{id}` {kind} reference `{reference}` points at a missing file"
        ));
        return;
    }
    if let Some((_, test_name)) = reference.split_once("::") {
        let Ok(text) = fs::read_to_string(&path) else {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: family `{id}` {kind} reference `{reference}` could not be read"
            ));
            return;
        };
        if !text.contains(test_name) {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: family `{id}` {kind} reference `{reference}` does not name an existing test"
            ));
        }
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
    family_id: &str,
) -> Vec<String> {
    let Some(items) = value.get(field).and_then(Value::as_array) else {
        diagnostics.push(format!(
            "{PATTERN_MATRIX}: family `{family_id}` missing `{field}` array"
        ));
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        if let Some(text) = item.as_str().filter(|text| !text.trim().is_empty()) {
            out.push(text.to_string());
        } else {
            diagnostics.push(format!(
                "{PATTERN_MATRIX}: family `{family_id}` has non-string `{field}` entry"
            ));
        }
    }
    out
}

#[cfg(test)]
#[path = "pattern_matching_support_test.rs"]
mod pattern_matching_support_test;
