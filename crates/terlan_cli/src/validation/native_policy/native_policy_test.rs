use super::*;

/// Verifies compiler-native annotations are treated as native usage.
///
/// Inputs:
/// - Source text with one `@compiler.native` operation.
///
/// Output:
/// - Test assertions over source detection and policy validation.
///
/// Transformation:
/// - Runs the early native-policy scanner over the annotation form used by
///   Rust-backed std modules.
#[test]
fn compiler_native_annotation_requires_native_policy() {
    let source = r#"module std.data.Json.

@compiler.native {std.data.json.parse}
pub parse(text: String): Json ->
    native.
"#;

    assert!(source_uses_native(source));
    assert!(validate_native_policy(source, NativePolicy::Pure).is_err());
    validate_native_policy(source, NativePolicy::SafeNativeOptional)
        .expect("safe native policy should allow compiler-native annotation");
}
