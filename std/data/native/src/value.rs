//! Maintained parser, encoder, and JSON value operations.

use super::{Json, JsonError};
use serde_json::{Map, Number, Value};

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

/// Appends every element from one JSON array to another JSON array.
pub fn extend(json: &mut Json, values: Json) -> Result<(), JsonError> {
    let Value::Array(source) = values.value else {
        return Err(JsonError::new(
            "json.not_array",
            "JSON extension value is not an array.",
            0,
        ));
    };
    match &mut json.value {
        Value::Array(target) => {
            target.extend(source);
            Ok(())
        }
        _ => Err(JsonError::new(
            "json.not_array",
            "JSON value is not an array.",
            0,
        )),
    }
}

/// Replaces one existing JSON array element.
pub fn set(json: &mut Json, index: i64, value: Json) -> Result<(), JsonError> {
    let index = usize::try_from(index).map_err(|_| {
        JsonError::new(
            "json.index_out_of_bounds",
            "JSON array index must be non-negative.",
            0,
        )
    })?;
    match &mut json.value {
        Value::Array(values) => {
            let slot = values.get_mut(index).ok_or_else(|| {
                JsonError::new(
                    "json.index_out_of_bounds",
                    format!("JSON array does not contain index `{index}`."),
                    0,
                )
            })?;
            *slot = value.value;
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

/// Removes one existing member from a JSON object.
///
/// Inputs:
/// - `json`: mutable JSON object.
/// - `key`: member name to remove.
///
/// Output:
/// - `Ok(())` after removal.
/// - `Err(JsonError)` for non-object values or absent keys.
///
/// Transformation:
/// - Mutates only the selected object and returns stable typed diagnostics.
pub fn remove(json: &mut Json, key: &str) -> Result<(), JsonError> {
    let Some(object) = json.value.as_object_mut() else {
        return Err(JsonError::new(
            "json.not_object",
            "JSON value is not an object.",
            0,
        ));
    };
    if object.remove(key).is_none() {
        return Err(JsonError::new(
            "json.missing_key",
            format!("JSON object has no member `{key}`"),
            0,
        ));
    }
    Ok(())
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

/// Renders a validated JSON value with the maintained compact Display implementation.
pub fn to_string(json: &Json) -> String {
    json.as_serde().to_string()
}

/// Renders one JSON value with stable two-space indentation.
pub fn stringify_pretty(json: &Json) -> Result<String, JsonError> {
    serde_json::to_string_pretty(json.as_serde())
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

/// Returns the stable member-name inventory of a JSON object.
///
/// Inputs:
/// - `json`: parsed JSON value expected to be an object.
///
/// Output:
/// - `Ok(Vec<String>)` containing bytewise sorted member names.
/// - `Err(JsonError)` when the receiver is not an object.
///
/// Transformation:
/// - Copies the backend object keys into an owned portable list and sorts it,
///   keeping the backend map representation opaque to Terlan source.
pub fn keys(json: &Json) -> Result<Vec<String>, JsonError> {
    match json.as_serde() {
        Value::Object(object) => {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            Ok(keys)
        }
        _ => Err(JsonError::new(
            "json.not_object",
            "JSON value is not an object.",
            0,
        )),
    }
}

/// Returns the member count of one JSON object without copying its keys.
pub fn object_length(json: &Json) -> Result<i64, JsonError> {
    match json.as_serde() {
        Value::Object(object) => i64::try_from(object.len()).map_err(|_| {
            JsonError::new(
                "json.length_overflow",
                "JSON object length exceeds the portable integer range.",
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
