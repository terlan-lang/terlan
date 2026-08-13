//! Backend-neutral Postgres row storage and typed decoding.
//!
//! The VM driver converts native rows into this deterministic Terlan-facing
//! representation before values cross the NativeBoundary.

use std::collections::BTreeMap;

#[cfg(any(
    test,
    all(not(feature = "serve-runtime-bin"), feature = "postgres-libpq")
))]
use crate::database_schema::DatabaseColumnCodec;
use crate::terlan_native::json as json_adapter;

use super::PostgresError;

/// Postgres row value used by row-decoding helpers.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    values: BTreeMap<String, DecodedValue>,
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
            .insert(name.into(), DecodedValue::String(value.into()));
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
        self.values.insert(name.into(), DecodedValue::Int(value));
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
        self.values.insert(name.into(), DecodedValue::Bool(value));
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
        self.values.insert(name.into(), DecodedValue::Json(value));
    }

    /// Copies one text-format libpq value into backend-neutral row storage.
    #[cfg(any(
        test,
        all(not(feature = "serve-runtime-bin"), feature = "postgres-libpq")
    ))]
    pub(crate) fn put_libpq_text(
        &mut self,
        name: impl Into<String>,
        oid: i64,
        value: Option<&str>,
    ) -> Result<(), PostgresError> {
        let name = name.into();
        let Some(value) = value else {
            self.values.insert(name, DecodedValue::Null);
            return Ok(());
        };
        let decoded = match DatabaseColumnCodec::resolve(None, Some(oid)) {
            Some(DatabaseColumnCodec::Bool) => {
                DecodedValue::Bool(matches!(value, "t" | "true" | "1"))
            }
            Some(DatabaseColumnCodec::Int) => {
                DecodedValue::Int(value.parse().map_err(|error| {
                    PostgresError::new(
                        "postgres.decode.int",
                        format!("Could not decode Postgres integer column: {error}."),
                    )
                })?)
            }
            Some(DatabaseColumnCodec::Json) => DecodedValue::Json(json_adapter::Json::from_serde(
                serde_json::from_str(value).map_err(|error| {
                    PostgresError::new(
                        "postgres.decode.json",
                        format!("Could not decode Postgres JSON column: {error}."),
                    )
                })?,
            )),
            Some(DatabaseColumnCodec::Binary) | None => DecodedValue::String(value.to_string()),
        };
        self.values.insert(name, decoded);
        Ok(())
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

/// Driver-decoded Postgres column value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DecodedValue {
    #[cfg(any(
        test,
        all(not(feature = "serve-runtime-bin"), feature = "postgres-libpq")
    ))]
    Null,
    String(String),
    Int(i64),
    Bool(bool),
    Json(json_adapter::Json),
}

/// Decodes one column using the concrete type already established by libpq.
///
/// This is reserved for runtime boundaries that carry a row descriptor
/// separately. Source-visible typed row access continues to use `string`,
/// `int`, `bool`, and `json` so callers cannot silently accept type drift.
#[cfg(any(
    test,
    all(not(feature = "serve-runtime-bin"), feature = "postgres-libpq")
))]
pub(crate) fn value(row: &Row, name: &str) -> Result<DecodedValue, PostgresError> {
    row.values
        .get(name)
        .cloned()
        .ok_or_else(|| missing_column(name))
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
        Some(DecodedValue::String(value)) => Ok(value.clone()),
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
        Some(DecodedValue::Int(value)) => Ok(*value),
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
        Some(DecodedValue::Bool(value)) => Ok(*value),
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
        Some(DecodedValue::Json(value)) => Ok(value.clone()),
        Some(value) => Err(type_error(name, "Json", value.kind())),
        None => Err(missing_column(name)),
    }
}

impl DecodedValue {
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
            #[cfg(any(
                test,
                all(not(feature = "serve-runtime-bin"), feature = "postgres-libpq")
            ))]
            Self::Null => "Null",
            Self::String(_) => "String",
            Self::Int(_) => "Int",
            Self::Bool(_) => "Bool",
            Self::Json(_) => "Json",
        }
    }
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
