//! Pure operation dispatcher for Rust-backed SafeNative std adapters.
//!
//! This module is the first shared execution surface between compiler-native
//! operation ids such as `std.data.json.parse` and concrete Rust adapter
//! functions. The BEAM/native worker layer can call this module after it has
//! decoded runtime terms into `SafeNativeValue`.

use crate::handle::SafeNativeHandle;
use crate::resource::{ResourceError, ResourceStore, ResourceValue};
use crate::{base64, json, path, uri};

/// Neutral value shape accepted and returned by SafeNative adapter dispatch.
#[derive(Clone, Debug, PartialEq)]
pub enum SafeNativeValue {
    /// Terlan `Unit`.
    Unit,
    /// Terlan `String`.
    Text(String),
    /// Terlan `Int`.
    Int(i64),
    /// Terlan `Float`.
    Float(f64),
    /// Terlan `Bool`.
    Bool(bool),
    /// Opaque `std.data.Json.Json`.
    Json(json::Json),
    /// Opaque `std.io.Path.Path`.
    Path(path::Path),
    /// Opaque `std.net.Uri.Uri`.
    Uri(uri::Uri),
    /// `Option[String]` for string component accessors.
    OptionalText(Option<String>),
    /// `Option[Path]` for path component accessors.
    OptionalPath(Option<path::Path>),
}

/// Bridge-facing value shape that carries opaque resources as handles.
#[derive(Clone, Debug, PartialEq)]
pub enum SafeNativeBridgeValue {
    /// Terlan `Unit`.
    Unit,
    /// Terlan `String`.
    Text(String),
    /// Terlan `Int`.
    Int(i64),
    /// Terlan `Float`.
    Float(f64),
    /// Terlan `Bool`.
    Bool(bool),
    /// Opaque resource handle for JSON, path, URI, or later native resources.
    Handle(SafeNativeHandle),
    /// `Option[String]` for string component accessors.
    OptionalText(Option<String>),
    /// `Option[Handle]` for optional opaque resources such as path parents.
    OptionalHandle(Option<SafeNativeHandle>),
}

/// Stable dispatcher error returned before crossing a runtime boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchError {
    code: &'static str,
    message: String,
    offset: usize,
}

