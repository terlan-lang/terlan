//! Rust-native JSON adapter operations for `std.data.Json`.
//!
//! This module owns the concrete Rust JSON behavior for the portable
//! `std.data.Json` contract. It delegates parsing and rendering to
//! `serde_json`, while exposing only stable Terlan-facing shapes to the
//! NativeBoundary bridge.

use serde_json::{Map, Number, Value};

/// Parsed JSON value owned by the Rust-native JSON adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct Json {
    value: Value,
}

/// Owned string-field projection for one JSON array element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringFieldRow {
    /// Whether the source array element was an object.
    pub object: bool,
    /// Requested string fields in caller order.
    pub values: Vec<Option<String>>,
}

/// Owned projection for one object in an array with one nested object array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NestedStringFieldRow {
    /// Whether the parent array element was an object.
    pub object: bool,
    /// Requested parent string fields in caller order.
    pub values: Vec<Option<String>>,
    /// Whether the requested child member existed as an array.
    pub child_array: bool,
    /// Requested fields for every child-array element.
    pub children: Vec<StringFieldRow>,
}

/// Strict owned projection of required fields from one JSON object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequiredFieldProjection {
    /// Required string values in caller order.
    pub strings: Vec<String>,
    /// Required integer values in caller order.
    pub ints: Vec<i64>,
    /// Required array lengths in caller order.
    pub array_lengths: Vec<i64>,
}

/// Strict owned scalar projection for one JSON object-array element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequiredFieldRow {
    /// Required string values in caller order.
    pub strings: Vec<String>,
    /// Required integer values in caller order.
    pub ints: Vec<i64>,
    /// Required boolean values in caller order.
    pub bools: Vec<bool>,
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

