#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used
)]

//! Package-owned serde_json adapter for std.data.Json.
//!
//! Parser, value, and projection behavior lives here, independently of the
//! compiler and VM. Legacy resource dispatch still calls this API during the
//! migration to generic package resources and Terlan-owned library policy.

use serde_json::Value;

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

mod invocation;
mod operations;
mod projection;
mod projection_values;
mod scalar;
mod value;

pub use invocation::{invoke, mutate};
pub use operations::{operation, RESOURCE_OPERATIONS};

/// Resource-aware package entry points for any owner-checked host registry.
pub static RESOURCE_ADAPTER: terlan_runtime_abi::NativeResourceAdapter<Json> =
    terlan_runtime_abi::NativeResourceAdapter {
        operations: RESOURCE_OPERATIONS,
        invoke,
        mutate,
    };

pub use projection::{
    nested_string_field_rows, nested_string_field_rows_page, required_field_rows,
    required_field_rows_page, required_fields, string_field_rows, string_object_rows,
};
pub use scalar::{as_bool, as_float, as_int, as_string, at, is_null, length};
pub use value::{
    array, extend, float, get, int, keys, null, object, object_length, parse, push, put, r#bool,
    remove, set, string, stringify, stringify_pretty, to_string,
};

#[cfg(test)]
mod json_test;
