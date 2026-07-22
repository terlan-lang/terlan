use super::test_support::{check_syntax_output, check_syntax_output_with_std_interfaces};
use super::*;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn syntax_output_accepts_fixed_integer_binary_layout_constructor() {
    let diagnostics = check_syntax_output(
        "\
module binary_layout_constructor_typecheck.\n\
\n\
import std.vm.BitString.{BitString}.\n\
\n\
pub packet(source_port: Int, delta: Int): BitString ->\n\
    Binary[big] { source_port: UInt[16], delta: IntBits[8] }.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn syntax_output_accepts_exact_bytes_binary_layout_constructor() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module binary_layout_bytes_constructor_typecheck.\n\
\n\
import type std.vm.Bytes.Bytes.\n\
\n\
pub packet(prefix: Bytes): Dynamic ->\n\
    Binary[big] { prefix: Bytes[2] }.\n\
",
        "std/vm/Bytes.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn syntax_output_accepts_exact_bits_binary_layout_constructor() {
    let diagnostics = check_syntax_output(
        "\
module binary_layout_bits_constructor_typecheck.\n\
\n\
import std.vm.BitString.{BitString}.\n\
\n\
pub packet(prefix: BitString): BitString ->\n\
    Binary[big] { prefix: Bits[3] }.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn syntax_output_accepts_terminal_rest_binary_layout_constructor() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module binary_layout_rest_constructor_typecheck.\n\
\n\
import type std.vm.Bytes.Bytes.\n\
\n\
pub packet(body: Bytes): Dynamic ->\n\
    Binary[big] { body: Rest }.\n\
",
        "std/vm/Bytes.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn syntax_output_accepts_unicode_binary_layout_constructors() {
    let diagnostics = check_syntax_output(
        "\
module binary_layout_unicode_constructor_typecheck.\n\
\n\
pub packet(utf8: Int, utf16: Int, utf32: Int): Dynamic ->\n\
    Binary[little] { utf8: Utf8, utf16: Utf16, utf32: Utf32 }.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn syntax_output_rejects_non_integer_unicode_binary_layout_constructor_fields() {
    let diagnostics = check_syntax_output(
        "\
module binary_layout_unicode_constructor_type_mismatch.\n\
\n\
pub packet(utf8: String, utf16: String, utf32: String): Dynamic ->\n\
    Binary[big] { utf8: Utf8, utf16: Utf16, utf32: Utf32 }.\n\
",
    );

    let mismatches = diagnostics
        .iter()
        .filter(|diag| {
            diag.message
                .contains("binary_constructor_field_type_mismatch")
                && diag.message.contains("Int Unicode scalar")
        })
        .count();
    assert_eq!(mismatches, 3, "diagnostics: {diagnostics:?}");
}

#[test]
fn syntax_output_rejects_non_bitstring_binary_layout_constructor_fields() {
    let diagnostics = check_syntax_output(
        "\
module binary_layout_bits_constructor_type_mismatch.\n\
\n\
pub packet(prefix: String): Dynamic ->\n\
    Binary[big] { prefix: Bits[3] }.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.message
                .contains("binary_constructor_field_type_mismatch")
                && diag.message.contains("std.vm.BitString.BitString")
        }),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_rejects_non_bytes_binary_layout_constructor_fields() {
    let diagnostics = check_syntax_output(
        "\
module binary_layout_bytes_constructor_type_mismatch.\n\
\n\
pub packet(prefix: String): Dynamic ->\n\
    Binary[big] { prefix: Bytes[2] }.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.message
                .contains("binary_constructor_field_type_mismatch")
                && diag.message.contains("std.vm.Bytes.Bytes")
        }),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_rejects_oversized_binary_layout_byte_width() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module binary_layout_constructor_byte_width.\n\
\n\
import type std.vm.Bytes.Bytes.\n\
\n\
pub packet(value: Bytes): Dynamic ->\n\
    Binary[big] { value: Bytes[1152921504606846976] }.\n\
",
        "std/vm/Bytes.terl",
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.message.contains("invalid_binary_constructor_width")
                && diag.message.contains("byte width")
        }),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_rejects_non_integer_binary_layout_constructor_fields() {
    let diagnostics = check_syntax_output(
        "\
module binary_layout_constructor_type_mismatch.\n\
\n\
pub packet(source_port: String): Dynamic ->\n\
    Binary[big] { source_port: UInt[16] }.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.message
                .contains("binary_constructor_field_type_mismatch")
        }),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_rejects_non_bytes_terminal_rest_fields() {
    let diagnostics = check_syntax_output(
        "\
module binary_layout_constructor_rest_type_mismatch.\n\
\n\
pub packet(payload: String): Dynamic ->\n\
    Binary[big] { payload: Rest }.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.message
                .contains("binary_constructor_field_type_mismatch")
                && diag
                    .message
                    .contains("std.vm.Bytes.Bytes for terminal Rest")
        }),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_rejects_oversized_binary_layout_integer_width() {
    let diagnostics = check_syntax_output(
        "\
module binary_layout_constructor_width.\n\
\n\
pub packet(value: Int): Dynamic ->\n\
    Binary[big] { value: UInt[64] }.\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("invalid_binary_constructor_width")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_rejects_unbound_binary_layout_field_value() {
    let diagnostics = check_syntax_output(
        "\
module binary_layout_constructor_unbound.\n\
\n\
pub packet(): Dynamic ->\n\
    Binary[big] { missing: UInt[8] }.\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("unknown_binary_constructor_field")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_lowering_to_core_builds_fixed_integer_binary_layout() {
    let module = parse_module_as_syntax_output(
        "\
module core_binary_layout.\n\
\n\
pub packet(source_port: Int, delta: Int): Dynamic ->\n\
    Binary[little] { source_port: UInt[16], delta: IntBits[8] }.\n",
    )
    .expect("parse binary layout fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let body = core.functions[0].clauses[0]
        .body
        .core_expr
        .as_ref()
        .expect("typed binary constructor CoreIR");

    let CoreExpr::Intrinsic(concat) = body else {
        panic!("expected concat intrinsic, found {body:?}");
    };
    assert_eq!(
        concat.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmBitStringConcat)
    );
    assert!(matches!(
        concat.args.as_slice(),
        [
            CoreExpr::Intrinsic(CoreIntrinsicCall {
                id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmBitStringFromUintLe),
                ..
            }),
            CoreExpr::Intrinsic(CoreIntrinsicCall {
                id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmBitStringFromIntLe),
                ..
            })
        ]
    ));
}

#[test]
fn syntax_output_lowering_to_core_builds_exact_bytes_binary_layout() {
    let module = parse_module_as_syntax_output(
        "\
module core_binary_bytes_layout.\n\
\n\
import type std.vm.Bytes.Bytes.\n\
\n\
pub packet(prefix: Bytes, marker: Int): Dynamic ->\n\
    Binary[big] { prefix: Bytes[2], marker: UInt[8] }.\n",
    )
    .expect("parse binary bytes layout fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let body = core.functions[0].clauses[0]
        .body
        .core_expr
        .as_ref()
        .expect("typed binary bytes constructor CoreIR");

    let CoreExpr::Intrinsic(concat) = body else {
        panic!("expected concat intrinsic, found {body:?}");
    };
    let [CoreExpr::Intrinsic(bytes), CoreExpr::Intrinsic(marker)] = concat.args.as_slice() else {
        panic!("expected exact bytes and marker intrinsics: {concat:?}");
    };
    assert_eq!(
        bytes.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmBitStringFromExactBytes)
    );
    assert_eq!(
        CorePrimitiveIntrinsic::VmBitStringFromExactBytes.registry_key(),
        "vm.bitstring.from_exact_bytes"
    );
    assert_eq!(bytes.return_type, CoreType::Named("BitString".to_string()));
    assert_eq!(
        bytes.args.as_slice(),
        [CoreExpr::Var("prefix".to_string()), CoreExpr::Int(2)]
    );
    assert_eq!(
        marker.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmBitStringFromUintBe)
    );
}

