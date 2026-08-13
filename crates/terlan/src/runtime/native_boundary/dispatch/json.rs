//! Typed JSON adapter dispatch kept separate from the protocol-neutral table.

use crate::terlan_native::json;

use super::args::{
    dispatch_json_error, expect_bool, expect_float, expect_int, expect_json, expect_text,
    type_error, unknown_operation,
};
use super::filesystem::expect_text_list;
use super::{DispatchError, NativeBoundaryValue};

pub(super) fn dispatch(
    operation: &str,
    args: &[NativeBoundaryValue],
) -> Result<NativeBoundaryValue, DispatchError> {
    match operation {
        "std.data.json.null" => Ok(NativeBoundaryValue::Json(json::null())),
        "std.data.json.bool" => Ok(NativeBoundaryValue::Json(json::r#bool(expect_bool(
            operation, args, 0,
        )?))),
        "std.data.json.int" => Ok(NativeBoundaryValue::Json(json::int(expect_int(
            operation, args, 0,
        )?))),
        "std.data.json.float" => json::float(expect_float(operation, args, 0)?)
            .map(NativeBoundaryValue::Json)
            .map_err(dispatch_json_error),
        "std.data.json.string" => Ok(NativeBoundaryValue::Json(json::string(expect_text(
            operation, args, 0,
        )?))),
        "std.data.json.array" => Ok(NativeBoundaryValue::Json(json::array())),
        "std.data.json.object" => Ok(NativeBoundaryValue::Json(json::object())),
        "std.data.json.array_extend"
        | "std.data.json.array_push"
        | "std.data.json.array_set"
        | "std.data.json.object_put" => Err(DispatchError::new(
            "dispatch.mutable_receiver_requires_direct_lowering",
            format!(
                "operation `{operation}` mutates a receiver and must use direct native lowering"
            ),
            0,
        )),
        "std.data.json.parse" => json::parse(expect_text(operation, args, 0)?)
            .map(NativeBoundaryValue::Json)
            .map_err(dispatch_json_error),
        "std.data.json.stringify" => json::stringify(expect_json(operation, args, 0)?)
            .map(NativeBoundaryValue::Text)
            .map_err(dispatch_json_error),
        "std.data.json.stringify_pretty" => {
            json::stringify_pretty(expect_json(operation, args, 0)?)
                .map(NativeBoundaryValue::Text)
                .map_err(dispatch_json_error)
        }
        "std.data.json.get" => json::get(
            expect_json(operation, args, 0)?,
            expect_text(operation, args, 1)?,
        )
        .map(NativeBoundaryValue::Json)
        .map_err(dispatch_json_error),
        "std.data.json.keys" => json::keys(expect_json(operation, args, 0)?)
            .map(|keys| {
                NativeBoundaryValue::List(keys.into_iter().map(NativeBoundaryValue::Text).collect())
            })
            .map_err(dispatch_json_error),
        "std.data.json.object_length" => json::object_length(expect_json(operation, args, 0)?)
            .map(NativeBoundaryValue::Int)
            .map_err(dispatch_json_error),
        "std.data.json.string_field_rows" => {
            let rows = json::string_field_rows(
                expect_json(operation, args, 0)?,
                &expect_text_list(operation, args, 1)?,
            )
            .map_err(dispatch_json_error)?;
            Ok(NativeBoundaryValue::List(
                rows.into_iter()
                    .map(|row| NativeBoundaryValue::Record {
                        name: "StringFieldRow".to_string(),
                        fields: vec![
                            ("object".to_string(), NativeBoundaryValue::Bool(row.object)),
                            (
                                "values".to_string(),
                                NativeBoundaryValue::List(
                                    row.values
                                        .into_iter()
                                        .map(NativeBoundaryValue::OptionalText)
                                        .collect(),
                                ),
                            ),
                        ],
                    })
                    .collect(),
            ))
        }
        "std.data.json.nested_string_field_rows" => {
            let rows = json::nested_string_field_rows(
                expect_json(operation, args, 0)?,
                &expect_text_list(operation, args, 1)?,
                expect_text(operation, args, 2)?,
                &expect_text_list(operation, args, 3)?,
            )
            .map_err(dispatch_json_error)?;
            Ok(NativeBoundaryValue::List(
                rows.into_iter().map(nested_string_field_row).collect(),
            ))
        }
        "std.data.json.nested_string_field_rows_page" => {
            let rows = json::nested_string_field_rows_page(
                expect_json(operation, args, 0)?,
                expect_int(operation, args, 1)?,
                expect_int(operation, args, 2)?,
                &expect_text_list(operation, args, 3)?,
                expect_text(operation, args, 4)?,
                &expect_text_list(operation, args, 5)?,
            )
            .map_err(dispatch_json_error)?;
            Ok(NativeBoundaryValue::List(
                rows.into_iter().map(nested_string_field_row).collect(),
            ))
        }
        "std.data.json.string_object_rows" => json::string_object_rows(
            &expect_text_list(operation, args, 0)?,
            &expect_text_rows(operation, args, 1)?,
        )
        .map(NativeBoundaryValue::Json)
        .map_err(dispatch_json_error),
        "std.data.json.string_fields" => json::string_fields(
            expect_json(operation, args, 0)?,
            &expect_text_list(operation, args, 1)?,
        )
        .map(|values| {
            NativeBoundaryValue::List(
                values
                    .into_iter()
                    .map(NativeBoundaryValue::OptionalText)
                    .collect(),
            )
        })
        .map_err(dispatch_json_error),
        "std.data.json.required_fields" => json::required_fields(
            expect_json(operation, args, 0)?,
            &expect_text_list(operation, args, 1)?,
            &expect_text_list(operation, args, 2)?,
            &expect_text_list(operation, args, 3)?,
        )
        .map(required_field_projection)
        .map_err(dispatch_json_error),
        "std.data.json.required_field_rows" => json::required_field_rows(
            expect_json(operation, args, 0)?,
            &expect_text_list(operation, args, 1)?,
            &expect_text_list(operation, args, 2)?,
            &expect_text_list(operation, args, 3)?,
        )
        .map(|rows| NativeBoundaryValue::List(rows.into_iter().map(required_field_row).collect()))
        .map_err(dispatch_json_error),
        "std.data.json.required_field_rows_page" => json::required_field_rows_page(
            expect_json(operation, args, 0)?,
            expect_int(operation, args, 1)?,
            expect_int(operation, args, 2)?,
            &expect_text_list(operation, args, 3)?,
            &expect_text_list(operation, args, 4)?,
            &expect_text_list(operation, args, 5)?,
        )
        .map(|rows| NativeBoundaryValue::List(rows.into_iter().map(required_field_row).collect()))
        .map_err(dispatch_json_error),
        "std.data.json.length" => json::length(expect_json(operation, args, 0)?)
            .map(NativeBoundaryValue::Int)
            .map_err(dispatch_json_error),
        "std.data.json.at" => json::at(
            expect_json(operation, args, 0)?,
            expect_int(operation, args, 1)?,
        )
        .map(NativeBoundaryValue::Json)
        .map_err(dispatch_json_error),
        "std.data.json.as_string" => json::as_string(expect_json(operation, args, 0)?)
            .map(NativeBoundaryValue::Text)
            .map_err(dispatch_json_error),
        "std.data.json.as_int" => json::as_int(expect_json(operation, args, 0)?)
            .map(NativeBoundaryValue::Int)
            .map_err(dispatch_json_error),
        "std.data.json.as_float" => json::as_float(expect_json(operation, args, 0)?)
            .map(NativeBoundaryValue::Float)
            .map_err(dispatch_json_error),
        "std.data.json.as_bool" => json::as_bool(expect_json(operation, args, 0)?)
            .map(NativeBoundaryValue::Bool)
            .map_err(dispatch_json_error),
        "std.data.json.is_null" => Ok(NativeBoundaryValue::Bool(json::is_null(expect_json(
            operation, args, 0,
        )?))),
        _ => Err(unknown_operation(operation)),
    }
}

