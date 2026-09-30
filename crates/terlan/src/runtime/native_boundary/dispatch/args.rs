use crate::terlan_native::{json, path, postgres, regex, vector};
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;
use crate::terlan_native_boundary::resource::ResourceError;

use super::{DispatchError, NativeBoundaryBridgeValue, NativeBoundaryValue};

/// Reads a text argument from a neutral value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied neutral values.
/// - `index`: expected text argument index.
///
/// Output:
/// - Borrowed string slice when the value is `Text`.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs a runtime shape check before adapter invocation.
pub(super) fn expect_text<'a>(
    operation: &str,
    args: &'a [NativeBoundaryValue],
    index: usize,
) -> Result<&'a str, DispatchError> {
    match args.get(index) {
        Some(NativeBoundaryValue::Text(value)) => Ok(value),
        _ => Err(type_error(operation, index, "String")),
    }
}

/// Reads a boolean argument from a neutral value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied neutral values.
/// - `index`: expected boolean argument index.
///
/// Output:
/// - Boolean value when the neutral value is `Bool`.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs a runtime shape check before adapter invocation.
pub(super) fn expect_bool(
    operation: &str,
    args: &[NativeBoundaryValue],
    index: usize,
) -> Result<bool, DispatchError> {
    match args.get(index) {
        Some(NativeBoundaryValue::Bool(value)) => Ok(*value),
        _ => Err(type_error(operation, index, "Bool")),
    }
}

/// Reads an integer argument from a neutral value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied neutral values.
/// - `index`: expected integer argument index.
///
/// Output:
/// - Integer value when the neutral value is `Int`.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs a runtime shape check before adapter invocation.
pub(super) fn expect_int(
    operation: &str,
    args: &[NativeBoundaryValue],
    index: usize,
) -> Result<i64, DispatchError> {
    match args.get(index) {
        Some(NativeBoundaryValue::Int(value)) => Ok(*value),
        _ => Err(type_error(operation, index, "Int")),
    }
}

/// Reads a compiled regex argument from a neutral value slice.
pub(super) fn expect_regex<'a>(
    operation: &str,
    args: &'a [NativeBoundaryValue],
    index: usize,
) -> Result<&'a regex::Regex, DispatchError> {
    match args.get(index) {
        Some(NativeBoundaryValue::Regex(value)) => Ok(value),
        _ => Err(type_error(operation, index, "Regex")),
    }
}

/// Reads a resource handle from a bridge value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied bridge values.
/// - `index`: expected handle argument index.
///
/// Output:
/// - NativeBoundary handle when present.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs the bridge-side shape check required before mutable resource
///   borrowing.
pub(super) fn expect_bridge_handle(
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
    index: usize,
) -> Result<NativeBoundaryHandle, DispatchError> {
    match args.get(index) {
        Some(NativeBoundaryBridgeValue::Handle(value)) => Ok(*value),
        _ => Err(type_error(operation, index, "Handle")),
    }
}

/// Reads an integer argument from a bridge value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied bridge values.
/// - `index`: expected integer argument index.
///
/// Output:
/// - Integer value when present.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs the bridge-side shape check required before indexed resource
///   operations.
pub(super) fn expect_bridge_int(
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
    index: usize,
) -> Result<i64, DispatchError> {
    match args.get(index) {
        Some(NativeBoundaryBridgeValue::Int(value)) => Ok(*value),
        _ => Err(type_error(operation, index, "Int")),
    }
}

/// Reads a list argument from a bridge value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied bridge values.
/// - `index`: expected list argument index.
///
/// Output:
/// - Borrowed bridge-value slice when present.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Keeps constructor/list conversion validation at the NativeBoundary boundary
///   before resource allocation.
pub(super) fn expect_bridge_list<'a>(
    operation: &str,
    args: &'a [NativeBoundaryBridgeValue],
    index: usize,
) -> Result<&'a [NativeBoundaryBridgeValue], DispatchError> {
    match args.get(index) {
        Some(NativeBoundaryBridgeValue::List(values)) => Ok(values),
        _ => Err(type_error(operation, index, "List")),
    }
}

/// Reads a path argument from a neutral value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied neutral values.
/// - `index`: expected path argument index.
///
/// Output:
/// - Borrowed path wrapper when the value is `Path`.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs a runtime shape check before adapter invocation.
pub(super) fn expect_path<'a>(
    operation: &str,
    args: &'a [NativeBoundaryValue],
    index: usize,
) -> Result<&'a path::Path, DispatchError> {
    match args.get(index) {
        Some(NativeBoundaryValue::Path(value)) => Ok(value),
        _ => Err(type_error(operation, index, "Path")),
    }
}

/// Reads a Postgres config argument from a neutral value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied neutral values.
/// - `index`: expected config argument index.
///
/// Output:
/// - Borrowed Postgres config when the value is `PostgresConfig`.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs a runtime shape check before adapter invocation.
pub(super) fn expect_postgres_config<'a>(
    operation: &str,
    args: &'a [NativeBoundaryValue],
    index: usize,
) -> Result<&'a postgres::Config, DispatchError> {
    match args.get(index) {
        Some(NativeBoundaryValue::PostgresConfig(value)) => Ok(value),
        _ => Err(type_error(operation, index, "PostgresConfig")),
    }
}