#[test]
fn syntax_output_lowering_to_core_builds_exact_bits_binary_layout() {
    let module = parse_module_as_syntax_output(
        "\
module core_binary_bits_layout.\n\
\n\
import std.vm.BitString.{BitString}.\n\
\n\
pub packet(prefix: BitString, marker: Int): Dynamic ->\n\
    Binary[big] { prefix: Bits[3], marker: UInt[5] }.\n",
    )
    .expect("parse binary bits layout fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let body = core.functions[0].clauses[0]
        .body
        .core_expr
        .as_ref()
        .expect("typed binary bits constructor CoreIR");

    let CoreExpr::Intrinsic(concat) = body else {
        panic!("expected concat intrinsic, found {body:?}");
    };
    let [CoreExpr::Intrinsic(bits), CoreExpr::Intrinsic(marker)] = concat.args.as_slice() else {
        panic!("expected exact bits and marker intrinsics: {concat:?}");
    };
    assert_eq!(
        bits.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmBitStringRequireExactBits)
    );
    assert_eq!(
        CorePrimitiveIntrinsic::VmBitStringRequireExactBits.registry_key(),
        "vm.bitstring.require_exact_bits"
    );
    assert_eq!(bits.return_type, CoreType::Named("BitString".to_string()));
    assert_eq!(
        bits.args.as_slice(),
        [CoreExpr::Var("prefix".to_string()), CoreExpr::Int(3)]
    );
    assert_eq!(
        marker.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmBitStringFromUintBe)
    );
}

