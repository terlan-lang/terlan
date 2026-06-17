//! JSON adapter operations for `std.data.Json`.
//!
//! This module is the first concrete Rust/SafeNative runtime slice for the
//! portable `std.data.Json` contract. It delegates parsing and rendering to
//! `serde_json`, while exposing only stable Terlan-facing shapes.

use serde_json::Value;

/// Parsed JSON value owned by the SafeNative adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct Json {
    value: Value,
}

impl Json {
    /// Builds a SafeNative JSON value from a `serde_json` value.
    ///
    /// Inputs:
    /// - `value`: backend JSON value produced by `serde_json`.
    ///
    /// Output:
    /// - A `Json` wrapper suitable for the portable `std.data.Json` API.
    ///
    /// Transformation:
    /// - Wraps the backend representation so callers do not depend on the
    ///   selected Rust JSON crate directly.
    pub fn from_serde(value: Value) -> Self {
        Self { value }
    }

    /// Returns the wrapped `serde_json` value by shared reference.
    ///
    /// Inputs:
    /// - `self`: SafeNative JSON wrapper.
    ///
    /// Output:
    /// - Shared reference to the backend JSON value.
    ///
    /// Transformation:
    /// - Exposes a read-only view for adapter internals without cloning.
    pub fn as_serde(&self) -> &Value {
        &self.value
    }
}

/// Portable JSON error returned by SafeNative JSON operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError {
    code: &'static str,
    message: String,
    offset: usize,
}

impl JsonError {
    /// Builds a portable JSON error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    /// - `offset`: byte offset when known, or `0` when unavailable.
    ///
    /// Output:
    /// - A `JsonError` with stable fields.
    ///
    /// Transformation:
    /// - Converts operation-specific failures into one portable shape.
    pub fn new(code: &'static str, message: impl Into<String>, offset: usize) -> Self {
        Self {
            code,
            message: message.into(),
            offset,
        }
    }

    /// Returns the stable machine-readable error code.
    ///
    /// Inputs:
    /// - `self`: JSON error value.
    ///
    /// Output:
    /// - Static error code string.
    ///
    /// Transformation:
    /// - Reads the code field without allocation or mutation.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the human-readable error message.
    ///
    /// Inputs:
    /// - `self`: JSON error value.
    ///
    /// Output:
    /// - Borrowed message text.
    ///
    /// Transformation:
    /// - Reads the message field without allocation or mutation.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the byte offset associated with the JSON error.
    ///
    /// Inputs:
    /// - `self`: JSON error value.
    ///
    /// Output:
    /// - Byte offset, or `0` when the backend did not provide a useful offset.
    ///
    /// Transformation:
    /// - Reads the offset field without allocation or mutation.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

/// Parses UTF-8 JSON text into a SafeNative JSON value.
///
/// Inputs:
/// - `text`: JSON source text.
///
/// Output:
/// - `Ok(Json)` when `serde_json` accepts the source.
/// - `Err(JsonError)` with stable code `json.parse` when parsing fails.
///
/// Transformation:
/// - Delegates JSON parsing to `serde_json` and converts backend diagnostics
///   into the portable Terlan JSON error shape.
pub fn parse(text: &str) -> Result<Json, JsonError> {
    serde_json::from_str::<Value>(text)
        .map(Json::from_serde)
        .map_err(|error| JsonError::new("json.parse", error.to_string(), 0))
}

/// Renders a SafeNative JSON value to compact JSON text.
///
/// Inputs:
/// - `json`: parsed JSON value.
///
/// Output:
/// - `Ok(String)` containing compact JSON when rendering succeeds.
/// - `Err(JsonError)` with stable code `json.stringify` if serialization fails.
///
/// Transformation:
/// - Delegates JSON rendering to `serde_json` and maps backend errors into the
///   portable Terlan JSON error shape.
pub fn stringify(json: &Json) -> Result<String, JsonError> {
    serde_json::to_string(json.as_serde())
        .map_err(|error| JsonError::new("json.stringify", error.to_string(), 0))
}

/// Reads an object member from a SafeNative JSON value.
///
/// Inputs:
/// - `json`: parsed JSON value expected to be an object.
/// - `key`: object member name.
///
/// Output:
/// - `Ok(Json)` containing the cloned member value.
/// - `Err(JsonError)` when the receiver is not an object or the key is absent.
///
/// Transformation:
/// - Performs a typed object lookup while preserving backend representation
///   opacity for Terlan source code.
pub fn get(json: &Json, key: &str) -> Result<Json, JsonError> {
    match json.as_serde() {
        Value::Object(object) => object
            .get(key)
            .cloned()
            .map(Json::from_serde)
            .ok_or_else(|| {
                JsonError::new(
                    "json.key_not_found",
                    format!("JSON object does not contain key `{key}`."),
                    0,
                )
            }),
        _ => Err(JsonError::new(
            "json.not_object",
            "JSON value is not an object.",
            0,
        )),
    }
}

/// Reads a JSON string as a Terlan string.
///
/// Inputs:
/// - `json`: parsed JSON value expected to be a string.
///
/// Output:
/// - `Ok(String)` when the value is a JSON string.
/// - `Err(JsonError)` when the value has another kind.
///
/// Transformation:
/// - Validates the JSON kind and copies the UTF-8 string into an owned value.
pub fn as_string(json: &Json) -> Result<String, JsonError> {
    json.as_serde()
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| JsonError::new("json.not_string", "JSON value is not a string.", 0))
}

