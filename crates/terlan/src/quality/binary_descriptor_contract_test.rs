use super::*;
use std::time::UNIX_EPOCH;

#[test]
fn binary_descriptor_matrix_accepts_complete_fixture() {
    let root = make_quality_temp_dir("binary_descriptor_complete");
    write_complete_fixture(&root, complete_matrix());

    let summary = run_binary_descriptor_contract(&root).expect("complete fixture should pass");

    assert_eq!(REQUIRED_DESCRIPTOR_TYPES.len(), summary.descriptor_count);
    assert_eq!(
        REQUIRED_UNSUPPORTED_TESTS.len(),
        summary.unsupported_runtime_test_count
    );
    assert_eq!(3, summary.coverage_inventory_count);
    fs::remove_dir_all(root).expect("remove temp dir");
}

#[test]
fn binary_descriptor_matrix_rejects_missing_required_descriptor() {
    let root = make_quality_temp_dir("binary_descriptor_missing_required");
    write_complete_fixture(
        &root,
        r#"{
  "schema": "terlan.binary-descriptor.v1",
  "module": "std.binary.Binary",
  "stage": "protocol-encoding",
  "descriptors": [],
  "unsupported_runtime_tests": [],
  "coverage_inventories": [
    "std/RELEASE_MANIFEST.tsv",
    "tests/std/RELEASE_API_TESTS.tsv",
    "std/summaries/std.binary.Binary.typi"
  ],
  "gate": "binary-descriptor-check"
}"#,
    );

    let error = run_binary_descriptor_contract(&root).expect_err("missing descriptors should fail");

    assert!(
        error.contains("missing descriptor type `UInt`"),
        "expected UInt diagnostic, got {error}"
    );
    fs::remove_dir_all(root).expect("remove temp dir");
}

#[test]
fn binary_descriptor_matrix_rejects_missing_test_anchor() {
    let root = make_quality_temp_dir("binary_descriptor_missing_test");
    write_complete_fixture(&root, complete_matrix());
    fs::write(root.join(STD_TEST_PATH), "module std.binary.BinaryTest.\n")
        .expect("overwrite test fixture");

    let error =
        run_binary_descriptor_contract(&root).expect_err("missing test anchors should fail");

    assert!(
        error.contains("references missing test `uint_descriptor_records_width`"),
        "expected missing test diagnostic, got {error}"
    );
    fs::remove_dir_all(root).expect("remove temp dir");
}

#[test]
fn binary_descriptor_contract_requires_protocol_encoding_docs() {
    let root = make_quality_temp_dir("binary_descriptor_docs");
    write_complete_fixture(&root, complete_matrix());
    fs::write(root.join(STD_README_PATH), "# std.binary\n").expect("overwrite README");

    let error = run_binary_descriptor_contract(&root).expect_err("missing docs terms should fail");

    assert!(
        error.contains("descriptor-directed protocol encoding"),
        "expected doc term diagnostic, got {error}"
    );
    fs::remove_dir_all(root).expect("remove temp dir");
}