#[test]
fn syntax_output_lowering_to_core_builds_terminal_rest_binary_layout() {
    let module = parse_module_as_syntax_output(
        "\
module core_binary_rest_layout.\n\
\n\
import type std.vm.Bytes.Bytes.\n\
\n\
pub packet(marker: Int, body: Bytes): Dynamic ->\n\
    Binary[big] { marker: UInt[8], body: Rest }.\n",
    )
    .expect("parse binary rest layout fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let body = core.functions[0].clauses[0]
        .body
        .core_expr
        .as_ref()
        .expect("typed binary rest constructor CoreIR");

    let CoreExpr::Intrinsic(concat) = body else {
        panic!("expected concat intrinsic, found {body:?}");
    };
    let [CoreExpr::Intrinsic(marker), CoreExpr::Intrinsic(rest)] = concat.args.as_slice() else {
        panic!("expected marker and terminal rest intrinsics: {concat:?}");
    };
    assert_eq!(
        marker.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmBitStringFromUintBe)
    );
    assert_eq!(
        rest.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmBitStringFromAllBytes)
    );
    assert_eq!(
        CorePrimitiveIntrinsic::VmBitStringFromAllBytes.registry_key(),
        "vm.bitstring.from_all_bytes"
    );
    assert_eq!(rest.return_type, CoreType::Named("BitString".to_string()));
    assert_eq!(rest.args.as_slice(), [CoreExpr::Var("body".to_string())]);
}

#[test]
fn syntax_output_lowering_to_core_builds_unicode_binary_layouts() {
    let module = parse_module_as_syntax_output(
        "\
module core_binary_unicode_layout.\n\
\n\
pub utf8_packet(scalar: Int): Dynamic -> Binary[big] { scalar: Utf8 }.\n\
pub utf16_be_packet(scalar: Int): Dynamic -> Binary[big] { scalar: Utf16 }.\n\
pub utf16_le_packet(scalar: Int): Dynamic -> Binary[little] { scalar: Utf16 }.\n\
pub utf32_be_packet(scalar: Int): Dynamic -> Binary[big] { scalar: Utf32 }.\n\
pub utf32_le_packet(scalar: Int): Dynamic -> Binary[little] { scalar: Utf32 }.\n",
    )
    .expect("parse binary Unicode layout fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let expected = [
        ("utf8_packet", CorePrimitiveIntrinsic::VmBitStringUtf8Scalar),
        (
            "utf16_be_packet",
            CorePrimitiveIntrinsic::VmBitStringUtf16BeScalar,
        ),
        (
            "utf16_le_packet",
            CorePrimitiveIntrinsic::VmBitStringUtf16LeScalar,
        ),
        (
            "utf32_be_packet",
            CorePrimitiveIntrinsic::VmBitStringUtf32BeScalar,
        ),
        (
            "utf32_le_packet",
            CorePrimitiveIntrinsic::VmBitStringUtf32LeScalar,
        ),
    ];
    for (name, expected) in expected {
        let function = core
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect("named Unicode constructor");
        let body = function.clauses[0]
            .body
            .core_expr
            .as_ref()
            .expect("typed binary Unicode constructor CoreIR");
        let CoreExpr::Intrinsic(scalar) = body else {
            panic!("expected Unicode scalar intrinsic, found {body:?}");
        };
        assert_eq!(scalar.id, CoreIntrinsicId::Primitive(expected));
        assert_eq!(
            scalar.args.as_slice(),
            [CoreExpr::Var("scalar".to_string())]
        );
    }
}