/// Reads a JSON integer as a Terlan integer.
///
/// Inputs:
/// - `json`: parsed JSON value expected to be an integer.
///
/// Output:
/// - `Ok(i64)` when the value is a JSON integer representable by `i64`.
/// - `Err(JsonError)` when the value has another kind or range.
///
/// Transformation:
/// - Validates the JSON numeric shape before returning the integer.
pub fn as_int(json: &Json) -> Result<i64, JsonError> {
    json.as_serde()
        .as_i64()
        .ok_or_else(|| JsonError::new("json.not_int", "JSON value is not an integer.", 0))
}

/// Reads a JSON number as a Terlan float.
///
/// Inputs:
/// - `json`: parsed JSON value expected to be numeric.
///
/// Output:
/// - `Ok(f64)` when the value is a JSON number representable by `f64`.
/// - `Err(JsonError)` when the value has another kind.
///
/// Transformation:
/// - Validates the JSON numeric shape before returning the float.
pub fn as_float(json: &Json) -> Result<f64, JsonError> {
    json.as_serde()
        .as_f64()
        .ok_or_else(|| JsonError::new("json.not_float", "JSON value is not a number.", 0))
}

/// Reads a JSON boolean as a Terlan boolean.
///
/// Inputs:
/// - `json`: parsed JSON value expected to be a boolean.
///
/// Output:
/// - `Ok(bool)` when the value is a JSON boolean.
/// - `Err(JsonError)` when the value has another kind.
///
/// Transformation:
/// - Validates the JSON kind before returning the boolean.
pub fn as_bool(json: &Json) -> Result<bool, JsonError> {
    json.as_serde()
        .as_bool()
        .ok_or_else(|| JsonError::new("json.not_bool", "JSON value is not a boolean.", 0))
}

