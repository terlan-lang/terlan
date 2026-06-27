//! Rust-native JSON adapter operations for `std.data.Json`.
//!
//! This module owns the concrete Rust JSON behavior for the portable
//! `std.data.Json` contract. It delegates parsing and rendering to
//! `serde_json`, while exposing only stable Terlan-facing shapes to the
//! SafeNative bridge.

use serde_json::{Map, Number, Value};

/// Parsed JSON value owned by the Rust-native JSON adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct Json {
    value: Value,
}

impl Json {
    /// Builds a native JSON value from a `serde_json` value.
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
    /// - `self`: native JSON wrapper.
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

/// Portable JSON error returned by native JSON operations.
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

/// Parses UTF-8 JSON text into a native JSON value.
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

/// Creates a JSON null value.
///
/// Inputs:
/// - No value input.
///
/// Output:
/// - `Json` containing `serde_json::Value::Null`.
///
/// Transformation:
/// - Wraps the backend JSON null representation in the portable adapter type.
pub fn null() -> Json {
    Json::from_serde(Value::Null)
}

/// Creates a JSON boolean value.
///
/// Inputs:
/// - `value`: boolean to represent as JSON.
///
/// Output:
/// - `Json` containing a JSON boolean.
///
/// Transformation:
/// - Converts the primitive boolean into the backend JSON value shape.
pub fn r#bool(value: bool) -> Json {
    Json::from_serde(Value::Bool(value))
}

/// Creates a JSON integer value.
///
/// Inputs:
/// - `value`: integer to represent as JSON.
///
/// Output:
/// - `Json` containing a JSON number.
///
/// Transformation:
/// - Converts the primitive integer into the backend JSON number shape.
pub fn int(value: i64) -> Json {
    Json::from_serde(Value::Number(Number::from(value)))
}

/// Creates a JSON floating-point value.
///
/// Inputs:
/// - `value`: floating-point number to represent as JSON.
///
/// Output:
/// - `Ok(Json)` when the value is finite.
/// - `Err(JsonError)` with code `json.invalid_float` for NaN or infinity.
///
/// Transformation:
/// - Validates JSON numeric compatibility before constructing the backend
///   number representation.
pub fn float(value: f64) -> Result<Json, JsonError> {
    Number::from_f64(value)
        .map(Value::Number)
        .map(Json::from_serde)
        .ok_or_else(|| JsonError::new("json.invalid_float", "JSON numbers must be finite.", 0))
}

/// Creates a JSON string value.
///
/// Inputs:
/// - `value`: UTF-8 text to represent as JSON.
///
/// Output:
/// - `Json` containing a JSON string.
///
/// Transformation:
/// - Copies the borrowed string into the backend JSON string representation.
pub fn string(value: &str) -> Json {
    Json::from_serde(Value::String(value.to_owned()))
}

/// Creates an empty JSON array.
///
/// Inputs:
/// - No value input.
///
/// Output:
/// - `Json` containing an empty JSON array.
///
/// Transformation:
/// - Allocates the backend JSON array representation.
pub fn array() -> Json {
    Json::from_serde(Value::Array(Vec::new()))
}

/// Creates an empty JSON object.
///
/// Inputs:
/// - No value input.
///
/// Output:
/// - `Json` containing an empty JSON object.
///
/// Transformation:
/// - Allocates the backend JSON object representation.
pub fn object() -> Json {
    Json::from_serde(Value::Object(Map::new()))
}

/// Appends a value to a JSON array.
///
/// Inputs:
/// - `json`: mutable JSON value expected to be an array.
/// - `value`: JSON value to append.
///
/// Output:
/// - `Ok(())` when the receiver is an array.
/// - `Err(JsonError)` with code `json.not_array` otherwise.
///
/// Transformation:
/// - Mutates the backend JSON array in place while keeping the receiver
///   wrapped as the portable adapter type.
pub fn push(json: &mut Json, value: Json) -> Result<(), JsonError> {
    match &mut json.value {
        Value::Array(values) => {
            values.push(value.value);
            Ok(())
        }
        _ => Err(JsonError::new(
            "json.not_array",
            "JSON value is not an array.",
            0,
        )),
    }
}

/// Inserts or replaces a value in a JSON object.
///
/// Inputs:
/// - `json`: mutable JSON value expected to be an object.
/// - `key`: object member key.
/// - `value`: JSON value to store.
///
/// Output:
/// - `Ok(())` when the receiver is an object.
/// - `Err(JsonError)` with code `json.not_object` otherwise.
///
/// Transformation:
/// - Mutates the backend JSON object in place while keeping the receiver
///   wrapped as the portable adapter type.
pub fn put(json: &mut Json, key: &str, value: Json) -> Result<(), JsonError> {
    match &mut json.value {
        Value::Object(object) => {
            object.insert(key.to_owned(), value.value);
            Ok(())
        }
        _ => Err(JsonError::new(
            "json.not_object",
            "JSON value is not an object.",
            0,
        )),
    }
}

/// Renders a native JSON value to compact JSON text.
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

/// Reads an object member from a native JSON value.
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

/// Returns the length of a JSON array.
///
/// Inputs:
/// - `json`: parsed JSON value expected to be an array.
///
/// Output:
/// - `Ok(i64)` containing the array length.
/// - `Err(JsonError)` when the receiver is not an array or the length cannot
///   be represented as a Terlan `Int`.
///
/// Transformation:
/// - Observes the backend array length and converts it to the portable integer
///   shape used by Terlan.
pub fn length(json: &Json) -> Result<i64, JsonError> {
    match json.as_serde() {
        Value::Array(values) => i64::try_from(values.len()).map_err(|_| {
            JsonError::new("json.length_overflow", "JSON array length exceeds Int.", 0)
        }),
        _ => Err(JsonError::new(
            "json.not_array",
            "JSON value is not an array.",
            0,
        )),
    }
}

/// Reads a JSON array element by index.
///
/// Inputs:
/// - `json`: parsed JSON value expected to be an array.
/// - `index`: zero-based array index.
///
/// Output:
/// - `Ok(Json)` containing the cloned array element.
/// - `Err(JsonError)` when the receiver is not an array, the index is
///   negative, or the index is outside the array bounds.
///
/// Transformation:
/// - Validates the receiver and index before cloning the selected backend JSON
///   value into the portable adapter wrapper.
pub fn at(json: &Json, index: i64) -> Result<Json, JsonError> {
    let index = usize::try_from(index).map_err(|_| {
        JsonError::new(
            "json.index_out_of_bounds",
            "JSON array index must be non-negative.",
            0,
        )
    })?;

    match json.as_serde() {
        Value::Array(values) => values
            .get(index)
            .cloned()
            .map(Json::from_serde)
            .ok_or_else(|| {
                JsonError::new(
                    "json.index_out_of_bounds",
                    format!("JSON array does not contain index `{index}`."),
                    0,
                )
            }),
        _ => Err(JsonError::new(
            "json.not_array",
            "JSON value is not an array.",
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

/// Returns whether a native JSON value is JSON null.
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
#[path = "json_test.rs"]
mod json_test;
