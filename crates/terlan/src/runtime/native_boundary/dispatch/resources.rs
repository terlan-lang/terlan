use crate::terlan_native::vector;
use crate::terlan_native_boundary::cancellation::NativeBoundaryCancellationToken;
use crate::terlan_native_boundary::metadata::NativeBoundaryWorkerClass;
use crate::terlan_native_boundary::resource::{
    ResourceStore, ResourceValue, SYSTEM_RESOURCE_OWNER,
};

use super::args::{
    dispatch_http_error, dispatch_resource_error, dispatch_vector_error, expect_bridge_bool,
    expect_bridge_handle, expect_bridge_int, expect_bridge_list, expect_bridge_text, type_error,
    unknown_operation,
};
use super::manifest::validate_native_boundary_dispatch;
use super::panic_boundary::catch_native_boundary_panic;
use super::{
    dispatch, validate_operation_arity, DispatchError, NativeBoundaryBridgeValue,
    NativeBoundaryValue,
};

/// Dispatches an operation through handle-backed resource ownership.
///
/// Inputs:
/// - `store`: resource store owned by the native worker.
/// - `operation`: compiler-native operation id from `@compiler.native`.
/// - `args`: bridge-facing values where opaque adapter values are handles.
///
/// Output:
/// - `Ok(NativeBoundaryBridgeValue)` with opaque adapter outputs stored and
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
    args: &[NativeBoundaryBridgeValue],
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    dispatch_with_resources_authorized(
        store,
        SYSTEM_RESOURCE_OWNER,
        None,
        None,
        operation,
        args,
        None,
    )
}

/// Dispatches an operation for one VM process with resource-owner enforcement.
pub fn dispatch_with_resources_for_process(
    store: &mut ResourceStore,
    caller_process_id: u64,
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    dispatch_with_resources_for_process_with_capabilities(
        store,
        caller_process_id,
        &[],
        operation,
        args,
    )
}

/// Dispatches for one VM process after validating its granted capabilities.
pub fn dispatch_with_resources_for_process_with_capabilities(
    store: &mut ResourceStore,
    caller_process_id: u64,
    granted_capabilities: &[&str],
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    dispatch_with_resources_authorized(
        store,
        caller_process_id,
        Some(granted_capabilities),
        Some(&[]),
        operation,
        args,
        None,
    )
}

/// Dispatches for one process with capability and scheduler-class admission.
pub fn dispatch_with_resources_for_process_with_policy(
    store: &mut ResourceStore,
    caller_process_id: u64,
    granted_capabilities: &[&str],
    admitted_worker_classes: &[NativeBoundaryWorkerClass],
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    dispatch_with_resources_authorized(
        store,
        caller_process_id,
        Some(granted_capabilities),
        Some(admitted_worker_classes),
        operation,
        args,
        None,
    )
}

/// Dispatches an admitted operation while observing cooperative cancellation.
pub fn dispatch_with_resources_for_process_with_policy_and_cancellation(
    store: &mut ResourceStore,
    caller_process_id: u64,
    granted_capabilities: &[&str],
    admitted_worker_classes: &[NativeBoundaryWorkerClass],
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
    cancellation: &NativeBoundaryCancellationToken,
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    dispatch_with_resources_authorized(
        store,
        caller_process_id,
        Some(granted_capabilities),
        Some(admitted_worker_classes),
        operation,
        args,
        Some(cancellation),
    )
}

fn dispatch_with_resources_authorized(
    store: &mut ResourceStore,
    caller_process_id: u64,
    granted_capabilities: Option<&[&str]>,
    admitted_worker_classes: Option<&[NativeBoundaryWorkerClass]>,
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
    cancellation: Option<&NativeBoundaryCancellationToken>,
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    validate_native_boundary_dispatch(
        operation,
        args,
        granted_capabilities,
        admitted_worker_classes,
    )?;
    validate_bridge_arity(operation, args)?;
    validate_bridge_resource_owners(store, caller_process_id, args)?;
    reject_cancelled(cancellation)?;
    catch_native_boundary_panic(operation, || {
        let result =
            execute_resource_dispatch(store, caller_process_id, operation, args, cancellation)?;
        reject_cancelled(cancellation)?;
        Ok(result)
    })
}

/// Converts a cooperative cancellation observation into the stable boundary error.
fn reject_cancelled(
    cancellation: Option<&NativeBoundaryCancellationToken>,
) -> Result<(), DispatchError> {
    if cancellation.is_some_and(NativeBoundaryCancellationToken::is_cancelled) {
        let error = crate::terlan_native_boundary::error::error_for(
            crate::terlan_native_boundary::error::ErrorKind::Cancelled,
        );
        return Err(DispatchError::new(error.code, error.message, 0));
    }
    Ok(())
}