impl DispatchError {
    /// Builds a dispatcher error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    /// - `offset`: source/input byte offset when available, or `0`.
    ///
    /// Output:
    /// - A `DispatchError` suitable for the SafeNative boundary.
    ///
    /// Transformation:
    /// - Stores adapter-independent error metadata without exposing backend
    ///   exception types.
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
    /// - `self`: dispatcher error.
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
    /// - `self`: dispatcher error.
    ///
    /// Output:
    /// - Borrowed message text.
    ///
    /// Transformation:
    /// - Reads the message field without allocation or mutation.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the byte offset associated with the error.
    ///
    /// Inputs:
    /// - `self`: dispatcher error.
    ///
    /// Output:
    /// - Byte offset, or `0` when no adapter supplied one.
    ///
    /// Transformation:
    /// - Reads the offset field without allocation or mutation.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

/// Dispatches one compiler-native operation to a SafeNative adapter function.
///
/// Inputs:
/// - `operation`: compiler-native operation id from `@compiler.native`.
/// - `args`: neutral runtime values decoded by the native bridge.
///
/// Output:
/// - `Ok(SafeNativeValue)` with the adapter result.
/// - `Err(DispatchError)` for unknown operation ids, arity mismatches, type
///   mismatches, or adapter-specific stable errors.
///
/// Transformation:
/// - Validates the operation id and argument shapes, calls the corresponding
///   Rust adapter, and converts adapter-specific errors into one dispatch
///   error shape.
pub fn dispatch(
    operation: &str,
    args: &[SafeNativeValue],
) -> Result<SafeNativeValue, DispatchError> {
    validate_arity(operation, args)?;
    match operation {
        "std.data.json.parse" => {
            let text = expect_text(operation, args, 0)?;
            json::parse(text)
                .map(SafeNativeValue::Json)
                .map_err(dispatch_json_error)
        }
        "std.data.json.stringify" => {
            let value = expect_json(operation, args, 0)?;
            json::stringify(value)
                .map(SafeNativeValue::Text)
                .map_err(dispatch_json_error)
        }
        "std.data.json.get" => {
            let value = expect_json(operation, args, 0)?;
            let key = expect_text(operation, args, 1)?;
            json::get(value, key)
                .map(SafeNativeValue::Json)
                .map_err(dispatch_json_error)
        }
        "std.data.json.as_string" => {
            let value = expect_json(operation, args, 0)?;
            json::as_string(value)
                .map(SafeNativeValue::Text)
                .map_err(dispatch_json_error)
        }
        "std.data.json.as_int" => {
            let value = expect_json(operation, args, 0)?;
            json::as_int(value)
                .map(SafeNativeValue::Int)
                .map_err(dispatch_json_error)
        }
        "std.data.json.as_float" => {
            let value = expect_json(operation, args, 0)?;
            json::as_float(value)
                .map(SafeNativeValue::Float)
                .map_err(dispatch_json_error)
        }
        "std.data.json.as_bool" => {
            let value = expect_json(operation, args, 0)?;
            json::as_bool(value)
                .map(SafeNativeValue::Bool)
                .map_err(dispatch_json_error)
        }
        "std.data.json.is_null" => {
            let value = expect_json(operation, args, 0)?;
            Ok(SafeNativeValue::Bool(json::is_null(value)))
        }
        "std.encoding.base64.encode" => {
            let text = expect_text(operation, args, 0)?;
            Ok(SafeNativeValue::Text(base64::encode(text)))
        }
        "std.encoding.base64.decode" => {
            let text = expect_text(operation, args, 0)?;
            base64::decode(text)
                .map(SafeNativeValue::Text)
                .map_err(dispatch_base64_error)
        }
        "std.encoding.base64.encode_url" => {
            let text = expect_text(operation, args, 0)?;
            Ok(SafeNativeValue::Text(base64::encode_url(text)))
        }
        "std.encoding.base64.decode_url" => {
            let text = expect_text(operation, args, 0)?;
            base64::decode_url(text)
                .map(SafeNativeValue::Text)
                .map_err(dispatch_base64_error)
        }
        "std.io.path.from_string" => {
            let text = expect_text(operation, args, 0)?;
            path::from_string(text)
                .map(SafeNativeValue::Path)
                .map_err(dispatch_path_error)
        }
        "std.io.path.to_string" => {
            let value = expect_path(operation, args, 0)?;
            Ok(SafeNativeValue::Text(path::to_string(value)))
        }
        "std.io.path.join" => {
            let value = expect_path(operation, args, 0)?;
            let child = expect_text(operation, args, 1)?;
            path::join(value, child)
                .map(SafeNativeValue::Path)
                .map_err(dispatch_path_error)
        }
        "std.io.path.file_name" => {
            let value = expect_path(operation, args, 0)?;
            Ok(SafeNativeValue::OptionalText(path::file_name(value)))
        }
        "std.io.path.extension" => {
            let value = expect_path(operation, args, 0)?;
            Ok(SafeNativeValue::OptionalText(path::extension(value)))
        }
        "std.io.path.parent" => {
            let value = expect_path(operation, args, 0)?;
            Ok(SafeNativeValue::OptionalPath(path::parent(value)))
        }
        "std.io.path.is_absolute" => {
            let value = expect_path(operation, args, 0)?;
            Ok(SafeNativeValue::Bool(path::is_absolute(value)))
        }
        "std.net.uri.parse" => {
            let text = expect_text(operation, args, 0)?;
            uri::parse(text)
                .map(SafeNativeValue::Uri)
                .map_err(dispatch_uri_error)
        }
        "std.net.uri.to_string" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(SafeNativeValue::Text(uri::to_string(value)))
        }
        "std.net.uri.scheme" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(SafeNativeValue::Text(uri::scheme(value)))
        }
        "std.net.uri.host" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(SafeNativeValue::OptionalText(uri::host(value)))
        }
        "std.net.uri.path" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(SafeNativeValue::Text(uri::path(value)))
        }
        "std.net.uri.query" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(SafeNativeValue::OptionalText(uri::query(value)))
        }
        "std.net.uri.fragment" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(SafeNativeValue::OptionalText(uri::fragment(value)))
        }
        _ => Err(unknown_operation(operation)),
    }
}