fn required_field_projection(projection: json::RequiredFieldProjection) -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: "RequiredFieldProjection".to_string(),
        fields: vec![
            (
                "strings".to_string(),
                NativeBoundaryValue::List(
                    projection
                        .strings
                        .into_iter()
                        .map(NativeBoundaryValue::Text)
                        .collect(),
                ),
            ),
            (
                "ints".to_string(),
                NativeBoundaryValue::List(
                    projection
                        .ints
                        .into_iter()
                        .map(NativeBoundaryValue::Int)
                        .collect(),
                ),
            ),
            (
                "array_lengths".to_string(),
                NativeBoundaryValue::List(
                    projection
                        .array_lengths
                        .into_iter()
                        .map(NativeBoundaryValue::Int)
                        .collect(),
                ),
            ),
        ],
    }
}

fn required_field_row(row: json::RequiredFieldRow) -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: "RequiredFieldRow".to_string(),
        fields: vec![
            (
                "strings".to_string(),
                NativeBoundaryValue::List(
                    row.strings
                        .into_iter()
                        .map(NativeBoundaryValue::Text)
                        .collect(),
                ),
            ),
            (
                "ints".to_string(),
                NativeBoundaryValue::List(
                    row.ints.into_iter().map(NativeBoundaryValue::Int).collect(),
                ),
            ),
            (
                "bools".to_string(),
                NativeBoundaryValue::List(
                    row.bools
                        .into_iter()
                        .map(NativeBoundaryValue::Bool)
                        .collect(),
                ),
            ),
        ],
    }
}

fn string_field_row(row: json::StringFieldRow) -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: "StringFieldRow".to_string(),
        fields: vec![
            ("object".to_string(), NativeBoundaryValue::Bool(row.object)),
            (
                "values".to_string(),
                NativeBoundaryValue::List(
                    row.values
                        .into_iter()
                        .map(NativeBoundaryValue::OptionalText)
                        .collect(),
                ),
            ),
        ],
    }
}

fn nested_string_field_row(row: json::NestedStringFieldRow) -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: "NestedStringFieldRow".to_string(),
        fields: vec![
            ("object".to_string(), NativeBoundaryValue::Bool(row.object)),
            (
                "values".to_string(),
                NativeBoundaryValue::List(
                    row.values
                        .into_iter()
                        .map(NativeBoundaryValue::OptionalText)
                        .collect(),
                ),
            ),
            (
                "child_array".to_string(),
                NativeBoundaryValue::Bool(row.child_array),
            ),
            (
                "children".to_string(),
                NativeBoundaryValue::List(row.children.into_iter().map(string_field_row).collect()),
            ),
        ],
    }
}

fn expect_text_rows(
    operation: &str,
    args: &[NativeBoundaryValue],
    index: usize,
) -> Result<Vec<Vec<String>>, DispatchError> {
    let Some(NativeBoundaryValue::List(rows)) = args.get(index) else {
        return Err(type_error(operation, index, "List[List[String]]"));
    };
    rows.iter()
        .map(|row| {
            let NativeBoundaryValue::List(values) = row else {
                return Err(type_error(operation, index, "List[List[String]]"));
            };
            values
                .iter()
                .map(|value| match value {
                    NativeBoundaryValue::Text(value) => Ok(value.clone()),
                    _ => Err(type_error(operation, index, "List[List[String]]")),
                })
                .collect()
        })
        .collect()
}
