//! Legacy owned projections, pending migration of library policy to Terlan.

use super::{
    Json, JsonError, NestedStringFieldRow, RequiredFieldProjection, RequiredFieldRow,
    StringFieldRow,
};
use serde_json::{Map, Value};

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
