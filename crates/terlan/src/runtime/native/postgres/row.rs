//! Postgres row storage and driver-row decoding.
//!
//! This module owns the deterministic Terlan-facing row representation and the
//! conversion from maintained `tokio-postgres` driver rows.

use std::collections::BTreeMap;

use tokio_postgres::types::Type;
use tokio_postgres::Row as DriverRow;

use crate::terlan_native::json as json_adapter;

use super::PostgresError;

/// Postgres row value used by row-decoding helpers.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    values: BTreeMap<String, PostgresValue>,
}

impl Row {
    /// Builds an empty row.
    ///
    /// Inputs:
    /// - No external input.
    ///
    /// Output:
    /// - Row with no columns.
    ///
    /// Transformation:
    /// - Initializes deterministic map-backed row storage for adapter tests
    ///   before live database rows are wired in.
    pub fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    /// Inserts one string column.
    ///
    /// Inputs:
    /// - `self`: mutable row fixture.
    /// - `name`: column name.
    /// - `value`: column value.
    ///
    /// Output:
    /// - No return value.
    ///
    /// Transformation:
    /// - Stores the value under the supplied name for later typed decoding.
    pub fn put_string(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.values
            .insert(name.into(), PostgresValue::String(value.into()));
    }

    /// Inserts one integer column.
    ///
    /// Inputs:
    /// - `self`: mutable row fixture.
    /// - `name`: column name.
    /// - `value`: column value.
    ///
    /// Output:
    /// - No return value.
    ///
    /// Transformation:
    /// - Stores the value under the supplied name for later typed decoding.
    pub fn put_int(&mut self, name: impl Into<String>, value: i64) {
        self.values.insert(name.into(), PostgresValue::Int(value));
    }

    /// Inserts one boolean column.
    ///
    /// Inputs:
    /// - `self`: mutable row fixture.
    /// - `name`: column name.
    /// - `value`: column value.
    ///
    /// Output:
    /// - No return value.
    ///
    /// Transformation:
    /// - Stores the value under the supplied name for later typed decoding.
    pub fn put_bool(&mut self, name: impl Into<String>, value: bool) {
        self.values.insert(name.into(), PostgresValue::Bool(value));
    }

    /// Inserts one JSON column.
    ///
    /// Inputs:
    /// - `self`: mutable row fixture.
    /// - `name`: column name.
    /// - `value`: JSON column value.
    ///
    /// Output:
    /// - No return value.
    ///
    /// Transformation:
    /// - Stores the value under the supplied name for later typed decoding.
    pub fn put_json(&mut self, name: impl Into<String>, value: json_adapter::Json) {
        self.values.insert(name.into(), PostgresValue::Json(value));
    }
}

impl Default for Row {
    /// Builds the default row value.
    ///
    /// Inputs:
    /// - No external input.
    ///
    /// Output:
    /// - Empty row.
    ///
    /// Transformation:
    /// - Delegates to `Row::new`.
    fn default() -> Self {
        Self::new()
    }
}

/// Typed Postgres column value.
#[derive(Clone, Debug, PartialEq)]
enum PostgresValue {
    String(String),
    Int(i64),
    Bool(bool),
    Json(json_adapter::Json),
}

/// Reads a string column by name.
///
/// Inputs:
/// - `row`: Postgres row.
/// - `name`: column name.
///
/// Output:
/// - `Ok(value)` when the column is present and is a string.
/// - Stable error for missing columns or type mismatches.
///
/// Transformation:
/// - Decodes the fixture/native row value through the same typed accessor
///   surface exposed by `std.db.Postgres.Row.string`.
pub fn string(row: &Row, name: &str) -> Result<String, PostgresError> {
    match row.values.get(name) {
        Some(PostgresValue::String(value)) => Ok(value.clone()),
        Some(value) => Err(type_error(name, "String", value.kind())),
        None => Err(missing_column(name)),
    }
}

/// Reads an integer column by name.
///
/// Inputs:
/// - `row`: Postgres row.
/// - `name`: column name.
///
/// Output:
/// - `Ok(value)` when the column is present and is an integer.
/// - Stable error for missing columns or type mismatches.
///
/// Transformation:
/// - Decodes the fixture/native row value through the same typed accessor
///   surface exposed by `std.db.Postgres.Row.int`.
pub fn int(row: &Row, name: &str) -> Result<i64, PostgresError> {
    match row.values.get(name) {
        Some(PostgresValue::Int(value)) => Ok(*value),
        Some(value) => Err(type_error(name, "Int", value.kind())),
        None => Err(missing_column(name)),
    }
}

/// Reads a boolean column by name.
///
/// Inputs:
/// - `row`: Postgres row.
/// - `name`: column name.
///
/// Output:
/// - `Ok(value)` when the column is present and is a boolean.
/// - Stable error for missing columns or type mismatches.
///
/// Transformation:
/// - Decodes the fixture/native row value through the same typed accessor
///   surface exposed by `std.db.Postgres.Row.bool`.
pub fn r#bool(row: &Row, name: &str) -> Result<bool, PostgresError> {
    match row.values.get(name) {
        Some(PostgresValue::Bool(value)) => Ok(*value),
        Some(value) => Err(type_error(name, "Bool", value.kind())),
        None => Err(missing_column(name)),
    }
}

