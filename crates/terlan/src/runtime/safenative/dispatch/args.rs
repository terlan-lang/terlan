use crate::terlan_native::{base64, http, json, path, postgres, uri, vector};
use crate::terlan_safenative::handle::SafeNativeHandle;
use crate::terlan_safenative::resource::ResourceError;

use super::{DispatchError, SafeNativeBridgeValue, SafeNativeValue};

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
    args: &'a [SafeNativeValue],
    index: usize,
) -> Result<&'a str, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::Text(value)) => Ok(value),
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
    args: &[SafeNativeValue],
    index: usize,
) -> Result<bool, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::Bool(value)) => Ok(*value),
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
    args: &[SafeNativeValue],
    index: usize,
) -> Result<i64, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::Int(value)) => Ok(*value),
        _ => Err(type_error(operation, index, "Int")),
    }
}

/// Reads a floating-point argument from a neutral value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied neutral values.
/// - `index`: expected floating-point argument index.
///
/// Output:
/// - Floating-point value when the neutral value is `Float`.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs a runtime shape check before adapter invocation.
pub(super) fn expect_float(
    operation: &str,
    args: &[SafeNativeValue],
    index: usize,
) -> Result<f64, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::Float(value)) => Ok(*value),
        _ => Err(type_error(operation, index, "Float")),
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
pub(super) fn expect_json<'a>(
    operation: &str,
    args: &'a [SafeNativeValue],
    index: usize,
) -> Result<&'a json::Json, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::Json(value)) => Ok(value),
        _ => Err(type_error(operation, index, "Json")),
    }
}

/// Reads an HTTP request argument from a neutral value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied neutral values.
/// - `index`: expected HTTP request argument index.
///
/// Output:
/// - Borrowed HTTP request wrapper when the value is `HttpRequest`.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs a runtime shape check before adapter invocation.
pub(super) fn expect_http_request<'a>(
    operation: &str,
    args: &'a [SafeNativeValue],
    index: usize,
) -> Result<&'a http::Request, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::HttpRequest(value)) => Ok(value),
        _ => Err(type_error(operation, index, "HttpRequest")),
    }
}

/// Reads an HTTP cookie jar argument from a neutral value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied neutral values.
/// - `index`: expected HTTP cookie jar argument index.
///
/// Output:
/// - Borrowed HTTP cookie jar when the value is `HttpCookieJar`.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs a runtime shape check before adapter invocation.
pub(super) fn expect_http_cookie_jar<'a>(
    operation: &str,
    args: &'a [SafeNativeValue],
    index: usize,
) -> Result<&'a http::CookieJar, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::HttpCookieJar(value)) => Ok(value),
        _ => Err(type_error(operation, index, "HttpCookieJar")),
    }
}

/// Builds cookie options from explicit dispatch arguments.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: neutral dispatch arguments for `set_header_with_options`.
///
/// Output:
/// - Rust `CookieOptions` ready for HTTP adapter serialization.
/// - `Err(DispatchError)` when argument shapes or SameSite text are invalid.
///
/// Transformation:
/// - Converts the source-visible full cookie helper into the typed Rust option
///   struct while using empty strings for absent optional text attributes and
///   an explicit `include_max_age` flag until Terlan records cross SafeNative.
pub(super) fn cookie_options_from_args(
    operation: &str,
    args: &[SafeNativeValue],
) -> Result<http::CookieOptions, DispatchError> {
    let path = expect_text(operation, args, 2)?.to_string();
    let domain = optional_text_arg(operation, args, 3)?;
    let max_age = optional_included_int_arg(operation, args, 4, 5)?;
    let expires = optional_text_arg(operation, args, 6)?;
    let http_only = expect_bool(operation, args, 7)?;
    let secure = expect_bool(operation, args, 8)?;
    let same_site = optional_same_site_arg(operation, args, 9)?;

    Ok(http::CookieOptions {
        path,
        domain,
        max_age,
        expires,
        http_only,
        secure,
        same_site,
    })
}

/// Converts an empty-stringable text argument into an optional string.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: neutral dispatch arguments.
/// - `index`: argument index to read.
///
/// Output:
/// - `None` for an empty string, otherwise `Some(value)`.
/// - `Err(DispatchError)` when the argument is not text.
///
/// Transformation:
/// - Preserves a compact primitive SafeNative ABI while representing absent
///   optional cookie string attributes.
fn optional_text_arg(
    operation: &str,
    args: &[SafeNativeValue],
    index: usize,
) -> Result<Option<String>, DispatchError> {
    let value = expect_text(operation, args, index)?;
    Ok((!value.is_empty()).then(|| value.to_string()))
}

/// Converts an explicit inclusion flag and integer into an optional integer.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: neutral dispatch arguments.
/// - `value_index`: integer argument index to read.
/// - `include_index`: boolean inclusion flag index to read.
///
/// Output:
/// - `None` when the include flag is false, otherwise `Some(value)`.
/// - `Err(DispatchError)` when either argument has the wrong shape.
///
/// Transformation:
/// - Encodes optional `Max-Age` without requiring an `Option[Int]` bridge value
///   or a non-constant sentinel default in source.
fn optional_included_int_arg(
    operation: &str,
    args: &[SafeNativeValue],
    value_index: usize,
    include_index: usize,
) -> Result<Option<i64>, DispatchError> {
    let value = expect_int(operation, args, value_index)?;
    let include = expect_bool(operation, args, include_index)?;
    Ok(include.then_some(value))
}