fn execute_resource_dispatch(
    store: &mut ResourceStore,
    caller_process_id: u64,
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
    cancellation: Option<&NativeBoundaryCancellationToken>,
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    if operation == "std.data.json.array_extend" {
        return dispatch_json_array_extend_with_resources(store, operation, args);
    }
    if operation == "std.data.json.array_push" {
        return dispatch_json_array_push_with_resources(store, operation, args);
    }
    if operation == "std.data.json.array_set" {
        return dispatch_json_array_set_with_resources(store, operation, args);
    }
    if operation == "std.data.json.object_put" {
        return dispatch_json_object_put_with_resources(store, operation, args);
    }
    if operation == "std.data.json.object_remove" {
        return dispatch_json_object_remove_with_resources(store, operation, args);
    }
    if operation == "std.http.cookies.set" {
        return dispatch_cookie_set_with_resources(store, operation, args);
    }
    if operation == "std.http.cookies.delete" {
        return dispatch_cookie_delete_with_resources(store, operation, args);
    }
    if operation.starts_with("std.native.collections.vector.") {
        return dispatch_native_vector_with_resources(store, caller_process_id, operation, args);
    }
    let decoded = decode_bridge_args(store, operation, args)?;
    if matches!(
        operation,
        "std.system.process.run"
            | "std.system.process.run_many"
            | "std.system.process.run_length_framed"
    ) {
        let result = match operation {
            "std.system.process.run" => super::process::run_process(&decoded, cancellation)?,
            "std.system.process.run_many" => {
                super::process::run_process_many(&decoded, cancellation)?
            }
            _ => super::process::run_process_length_framed(&decoded, cancellation)?,
        };
        return encode_bridge_result(store, caller_process_id, result);
    }
    let result = dispatch(operation, &decoded)?;
    encode_bridge_result(store, caller_process_id, result)
}

/// Extends one JSON array while both resources remain VM-owned.
fn dispatch_json_array_extend_with_resources(
    store: &mut ResourceStore,
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    let receiver = expect_bridge_handle(operation, args, 0)?;
    let value = expect_bridge_handle(operation, args, 1)?;
    let value = store
        .json(value)
        .cloned()
        .map_err(dispatch_resource_error)?;
    crate::terlan_native::json::extend(
        store.json_mut(receiver).map_err(dispatch_resource_error)?,
        value,
    )
    .map_err(super::args::dispatch_json_error)?;
    Ok(NativeBoundaryBridgeValue::Handle(receiver))
}

/// Appends one JSON value while both resources remain VM-owned.
fn dispatch_json_array_push_with_resources(
    store: &mut ResourceStore,
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    let receiver = expect_bridge_handle(operation, args, 0)?;
    let value = expect_bridge_handle(operation, args, 1)?;
    let value = store
        .json(value)
        .cloned()
        .map_err(dispatch_resource_error)?;
    crate::terlan_native::json::push(
        store.json_mut(receiver).map_err(dispatch_resource_error)?,
        value,
    )
    .map_err(super::args::dispatch_json_error)?;
    Ok(NativeBoundaryBridgeValue::Handle(receiver))
}

/// Replaces one JSON array element while both resources remain VM-owned.
fn dispatch_json_array_set_with_resources(
    store: &mut ResourceStore,
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    let receiver = expect_bridge_handle(operation, args, 0)?;
    let index = expect_bridge_int(operation, args, 1)?;
    let value = expect_bridge_handle(operation, args, 2)?;
    let value = store
        .json(value)
        .cloned()
        .map_err(dispatch_resource_error)?;
    crate::terlan_native::json::set(
        store.json_mut(receiver).map_err(dispatch_resource_error)?,
        index,
        value,
    )
    .map_err(super::args::dispatch_json_error)?;
    Ok(NativeBoundaryBridgeValue::Handle(receiver))
}

/// Inserts one JSON object member while both resources remain VM-owned.
fn dispatch_json_object_put_with_resources(
    store: &mut ResourceStore,
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    let receiver = expect_bridge_handle(operation, args, 0)?;
    let key = expect_bridge_text(operation, args, 1)?;
    let value = expect_bridge_handle(operation, args, 2)?;
    let value = store
        .json(value)
        .cloned()
        .map_err(dispatch_resource_error)?;
    crate::terlan_native::json::put(
        store.json_mut(receiver).map_err(dispatch_resource_error)?,
        key,
        value,
    )
    .map_err(super::args::dispatch_json_error)?;
    Ok(NativeBoundaryBridgeValue::Handle(receiver))
}

