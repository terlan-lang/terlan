//! Redaction helpers for mobile bridge inspection output.
#![allow(dead_code)]
//!
//! Inputs:
//! - JSON-shaped bridge inspection metadata and native shell configuration.
//!
//! Outputs:
//! - JSON with sensitive values replaced by a stable redaction marker.
//!
//! Transformation:
//! - Recursively walks inspection output and redacts values whose keys are
//!   known secret-bearing configuration names.

use serde_json::{Map, Value};

/// Stable placeholder used when bridge inspection output contains secrets.
pub(crate) const MOBILE_BRIDGE_REDACTION: &str = "[redacted]";

/// Redacts secrets from mobile bridge inspection output.
///
/// Inputs:
/// - `value`: JSON-shaped inspection output.
///
/// Output:
/// - A redacted copy of the input value.
///
/// Transformation:
/// - Recursively traverses arrays and objects. Object values whose key matches
///   a known sensitive name are replaced with `[redacted]`; all other values
///   preserve shape and type.
pub(crate) fn redact_mobile_bridge_inspection_output(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(redact_mobile_bridge_inspection_output)
                .collect(),
        ),
        Value::Object(object) => Value::Object(redact_mobile_bridge_inspection_object(object)),
        _ => value.clone(),
    }
}

/// Redacts one JSON object from mobile bridge inspection output.
fn redact_mobile_bridge_inspection_object(object: &Map<String, Value>) -> Map<String, Value> {
    object
        .iter()
        .map(|(key, value)| {
            let redacted = if is_sensitive_bridge_inspection_key(key) {
                Value::String(MOBILE_BRIDGE_REDACTION.to_string())
            } else {
                redact_mobile_bridge_inspection_output(value)
            };
            (key.clone(), redacted)
        })
        .collect()
}

/// Returns true when a bridge inspection key carries sensitive config.
fn is_sensitive_bridge_inspection_key(key: &str) -> bool {
    matches!(
        normalize_bridge_inspection_key(key).as_str(),
        "apikey"
            | "authtoken"
            | "authorization"
            | "clientsecret"
            | "cookie"
            | "password"
            | "privatekey"
            | "refreshtoken"
            | "secret"
            | "setcookie"
            | "token"
    )
}

/// Normalizes a key before sensitive-name matching.
fn normalize_bridge_inspection_key(key: &str) -> String {
    key.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
#[path = "mobile_bridge_inspection_test.rs"]
mod mobile_bridge_inspection_test;