/// Returns whether a SafeNative JSON value is JSON null.
///
/// Inputs:
/// - `json`: parsed JSON value.
///
/// Output:
/// - `true` when the value is JSON null, otherwise `false`.
///
/// Transformation:
/// - Observes the backend JSON value kind without mutation or allocation.
pub fn is_null(json: &Json) -> bool {
    json.as_serde().is_null()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses a JSON fixture for adapter tests.
    ///
    /// Inputs:
    /// - `text`: JSON source expected to parse.
    ///
    /// Output:
    /// - Parsed `Json` value, or JSON null after a failing assertion.
    ///
    /// Transformation:
    /// - Converts a `Result` into a convenient test value without unwrap/expect.
    fn parsed_fixture(text: &str) -> Json {
        let result = parse(text);
        assert!(result.is_ok());
        result.unwrap_or_else(|_| Json::from_serde(Value::Null))
    }

    /// Validates JSON parsing and compact string rendering.
    ///
    /// Inputs:
    /// - Object JSON text with stable key ordering.
    ///
    /// Output:
    /// - Test passes when parsing succeeds and rendering returns compact JSON.
    ///
    /// Transformation:
    /// - Exercises the parse/stringify path over the `serde_json` backend.
    #[test]
    fn parse_and_stringify_round_trip_json_text() {
        let json = parsed_fixture(r#"{"name":"Ada","active":true}"#);
        assert_eq!(
            stringify(&json),
            Ok(String::from(r#"{"active":true,"name":"Ada"}"#))
        );
    }

    /// Validates stable parse error conversion.
    ///
    /// Inputs:
    /// - Invalid JSON text.
    ///
    /// Output:
    /// - Test passes when parsing returns the stable `json.parse` code.
    ///
    /// Transformation:
    /// - Converts a backend parser error into the portable JSON error shape.
    #[test]
    fn parse_error_uses_stable_error_code() {
        let result = parse("{");
        assert!(result.is_err());
        let error = result
            .err()
            .unwrap_or_else(|| JsonError::new("missing", "", 0));
        assert_eq!(error.code(), "json.parse");
        assert_eq!(error.offset(), 0);
    }

    /// Validates object lookup and typed accessors.
    ///
    /// Inputs:
    /// - Object JSON text with string, integer, float, boolean, and null fields.
    ///
    /// Output:
    /// - Test passes when each accessor returns the expected typed value.
    ///
    /// Transformation:
    /// - Exercises `get` plus all typed reader operations.
    #[test]
    fn object_lookup_supports_typed_accessors() {
        let json =
            parsed_fixture(r#"{"name":"Ada","count":3,"ratio":1.5,"active":true,"none":null}"#);
        let name = get(&json, "name").unwrap_or_else(|_| Json::from_serde(Value::Null));
        let count = get(&json, "count").unwrap_or_else(|_| Json::from_serde(Value::Null));
        let ratio = get(&json, "ratio").unwrap_or_else(|_| Json::from_serde(Value::Null));
        let active = get(&json, "active").unwrap_or_else(|_| Json::from_serde(Value::Null));
        let none = get(&json, "none").unwrap_or_else(|_| Json::from_serde(Value::Null));

        assert_eq!(as_string(&name), Ok(String::from("Ada")));
        assert_eq!(as_int(&count), Ok(3));
        assert_eq!(as_float(&ratio), Ok(1.5));
        assert_eq!(as_bool(&active), Ok(true));
        assert!(is_null(&none));
    }

    /// Validates object lookup failure conversion.
    ///
    /// Inputs:
    /// - Object JSON text and an absent key.
    ///
    /// Output:
    /// - Test passes when lookup returns the stable missing-key code.
    ///
    /// Transformation:
    /// - Converts a missing object member into a portable JSON error.
    #[test]
    fn missing_key_uses_stable_error_code() {
        let json = parsed_fixture(r#"{"name":"Ada"}"#);
        let error = get(&json, "missing")
            .err()
            .unwrap_or_else(|| JsonError::new("missing", "", 0));
        assert_eq!(error.code(), "json.key_not_found");
    }

    /// Validates typed accessor failure conversion.
    ///
    /// Inputs:
    /// - JSON string value read as an integer.
    ///
    /// Output:
    /// - Test passes when the accessor returns the stable wrong-kind code.
    ///
    /// Transformation:
    /// - Converts a JSON kind mismatch into a portable JSON error.
    #[test]
    fn wrong_kind_accessor_uses_stable_error_code() {
        let json = parsed_fixture(r#""Ada""#);
        let error = as_int(&json)
            .err()
            .unwrap_or_else(|| JsonError::new("missing", "", 0));
        assert_eq!(error.code(), "json.not_int");
    }
}