/// Dispatches an operation through handle-backed resource ownership.
///
/// Inputs:
/// - `store`: resource store owned by the native worker.
/// - `operation`: compiler-native operation id from `@compiler.native`.
/// - `args`: bridge-facing values where opaque adapter values are handles.
///
/// Output:
/// - `Ok(SafeNativeBridgeValue)` with opaque adapter outputs stored and
///   returned as handles.
/// - `Err(DispatchError)` for unknown operations, arity/type mismatches,
///   stale handles, resource kind mismatches, or adapter failures.
///
/// Transformation:
/// - Validates operation arity, decodes bridge handles into pure adapter
///   values, calls `dispatch`, and stores opaque adapter outputs back into the
///   resource store before returning handles.
pub fn dispatch_with_resources(
    store: &mut ResourceStore,
    operation: &str,
    args: &[SafeNativeBridgeValue],
) -> Result<SafeNativeBridgeValue, DispatchError> {
    validate_bridge_arity(operation, args)?;
    let decoded = decode_bridge_args(store, operation, args)?;
    let result = dispatch(operation, &decoded)?;
    encode_bridge_result(store, result)
}

/// Returns the expected arity for a supported operation.
///
/// Inputs:
/// - `operation`: compiler-native operation id.
///
/// Output:
/// - Expected runtime argument count, or `None` for an unknown operation.
///
/// Transformation:
/// - Maps operation ids to the same backend arities recorded in
///   `std/RUST_BACKED_MANIFEST.tsv`.
pub fn operation_arity(operation: &str) -> Option<usize> {
    match operation {
        "std.data.json.get" | "std.io.path.join" => Some(2),
        "std.data.json.parse"
        | "std.data.json.stringify"
        | "std.data.json.as_string"
        | "std.data.json.as_int"
        | "std.data.json.as_float"
        | "std.data.json.as_bool"
        | "std.data.json.is_null"
        | "std.encoding.base64.encode"
        | "std.encoding.base64.decode"
        | "std.encoding.base64.encode_url"
        | "std.encoding.base64.decode_url"
        | "std.io.path.from_string"
        | "std.io.path.to_string"
        | "std.io.path.file_name"
        | "std.io.path.extension"
        | "std.io.path.parent"
        | "std.io.path.is_absolute"
        | "std.net.uri.parse"
        | "std.net.uri.to_string"
        | "std.net.uri.scheme"
        | "std.net.uri.host"
        | "std.net.uri.path"
        | "std.net.uri.query"
        | "std.net.uri.fragment" => Some(1),
        _ => None,
    }
}

/// Validates argument count for one operation.
///
/// Inputs:
/// - `operation`: compiler-native operation id.
/// - `args`: neutral runtime values supplied by the bridge.
///
/// Output:
/// - `Ok(())` when arity matches.
/// - `Err(DispatchError)` for unknown operations or wrong arity.
///
/// Transformation:
/// - Compares supplied argument count with `operation_arity`.
fn validate_arity(operation: &str, args: &[SafeNativeValue]) -> Result<(), DispatchError> {
    match operation_arity(operation) {
        Some(expected) if expected == args.len() => Ok(()),
        Some(expected) => Err(DispatchError::new(
            "dispatch.arity",
            format!(
                "Operation `{operation}` expects {expected} argument(s), got {}.",
                args.len()
            ),
            0,
        )),
        None => Err(unknown_operation(operation)),
    }
}

/// Validates bridge argument count for one operation.
///
/// Inputs:
/// - `operation`: compiler-native operation id.
/// - `args`: bridge-facing values supplied by the worker boundary.
///
/// Output:
/// - `Ok(())` when arity matches.
/// - `Err(DispatchError)` for unknown operations or wrong arity.
///
/// Transformation:
/// - Compares supplied bridge argument count with `operation_arity`.
fn validate_bridge_arity(
    operation: &str,
    args: &[SafeNativeBridgeValue],
) -> Result<(), DispatchError> {
    match operation_arity(operation) {
        Some(expected) if expected == args.len() => Ok(()),
        Some(expected) => Err(DispatchError::new(
            "dispatch.arity",
            format!(
                "Operation `{operation}` expects {expected} argument(s), got {}.",
                args.len()
            ),
            0,
        )),
        None => Err(unknown_operation(operation)),
    }
}