/// Reads a Postgres pool argument from a neutral value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied neutral values.
/// - `index`: expected pool argument index.
///
/// Output:
/// - Borrowed Postgres pool when the value is `PostgresPool`.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs a runtime shape check before adapter invocation.
pub(super) fn expect_postgres_pool<'a>(
    operation: &str,
    args: &'a [NativeBoundaryValue],
    index: usize,
) -> Result<&'a postgres::Pool, DispatchError> {
    match args.get(index) {
        Some(NativeBoundaryValue::PostgresPool(value)) => Ok(value),
        _ => Err(type_error(operation, index, "PostgresPool")),
    }
}

/// Reads a Postgres row argument from a neutral value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied neutral values.
/// - `index`: expected row argument index.
///
/// Output:
/// - Borrowed Postgres row when the value is `PostgresRow`.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs a runtime shape check before adapter invocation.
pub(super) fn expect_postgres_row<'a>(
    operation: &str,
    args: &'a [NativeBoundaryValue],
    index: usize,
) -> Result<&'a postgres::Row, DispatchError> {
    match args.get(index) {
        Some(NativeBoundaryValue::PostgresRow(value)) => Ok(value),
        _ => Err(type_error(operation, index, "PostgresRow")),
    }
}

/// Reads a JSON-list argument from a neutral value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied neutral values.
/// - `index`: expected JSON list argument index.
///
/// Output:
/// - Borrowed JSON slice when the value is `JsonList`.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs a runtime shape check before adapter invocation.
pub(super) fn expect_json_list<'a>(
    operation: &str,
    args: &'a [NativeBoundaryValue],
    index: usize,
) -> Result<&'a [json::Json], DispatchError> {
    match args.get(index) {
        Some(NativeBoundaryValue::JsonList(value)) => Ok(value),
        _ => Err(type_error(operation, index, "List[Json]")),
    }
}

/// Builds an unknown-operation dispatch error.
///
/// Inputs:
/// - `operation`: unsupported compiler-native operation id.
///
/// Output:
/// - `DispatchError` with stable code `dispatch.unknown_operation`.
///
/// Transformation:
/// - Converts a missing dispatch branch into a stable boundary error.
pub(super) fn unknown_operation(operation: &str) -> DispatchError {
    DispatchError::new(
        "dispatch.unknown_operation",
        format!("No NativeBoundary adapter is registered for `{operation}`."),
        0,
    )
}

/// Builds a type-mismatch dispatch error.
///
/// Inputs:
/// - `operation`: compiler-native operation id.
/// - `index`: mismatched argument index.
/// - `expected`: expected Terlan-facing value kind.
///
/// Output:
/// - `DispatchError` with stable code `dispatch.type`.
///
/// Transformation:
/// - Converts a runtime argument shape mismatch into one diagnostic form.
pub(super) fn type_error(operation: &str, index: usize, expected: &str) -> DispatchError {
    DispatchError::new(
        "dispatch.type",
        format!("Operation `{operation}` argument {index} must be `{expected}`."),
        0,
    )
}

/// Converts a JSON adapter error into a dispatch error.
///
/// Inputs:
/// - `error`: JSON adapter error.
///
/// Output:
/// - Dispatch error preserving JSON code, message, and offset.
///
/// Transformation:
/// - Erases the adapter-specific error type while preserving stable fields.
pub(super) fn dispatch_json_error(error: json::JsonError) -> DispatchError {
    DispatchError::new(error.code(), error.message(), error.offset())
}

/// Converts a regex adapter error into a stable dispatch error.
pub(super) fn dispatch_regex_error(error: regex::RegexError) -> DispatchError {
    DispatchError::new(error.code(), error.message(), error.offset())
}

/// Converts a path adapter error into a dispatch error.
///
/// Inputs:
/// - `error`: path adapter error.
///
/// Output:
/// - Dispatch error preserving path code, message, and offset.
///
/// Transformation:
/// - Erases the adapter-specific error type while preserving stable fields.
pub(super) fn dispatch_path_error(error: path::PathError) -> DispatchError {
    DispatchError::new(error.code(), error.message(), error.offset())
}

/// Converts a native vector adapter error into a dispatch error.
///
/// Inputs:
/// - `error`: native vector adapter error.
///
/// Output:
/// - Dispatch error preserving the vector code and message.
///
/// Transformation:
/// - Reuses the stable NativeBoundary dispatch error envelope for vector resource
///   failures.
pub(super) fn dispatch_vector_error(error: vector::VectorError) -> DispatchError {
    DispatchError::new(error.code(), error.message(), 0)
}

/// Converts a Postgres adapter error into a dispatch error.
///
/// Inputs:
/// - `error`: Postgres adapter error.
///
/// Output:
/// - Dispatch error preserving Postgres code and message.
///
/// Transformation:
/// - Erases the adapter-specific error type while preserving stable fields
///   relevant to the generic NativeBoundary dispatch layer.
pub(super) fn dispatch_postgres_error(error: postgres::PostgresError) -> DispatchError {
    DispatchError::new(error.code(), error.message(), 0)
}

/// Converts a resource-store error into a dispatch error.
///
/// Inputs:
/// - `error`: resource-store error.
///
/// Output:
/// - Dispatch error preserving resource code and message.
///
/// Transformation:
/// - Erases the resource-specific error type while preserving stable fields.
pub(super) fn dispatch_resource_error(error: ResourceError) -> DispatchError {
    DispatchError::new(error.code(), error.message(), 0)
}
