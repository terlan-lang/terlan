use std::collections::BTreeMap;

use crate::terlan_syntax::parse_module_as_syntax_output;

/// Verifies primitive receiver named arguments lower in intrinsic ABI order.
///
/// Inputs:
/// - A string receiver `replace` call whose named arguments are written out of
///   declaration order.
///
/// Output:
/// - Test passes when generated Erlang calls `string:replace` with pattern
///   before replacement.
///
/// Transformation:
/// - Reorders source named arguments before delegating to the primitive
///   intrinsic Erlang lowerer.
#[test]
fn formal_syntax_output_direct_emit_reorders_primitive_receiver_named_arguments() {
    let module = parse_module_as_syntax_output(
        r#"
module primitive_receiver_named_emit.

pub demo(): String ->
    "hello".replace(replacement = "x", pattern = "l").
"#,
    )
    .expect("parse primitive receiver named argument fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("primitive receiver named argument should lower")
    .render();

    assert!(
        output.contains("string:replace(\"hello\", \"l\", \"x\", 'all')"),
        "output:\n{}",
        output
    );
}

/// Verifies string-plus-int lowers to binary-safe text concatenation.
///
/// Inputs:
/// - A syntax-output module returning `"index: " + index`.
///
/// Output:
/// - Test passes when emitted Erlang converts operands through
///   `unicode:characters_to_binary`.
///
/// Transformation:
/// - Keeps the syntax bridge aligned with Terlan's string-plus-printable-scalar
///   type rule instead of leaking Erlang `string:concat/2` limitations.
#[test]
fn formal_syntax_output_direct_emit_lowers_string_concat_with_int_operand() {
    let module = parse_module_as_syntax_output(
        r#"
module string_concat_int_emit.

pub label(index: Int): String ->
    "index: " + index.
"#,
    )
    .expect("parse string concat fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("string concat with int should lower")
    .render();

    assert!(
        output.contains("unicode:characters_to_binary(["),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("integer_to_binary(V)"),
        "output:\n{}",
        output
    );
    assert!(
        !output.contains("string:concat"),
        "string-plus-scalar should not lower through string:concat/2:\n{}",
        output
    );
}
