mod bridge_values;
use bridge_values::{decode_bridge_args, encode_bridge_result};
mod random;

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
    if operation.starts_with("std.random.random.") {
        return random::dispatch(store, caller_process_id, operation, args);
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
        NativeBoundaryBridgeValue::List(values) | NativeBoundaryBridgeValue::Tuple(values) => {
            validate_bridge_resource_owners(store, caller_process_id, values)
        }
        NativeBoundaryBridgeValue::Record { fields, .. } => {
            for (_, value) in fields {
                validate_bridge_resource_owner(store, caller_process_id, value)?;
            }
            Ok(())
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