/// Decodes bridge-facing arguments into pure dispatch values.
///
/// Inputs:
/// - `store`: resource store used to resolve opaque handles.
/// - `operation`: compiler-native operation id.
/// - `args`: bridge-facing operation arguments.
///
/// Output:
/// - Pure dispatch values suitable for `dispatch`.
/// - `Err(DispatchError)` when a handle is stale or has the wrong kind.
///
/// Transformation:
/// - Resolves handles according to the operation family and clones the
///   adapter-owned value for pure dispatch.
fn decode_bridge_args(
    store: &ResourceStore,
    operation: &str,
    args: &[SafeNativeBridgeValue],
) -> Result<Vec<SafeNativeValue>, DispatchError> {
    args.iter()
        .enumerate()
        .map(|(index, arg)| decode_bridge_arg(store, operation, index, arg))
        .collect()
}

/// Decodes one bridge-facing argument into a pure dispatch value.
///
/// Inputs:
/// - `store`: resource store used to resolve opaque handles.
/// - `operation`: compiler-native operation id.
/// - `index`: argument index for diagnostics.
/// - `arg`: bridge-facing argument.
///
/// Output:
/// - Pure dispatch value.
/// - `Err(DispatchError)` for unsupported bridge value shapes.
///
/// Transformation:
/// - Converts primitive bridge values directly and resolves handles to the
///   resource kind implied by the operation namespace.
fn decode_bridge_arg(
    store: &ResourceStore,
    operation: &str,
    index: usize,
    arg: &SafeNativeBridgeValue,
) -> Result<SafeNativeValue, DispatchError> {
    match arg {
        SafeNativeBridgeValue::Unit => Ok(SafeNativeValue::Unit),
        SafeNativeBridgeValue::Text(value) => Ok(SafeNativeValue::Text(value.clone())),
        SafeNativeBridgeValue::Int(value) => Ok(SafeNativeValue::Int(*value)),
        SafeNativeBridgeValue::Float(value) => Ok(SafeNativeValue::Float(*value)),
        SafeNativeBridgeValue::Bool(value) => Ok(SafeNativeValue::Bool(*value)),
        SafeNativeBridgeValue::Handle(handle) if operation.starts_with("std.data.json.") => store
            .json(*handle)
            .cloned()
            .map(SafeNativeValue::Json)
            .map_err(dispatch_resource_error),
        SafeNativeBridgeValue::Handle(handle) if operation.starts_with("std.io.path.") => store
            .path(*handle)
            .cloned()
            .map(SafeNativeValue::Path)
            .map_err(dispatch_resource_error),
        SafeNativeBridgeValue::Handle(handle) if operation.starts_with("std.net.uri.") => store
            .uri(*handle)
            .cloned()
            .map(SafeNativeValue::Uri)
            .map_err(dispatch_resource_error),
        SafeNativeBridgeValue::Handle(_) => Err(type_error(operation, index, "non-handle value")),
        SafeNativeBridgeValue::OptionalText(_) | SafeNativeBridgeValue::OptionalHandle(_) => {
            Err(type_error(operation, index, "non-optional argument"))
        }
    }
}

/// Encodes a pure dispatch result into a bridge-facing value.
///
/// Inputs:
/// - `store`: resource store that will own opaque adapter outputs.
/// - `value`: pure dispatch result.
///
/// Output:
/// - Bridge-facing result with opaque values represented as handles.
/// - `Err(DispatchError)` when resource insertion fails.
///
/// Transformation:
/// - Stores JSON/path/URI outputs in the resource store and returns only their
///   handles across the bridge surface.
fn encode_bridge_result(
    store: &mut ResourceStore,
    value: SafeNativeValue,
) -> Result<SafeNativeBridgeValue, DispatchError> {
    match value {
        SafeNativeValue::Unit => Ok(SafeNativeBridgeValue::Unit),
        SafeNativeValue::Text(value) => Ok(SafeNativeBridgeValue::Text(value)),
        SafeNativeValue::Int(value) => Ok(SafeNativeBridgeValue::Int(value)),
        SafeNativeValue::Float(value) => Ok(SafeNativeBridgeValue::Float(value)),
        SafeNativeValue::Bool(value) => Ok(SafeNativeBridgeValue::Bool(value)),
        SafeNativeValue::Json(value) => store
            .insert(ResourceValue::Json(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::Path(value) => store
            .insert(ResourceValue::Path(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::Uri(value) => store
            .insert(ResourceValue::Uri(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::OptionalText(value) => Ok(SafeNativeBridgeValue::OptionalText(value)),
        SafeNativeValue::OptionalPath(value) => value
            .map(|path| store.insert(ResourceValue::Path(path)))
            .transpose()
            .map(SafeNativeBridgeValue::OptionalHandle)
            .map_err(dispatch_resource_error),
    }
}

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
fn expect_text<'a>(
    operation: &str,
    args: &'a [SafeNativeValue],
    index: usize,
) -> Result<&'a str, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::Text(value)) => Ok(value),
        _ => Err(type_error(operation, index, "String")),
    }
}

/// Reads a JSON argument from a neutral value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied neutral values.
/// - `index`: expected JSON argument index.
///
/// Output:
/// - Borrowed JSON wrapper when the value is `Json`.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs a runtime shape check before adapter invocation.
fn expect_json<'a>(
    operation: &str,
    args: &'a [SafeNativeValue],
    index: usize,
) -> Result<&'a json::Json, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::Json(value)) => Ok(value),
        _ => Err(type_error(operation, index, "Json")),
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
fn expect_path<'a>(
    operation: &str,
    args: &'a [SafeNativeValue],
    index: usize,
) -> Result<&'a path::Path, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::Path(value)) => Ok(value),
        _ => Err(type_error(operation, index, "Path")),
    }
}

