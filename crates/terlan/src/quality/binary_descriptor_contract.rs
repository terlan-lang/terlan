use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::terlan_quality::{render_failure, QualityResult};

const MATRIX_PATH: &str = "docs/compiler/type_spec/binary_descriptor_matrix.json";
const STD_SOURCE_PATH: &str = "std/binary/Binary.terl";
const STD_TEST_PATH: &str = "std/binary/BinaryTest.terl";
const STD_README_PATH: &str = "std/binary/README.md";
const STD_SUMMARY_PATH: &str = "std/summaries/std.binary.Binary.typi";
const RELEASE_MANIFEST_PATH: &str = "std/RELEASE_MANIFEST.tsv";
const RELEASE_API_TESTS_PATH: &str = "tests/std/RELEASE_API_TESTS.tsv";

const REQUIRED_DESCRIPTOR_TYPES: &[&str] = &[
    "UInt",
    "IntBits",
    "Bytes",
    "Bits",
    "Rest",
    "ProtocolField",
    "ProtocolShape",
    "ProtocolShapeAlias",
    "ProtocolShapeSet",
];

const REQUIRED_UNSUPPORTED_TESTS: &[&str] = &[
    "decode_exact_returns_typed_unsupported_runtime",
    "decode_prefix_returns_typed_unsupported_runtime",
    "construct_returns_typed_unsupported_runtime",
];

const REQUIRED_DOC_TERMS: &[&str] = &[
    "descriptor-directed protocol encoding",
    "does not enable source-level binary pattern matching",
    "UnsupportedRuntime",
    "ProtocolShapeSet",
    "make binary-descriptor-check",
];

const REQUIRED_SOURCE_TERMS: &[&str] = &[
    "pub type UInt[Width]",
    "pub type IntBits[Width]",
    "pub type Bytes[Width]",
    "pub type Bits[Width]",
    "pub type Rest",
    "pub struct ProtocolField",
    "pub struct ProtocolShape",
    "pub struct ProtocolShapeAlias",
    "pub struct ProtocolShapeSet",
    "pub validate_descriptor",
    "pub validate_protocol_shape",
    "pub validate_protocol_shape_set",
];

/// Summary produced by the binary descriptor contract gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryDescriptorContractSummary {
    pub descriptor_count: usize,
    pub unsupported_runtime_test_count: usize,
    pub coverage_inventory_count: usize,
}

#[derive(Debug, Deserialize)]
struct BinaryDescriptorMatrix {
    schema: String,
    module: String,
    stage: String,
    descriptors: Vec<BinaryDescriptorRow>,
    unsupported_runtime_tests: Vec<String>,
    coverage_inventories: Vec<String>,
    gate: String,
}

#[derive(Debug, Deserialize)]
struct BinaryDescriptorRow {
    id: String,
    #[serde(rename = "type")]
    type_name: String,
    canonical_example: String,
    runtime: String,
    positive_tests: Vec<String>,
    adversarial_tests: Vec<String>,
}

/// Runs the binary descriptor release-contract gate.
///
/// Inputs:
/// - Repository root containing the binary descriptor std module, generated
///   summary, release manifests, and descriptor matrix.
///
/// Output:
/// - Success summary when binary descriptors and protocol encoding are
///   documented, tested, and represented in release inventories.
/// - Stable diagnostics when a descriptor, test anchor, or inventory entry
///   drifts from the source contract.
///
/// Transformation:
/// - Treats `std.binary.Binary` as a descriptor and protocol-encoding release
///   surface while source-level binary matching remains unsupported.
pub fn run_binary_descriptor_contract(
    root: &Path,
) -> QualityResult<BinaryDescriptorContractSummary> {
    let mut diagnostics = Vec::new();
    let matrix = read_matrix(root, &mut diagnostics);
    let source = read_text(root, STD_SOURCE_PATH, &mut diagnostics);
    let test_source = read_text(root, STD_TEST_PATH, &mut diagnostics);
    let readme = read_text(root, STD_README_PATH, &mut diagnostics);
    let summary = read_text(root, STD_SUMMARY_PATH, &mut diagnostics);
    let release_manifest = read_text(root, RELEASE_MANIFEST_PATH, &mut diagnostics);
    let release_api_tests = read_text(root, RELEASE_API_TESTS_PATH, &mut diagnostics);

    let mut descriptor_count = 0;
    let mut unsupported_runtime_test_count = 0;
    let mut coverage_inventory_count = 0;

    if let Some(matrix) = matrix.as_ref() {
        descriptor_count = matrix.descriptors.len();
        unsupported_runtime_test_count = matrix.unsupported_runtime_tests.len();
        coverage_inventory_count = matrix.coverage_inventories.len();
        diagnostics.extend(validate_matrix_header(matrix));
        diagnostics.extend(validate_descriptor_rows(
            matrix,
            &source,
            &test_source,
            &release_api_tests,
        ));
        diagnostics.extend(validate_unsupported_runtime_tests(matrix, &test_source));
        diagnostics.extend(validate_coverage_inventories(matrix, root));
    }

    diagnostics.extend(validate_required_source_terms(&source));
    diagnostics.extend(validate_required_doc_terms(STD_README_PATH, &readme));
    diagnostics.extend(validate_required_doc_terms(STD_SUMMARY_PATH, &summary));
    diagnostics.extend(validate_release_manifest(&release_manifest));

    if diagnostics.is_empty() {
        Ok(BinaryDescriptorContractSummary {
            descriptor_count,
            unsupported_runtime_test_count,
            coverage_inventory_count,
        })
    } else {
        Err(render_failure("binary-descriptor-contract", &diagnostics))
    }
}