/// Projects selected optional string fields from one JSON object.
pub fn string_fields(json: &Json, fields: &[&str]) -> Result<Vec<Option<String>>, JsonError> {
    let Value::Object(object) = json.as_serde() else {
        return Err(JsonError::new(
            "json.not_object",
            "JSON value is not an object.",
            0,
        ));
    };
    Ok(fields
        .iter()
        .map(|field| {
            object
                .get(*field)
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect())
}

/// Projects required object fields into continuation-safe owned values.
pub fn required_fields(
    json: &Json,
    string_fields: &[&str],
    int_fields: &[&str],
    array_fields: &[&str],
) -> Result<RequiredFieldProjection, JsonError> {
    let Value::Object(object) = json.as_serde() else {
        return Err(JsonError::new(
            "json.not_object",
            "JSON value is not an object.",
            0,
        ));
    };
    let strings = string_fields
        .iter()
        .map(|field| {
            object
                .get(*field)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| required_field_error(field, "string"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let ints = int_fields
        .iter()
        .map(|field| {
            object
                .get(*field)
                .and_then(Value::as_i64)
                .ok_or_else(|| required_field_error(field, "integer"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let array_lengths = array_fields
        .iter()
        .map(|field| {
            let length = object
                .get(*field)
                .and_then(Value::as_array)
                .map(Vec::len)
                .ok_or_else(|| required_field_error(field, "array"))?;
            i64::try_from(length).map_err(|_| {
                JsonError::new(
                    "json.length_overflow",
                    format!("JSON array field `{field}` exceeds the portable integer range."),
                    0,
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RequiredFieldProjection {
        strings,
        ints,
        array_lengths,
    })
}

/// Projects required scalar fields from every JSON object-array element.
///
/// The complete source array is traversed in one native call for moderate
/// inputs. Large report consumers should use `required_field_rows_page` so the
/// public managed-value conversion budget remains bounded.
pub fn required_field_rows(
    json: &Json,
    string_fields: &[&str],
    int_fields: &[&str],
    bool_fields: &[&str],
) -> Result<Vec<RequiredFieldRow>, JsonError> {
    let Value::Array(rows) = json.as_serde() else {
        return Err(JsonError::new(
            "json.not_array",
            "JSON value is not an array.",
            0,
        ));
    };
    project_required_field_rows(rows, 0, string_fields, int_fields, bool_fields)
}

/// Projects one bounded page of required scalar object-array fields.
pub fn required_field_rows_page(
    json: &Json,
    start: i64,
    maximum: i64,
    string_fields: &[&str],
    int_fields: &[&str],
    bool_fields: &[&str],
) -> Result<Vec<RequiredFieldRow>, JsonError> {
    const MAXIMUM_PAGE_ROWS: usize = 256;
    let start = usize::try_from(start).map_err(|_| {
        JsonError::new(
            "json.invalid_page",
            "JSON projection page start must be non-negative.",
            0,
        )
    })?;
    let maximum = usize::try_from(maximum).map_err(|_| {
        JsonError::new(
            "json.invalid_page",
            "JSON projection page maximum must be positive.",
            0,
        )
    })?;
    if maximum == 0 || maximum > MAXIMUM_PAGE_ROWS {
        return Err(JsonError::new(
            "json.invalid_page",
            format!("JSON projection page maximum must be between 1 and {MAXIMUM_PAGE_ROWS}."),
            0,
        ));
    }
    let Value::Array(rows) = json.as_serde() else {
        return Err(JsonError::new(
            "json.not_array",
            "JSON value is not an array.",
            0,
        ));
    };
    let end = start.saturating_add(maximum).min(rows.len());
    let page = rows.get(start..end).unwrap_or_default();
    project_required_field_rows(page, start, string_fields, int_fields, bool_fields)
}

fn project_required_field_rows(
    rows: &[Value],
    start: usize,
    string_fields: &[&str],
    int_fields: &[&str],
    bool_fields: &[&str],
) -> Result<Vec<RequiredFieldRow>, JsonError> {
    rows.iter()
        .enumerate()
        .map(|(page_index, row)| {
            let index = start.saturating_add(page_index);
            let Value::Object(object) = row else {
                return Err(JsonError::new(
                    "json.not_object",
                    format!("JSON array row {index} is not an object."),
                    index,
                ));
            };
            let strings = string_fields
                .iter()
                .map(|field| {
                    object
                        .get(*field)
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .ok_or_else(|| required_row_field_error(index, field, "string"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let ints = int_fields
                .iter()
                .map(|field| {
                    object
                        .get(*field)
                        .and_then(Value::as_i64)
                        .ok_or_else(|| required_row_field_error(index, field, "integer"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let bools = bool_fields
                .iter()
                .map(|field| {
                    object
                        .get(*field)
                        .and_then(Value::as_bool)
                        .ok_or_else(|| required_row_field_error(index, field, "boolean"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(RequiredFieldRow {
                strings,
                ints,
                bools,
            })
        })
        .collect()
}

fn required_field_error(field: &str, expected: &str) -> JsonError {
    let code = match expected {
        "string" => "json.not_string",
        "integer" => "json.not_int",
        "array" => "json.not_array",
        _ => "json.parse",
    };
    JsonError::new(
        code,
        format!("JSON field `{field}` is missing or is not a {expected}."),
        0,
    )
}

fn required_row_field_error(index: usize, field: &str, expected: &str) -> JsonError {
    let code = match expected {
        "string" => "json.not_string",
        "integer" => "json.not_int",
        "boolean" => "json.not_bool",
        _ => "json.parse",
    };
    JsonError::new(
        code,
        format!("JSON array row {index} field `{field}` is missing or is not a {expected}."),
        index,
    )
}

/// Projects selected optional string fields from a JSON object array.
///
/// The result preserves source-row and requested-field order. Missing and
/// non-string fields become `None`; a non-object row is retained with
/// `object == false` so callers can produce an index-specific diagnostic.
pub fn string_field_rows(json: &Json, fields: &[&str]) -> Result<Vec<StringFieldRow>, JsonError> {
    let Value::Array(rows) = json.as_serde() else {
        return Err(JsonError::new(
            "json.not_array",
            "JSON value is not an array.",
            0,
        ));
    };
    Ok(rows
        .iter()
        .map(|row| match row {
            Value::Object(object) => StringFieldRow {
                object: true,
                values: fields
                    .iter()
                    .map(|field| {
                        object
                            .get(*field)
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .collect(),
            },
            _ => StringFieldRow {
                object: false,
                values: fields.iter().map(|_| None).collect(),
            },
        })
        .collect())
}

/// Projects parent and nested-child string fields in one native traversal.
pub fn nested_string_field_rows(
    json: &Json,
    parent_fields: &[&str],
    child_array_field: &str,
    child_fields: &[&str],
) -> Result<Vec<NestedStringFieldRow>, JsonError> {
    let Value::Array(rows) = json.as_serde() else {
        return Err(JsonError::new(
            "json.not_array",
            "JSON value is not an array.",
            0,
        ));
    };
    Ok(project_nested_string_rows(
        rows,
        parent_fields,
        child_array_field,
        child_fields,
    ))
}

/// Projects one bounded page of parent and nested-child string fields.
pub fn nested_string_field_rows_page(
    json: &Json,
    start: i64,
    maximum: i64,
    parent_fields: &[&str],
    child_array_field: &str,
    child_fields: &[&str],
) -> Result<Vec<NestedStringFieldRow>, JsonError> {
    const MAXIMUM_PAGE_ROWS: usize = 256;
    let start = usize::try_from(start).map_err(|_| {
        JsonError::new(
            "json.invalid_page",
            "JSON projection page start must be non-negative.",
            0,
        )
    })?;
    let maximum = usize::try_from(maximum).map_err(|_| {
        JsonError::new(
            "json.invalid_page",
            "JSON projection page maximum must be positive.",
            0,
        )
    })?;
    if maximum == 0 || maximum > MAXIMUM_PAGE_ROWS {
        return Err(JsonError::new(
            "json.invalid_page",
            format!("JSON projection page maximum must be between 1 and {MAXIMUM_PAGE_ROWS}."),
            0,
        ));
    }
    let Value::Array(rows) = json.as_serde() else {
        return Err(JsonError::new(
            "json.not_array",
            "JSON value is not an array.",
            0,
        ));
    };
    let end = start.saturating_add(maximum).min(rows.len());
    let page = rows.get(start..end).unwrap_or_default();
    Ok(project_nested_string_rows(
        page,
        parent_fields,
        child_array_field,
        child_fields,
    ))
}

fn project_nested_string_rows(
    rows: &[Value],
    parent_fields: &[&str],
    child_array_field: &str,
    child_fields: &[&str],
) -> Vec<NestedStringFieldRow> {
    rows.iter()
        .map(|row| match row {
            Value::Object(object) => {
                let child = object.get(child_array_field).and_then(Value::as_array);
                NestedStringFieldRow {
                    object: true,
                    values: project_string_fields(object, parent_fields),
                    child_array: child.is_some(),
                    children: child
                        .into_iter()
                        .flatten()
                        .map(|value| match value {
                            Value::Object(object) => StringFieldRow {
                                object: true,
                                values: project_string_fields(object, child_fields),
                            },
                            _ => StringFieldRow {
                                object: false,
                                values: child_fields.iter().map(|_| None).collect(),
                            },
                        })
                        .collect(),
                }
            }
            _ => NestedStringFieldRow {
                object: false,
                values: parent_fields.iter().map(|_| None).collect(),
                child_array: false,
                children: Vec::new(),
            },
        })
        .collect()
}

/// Builds one JSON object array from exact-width string rows.
pub fn string_object_rows(fields: &[&str], rows: &[Vec<String>]) -> Result<Json, JsonError> {
    let mut seen = std::collections::HashSet::new();
    if let Some(duplicate) = fields.iter().find(|field| !seen.insert(**field)) {
        return Err(JsonError::new(
            "json.duplicate_field",
            format!("JSON object-row field `{duplicate}` is duplicated."),
            0,
        ));
    }
    let mut output = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        if row.len() != fields.len() {
            return Err(JsonError::new(
                "json.row_width_mismatch",
                format!(
                    "JSON object row {index} has width {}; expected {}.",
                    row.len(),
                    fields.len()
                ),
                index,
            ));
        }
        output.push(Value::Object(
            fields
                .iter()
                .zip(row)
                .map(|(field, value)| ((*field).to_string(), Value::String(value.clone())))
                .collect(),
        ));
    }
    Ok(Json::from_serde(Value::Array(output)))
}

fn project_string_fields(object: &Map<String, Value>, fields: &[&str]) -> Vec<Option<String>> {
    fields
        .iter()
        .map(|field| {
            object
                .get(*field)
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

#[path = "json/scalar.rs"]
mod scalar;
pub use scalar::{as_bool, as_float, as_int, as_string, at, is_null, length};

#[cfg(test)]
#[path = "json_test.rs"]
#[cfg(test)]
mod json_test;
