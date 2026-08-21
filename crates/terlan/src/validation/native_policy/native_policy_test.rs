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
    validate_native_policy(source, NativePolicy::NativeBoundaryOptional)
        .expect("native-boundary policy should allow compiler-native annotation");
}

/// Verifies VM target native-boundary declarations are treated as native usage.
///
/// Inputs:
/// - Source text with a `target vm with native_boundary` declaration.
///
/// Output:
/// - Test assertions over source detection and pure-policy rejection.
///
/// Transformation:
/// - Runs the early native-policy scanner over the VM-owned target marker so
///   the removed Vm target spelling cannot remain the canonical trigger.
#[test]
fn vm_target_native_boundary_declaration_requires_native_policy() {
    let source = r#"module native_meta.

target vm with native_boundary.
"#;

    assert!(source_uses_native(source));
    assert!(validate_native_policy(source, NativePolicy::Pure).is_err());
    validate_native_policy(source, NativePolicy::NativeBoundaryOptional)
        .expect("native-boundary policy should allow VM native-boundary target marker");
}

/// Verifies new native-boundary CLI spellings are canonical parser inputs.
///
/// Inputs:
/// - Canonical native-boundary policy values.
///
/// Output:
/// - Test passes when every supported spelling maps to the expected policy.
///
/// Transformation:
/// - Locks the 0.0.7 public CLI spelling.
#[test]
fn parses_native_boundary_policy_values() {
    assert_eq!(NativePolicy::from_cli("pure"), Some(NativePolicy::Pure));
    assert_eq!(
        NativePolicy::from_cli("native_boundary_optional"),
        Some(NativePolicy::NativeBoundaryOptional)
    );
    assert_eq!(
        NativePolicy::from_cli("native_boundary_required"),
        Some(NativePolicy::NativeBoundaryRequired)
    );
    assert_eq!(NativePolicy::from_cli("native"), None);
}

/// Verifies policy diagnostics name the native-boundary spellings.
///
/// Inputs:
/// - Native-using source validated under pure policy.
///
/// Output:
/// - Test passes when the diagnostic names canonical 0.0.7 policy values.
///
/// Transformation:
/// - Keeps user-facing errors aligned with native-boundary terminology.
#[test]
fn pure_policy_error_names_native_boundary_values() {
    let error =
        validate_native_policy("@compiler.native {std.data.json.parse}", NativePolicy::Pure)
            .expect_err("native usage should fail under pure policy");
    assert!(error.contains("native_boundary_optional"), "{error}");
    assert!(error.contains("native_boundary_required"), "{error}");
}

/// Verifies unsafe-native rejection is scoped to declarations.
///
/// Inputs:
/// - Three supported leading declaration spellings.
/// - An identifier, comment, and string containing the same words.
///
/// Output:
/// - Declaration spellings are detected and ordinary source text is ignored.
///
/// Transformation:
/// - Exercises the lightweight pre-parser scan without allowing substring
///   false positives to reject valid Terlan validation programs.
#[test]
fn unsafe_native_detection_is_declaration_anchored() {
    for declaration in [
        "unsafe_native module Adapter {",
        "  unsafe native module Adapter {",
        "\tnative unsafe module Adapter {",
    ] {
        assert!(source_contains_unsafe_native(declaration), "{declaration}");
    }

    for ordinary_source in [
        "pub unsafe_native_identity_is_rejected(): Bool -> true.",
        "// unsafe native module Adapter {",
        "let message = \"native unsafe module Adapter\";",
    ] {
        assert!(
            !source_contains_unsafe_native(ordinary_source),
            "{ordinary_source}"
        );
    }
}