fn read_matrix(root: &Path, diagnostics: &mut Vec<String>) -> Option<BinaryDescriptorMatrix> {
    let text = read_text(root, MATRIX_PATH, diagnostics);
    if text.is_empty() {
        return None;
    }
    match serde_json::from_str(&text) {
        Ok(matrix) => Some(matrix),
        Err(err) => {
            diagnostics.push(format!("{MATRIX_PATH}: invalid JSON: {err}"));
            None
        }
    }
}

fn read_text(root: &Path, relative: &str, diagnostics: &mut Vec<String>) -> String {
    match fs::read_to_string(root.join(relative)) {
        Ok(text) => text,
        Err(err) => {
            diagnostics.push(format!("{relative}: failed to read: {err}"));
            String::new()
        }
    }
}

fn validate_matrix_header(matrix: &BinaryDescriptorMatrix) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if matrix.schema != "terlan.binary-descriptor.v1" {
        diagnostics.push(format!(
            "{MATRIX_PATH}: schema must be `terlan.binary-descriptor.v1`"
        ));
    }
    if matrix.module != "std.binary.Binary" {
        diagnostics.push(format!("{MATRIX_PATH}: module must be `std.binary.Binary`"));
    }
    if matrix.stage != "protocol-encoding" {
        diagnostics.push(format!(
            "{MATRIX_PATH}: stage must be `protocol-encoding` while source binary matching remains unsupported"
        ));
    }
    if matrix.gate != "binary-descriptor-check" {
        diagnostics.push(format!(
            "{MATRIX_PATH}: gate must be `binary-descriptor-check`"
        ));
    }
    diagnostics
}

fn validate_descriptor_rows(
    matrix: &BinaryDescriptorMatrix,
    source: &str,
    test_source: &str,
    release_api_tests: &str,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut seen_ids = BTreeSet::new();
    let mut seen_types = BTreeSet::new();

    for row in &matrix.descriptors {
        if row.id.trim().is_empty() {
            diagnostics.push(format!("{MATRIX_PATH}: descriptor row has empty `id`"));
        }
        if !seen_ids.insert(row.id.as_str()) {
            diagnostics.push(format!(
                "{MATRIX_PATH}: duplicate descriptor id `{}`",
                row.id
            ));
        }
        if !seen_types.insert(row.type_name.as_str()) {
            diagnostics.push(format!(
                "{MATRIX_PATH}: duplicate descriptor type `{}`",
                row.type_name
            ));
        }
        if row.canonical_example.trim().is_empty() {
            diagnostics.push(format!(
                "{MATRIX_PATH}: descriptor `{}` has empty canonical example",
                row.id
            ));
        }
        if !matches!(
            row.runtime.as_str(),
            "inert-descriptor" | "terminal-inert-descriptor" | "metadata-only"
        ) {
            diagnostics.push(format!(
                "{MATRIX_PATH}: descriptor `{}` has unsupported runtime classification `{}`",
                row.id, row.runtime
            ));
        }
        if !source.contains(&format!(" {}", row.type_name))
            && !source.contains(&format!("{} ", row.type_name))
        {
            diagnostics.push(format!(
                "{STD_SOURCE_PATH}: missing descriptor type `{}` from matrix row `{}`",
                row.type_name, row.id
            ));
        }
        validate_test_list(
            &row.id,
            "positive_tests",
            &row.positive_tests,
            test_source,
            &mut diagnostics,
        );
        validate_test_list(
            &row.id,
            "adversarial_tests",
            &row.adversarial_tests,
            test_source,
            &mut diagnostics,
        );
        if !release_api_tests.contains(&format!("std.binary.Binary.{}", row.type_name)) {
            diagnostics.push(format!(
                "{RELEASE_API_TESTS_PATH}: missing release API row for `std.binary.Binary.{}`",
                row.type_name
            ));
        }
    }

    for required in REQUIRED_DESCRIPTOR_TYPES {
        if !seen_types.contains(required) {
            diagnostics.push(format!(
                "{MATRIX_PATH}: missing descriptor type `{required}`"
            ));
        }
    }

    diagnostics
}

