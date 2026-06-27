use crate::terlan_native::vector;
use crate::terlan_safenative::resource::{ResourceStore, ResourceValue};

use super::args::{
    dispatch_http_error, dispatch_resource_error, dispatch_vector_error, expect_bridge_bool,
    expect_bridge_handle, expect_bridge_int, expect_bridge_list, expect_bridge_text, type_error,
    unknown_operation,
};
use super::{dispatch, operation_arity, DispatchError, SafeNativeBridgeValue, SafeNativeValue};

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
    if operation == "std.http.cookies.set" {
        return dispatch_cookie_set_with_resources(store, operation, args);
    }
    if operation == "std.http.cookies.delete" {
        return dispatch_cookie_delete_with_resources(store, operation, args);
    }
    if operation.starts_with("std.native.collections.vector.") {
        return dispatch_native_vector_with_resources(store, operation, args);
    }
    let decoded = decode_bridge_args(store, operation, args)?;
    let result = dispatch(operation, &decoded)?;
    encode_bridge_result(store, result)
}

/// Dispatches a native vector operation through resource ownership.
///
/// Inputs:
/// - `store`: resource registry owning vector handles.
/// - `operation`: compiler-native vector operation id.
/// - `args`: bridge-facing vector arguments.
///
/// Output:
/// - Bridge value result for the vector operation.
/// - `DispatchError` for bad arity, bad handle, bad argument, or vector
///   bounds failures.
///
/// Transformation:
/// - Allocates, reads, or mutates Rust-owned vector resources while preserving
///   stable opaque handles for BEAM-side code.
fn dispatch_native_vector_with_resources(
    store: &mut ResourceStore,
    operation: &str,
    args: &[SafeNativeBridgeValue],
) -> Result<SafeNativeBridgeValue, DispatchError> {
    match operation {
        "std.native.collections.vector.new" => store
            .insert(ResourceValue::NativeVector(vector::new()))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        "std.native.collections.vector.from_list" => {
            let values = expect_bridge_list(operation, args, 0)?;
            store
                .insert(ResourceValue::NativeVector(vector::from_list(
                    values.to_vec(),
                )))
                .map(SafeNativeBridgeValue::Handle)
                .map_err(dispatch_resource_error)
        }
        "std.native.collections.vector.length" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            vector::length(
                store
                    .native_vector(handle)
                    .map_err(dispatch_resource_error)?,
            )
            .map(SafeNativeBridgeValue::Int)
            .map_err(dispatch_vector_error)
        }
        "std.native.collections.vector.get_at" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            let index = expect_bridge_int(operation, args, 1)?;
            vector::get_at(
                store
                    .native_vector(handle)
                    .map_err(dispatch_resource_error)?,
                index,
            )
            .map_err(dispatch_vector_error)
        }
        "std.native.collections.vector.set_at" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            let index = expect_bridge_int(operation, args, 1)?;
            let value = args
                .get(2)
                .cloned()
                .ok_or_else(|| type_error(operation, 2, "value"))?;
            vector::set_at(
                store
                    .native_vector_mut(handle)
                    .map_err(dispatch_resource_error)?,
                index,
                value,
            )
            .map_err(dispatch_vector_error)?;
            Ok(SafeNativeBridgeValue::Handle(handle))
        }
        "std.native.collections.vector.swap" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            let left = expect_bridge_int(operation, args, 1)?;
            let right = expect_bridge_int(operation, args, 2)?;
            vector::swap(
                store
                    .native_vector_mut(handle)
                    .map_err(dispatch_resource_error)?,
                left,
                right,
            )
            .map_err(dispatch_vector_error)?;
            Ok(SafeNativeBridgeValue::Handle(handle))
        }
        "std.native.collections.vector.push" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            let value = args
                .get(1)
                .cloned()
                .ok_or_else(|| type_error(operation, 1, "value"))?;
            vector::push(
                store
                    .native_vector_mut(handle)
                    .map_err(dispatch_resource_error)?,
                value,
            );
            Ok(SafeNativeBridgeValue::Handle(handle))
        }
        "std.native.collections.vector.to_list" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            store
                .native_vector(handle)
                .map(|vector| SafeNativeBridgeValue::List(vector::to_list(vector)))
                .map_err(dispatch_resource_error)
        }
        _ => Err(unknown_operation(operation)),
    }
}