/// Reads a JSON column by name.
///
/// Inputs:
/// - `row`: Postgres row.
/// - `name`: column name.
///
/// Output:
/// - `Ok(value)` when the column is present and is JSON.
/// - Stable error for missing columns or type mismatches.
///
/// Transformation:
/// - Decodes the fixture/native row value through the same typed accessor
///   surface exposed by `std.db.Postgres.Row.json`.
pub fn json(row: &Row, name: &str) -> Result<json_adapter::Json, PostgresError> {
    match row.values.get(name) {
        Some(PostgresValue::Json(value)) => Ok(value.clone()),
        Some(value) => Err(type_error(name, "Json", value.kind())),
        None => Err(missing_column(name)),
    }
}

impl PostgresValue {
    /// Returns the stable Terlan type name for this column value.
    ///
    /// Inputs:
    /// - `self`: stored column value.
    ///
    /// Output:
    /// - Source-visible type name used in diagnostics.
    ///
    /// Transformation:
    /// - Maps internal row variants to public names without exposing backend
    ///   row storage.
    fn kind(&self) -> &'static str {
        match self {
            Self::String(_) => "String",
            Self::Int(_) => "Int",
            Self::Bool(_) => "Bool",
            Self::Json(_) => "Json",
        }
    }
}

/// Converts one driver row into the SafeNative row shape.
///
/// Inputs:
/// - `row`: row returned by `tokio-postgres`.
///
/// Output:
/// - SafeNative row with supported typed column values.
/// - Stable row error for unsupported column types.
///
/// Transformation:
/// - Reads column metadata from the maintained driver and copies supported
///   values into Terlan's backend-neutral row representation.
pub(super) fn row_from_driver(row: &DriverRow) -> Result<Row, PostgresError> {
    let mut output = Row::new();
    for column in row.columns() {
        put_column_from_driver(row, column.name(), column.type_(), &mut output)?;
    }
    Ok(output)
}

/// Copies one supported driver column into a SafeNative row.
///
/// Inputs:
/// - `row`: driver row containing the column.
/// - `name`: column name.
/// - `ty`: Postgres column type.
/// - `output`: row being populated.
///
/// Output:
/// - `Ok(())` when the column is supported.
/// - Stable row error for unsupported or undecodable values.
///
/// Transformation:
/// - Converts driver-specific values into the limited typed row value set
///   currently exposed by `std.db.Postgres`.
fn put_column_from_driver(
    row: &DriverRow,
    name: &str,
    ty: &Type,
    output: &mut Row,
) -> Result<(), PostgresError> {
    match *ty {
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => {
            if let Some(value) = try_get_optional::<String>(row, name)? {
                output.put_string(name, value);
            } else {
                output.put_json(name, json_adapter::null());
            }
            Ok(())
        }
        Type::INT8 => {
            if let Some(value) = try_get_optional::<i64>(row, name)? {
                output.put_int(name, value);
            } else {
                output.put_json(name, json_adapter::null());
            }
            Ok(())
        }
        Type::INT4 => {
            if let Some(value) = try_get_optional::<i32>(row, name)? {
                output.put_int(name, i64::from(value));
            } else {
                output.put_json(name, json_adapter::null());
            }
            Ok(())
        }
        Type::INT2 => {
            if let Some(value) = try_get_optional::<i16>(row, name)? {
                output.put_int(name, i64::from(value));
            } else {
                output.put_json(name, json_adapter::null());
            }
            Ok(())
        }
        Type::BOOL => {
            if let Some(value) = try_get_optional::<bool>(row, name)? {
                output.put_bool(name, value);
            } else {
                output.put_json(name, json_adapter::null());
            }
            Ok(())
        }
        Type::JSON | Type::JSONB => {
            let value = try_get_optional::<serde_json::Value>(row, name)?
                .unwrap_or(serde_json::Value::Null);
            output.put_json(name, json_adapter::Json::from_serde(value));
            Ok(())
        }
        _ => Err(PostgresError::new(
            "postgres.row.unsupported_type",
            format!(
                "Postgres row column `{name}` has unsupported type `{}`.",
                ty.name()
            ),
        )),
    }
}

/// Reads one nullable column value from a driver row.
///
/// Inputs:
/// - `row`: driver row.
/// - `name`: column name.
///
/// Output:
/// - `Ok(Some(value))` when a non-null value is present.
/// - `Ok(None)` for SQL null.
/// - Stable row error when the driver cannot decode the value as `T`.
///
/// Transformation:
/// - Delegates decoding to `tokio-postgres` and erases driver diagnostics into
///   Terlan's stable Postgres error envelope.
fn try_get_optional<T>(row: &DriverRow, name: &str) -> Result<Option<T>, PostgresError>
where
    for<'a> T: tokio_postgres::types::FromSql<'a>,
{
    row.try_get::<_, Option<T>>(name).map_err(|error| {
        PostgresError::new(
            "postgres.row.decode",
            format!("Could not decode Postgres row column `{name}`: {error}."),
        )
    })
}

/// Builds a missing-column error.
///
/// Inputs:
/// - `name`: missing column name.
///
/// Output:
/// - Stable missing-column error.
///
/// Transformation:
/// - Converts row lookup absence into portable row-decoding diagnostics.
fn missing_column(name: &str) -> PostgresError {
    PostgresError::new(
        "postgres.row.missing_column",
        format!("Postgres row does not contain column `{name}`."),
    )
}

/// Builds a type-mismatch error.
///
/// Inputs:
/// - `name`: column name.
/// - `expected`: expected Terlan type name.
/// - `actual`: actual Terlan type name.
///
/// Output:
/// - Stable row type-mismatch error.
///
/// Transformation:
/// - Converts a stored row variant mismatch into portable row-decoding
///   diagnostics.
fn type_error(name: &str, expected: &str, actual: &str) -> PostgresError {
    PostgresError::new(
        "postgres.row.type",
        format!("Postgres row column `{name}` is {actual}, expected {expected}."),
    )
}
