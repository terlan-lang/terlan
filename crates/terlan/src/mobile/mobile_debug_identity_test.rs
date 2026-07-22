use super::*;

/// Builds a representative source identity.
fn identity() -> MobileSourceIdentity {
    MobileSourceIdentity {
        module_path: "app.Mobile".to_string(),
        function_name: "openRoute".to_string(),
        span: MobileSourceSpan {
            file: "src/app/Mobile.terl".to_string(),
            start_line: 12,
            start_column: 5,
            end_line: 14,
            end_column: 6,
        },
    }
}

/// Verifies source identity metadata includes a stable debug key.
///
/// Inputs:
/// - One module/function/source span identity.
///
/// Output:
/// - Metadata containing the original coordinates and compact debug key.
///
/// Transformation:
/// - Exercises source-to-mobile metadata conversion without route or bridge
///   declarations.
#[test]
fn mobile_debug_identity_generates_metadata() {
    let metadata = generate_mobile_debug_identity_metadata(&identity()).expect("metadata");

    assert_eq!(metadata.module_path, "app.Mobile");
    assert_eq!(metadata.function_name, "openRoute");
    assert_eq!(metadata.file, "src/app/Mobile.terl");
    assert_eq!(
        metadata.debug_key,
        "app.Mobile.openRoute@src/app/Mobile.terl:12:5-14:6"
    );
}

/// Verifies incomplete source identity is rejected.
///
/// Inputs:
/// - Empty module/function/file names and zero coordinates.
///
/// Output:
/// - Stable diagnostics for every invalid identity field.
///
/// Transformation:
/// - Prevents native replies from pointing at unusable source identities.
#[test]
fn mobile_debug_identity_rejects_incomplete_identity() {
    let invalid = MobileSourceIdentity {
        module_path: String::new(),
        function_name: String::new(),
        span: MobileSourceSpan {
            file: String::new(),
            start_line: 0,
            start_column: 0,
            end_line: 0,
            end_column: 0,
        },
    };

    let diagnostics =
        generate_mobile_debug_identity_metadata(&invalid).expect_err("invalid identity");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"mobile_debug_identity_empty_module"));
    assert!(codes.contains(&"mobile_debug_identity_empty_function"));
    assert!(codes.contains(&"mobile_debug_identity_empty_file"));
    assert!(codes.contains(&"mobile_debug_identity_zero_coordinate"));
}

/// Verifies inverted spans are rejected.
///
/// Inputs:
/// - One source identity whose end coordinate precedes its start coordinate.
///
/// Output:
/// - Stable inverted-span diagnostic.
///
/// Transformation:
/// - Keeps generated debug identity source ranges coherent.
#[test]
fn mobile_debug_identity_rejects_inverted_span() {
    let mut invalid = identity();
    invalid.span.end_line = 11;

    let diagnostics = generate_mobile_debug_identity_metadata(&invalid).expect_err("inverted span");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mobile_debug_identity_inverted_span"));
}