/// Reads a URI argument from a neutral value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied neutral values.
/// - `index`: expected URI argument index.
///
/// Output:
/// - Borrowed URI wrapper when the value is `Uri`.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs a runtime shape check before adapter invocation.
fn expect_uri<'a>(
    operation: &str,
    args: &'a [SafeNativeValue],
    index: usize,
) -> Result<&'a uri::Uri, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::Uri(value)) => Ok(value),
        _ => Err(type_error(operation, index, "Uri")),
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
fn unknown_operation(operation: &str) -> DispatchError {
    DispatchError::new(
        "dispatch.unknown_operation",
        format!("No SafeNative adapter is registered for `{operation}`."),
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
fn type_error(operation: &str, index: usize, expected: &str) -> DispatchError {
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
fn dispatch_json_error(error: json::JsonError) -> DispatchError {
    DispatchError::new(error.code(), error.message(), error.offset())
}

/// Converts a Base64 adapter error into a dispatch error.
///
/// Inputs:
/// - `error`: Base64 adapter error.
///
/// Output:
/// - Dispatch error preserving Base64 code, message, and offset.
///
/// Transformation:
/// - Erases the adapter-specific error type while preserving stable fields.
fn dispatch_base64_error(error: base64::Base64Error) -> DispatchError {
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
fn dispatch_path_error(error: path::PathError) -> DispatchError {
    DispatchError::new(error.code(), error.message(), error.offset())
}

/// Converts a URI adapter error into a dispatch error.
///
/// Inputs:
/// - `error`: URI adapter error.
///
/// Output:
/// - Dispatch error preserving URI code, message, and offset.
///
/// Transformation:
/// - Erases the adapter-specific error type while preserving stable fields.
fn dispatch_uri_error(error: uri::UriError) -> DispatchError {
    DispatchError::new(error.code(), error.message(), error.offset())
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
fn dispatch_resource_error(error: ResourceError) -> DispatchError {
    DispatchError::new(error.code(), error.message(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses Rust-backed std operations from the release manifest.
    ///
    /// Inputs:
    /// - Checked-in `std/RUST_BACKED_MANIFEST.tsv` embedded at compile time.
    ///
    /// Output:
    /// - Operation ids and arities from manifest rows.
    ///
    /// Transformation:
    /// - Skips comments/header lines, splits TSV rows, and keeps only rows
    ///   with a valid operation and integer arity.
    fn rust_backed_manifest_operations() -> Vec<(&'static str, usize)> {
        include_str!("../../../std/RUST_BACKED_MANIFEST.tsv")
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("module\t")
                {
                    return None;
                }
                let parts = line.split('\t').collect::<Vec<_>>();
                let operation = parts.get(3)?;
                let arity_text = parts.get(5)?;
                let arity = arity_text.parse::<usize>().ok()?;
                Some((*operation, arity))
            })
            .collect()
    }

    /// Dispatches an operation and returns a neutral value for tests.
    ///
    /// Inputs:
    /// - `operation`: compiler-native operation id expected to succeed.
    /// - `args`: neutral operation arguments.
    ///
    /// Output:
    /// - `Some(value)` when dispatch succeeds.
    /// - `None` after asserting failure is unexpected.
    ///
    /// Transformation:
    /// - Converts a dispatch result into an optional test value without
    ///   unwrap/expect.
    fn dispatch_ok(operation: &str, args: &[SafeNativeValue]) -> Option<SafeNativeValue> {
        let result = dispatch(operation, args);
        assert!(result.is_ok());
        result.ok()
    }

    /// Dispatches a bridge operation and returns a bridge value for tests.
    ///
    /// Inputs:
    /// - `store`: resource store used by the bridge dispatcher.
    /// - `operation`: compiler-native operation id expected to succeed.
    /// - `args`: bridge-facing operation arguments.
    ///
    /// Output:
    /// - `Some(value)` when dispatch succeeds.
    /// - `None` after asserting failure is unexpected.
    ///
    /// Transformation:
    /// - Converts a bridge dispatch result into an optional test value without
    ///   unwrap/expect.
    fn bridge_dispatch_ok(
        store: &mut ResourceStore,
        operation: &str,
        args: &[SafeNativeBridgeValue],
    ) -> Option<SafeNativeBridgeValue> {
        let result = dispatch_with_resources(store, operation, args);
        assert!(result.is_ok());
        result.ok()
    }

    /// Validates dispatcher arities against the Rust-backed std manifest.
    ///
    /// Inputs:
    /// - Checked-in manifest rows for Rust-backed std operations.
    ///
    /// Output:
    /// - Test passes when each manifest operation is known to dispatch.
    ///
    /// Transformation:
    /// - Compares the release manifest operation inventory to
    ///   `operation_arity` so dispatch cannot silently drift from std.
    #[test]
    fn operation_arities_cover_rust_backed_std_manifest() {
        let operations = rust_backed_manifest_operations();
        assert_eq!(operations.len(), 26);

        for (operation, arity) in operations {
            assert_eq!(operation_arity(operation), Some(arity), "{operation}");
        }
    }

    /// Validates manifest-backed dispatch arity rejects unsupported operations.
    ///
    /// Inputs:
    /// - Operation id absent from the manifest.
    ///
    /// Output:
    /// - Test passes when the dispatch table returns `None`.
    ///
    /// Transformation:
    /// - Guards the negative branch of the manifest-backed arity table.
    #[test]
    fn operation_arity_rejects_non_manifest_operation() {
        assert_eq!(operation_arity("std.nope.missing"), None);
    }

    /// Validates bridge JSON operations use opaque handles.
    ///
    /// Inputs:
    /// - JSON source text, an object key, and a bridge resource store.
    ///
    /// Output:
    /// - Test passes when parse/get return handles and accessor returns text.
    ///
    /// Transformation:
    /// - Exercises resource-backed dispatch without exposing Rust `Json`
    ///   values across the bridge-facing API.
    #[test]
    fn bridge_dispatch_json_returns_and_accepts_handles() {
        let mut store = ResourceStore::new();
        let Some(SafeNativeBridgeValue::Handle(root)) = bridge_dispatch_ok(
            &mut store,
            "std.data.json.parse",
            &[SafeNativeBridgeValue::Text(String::from(
                r#"{"name":"Ada"}"#,
            ))],
        ) else {
            return;
        };
        let Some(SafeNativeBridgeValue::Handle(name)) = bridge_dispatch_ok(
            &mut store,
            "std.data.json.get",
            &[
                SafeNativeBridgeValue::Handle(root),
                SafeNativeBridgeValue::Text(String::from("name")),
            ],
        ) else {
            return;
        };

        assert_eq!(
            dispatch_with_resources(
                &mut store,
                "std.data.json.as_string",
                &[SafeNativeBridgeValue::Handle(name)]
            ),
            Ok(SafeNativeBridgeValue::Text(String::from("Ada")))
        );
    }

    /// Validates bridge path operations use opaque handles.
    ///
    /// Inputs:
    /// - Path source text and child segment.
    ///
    /// Output:
    /// - Test passes when path outputs are handles and component access returns
    ///   optional text.
    ///
    /// Transformation:
    /// - Exercises resource-backed path parse/join/file-name dispatch.
    #[test]
    fn bridge_dispatch_path_returns_and_accepts_handles() {
        let mut store = ResourceStore::new();
        let Some(SafeNativeBridgeValue::Handle(base)) = bridge_dispatch_ok(
            &mut store,
            "std.io.path.from_string",
            &[SafeNativeBridgeValue::Text(String::from("src"))],
        ) else {
            return;
        };
        let Some(SafeNativeBridgeValue::Handle(joined)) = bridge_dispatch_ok(
            &mut store,
            "std.io.path.join",
            &[
                SafeNativeBridgeValue::Handle(base),
                SafeNativeBridgeValue::Text(String::from("main.terl")),
            ],
        ) else {
            return;
        };

        assert_eq!(
            dispatch_with_resources(
                &mut store,
                "std.io.path.file_name",
                &[SafeNativeBridgeValue::Handle(joined)]
            ),
            Ok(SafeNativeBridgeValue::OptionalText(Some(String::from(
                "main.terl"
            ))))
        );
    }

    /// Validates bridge URI operations use opaque handles.
    ///
    /// Inputs:
    /// - URI source text.
    ///
    /// Output:
    /// - Test passes when parse returns a handle and component access accepts
    ///   that handle.
    ///
    /// Transformation:
    /// - Exercises resource-backed URI parse and component dispatch.
    #[test]
    fn bridge_dispatch_uri_returns_and_accepts_handles() {
        let mut store = ResourceStore::new();
        let Some(SafeNativeBridgeValue::Handle(uri)) = bridge_dispatch_ok(
            &mut store,
            "std.net.uri.parse",
            &[SafeNativeBridgeValue::Text(String::from(
                "https://example.com/docs",
            ))],
        ) else {
            return;
        };

        assert_eq!(
            dispatch_with_resources(
                &mut store,
                "std.net.uri.host",
                &[SafeNativeBridgeValue::Handle(uri)]
            ),
            Ok(SafeNativeBridgeValue::OptionalText(Some(String::from(
                "example.com"
            ))))
        );
    }

    /// Validates bridge dispatch rejects stale resource handles.
    ///
    /// Inputs:
    /// - JSON parse output handle that is disposed before use.
    ///
    /// Output:
    /// - Test passes when later accessor dispatch returns `resource.stale_handle`.
    ///
    /// Transformation:
    /// - Exercises resource liveness before adapter invocation.
    #[test]
    fn bridge_dispatch_rejects_stale_handle_with_stable_error_code() {
        let mut store = ResourceStore::new();
        let Some(SafeNativeBridgeValue::Handle(root)) = bridge_dispatch_ok(
            &mut store,
            "std.data.json.parse",
            &[SafeNativeBridgeValue::Text(String::from("null"))],
        ) else {
            return;
        };
        assert_eq!(store.dispose(root), Ok(()));

        let error = dispatch_with_resources(
            &mut store,
            "std.data.json.is_null",
            &[SafeNativeBridgeValue::Handle(root)],
        )
        .err()
        .unwrap_or_else(|| DispatchError::new("missing", "", 0));
        assert_eq!(error.code(), "resource.stale_handle");
    }

    /// Validates JSON parse, object lookup, and string accessor dispatch.
    ///
    /// Inputs:
    /// - JSON source text and object key.
    ///
    /// Output:
    /// - Test passes when dispatcher chains through JSON adapter functions.
    ///
    /// Transformation:
    /// - Exercises JSON operations through operation ids rather than direct
    ///   adapter calls.
    #[test]
    fn dispatches_json_parse_get_and_as_string() {
        let Some(SafeNativeValue::Json(root)) = dispatch_ok(
            "std.data.json.parse",
            &[SafeNativeValue::Text(String::from(r#"{"name":"Ada"}"#))],
        ) else {
            return;
        };
        let Some(SafeNativeValue::Json(name)) = dispatch_ok(
            "std.data.json.get",
            &[
                SafeNativeValue::Json(root),
                SafeNativeValue::Text(String::from("name")),
            ],
        ) else {
            return;
        };

        assert_eq!(
            dispatch("std.data.json.as_string", &[SafeNativeValue::Json(name)]),
            Ok(SafeNativeValue::Text(String::from("Ada")))
        );
    }

    /// Validates Base64 dispatch over standard encode/decode operations.
    ///
    /// Inputs:
    /// - Plain UTF-8 text.
    ///
    /// Output:
    /// - Test passes when encode and decode preserve the text.
    ///
    /// Transformation:
    /// - Routes Base64 operations through the shared dispatcher.
    #[test]
    fn dispatches_base64_round_trip() {
        let Some(SafeNativeValue::Text(encoded)) = dispatch_ok(
            "std.encoding.base64.encode",
            &[SafeNativeValue::Text(String::from("hello Terlan"))],
        ) else {
            return;
        };

        assert_eq!(
            dispatch(
                "std.encoding.base64.decode",
                &[SafeNativeValue::Text(encoded)]
            ),
            Ok(SafeNativeValue::Text(String::from("hello Terlan")))
        );
    }

    /// Validates lexical path dispatch over parse, join, and component access.
    ///
    /// Inputs:
    /// - Base path and child path text.
    ///
    /// Output:
    /// - Test passes when joined path exposes the expected final component.
    ///
    /// Transformation:
    /// - Routes path operations through the shared dispatcher.
    #[test]
    fn dispatches_path_join_and_file_name() {
        let Some(SafeNativeValue::Path(base)) = dispatch_ok(
            "std.io.path.from_string",
            &[SafeNativeValue::Text(String::from("src"))],
        ) else {
            return;
        };
        let Some(SafeNativeValue::Path(joined)) = dispatch_ok(
            "std.io.path.join",
            &[
                SafeNativeValue::Path(base),
                SafeNativeValue::Text(String::from("main.terl")),
            ],
        ) else {
            return;
        };

        assert_eq!(
            dispatch("std.io.path.file_name", &[SafeNativeValue::Path(joined)]),
            Ok(SafeNativeValue::OptionalText(Some(String::from(
                "main.terl"
            ))))
        );
    }

    /// Validates URI dispatch over parse and component accessors.
    ///
    /// Inputs:
    /// - HTTPS URI source text.
    ///
    /// Output:
    /// - Test passes when component accessors return stable values.
    ///
    /// Transformation:
    /// - Routes URI operations through the shared dispatcher.
    #[test]
    fn dispatches_uri_components() {
        let Some(SafeNativeValue::Uri(uri)) = dispatch_ok(
            "std.net.uri.parse",
            &[SafeNativeValue::Text(String::from(
                "https://example.com/docs?q=terlan",
            ))],
        ) else {
            return;
        };

        assert_eq!(
            dispatch("std.net.uri.scheme", &[SafeNativeValue::Uri(uri.clone())]),
            Ok(SafeNativeValue::Text(String::from("https")))
        );
        assert_eq!(
            dispatch("std.net.uri.host", &[SafeNativeValue::Uri(uri)]),
            Ok(SafeNativeValue::OptionalText(Some(String::from(
                "example.com"
            ))))
        );
    }

    /// Validates stable wrong-arity errors.
    ///
    /// Inputs:
    /// - Operation id with no supplied arguments.
    ///
    /// Output:
    /// - Test passes when the error uses `dispatch.arity`.
    ///
    /// Transformation:
    /// - Exercises the dispatcher argument-count guard before adapter calls.
    #[test]
    fn rejects_wrong_arity_with_stable_error_code() {
        let error = dispatch("std.data.json.parse", &[])
            .err()
            .unwrap_or_else(|| DispatchError::new("missing", "", 0));

        assert_eq!(error.code(), "dispatch.arity");
    }

    /// Validates stable wrong-type errors.
    ///
    /// Inputs:
    /// - JSON accessor with a text value instead of a JSON value.
    ///
    /// Output:
    /// - Test passes when the error uses `dispatch.type`.
    ///
    /// Transformation:
    /// - Exercises runtime argument shape validation before adapter calls.
    #[test]
    fn rejects_wrong_type_with_stable_error_code() {
        let error = dispatch(
            "std.data.json.as_string",
            &[SafeNativeValue::Text(String::from("not json"))],
        )
        .err()
        .unwrap_or_else(|| DispatchError::new("missing", "", 0));

        assert_eq!(error.code(), "dispatch.type");
    }

    /// Validates stable unknown-operation errors.
    ///
    /// Inputs:
    /// - Unsupported operation id.
    ///
    /// Output:
    /// - Test passes when the error uses `dispatch.unknown_operation`.
    ///
    /// Transformation:
    /// - Exercises dispatch-table miss handling.
    #[test]
    fn rejects_unknown_operation_with_stable_error_code() {
        let error = dispatch("std.unknown.nope", &[])
            .err()
            .unwrap_or_else(|| DispatchError::new("missing", "", 0));

        assert_eq!(error.code(), "dispatch.unknown_operation");
    }
}
