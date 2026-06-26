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

/// Verifies imported native vector aliases map to exported bridge specs.
///
/// Inputs:
/// - Short, angle-application, and fully qualified Terlan vector type text.
///
/// Output:
/// - Test passes when both forms render as the SafeNative vector bridge type.
///
/// Transformation:
/// - Prevents generated modules from emitting invalid local `vector < T >()`
///   or bare `vector(T)` specs for `std.native.collections.Vector` values.
#[test]
fn maps_native_vector_type_to_bridge_spec() {
    assert_eq!(
        super::lower_type_to_spec("Vector[Int]").render(),
        "std_native_collections_vector_safe_native:vector(integer())"
    );
    assert_eq!(
        super::lower_type_to_spec("Vector < Int >").render(),
        "std_native_collections_vector_safe_native:vector(integer())"
    );
    assert_eq!(
        super::lower_type_to_spec("std.native.collections.Vector.Vector[String]").render(),
        "std_native_collections_vector_safe_native:vector(binary())"
    );
}

/// Verifies native vector specs remain valid inside nested type expressions.
///
/// Inputs:
/// - `Option[Vector[Int]]`, `Result[Vector[Int], String]`, and a function type
///   that accepts a `Vector[Int]`.
///
/// Output:
/// - Test passes when every nested vector occurrence renders through the
///   SafeNative bridge type.
///
/// Transformation:
/// - Exercises recursive type-spec lowering so future backend-only generic
///   types cannot be fixed only at the top level while still breaking inside
///   common container and callback signatures.
#[test]
fn maps_nested_native_vector_types_to_bridge_specs() {
    assert_eq!(
        super::lower_type_to_spec("Option[Vector[Int]]").render(),
        "std_core_option:typer_option(std_native_collections_vector_safe_native:vector(integer()))"
    );
    assert_eq!(
        super::lower_type_to_spec("Result[Vector[Int], String]").render(),
        "std_core_result:result(std_native_collections_vector_safe_native:vector(integer()), binary())"
    );
    assert_eq!(
        super::lower_type_to_spec("(Vector[Int]) -> Int").render(),
        "fun((std_native_collections_vector_safe_native:vector(integer())) -> integer())"
    );
}

/// Verifies BEAM opaque primitive types map to native Erlang specs.
///
/// Inputs:
/// - Imported shorthand and fully qualified BEAM primitive type names.
///
/// Output:
/// - Test passes when byte buffers, sockets, ports, and timeouts render as
///   BEAM-native spec types.
///
/// Transformation:
/// - Prevents generated modules from inventing local opaque aliases for
///   `std.beam` values that are represented directly by Erlang binaries,
///   ports, and timeout values.
#[test]
fn maps_beam_opaque_types_to_native_erlang_specs() {
    assert_eq!(super::lower_type_to_spec("Bytes").render(), "binary()");
    assert_eq!(
        super::lower_type_to_spec("std.beam.Bytes.Bytes").render(),
        "binary()"
    );
    assert_eq!(super::lower_type_to_spec("Timeout").render(), "timeout()");
    assert_eq!(super::lower_type_to_spec("TcpSocket").render(), "port()");
    assert_eq!(
        super::lower_type_to_spec("std.beam.Tcp.TcpSocket").render(),
        "port()"
    );
    assert_eq!(super::lower_type_to_spec("Port").render(), "port()");
}

/// Verifies BEAM opaque types remain valid inside nested type expressions.
///
/// Inputs:
/// - `Result[Bytes, Error]`, `List[Port]`, and a function accepting a
///   `TcpSocket`.
///
/// Output:
/// - Test passes when every nested BEAM opaque occurrence renders through the
///   native Erlang type.
///
/// Transformation:
/// - Exercises recursive type-spec lowering so socket and port types stay
///   valid inside common standard-library containers and callbacks.
#[test]
fn maps_nested_beam_opaque_types_to_native_erlang_specs() {
    assert_eq!(
        super::lower_type_to_spec("Result[Bytes, Error]").render(),
        "std_core_result:result(binary(), error())"
    );
    assert_eq!(super::lower_type_to_spec("List[Port]").render(), "[port()]");
    assert_eq!(
        super::lower_type_to_spec("(TcpSocket) -> Timeout").render(),
        "fun((port()) -> timeout())"
    );
}
