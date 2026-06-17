use std::collections::BTreeMap;

use terlan_syntax::parse_module_as_syntax_output;

/// Verifies compiler-owned BEAM runtime calls still lower after method-call
/// syntax was added.
///
/// Inputs:
/// - A formal syntax-output module containing `erlang.integer_to_list(value)`.
///
/// Output:
/// - Test passes when direct Erlang lowering emits an Erlang remote call.
///
/// Transformation:
/// - Parses the source through canonical syntax output, where
///   `erlang.integer_to_list(...)` is method-shaped syntax, then verifies
///   the Erlang syntax bridge reclassifies the known backend runtime
///   root without enabling arbitrary receiver-method lowering.
#[test]
fn formal_syntax_output_direct_emit_lowers_known_backend_runtime_method_shape() {
    let module = parse_module_as_syntax_output(
        r#"
module backend_runtime_method_shape.

pub render(value: Int): String ->
erlang.integer_to_list(value).
"#,
    )
    .expect("parse backend runtime method-shaped call fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("formal subset should lower known backend runtime method shape")
    .render();

    assert!(
        output.contains("render(Value) ->\n    erlang:integer_to_list(Value)."),
        "output:\n{}",
        output
    );
}
