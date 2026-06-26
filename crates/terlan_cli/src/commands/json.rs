/// Escapes a string value for command JSON output.
///
/// Inputs:
/// - `value`: raw text.
///
/// Output:
/// - JSON string literal including quotes.
///
/// Transformation:
/// - Delegates string encoding to `serde_json` so command output follows the
///   same escaping rules as every other generated JSON artifact.
pub(crate) fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("JSON string serialization should not fail")
}

#[cfg(test)]
#[path = "json_test.rs"]
mod json_test;