fn validate_test_list(
    row_id: &str,
    field: &str,
    tests: &[String],
    test_source: &str,
    diagnostics: &mut Vec<String>,
) {
    if tests.is_empty() {
        diagnostics.push(format!(
            "{MATRIX_PATH}: descriptor `{row_id}` must list `{field}`"
        ));
    }
    for test in tests {
        let needle = format!("pub {test}(");
        if !test_source.contains(&needle) {
            diagnostics.push(format!(
                "{MATRIX_PATH}: descriptor `{row_id}` references missing test `{test}`"
            ));
        }
    }
}

fn validate_unsupported_runtime_tests(
    matrix: &BinaryDescriptorMatrix,
    test_source: &str,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let listed = matrix
        .unsupported_runtime_tests
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for required in REQUIRED_UNSUPPORTED_TESTS {
        if !listed.contains(required) {
            diagnostics.push(format!(
                "{MATRIX_PATH}: missing unsupported runtime test `{required}`"
            ));
        }
    }
    for test in &matrix.unsupported_runtime_tests {
        if !test_source.contains(&format!("pub {test}(")) {
            diagnostics.push(format!(
                "{MATRIX_PATH}: unsupported runtime test `{test}` is not declared in `{STD_TEST_PATH}`"
            ));
        }
    }
    diagnostics
}

fn validate_coverage_inventories(matrix: &BinaryDescriptorMatrix, root: &Path) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let inventories = matrix
        .coverage_inventories
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for required in [
        RELEASE_MANIFEST_PATH,
        RELEASE_API_TESTS_PATH,
        STD_SUMMARY_PATH,
    ] {
        if !inventories.contains(required) {
            diagnostics.push(format!(
                "{MATRIX_PATH}: coverage inventories must include `{required}`"
            ));
        }
    }
    for inventory in &matrix.coverage_inventories {
        if !root.join(inventory).is_file() {
            diagnostics.push(format!(
                "{MATRIX_PATH}: coverage inventory `{inventory}` does not exist"
            ));
        }
    }
    diagnostics
}

fn validate_required_source_terms(source: &str) -> Vec<String> {
    REQUIRED_SOURCE_TERMS
        .iter()
        .filter_map(|term| {
            if source.contains(term) {
                None
            } else {
                Some(format!("{STD_SOURCE_PATH}: missing required term `{term}`"))
            }
        })
        .collect()
}

fn validate_required_doc_terms(path: &str, text: &str) -> Vec<String> {
    let normalized_text = normalize_whitespace(text);
    REQUIRED_DOC_TERMS
        .iter()
        .filter_map(|term| {
            if normalized_text.contains(&normalize_whitespace(term)) {
                None
            } else {
                Some(format!(
                    "{path}: missing descriptor documentation term `{term}`"
                ))
            }
        })
        .collect()
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn validate_release_manifest(release_manifest: &str) -> Vec<String> {
    if release_manifest.contains("module\tstd.binary.Binary\tstd/binary/Binary.terl") {
        Vec::new()
    } else {
        vec![format!(
            "{RELEASE_MANIFEST_PATH}: missing `std.binary.Binary` release module row"
        )]
    }
}

#[cfg(test)]
#[path = "binary_descriptor_contract_test.rs"]
#[cfg(test)]
mod binary_descriptor_contract_test;