fn complete_matrix() -> &'static str {
    r#"{
  "schema": "terlan.binary-descriptor.v1",
  "module": "std.binary.Binary",
  "stage": "protocol-encoding",
  "descriptors": [
    {
      "id": "uint",
      "type": "UInt",
      "canonical_example": "UInt[16]",
      "runtime": "inert-descriptor",
      "positive_tests": ["uint_descriptor_records_width"],
      "adversarial_tests": ["zero_width_descriptor_is_rejected"]
    },
    {
      "id": "int_bits",
      "type": "IntBits",
      "canonical_example": "IntBits[32]",
      "runtime": "inert-descriptor",
      "positive_tests": ["int_bits_descriptor_records_width"],
      "adversarial_tests": ["negative_width_descriptor_is_rejected"]
    },
    {
      "id": "bytes",
      "type": "Bytes",
      "canonical_example": "Bytes[4]",
      "runtime": "inert-descriptor",
      "positive_tests": ["bytes_descriptor_records_width"],
      "adversarial_tests": ["zero_width_descriptor_is_rejected"]
    },
    {
      "id": "bits",
      "type": "Bits",
      "canonical_example": "Bits[3]",
      "runtime": "inert-descriptor",
      "positive_tests": ["bits_descriptor_records_width"],
      "adversarial_tests": ["negative_width_descriptor_is_rejected"]
    },
    {
      "id": "rest",
      "type": "Rest",
      "canonical_example": "Rest",
      "runtime": "terminal-inert-descriptor",
      "positive_tests": ["rest_descriptor_is_terminal"],
      "adversarial_tests": ["rest_descriptor_requires_zero_width"]
    },
    {
      "id": "protocol_field",
      "type": "ProtocolField",
      "canonical_example": "protocol_field(\"source_port\", make_uint(16))",
      "runtime": "metadata-only",
      "positive_tests": ["tcp_header_first_field_is_source_port_uint16"],
      "adversarial_tests": ["duplicate_protocol_field_names_are_rejected"]
    },
    {
      "id": "protocol_shape",
      "type": "ProtocolShape",
      "canonical_example": "protocol_shape(\"tcp_header\", BigEndian, fields, false)",
      "runtime": "metadata-only",
      "positive_tests": ["compact_frame_shape_records_terminal_payload"],
      "adversarial_tests": ["multiple_rest_fields_are_rejected"]
    },
    {
      "id": "protocol_shape_alias",
      "type": "ProtocolShapeAlias",
      "canonical_example": "protocol_shape_alias(\"current\", \"compact_frame\")",
      "runtime": "metadata-only",
      "positive_tests": ["protocol_shape_set_resolves_direct_and_alias_names"],
      "adversarial_tests": ["protocol_shape_set_rejects_alias_chains_and_missing_targets"]
    },
    {
      "id": "protocol_shape_set",
      "type": "ProtocolShapeSet",
      "canonical_example": "protocol_shape_set(\"wire\", shapes, aliases)",
      "runtime": "metadata-only",
      "positive_tests": ["protocol_shape_set_preserves_declaration_order"],
      "adversarial_tests": ["protocol_shape_set_rejects_duplicate_direct_names"]
    }
  ],
  "unsupported_runtime_tests": [
    "decode_exact_returns_typed_unsupported_runtime",
    "decode_prefix_returns_typed_unsupported_runtime",
    "construct_returns_typed_unsupported_runtime"
  ],
  "coverage_inventories": [
    "std/RELEASE_MANIFEST.tsv",
    "tests/std/RELEASE_API_TESTS.tsv",
    "std/summaries/std.binary.Binary.typi"
  ],
  "gate": "binary-descriptor-check"
}"#
}

fn write_complete_fixture(root: &Path, matrix: &str) {
    write_file(root, MATRIX_PATH, matrix);
    write_file(root, STD_SOURCE_PATH, complete_source());
    write_file(root, STD_TEST_PATH, complete_tests());
    write_file(root, STD_README_PATH, complete_docs());
    write_file(root, STD_SUMMARY_PATH, complete_docs());
    write_file(
        root,
        RELEASE_MANIFEST_PATH,
        "module\tstd.binary.Binary\tstd/binary/Binary.terl\tstd/summaries/std.binary.Binary.typi\ttests/std/RELEASE_API_TESTS.tsv\tstd.binary.Binary.html\n",
    );
    write_file(
        root,
        RELEASE_API_TESTS_PATH,
        "std.binary.Binary.UInt\tstd/binary/BinaryTest.terl\tuint_descriptor_records_width\nstd.binary.Binary.IntBits\tstd/binary/BinaryTest.terl\tint_bits_descriptor_records_width\nstd.binary.Binary.Bytes\tstd/binary/BinaryTest.terl\tbytes_descriptor_records_width\nstd.binary.Binary.Bits\tstd/binary/BinaryTest.terl\tbits_descriptor_records_width\nstd.binary.Binary.Rest\tstd/binary/BinaryTest.terl\trest_descriptor_is_terminal\nstd.binary.Binary.ProtocolField\tstd/binary/BinaryTest.terl\ttcp_header_first_field_is_source_port_uint16\nstd.binary.Binary.ProtocolShape\tstd/binary/BinaryTest.terl\tcompact_frame_shape_records_terminal_payload\nstd.binary.Binary.ProtocolShapeAlias\tstd/binary/BinaryTest.terl\tprotocol_shape_set_resolves_direct_and_alias_names\nstd.binary.Binary.ProtocolShapeSet\tstd/binary/BinaryTest.terl\tprotocol_shape_set_preserves_declaration_order\n",
    );
}