/// Mutates a cookie jar resource through `std.http.cookies.set`.
///
/// Inputs:
/// - `store`: resource registry owning the cookie jar.
/// - `operation`: compiler-native operation id used in diagnostics.
/// - `args`: bridge arguments containing jar handle and cookie values.
///
/// Output:
/// - `Unit` when the cookie mutation is recorded.
/// - `DispatchError` for bad handle, argument, or cookie validation failures.
///
/// Transformation:
/// - Borrows the jar mutably from the resource store and appends one
///   `Set-Cookie` mutation without cloning the jar.
fn dispatch_cookie_set_with_resources(
    store: &mut ResourceStore,
    operation: &str,
    args: &[SafeNativeBridgeValue],
) -> Result<SafeNativeBridgeValue, DispatchError> {
    let handle = expect_bridge_handle(operation, args, 0)?;
    let name = expect_bridge_text(operation, args, 1)?;
    let value = expect_bridge_text(operation, args, 2)?;
    let path = expect_bridge_text(operation, args, 3)?;
    let http_only = expect_bridge_bool(operation, args, 4)?;
    let secure = expect_bridge_bool(operation, args, 5)?;
    store
        .http_cookie_jar_mut(handle)
        .map_err(dispatch_resource_error)?
        .set(name, value, path, http_only, secure)
        .map_err(dispatch_http_error)?;
    Ok(SafeNativeBridgeValue::Unit)
}

