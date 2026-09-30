use serde_json::Value;

use super::{Json, JsonError};

/// Returns the length of a JSON array.
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

/// Reads a JSON array element by zero-based index.
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

/// Reads a JSON string as an owned Terlan string.
pub fn as_string(json: &Json) -> Result<String, JsonError> {
    json.as_serde()
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| JsonError::new("json.not_string", "JSON value is not a string.", 0))
}

/// Reads a JSON integer representable by `i64`.
pub fn as_int(json: &Json) -> Result<i64, JsonError> {
    json.as_serde()
        .as_i64()
        .ok_or_else(|| JsonError::new("json.not_int", "JSON value is not an integer.", 0))
}

/// Reads a JSON number representable by `f64`.
pub fn as_float(json: &Json) -> Result<f64, JsonError> {
    json.as_serde()
        .as_f64()
        .ok_or_else(|| JsonError::new("json.not_float", "JSON value is not a number.", 0))
}

/// Reads a JSON boolean.
pub fn as_bool(json: &Json) -> Result<bool, JsonError> {
    json.as_serde()
        .as_bool()
        .ok_or_else(|| JsonError::new("json.not_bool", "JSON value is not a boolean.", 0))
}

/// Returns whether a native JSON value is null.
pub fn is_null(json: &Json) -> bool {
    json.as_serde().is_null()
}
