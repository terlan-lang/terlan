use std::collections::BTreeMap;

use super::test_support::test_core_module_for_syntax;
use super::try_emit_core_module_to_erlang_with_syntax_bridge;
use terlan_syntax::parse_module_as_syntax_output;

/// Verifies intrinsic annotations replace placeholder source bodies.
///
/// Inputs:
/// - A Terlan syntax-output module declaring `@compiler.intrinsic`
///   for `core.string.contains`.
///
/// Output:
/// - Test assertions over emitted Erlang source text.
///
/// Transformation:
/// - Parses Terlan source with `@compiler.intrinsic`, wraps it in the
///   transitional CoreIR syntax-bridge payload, emits Erlang, and checks
///   that the backend intrinsic lowering is used instead of the source
///   placeholder expression.
#[test]
fn compiler_intrinsic_annotation_replaces_string_placeholder_body() {
    let module = parse_module_as_syntax_output(
        r#"
module intrinsic_annotation_fixture.

@compiler.intrinsic {core.string.contains}
pub contains(value: String, pattern: String): Bool ->
false.
"#,
    )
    .expect("syntax output with intrinsic annotation");
    let core = test_core_module_for_syntax(&module);

    let emitted = try_emit_core_module_to_erlang_with_syntax_bridge(
        &core,
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("annotated intrinsic module should emit");

    assert!(emitted.contains("string:find(Value, Pattern)"));
    assert!(emitted.contains("'nomatch'"));
    assert!(!emitted.contains("contains(Value, Pattern) ->\n    false."));
}

/// Verifies primitive Terlan type names map to BEAM specs.
///
/// Inputs:
/// - None; exercises the backend type-name mapper directly.
///
/// Output:
/// - Test passes when source-level `String` and transitional `Text` both lower
///   to BEAM `binary()` while `String` remains available as its own
///   frontend/CoreIR name.
///
/// Transformation:
/// - Converts primitive source type names into Erlang type-spec spelling.
#[test]
fn maps_string_primitive_to_binary_spec() {
    assert_eq!(super::map_type_name("String"), "binary()");
    assert_eq!(super::map_type_name("Text"), "binary()");
    assert_eq!(super::map_type_name("Binary"), "binary()");
}
