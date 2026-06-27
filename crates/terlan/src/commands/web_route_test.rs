use super::*;

/// Verifies ordered route capture extraction.
///
/// Inputs:
/// - Route patterns using colon params, typed brace params, slash wildcard,
///   and canonical fallback.
///
/// Output:
/// - Test passes when route capture names match serve-time capture behavior.
///
/// Transformation:
/// - Exercises the shared route contract used by browser manifest extraction
///   and `terlc serve` handler arity validation.
#[test]
fn route_param_names_extracts_ordered_captures() {
    assert_eq!(
        route_param_names("/users/:id/posts/{slug:String}").expect("valid route"),
        vec!["id".to_string(), "slug".to_string()]
    );
    assert_eq!(
        route_param_names("/assets/*").expect("wildcard route"),
        vec!["*".to_string()]
    );
    assert_eq!(
        route_param_names("*").expect("fallback route"),
        Vec::<String>::new()
    );
}

/// Verifies ordered route capture type extraction.
///
/// Inputs:
/// - Route patterns using colon params, typed brace params, and slash wildcard.
///
/// Output:
/// - Test passes when implicit captures default to `String` and typed captures
///   preserve their declared type.
///
/// Transformation:
/// - Locks the type metadata consumed by route handler validation and BEAM
///   route argument decoding.
#[test]
fn route_param_types_extracts_defaults_and_typed_captures() {
    assert_eq!(
        route_param_types("/users/:id/posts/{count:Int}/files/*").expect("valid route"),
        vec![
            ("id".to_string(), "String".to_string()),
            ("count".to_string(), "Int".to_string()),
            ("*".to_string(), "String".to_string()),
        ]
    );
}

/// Verifies unsupported typed route params are rejected.
///
/// Inputs:
/// - Route pattern with `{id:UserId}`.
///
/// Output:
/// - Test passes when validation rejects the unsupported capture type.
///
/// Transformation:
/// - Locks the 0.0.5 route-param boundary to the types the local server can
///   actually decode before invoking BEAM handlers.
#[test]
fn validate_route_pattern_rejects_unsupported_route_param_type() {
    let error =
        validate_route_pattern("/users/{id:UserId}").expect_err("custom route type should fail");

    assert!(error.contains("unsupported route parameter type `UserId`"));
}

/// Verifies route capture names must be lower-case handler bindings.
///
/// Inputs:
/// - Route patterns using uppercase and underscore capture names.
///
/// Output:
/// - Stable route validation diagnostics.
///
/// Transformation:
/// - Locks route capture names to the same source shape used by handler
///   parameter bindings, while leaving wildcard capture handling separate.
#[test]
fn validate_route_pattern_rejects_non_binding_capture_names() {
    let uppercase =
        validate_route_pattern("/users/:Id").expect_err("uppercase capture should fail");
    let underscore =
        validate_route_pattern("/users/{_id:Int}").expect_err("underscore capture should fail");

    assert!(uppercase.contains("invalid route parameter `:Id`"));
    assert!(underscore.contains("invalid typed route parameter `{_id:Int}`"));
}