/// Removes one JSON object member while the resource remains VM-owned.
fn dispatch_json_object_remove_with_resources(
    store: &mut ResourceStore,
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    let receiver = expect_bridge_handle(operation, args, 0)?;
    let key = expect_bridge_text(operation, args, 1)?;
    crate::terlan_native::json::remove(
        store.json_mut(receiver).map_err(dispatch_resource_error)?,
        key,
    )
    .map_err(super::args::dispatch_json_error)?;
    Ok(NativeBoundaryBridgeValue::Handle(receiver))
}

fn validate_bridge_resource_owners(
    store: &ResourceStore,
    caller_process_id: u64,
    args: &[NativeBoundaryBridgeValue],
) -> Result<(), DispatchError> {
    for argument in args {
        validate_bridge_resource_owner(store, caller_process_id, argument)?;
    }
    Ok(())
}

fn validate_bridge_resource_owner(
    store: &ResourceStore,
    caller_process_id: u64,
    value: &NativeBoundaryBridgeValue,
) -> Result<(), DispatchError> {
    match value {
        NativeBoundaryBridgeValue::Handle(handle) => store
            .validate_owner(*handle, caller_process_id)
            .map_err(dispatch_resource_error),
        NativeBoundaryBridgeValue::OptionalHandle(Some(handle)) => store
            .validate_owner(*handle, caller_process_id)
            .map_err(dispatch_resource_error),
        NativeBoundaryBridgeValue::List(values) => {
            validate_bridge_resource_owners(store, caller_process_id, values)
        }
        _ => Ok(()),
    }
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
///   stable opaque handles for VM-side code.
fn dispatch_native_vector_with_resources(
    store: &mut ResourceStore,
    caller_process_id: u64,
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    match operation {
        "std.native.collections.vector.new" => store
            .insert_for_owner(
                caller_process_id,
                ResourceValue::NativeVector(vector::new()),
            )
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        "std.native.collections.vector.from_list" => {
            let values = expect_bridge_list(operation, args, 0)?;
            store
                .insert_for_owner(
                    caller_process_id,
                    ResourceValue::NativeVector(vector::from_list(values.to_vec())),
                )
                .map(NativeBoundaryBridgeValue::Handle)
                .map_err(dispatch_resource_error)
        }
        "std.native.collections.vector.length" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            vector::length(
                store
                    .native_vector(handle)
                    .map_err(dispatch_resource_error)?,
            )
            .map(NativeBoundaryBridgeValue::Int)
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
        "std.native.collections.vector.get" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            let index = expect_bridge_int(operation, args, 1)?;
            Ok(NativeBoundaryBridgeValue::List(
                vector::get_optional_values(
                    store
                        .native_vector(handle)
                        .map_err(dispatch_resource_error)?,
                    index,
                ),
            ))
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
            Ok(NativeBoundaryBridgeValue::Handle(handle))
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
            Ok(NativeBoundaryBridgeValue::Handle(handle))
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
            Ok(NativeBoundaryBridgeValue::Handle(handle))
        }
        "std.native.collections.vector.to_list" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            store
                .native_vector(handle)
                .map(|vector| NativeBoundaryBridgeValue::List(vector::to_list(vector)))
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
    args: &[NativeBoundaryBridgeValue],
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
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
    Ok(NativeBoundaryBridgeValue::Unit)
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
    args: &[NativeBoundaryBridgeValue],
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    let handle = expect_bridge_handle(operation, args, 0)?;
    let name = expect_bridge_text(operation, args, 1)?;
    let path = expect_bridge_text(operation, args, 2)?;
    store
        .http_cookie_jar_mut(handle)
        .map_err(dispatch_resource_error)?
        .delete(name, path)
        .map_err(dispatch_http_error)?;
    Ok(NativeBoundaryBridgeValue::Unit)
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
    args: &[NativeBoundaryBridgeValue],
) -> Result<(), DispatchError> {
    validate_operation_arity(operation, args.len(), unknown_operation)
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
    args: &[NativeBoundaryBridgeValue],
) -> Result<Vec<NativeBoundaryValue>, DispatchError> {
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
    arg: &NativeBoundaryBridgeValue,
) -> Result<NativeBoundaryValue, DispatchError> {
    match arg {
        NativeBoundaryBridgeValue::Unit => Ok(NativeBoundaryValue::Unit),
        NativeBoundaryBridgeValue::Text(value) => Ok(NativeBoundaryValue::Text(value.clone())),
        NativeBoundaryBridgeValue::Bytes(value) => Ok(NativeBoundaryValue::Bytes(value.clone())),
        NativeBoundaryBridgeValue::Int(value) => Ok(NativeBoundaryValue::Int(*value)),
        NativeBoundaryBridgeValue::Float(value) => Ok(NativeBoundaryValue::Float(*value)),
        NativeBoundaryBridgeValue::Bool(value) => Ok(NativeBoundaryValue::Bool(*value)),
        NativeBoundaryBridgeValue::PostgresConfig(value) => {
            Ok(NativeBoundaryValue::PostgresConfig(value.clone()))
        }
        NativeBoundaryBridgeValue::Handle(handle) if operation == "std.http.response.json" => store
            .json(*handle)
            .cloned()
            .map(NativeBoundaryValue::Json)
            .map_err(dispatch_resource_error),
        NativeBoundaryBridgeValue::Handle(handle) if operation.starts_with("std.data.json.") => {
            store
                .json(*handle)
                .cloned()
                .map(NativeBoundaryValue::Json)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(handle) if operation.starts_with("std.regex.regex.") => {
            store
                .regex(*handle)
                .cloned()
                .map(NativeBoundaryValue::Regex)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(handle) if operation.starts_with("std.http.request.") => {
            store
                .http_request(*handle)
                .cloned()
                .map(NativeBoundaryValue::HttpRequest)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(handle) if operation.starts_with("std.http.cookies.") => {
            store
                .http_cookie_jar(*handle)
                .cloned()
                .map(NativeBoundaryValue::HttpCookieJar)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(handle)
            if operation.starts_with("std.http.response.") =>
        {
            store
                .http_response(*handle)
                .cloned()
                .map(NativeBoundaryValue::HttpResponse)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(handle) if operation.starts_with("std.io.path.") => store
            .path(*handle)
            .cloned()
            .map(NativeBoundaryValue::Path)
            .map_err(dispatch_resource_error),
        NativeBoundaryBridgeValue::Handle(handle) if operation.starts_with("std.net.uri.") => store
            .uri(*handle)
            .cloned()
            .map(NativeBoundaryValue::Uri)
            .map_err(dispatch_resource_error),
        NativeBoundaryBridgeValue::Handle(handle)
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
                .map(NativeBoundaryValue::PostgresPool)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(handle)
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
                .map(NativeBoundaryValue::PostgresRow)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(_) => {
            Err(type_error(operation, index, "non-handle value"))
        }
        NativeBoundaryBridgeValue::OptionalText(_)
        | NativeBoundaryBridgeValue::OptionalHandle(_) => {
            Err(type_error(operation, index, "non-optional argument"))
        }
        NativeBoundaryBridgeValue::Atom(value) => Ok(NativeBoundaryValue::Atom(value.clone())),
        NativeBoundaryBridgeValue::Record { name, fields } => Ok(NativeBoundaryValue::Record {
            name: name.clone(),
            fields: fields
                .iter()
                .enumerate()
                .map(|(field_index, (name, value))| {
                    decode_bridge_arg(store, operation, field_index, value)
                        .map(|value| (name.clone(), value))
                })
                .collect::<Result<Vec<_>, _>>()?,
        }),
        NativeBoundaryBridgeValue::List(values)
            if ((matches!(
                operation,
                "std.data.json.string_field_rows"
                    | "std.data.json.string_fields"
                    | "std.data.json.required_fields"
                    | "std.data.json.required_field_rows"
            ) && index == 1)
                || (operation == "std.data.json.required_fields" && matches!(index, 2 | 3))
                || (operation == "std.data.json.required_field_rows"
                    && matches!(index, 2 | 3))
                || (operation == "std.data.json.required_field_rows_page"
                    && matches!(index, 3..=5))
                || (operation == "std.data.json.nested_string_field_rows"
                    && matches!(index, 1 | 3))
                || (operation == "std.data.json.nested_string_field_rows_page"
                    && matches!(index, 3 | 5))
                || (operation == "std.data.json.string_object_rows" && index == 0))
                || (operation == "std.crypto.hash.sha256_framed" && index == 0)
                || (operation == "std.crypto.hash.sha256_nul_separated" && index == 0)
                || (operation == "std.crypto.hash.sha256_domain_framed" && index == 1) =>
        {
            Ok(NativeBoundaryValue::List(
                values
                    .iter()
                    .enumerate()
                    .map(|(list_index, value)| match value {
                        NativeBoundaryBridgeValue::Text(value) => {
                            Ok(NativeBoundaryValue::Text(value.clone()))
                        }
                        _ => Err(type_error(operation, list_index, "String")),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        }
        NativeBoundaryBridgeValue::List(rows)
            if operation == "std.data.json.string_object_rows" && index == 1 =>
        {
            Ok(NativeBoundaryValue::List(
                rows.iter()
                    .enumerate()
                    .map(|(row_index, row)| {
                        let NativeBoundaryBridgeValue::List(values) = row else {
                            return Err(type_error(operation, row_index, "List[String]"));
                        };
                        values
                            .iter()
                            .enumerate()
                            .map(|(value_index, value)| match value {
                                NativeBoundaryBridgeValue::Text(value) => {
                                    Ok(NativeBoundaryValue::Text(value.clone()))
                                }
                                _ => Err(type_error(operation, value_index, "String")),
                            })
                            .collect::<Result<Vec<_>, _>>()
                            .map(NativeBoundaryValue::List)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        }
        NativeBoundaryBridgeValue::List(values) => Ok(NativeBoundaryValue::JsonList(
            values
                .iter()
                .enumerate()
                .map(|(list_index, value)| match value {
                    NativeBoundaryBridgeValue::Handle(handle) => store
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
    caller_process_id: u64,
    value: NativeBoundaryValue,
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    match value {
        NativeBoundaryValue::Unit => Ok(NativeBoundaryBridgeValue::Unit),
        NativeBoundaryValue::Text(value) => Ok(NativeBoundaryBridgeValue::Text(value)),
        NativeBoundaryValue::Bytes(value) => Ok(NativeBoundaryBridgeValue::Bytes(value)),
        NativeBoundaryValue::Int(value) => Ok(NativeBoundaryBridgeValue::Int(value)),
        NativeBoundaryValue::Float(value) => Ok(NativeBoundaryBridgeValue::Float(value)),
        NativeBoundaryValue::Bool(value) => Ok(NativeBoundaryBridgeValue::Bool(value)),
        NativeBoundaryValue::Atom(value) => Ok(NativeBoundaryBridgeValue::Atom(value)),
        NativeBoundaryValue::Record { name, fields } => Ok(NativeBoundaryBridgeValue::Record {
            name,
            fields: fields
                .into_iter()
                .map(|(name, value)| {
                    encode_bridge_result(store, caller_process_id, value).map(|value| (name, value))
                })
                .collect::<Result<Vec<_>, _>>()?,
        }),
        NativeBoundaryValue::List(values) => Ok(NativeBoundaryBridgeValue::List(
            values
                .into_iter()
                .map(|value| encode_bridge_result(store, caller_process_id, value))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        NativeBoundaryValue::Json(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::Json(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::Regex(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::Regex(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::HttpRequest(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::HttpRequest(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::HttpResponse(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::HttpResponse(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::HttpCookieJar(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::HttpCookieJar(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::Path(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::Path(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::Uri(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::Uri(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::PostgresPool(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::PostgresPool(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::PostgresRow(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::PostgresRow(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::PostgresRows(values) => values
            .into_iter()
            .map(|row| {
                store
                    .insert_for_owner(caller_process_id, ResourceValue::PostgresRow(row))
                    .map(NativeBoundaryBridgeValue::Handle)
                    .map_err(dispatch_resource_error)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(NativeBoundaryBridgeValue::List),
        NativeBoundaryValue::OptionalPostgresRow(value) => value
            .map(|row| store.insert_for_owner(caller_process_id, ResourceValue::PostgresRow(row)))
            .transpose()
            .map(NativeBoundaryBridgeValue::OptionalHandle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::PostgresConfig(_) | NativeBoundaryValue::JsonList(_) => {
            Err(DispatchError::new(
                "dispatch.postgres_requires_runtime_bridge",
                "Postgres input-only values cannot be returned across the runtime bridge.",
                0,
            ))
        }
        NativeBoundaryValue::OptionalText(value) => {
            Ok(NativeBoundaryBridgeValue::OptionalText(value))
        }
        NativeBoundaryValue::OptionalPath(value) => value
            .map(|path| store.insert_for_owner(caller_process_id, ResourceValue::Path(path)))
            .transpose()
            .map(NativeBoundaryBridgeValue::OptionalHandle)
            .map_err(dispatch_resource_error),
    }
}
