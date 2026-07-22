use serde_json::json;

use super::*;

/// Verifies bridge inspection redaction preserves safe fields and redacts secrets.
///
/// Inputs:
/// - One JSON object with route metadata and several sensitive config keys.
///
/// Output:
/// - Same object shape with sensitive values replaced by `[redacted]`.
///
/// Transformation:
/// - Exercises the redaction contract without depending on a public
///   inspection command.
#[test]
fn mobile_bridge_inspection_redacts_top_level_sensitive_keys() {
    let output = json!({
        "bridge": "ShellBridge",
        "api_key": "abc123",
        "Authorization": "Bearer secret",
        "password": "open-sesame",
        "route": "/camera"
    });

    let redacted = redact_mobile_bridge_inspection_output(&output);

    assert_eq!(redacted["bridge"], "ShellBridge");
    assert_eq!(redacted["route"], "/camera");
    assert_eq!(redacted["api_key"], MOBILE_BRIDGE_REDACTION);
    assert_eq!(redacted["Authorization"], MOBILE_BRIDGE_REDACTION);
    assert_eq!(redacted["password"], MOBILE_BRIDGE_REDACTION);
}

/// Verifies bridge inspection redaction recurses through objects and arrays.
///
/// Inputs:
/// - Nested native shell configuration with secrets inside an array.
///
/// Output:
/// - Redacted nested values while preserving non-sensitive metadata.
///
/// Transformation:
/// - Prevents native bridge inspection from leaking secrets through nested
///   config sections or repeated service entries.
#[test]
fn mobile_bridge_inspection_redacts_nested_objects_and_arrays() {
    let output = json!({
        "services": [
            {
                "name": "camera",
                "config": {
                    "client-secret": "camera-secret",
                    "timeout_ms": 5000
                }
            },
            {
                "name": "push",
                "config": {
                    "refreshToken": "refresh-secret",
                    "enabled": true
                }
            }
        ]
    });

    let redacted = redact_mobile_bridge_inspection_output(&output);

    assert_eq!(redacted["services"][0]["name"], "camera");
    assert_eq!(redacted["services"][0]["config"]["timeout_ms"], 5000);
    assert_eq!(
        redacted["services"][0]["config"]["client-secret"],
        MOBILE_BRIDGE_REDACTION
    );
    assert_eq!(redacted["services"][1]["config"]["enabled"], true);
    assert_eq!(
        redacted["services"][1]["config"]["refreshToken"],
        MOBILE_BRIDGE_REDACTION
    );
}

/// Verifies bridge inspection redaction uses exact normalized key matching.
///
/// Inputs:
/// - Safe lookalike keys whose names contain sensitive words as substrings.
///
/// Output:
/// - Safe values are preserved.
///
/// Transformation:
/// - Avoids destroying useful debug data such as capability names or route
///   labels while still redacting exact secret-bearing keys.
#[test]
fn mobile_bridge_inspection_preserves_safe_lookalike_keys() {
    let output = json!({
        "tokenizer": "enabled",
        "secretary": "routing",
        "passwordPolicy": "strict",
        "capability": "push_notifications"
    });

    let redacted = redact_mobile_bridge_inspection_output(&output);

    assert_eq!(redacted, output);
}