/// Mutates a cookie jar resource through `std.http.cookies.delete`.
///
/// Inputs:
/// - `store`: resource registry owning the cookie jar.
/// - `operation`: compiler-native operation id used in diagnostics.
/// - `args`: bridge arguments containing jar handle, cookie name, and path.
///
/// Output:
/// - `Unit` when the deletion mutation is recorded.
/// - `DispatchError` for bad handle, argument, or cookie validation failures.
///
/// Transformation:
/// - Borrows the jar mutably from the resource store and appends one expiring
///   `Set-Cookie` mutation without cloning the jar.
fn dispatch_cookie_delete_with_resources(
    store: &mut ResourceStore,
    operation: &str,
    args: &[SafeNativeBridgeValue],
) -> Result<SafeNativeBridgeValue, DispatchError> {
    let handle = expect_bridge_handle(operation, args, 0)?;
    let name = expect_bridge_text(operation, args, 1)?;
    let path = expect_bridge_text(operation, args, 2)?;
    store
        .http_cookie_jar_mut(handle)
        .map_err(dispatch_resource_error)?
        .delete(name, path)
        .map_err(dispatch_http_error)?;
    Ok(SafeNativeBridgeValue::Unit)
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
        SafeNativeBridgeValue::PostgresConfig(value) => {
            Ok(SafeNativeValue::PostgresConfig(value.clone()))
        }
        SafeNativeBridgeValue::Handle(handle) if operation == "std.http.response.json" => store
            .json(*handle)
            .cloned()
            .map(SafeNativeValue::Json)
            .map_err(dispatch_resource_error),
        SafeNativeBridgeValue::Handle(handle) if operation.starts_with("std.data.json.") => store
            .json(*handle)
            .cloned()
            .map(SafeNativeValue::Json)
            .map_err(dispatch_resource_error),
        SafeNativeBridgeValue::Handle(handle) if operation.starts_with("std.http.request.") => {
            store
                .http_request(*handle)
                .cloned()
                .map(SafeNativeValue::HttpRequest)
                .map_err(dispatch_resource_error)
        }
        SafeNativeBridgeValue::Handle(handle) if operation.starts_with("std.http.cookies.") => {
            store
                .http_cookie_jar(*handle)
                .cloned()
                .map(SafeNativeValue::HttpCookieJar)
                .map_err(dispatch_resource_error)
        }
        SafeNativeBridgeValue::Handle(handle) if operation.starts_with("std.http.response.") => {
            store
                .http_response(*handle)
                .cloned()
                .map(SafeNativeValue::HttpResponse)
                .map_err(dispatch_resource_error)
        }
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
        SafeNativeBridgeValue::Handle(handle)
            if matches!(
                operation,
                "std.db.postgres.query"
                    | "std.db.postgres.query_one"
                    | "std.db.postgres.execute"
                    | "std.db.postgres.transaction"
            ) && index == 0 =>
        {
            store
                .postgres_pool(*handle)
                .cloned()
                .map(SafeNativeValue::PostgresPool)
                .map_err(dispatch_resource_error)
        }
        SafeNativeBridgeValue::Handle(handle)
            if matches!(
                operation,
                "std.db.postgres.string"
                    | "std.db.postgres.int"
                    | "std.db.postgres.bool"
                    | "std.db.postgres.json"
            ) && index == 0 =>
        {
            store
                .postgres_row(*handle)
                .cloned()
                .map(SafeNativeValue::PostgresRow)
                .map_err(dispatch_resource_error)
        }
        SafeNativeBridgeValue::Handle(_) => Err(type_error(operation, index, "non-handle value")),
        SafeNativeBridgeValue::OptionalText(_) | SafeNativeBridgeValue::OptionalHandle(_) => {
            Err(type_error(operation, index, "non-optional argument"))
        }
        SafeNativeBridgeValue::List(values) => Ok(SafeNativeValue::JsonList(
            values
                .iter()
                .enumerate()
                .map(|(list_index, value)| match value {
                    SafeNativeBridgeValue::Handle(handle) => store
                        .json(*handle)
                        .cloned()
                        .map_err(dispatch_resource_error),
                    _ => Err(type_error(operation, list_index, "Json handle")),
                })
                .collect::<Result<Vec<_>, _>>()?,
        )),
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
        SafeNativeValue::HttpRequest(value) => store
            .insert(ResourceValue::HttpRequest(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::HttpResponse(value) => store
            .insert(ResourceValue::HttpResponse(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::HttpCookieJar(value) => store
            .insert(ResourceValue::HttpCookieJar(value))
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
        SafeNativeValue::PostgresPool(value) => store
            .insert(ResourceValue::PostgresPool(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::PostgresRow(value) => store
            .insert(ResourceValue::PostgresRow(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::PostgresRows(values) => values
            .into_iter()
            .map(|row| {
                store
                    .insert(ResourceValue::PostgresRow(row))
                    .map(SafeNativeBridgeValue::Handle)
                    .map_err(dispatch_resource_error)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(SafeNativeBridgeValue::List),
        SafeNativeValue::OptionalPostgresRow(value) => value
            .map(|row| store.insert(ResourceValue::PostgresRow(row)))
            .transpose()
            .map(SafeNativeBridgeValue::OptionalHandle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::PostgresConfig(_) | SafeNativeValue::JsonList(_) => {
            Err(DispatchError::new(
                "dispatch.postgres_requires_runtime_bridge",
                "Postgres input-only values cannot be returned across the runtime bridge.",
                0,
            ))
        }
        SafeNativeValue::OptionalText(value) => Ok(SafeNativeBridgeValue::OptionalText(value)),
        SafeNativeValue::OptionalPath(value) => value
            .map(|path| store.insert(ResourceValue::Path(path)))
            .transpose()
            .map(SafeNativeBridgeValue::OptionalHandle)
            .map_err(dispatch_resource_error),
    }
}