fn write_file(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, text).expect("write fixture");
}

fn complete_source() -> &'static str {
    r#"
module std.binary.Binary.
pub type UInt[Width] = BinaryDescriptor.
pub type IntBits[Width] = BinaryDescriptor.
pub type Bytes[Width] = BinaryDescriptor.
pub type Bits[Width] = BinaryDescriptor.
pub type Rest = BinaryDescriptor.
pub struct ProtocolField { name: String, descriptor: BinaryDescriptor }.
pub struct ProtocolShape { name: String }.
pub struct ProtocolShapeAlias { name: String, target: String }.
pub struct ProtocolShapeSet { name: String }.
pub validate_descriptor(value: BinaryDescriptor): Result[BinaryDescriptor, BinaryDecodeError] -> Ok(value).
pub validate_protocol_shape(value: ProtocolShape): Result[ProtocolShape, BinaryDecodeError] -> Ok(value).
pub validate_protocol_shape_set(value: ProtocolShapeSet): Result[ProtocolShapeSet, BinaryDecodeError] -> Ok(value).
"#
}

fn complete_tests() -> &'static str {
    r#"
module std.binary.BinaryTest.
@test
pub uint_descriptor_records_width(): Bool -> true.
@test
pub int_bits_descriptor_records_width(): Bool -> true.
@test
pub bytes_descriptor_records_width(): Bool -> true.
@test
pub bits_descriptor_records_width(): Bool -> true.
@test
pub rest_descriptor_is_terminal(): Bool -> true.
@test
pub zero_width_descriptor_is_rejected(): Bool -> true.
@test
pub negative_width_descriptor_is_rejected(): Bool -> true.
@test
pub rest_descriptor_requires_zero_width(): Bool -> true.
@test
pub tcp_header_first_field_is_source_port_uint16(): Bool -> true.
@test
pub compact_frame_shape_records_terminal_payload(): Bool -> true.
@test
pub duplicate_protocol_field_names_are_rejected(): Bool -> true.
@test
pub multiple_rest_fields_are_rejected(): Bool -> true.
@test
pub protocol_shape_set_resolves_direct_and_alias_names(): Bool -> true.
@test
pub protocol_shape_set_rejects_alias_chains_and_missing_targets(): Bool -> true.
@test
pub protocol_shape_set_preserves_declaration_order(): Bool -> true.
@test
pub protocol_shape_set_rejects_duplicate_direct_names(): Bool -> true.
@test
pub decode_exact_returns_typed_unsupported_runtime(): Bool -> true.
@test
pub decode_prefix_returns_typed_unsupported_runtime(): Bool -> true.
@test
pub construct_returns_typed_unsupported_runtime(): Bool -> true.
"#
}

fn complete_docs() -> &'static str {
    "descriptor-directed protocol encoding\ndoes not enable source-level binary pattern matching\nUnsupportedRuntime\nProtocolShapeSet\nmake binary-descriptor-check\n"
}

fn make_quality_temp_dir(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("terlan_quality_{label}_{nanos}"))
}
