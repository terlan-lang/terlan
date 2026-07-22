use super::super::format_source_module;

/// Verifies canonical formatting preserves integral-valued Float syntax.
#[test]
fn formatter_preserves_integral_float_literals_and_patterns() {
    let output = format_source_module(
        "module float_literals.\npub zero(): Float -> 0.0.\npub classify(value: Float): Float -> case value { 0.0 -> 1.0; _ -> value }.\n",
    )
    .expect("format float literal source");

    assert!(output.contains("pub zero(): Float -> 0.0."), "{output}");
    assert!(output.contains("0.0 -> 1.0"), "{output}");
}

/// Verifies scientific literals remain Float values after canonical formatting.
#[test]
fn small_float_parity_formatter_canonicalizes_scientific_literals_without_changing_type() {
    let output = format_source_module(
        "module scientific_float_literals.\npub thousand(): Float -> 1e3.\npub tiny(): Float -> 5.0e-324.\n",
    )
    .expect("format scientific Float source");

    assert!(
        output.contains("pub thousand(): Float -> 1000.0."),
        "{output}"
    );
    assert!(output.contains("pub tiny(): Float -> 5e-324."), "{output}");
    assert_eq!(
        format_source_module(&output).expect("reformat scientific Float source"),
        output
    );
}