/// Converts optional SameSite text into the typed cookie policy.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: neutral dispatch arguments.
/// - `index`: argument index to read.
///
/// Output:
/// - Parsed SameSite policy, or `None` for an empty string.
/// - `Err(DispatchError)` for unsupported policy text.
///
/// Transformation:
/// - Keeps policy validation in the dispatch layer so the HTTP serializer only
///   receives typed cookie policy values.
fn optional_same_site_arg(
    operation: &str,
    args: &[SafeNativeValue],
    index: usize,
) -> Result<Option<http::CookieSameSite>, DispatchError> {
    match expect_text(operation, args, index)?.to_ascii_lowercase().as_str() {
        "" => Ok(None),
        "lax" => Ok(Some(http::CookieSameSite::Lax)),
        "strict" => Ok(Some(http::CookieSameSite::Strict)),
        "none" => Ok(Some(http::CookieSameSite::None)),
        other => Err(DispatchError::new(
            "dispatch.http.cookie.invalid_same_site",
            format!(
                "Operation `{operation}` expected SameSite value `lax`, `strict`, `none`, or empty string, got `{other}`."
            ),
            0,
        )),
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
/// - SafeNative handle when present.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs the bridge-side shape check required before mutable resource
///   borrowing.
pub(super) fn expect_bridge_handle(
    operation: &str,
    args: &[SafeNativeBridgeValue],
    index: usize,
) -> Result<SafeNativeHandle, DispatchError> {
    match args.get(index) {
        Some(SafeNativeBridgeValue::Handle(value)) => Ok(*value),
        _ => Err(type_error(operation, index, "Handle")),
    }
}

/// Reads a text argument from a bridge value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied bridge values.
/// - `index`: expected text argument index.
///
/// Output:
/// - Borrowed string slice when present.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs the bridge-side shape check required before mutable resource
///   operations.
pub(super) fn expect_bridge_text<'a>(
    operation: &str,
    args: &'a [SafeNativeBridgeValue],
    index: usize,
) -> Result<&'a str, DispatchError> {
    match args.get(index) {
        Some(SafeNativeBridgeValue::Text(value)) => Ok(value),
        _ => Err(type_error(operation, index, "String")),
    }
}

/// Reads a boolean argument from a bridge value slice.
///
/// Inputs:
/// - `operation`: operation id used in diagnostics.
/// - `args`: supplied bridge values.
/// - `index`: expected boolean argument index.
///
/// Output:
/// - Boolean value when present.
/// - `Err(DispatchError)` when another value kind is present.
///
/// Transformation:
/// - Performs the bridge-side shape check required before mutable resource
///   operations.
pub(super) fn expect_bridge_bool(
    operation: &str,
    args: &[SafeNativeBridgeValue],
    index: usize,
) -> Result<bool, DispatchError> {
    match args.get(index) {
        Some(SafeNativeBridgeValue::Bool(value)) => Ok(*value),
        _ => Err(type_error(operation, index, "Bool")),
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
    args: &[SafeNativeBridgeValue],
    index: usize,
) -> Result<i64, DispatchError> {
    match args.get(index) {
        Some(SafeNativeBridgeValue::Int(value)) => Ok(*value),
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
/// - Keeps constructor/list conversion validation at the SafeNative boundary
///   before resource allocation.
pub(super) fn expect_bridge_list<'a>(
    operation: &str,
    args: &'a [SafeNativeBridgeValue],
    index: usize,
) -> Result<&'a [SafeNativeBridgeValue], DispatchError> {
    match args.get(index) {
        Some(SafeNativeBridgeValue::List(values)) => Ok(values),
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
pub(super) fn expect_uri<'a>(
    operation: &str,
    args: &'a [SafeNativeValue],
    index: usize,
) -> Result<&'a uri::Uri, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::Uri(value)) => Ok(value),
        _ => Err(type_error(operation, index, "Uri")),
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
    args: &'a [SafeNativeValue],
    index: usize,
) -> Result<&'a postgres::Config, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::PostgresConfig(value)) => Ok(value),
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
    args: &'a [SafeNativeValue],
    index: usize,
) -> Result<&'a postgres::Pool, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::PostgresPool(value)) => Ok(value),
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
    args: &'a [SafeNativeValue],
    index: usize,
) -> Result<&'a postgres::Row, DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::PostgresRow(value)) => Ok(value),
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
    args: &'a [SafeNativeValue],
    index: usize,
) -> Result<&'a [json::Json], DispatchError> {
    match args.get(index) {
        Some(SafeNativeValue::JsonList(value)) => Ok(value),
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

/// Converts an HTTP adapter error into a dispatch error.
///
/// Inputs:
/// - `error`: HTTP adapter error.
///
/// Output:
/// - Dispatch error preserving HTTP code and message.
///
/// Transformation:
/// - Erases the adapter-specific error type while preserving stable fields
///   relevant to the generic SafeNative dispatch layer.
pub(super) fn dispatch_http_error(error: http::HttpError) -> DispatchError {
    DispatchError::new(error.code(), error.message(), 0)
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
pub(super) fn dispatch_base64_error(error: base64::Base64Error) -> DispatchError {
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
pub(super) fn dispatch_uri_error(error: uri::UriError) -> DispatchError {
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
/// - Reuses the stable SafeNative dispatch error envelope for vector resource
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
///   relevant to the generic SafeNative dispatch layer.
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
