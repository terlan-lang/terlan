//! Direct VM-owned execution for safe Rust-backed standard-library adapters.
//!
//! These operations use the same typed NativeBoundary dispatcher as external
//! workers, but retain their resources inside the execution-shard process.
//! Unsafe or package-owned blocking code remains on the supervised helper
//! protocol. The bounded `std.system.Process` capability is VM-owned and is
//! admitted here explicitly so trusted applications can invoke a fixed host
//! adapter without requiring a package-native helper binary.

use crate::runtime::vm::pure_native::PureNativeCapabilityRequest;
use crate::runtime::vm::{ReplValue, VmRuntimeError, VmRuntimeResult};
use crate::terlan_native_boundary::dispatch::{
    dispatch_with_resources_for_process, dispatch_with_resources_for_process_with_policy,
    DispatchError, NativeBoundaryBridgeValue,
};
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;
use crate::terlan_native_boundary::metadata::NativeBoundaryWorkerClass;
use crate::terlan_native_boundary::resource::{ResourceKind, ResourceStore};

/// Returns whether an operation belongs to the closed direct-safe adapter set.
pub(super) fn supports(operation: &str) -> bool {
    matches!(
        operation,
        "std.random.random.seed"
            | "std.random.random.entropy"
            | "std.random.random.int"
            | "std.random.random.bounded_int"
            | "std.random.random.float"
            | "std.random.random.bool"
            | "std.random.random.choice"
            | "std.random.random.shuffle"
            | "std.random.random.sample"
            | "std.native.collections.vector.new"
            | "std.native.collections.vector.from_list"
            | "std.native.collections.vector.length"
            | "std.native.collections.vector.get_at"
            | "std.native.collections.vector.get"
            | "std.native.collections.vector.set_at"
            | "std.native.collections.vector.swap"
            | "std.native.collections.vector.push"
            | "std.native.collections.vector.to_list"
    ) || crate::std_native_packages::resource_operation(operation).is_some()
        || operation == "std.data.toml.parse"
        || operation == "std.package.registry.parse_publish_request"
        || operation == "std.package.registry.parse_yank_request"
        || operation == "std.package.registry.archive_inventory_valid"
        || operation == "std.package.registry.sign_resource"
        || operation == "std.package.registry.canonical_payload"
        || operation == "std.package.registry.root_payload"
        || operation == "std.package.registry.signing_seed_valid"
        || operation == "std.package.registry.build_signed_resource"
        || operation == "std.package.registry.dependency_candidates_valid"
        || operation.starts_with("std.regex.regex.")
        || operation.starts_with("std.io.path.")
        || operation == "std.system.process.run"
        || operation == "std.system.process.run_many"
        || operation == "std.system.process.run_length_framed"
        || operation == "std.system.platform.current"
}

/// Executes one safe standard-library adapter call on the shard owner thread.
pub(super) fn call(
    resources: &mut ResourceStore,
    owner_process_id: u64,
    request: &PureNativeCapabilityRequest,
) -> VmRuntimeResult<ReplValue> {
    let arguments = request.package_arguments.as_ref().ok_or_else(|| {
        "error[native_boundary.direct_std]: direct std call has no package arguments".to_string()
    })?;
    let arguments = arguments
        .iter()
        .map(|value| repl_to_bridge(value, owner_process_id))
        .collect::<Result<Vec<_>, _>>()?;
    let dispatched = if request.operation.starts_with("std.system.process.") {
        dispatch_with_resources_for_process_with_policy(
            resources,
            owner_process_id,
            &["process"],
            &[NativeBoundaryWorkerClass::LongRunningCancellable],
            &request.operation,
            &arguments,
        )
    } else {
        dispatch_with_resources_for_process(
            resources,
            owner_process_id,
            &request.operation,
            &arguments,
        )
    };
    match dispatched {
        Ok(value) => {
            let value = bridge_to_repl(resources, owner_process_id, value)?;
            if typed_result_error_name(&request.operation).is_some() {
                Ok(result_ok(value))
            } else {
                Ok(value)
            }
        }
        Err(error) if typed_result_error_name(&request.operation).is_some() => {
            Ok(typed_result_error(
                error,
                typed_result_error_name(&request.operation)
                    .expect("typed result error name was checked"),
            ))
        }
        Err(error) => Err(dispatch_error(error).into()),
    }
}

fn typed_result_error_name(operation: &str) -> Option<&'static str> {
    if let Some(contract) = crate::std_native_packages::resource_operation(operation) {
        return contract.result_error;
    }
    if matches!(
        operation,
        "std.random.random.seed"
            | "std.random.random.bounded_int"
            | "std.random.random.choice"
            | "std.random.random.sample"
    ) {
        return Some("RandomError");
    }
    if operation == "std.data.toml.parse" {
        return Some("TomlError");
    }
    if matches!(
        operation,
        "std.package.registry.parse_publish_request" | "std.package.registry.parse_yank_request"
    ) {
        return Some("RegistryProtocolError");
    }
    if matches!(operation, "std.io.path.from_string" | "std.io.path.join") {
        return Some("PathError");
    }
    (operation == "std.regex.regex.compile").then_some("RegexError")
}

fn repl_to_bridge(
    value: &ReplValue,
    owner_process_id: u64,
) -> VmRuntimeResult<NativeBoundaryBridgeValue> {
    match value {
        ReplValue::Map(entries) => entries
            .iter()
            .map(|(key, value)| {
                Ok((
                    repl_to_bridge(key, owner_process_id)?,
                    repl_to_bridge(value, owner_process_id)?,
                ))
            })
            .collect::<VmRuntimeResult<_>>()
            .map(NativeBoundaryBridgeValue::Map),
        ReplValue::Unit => Ok(NativeBoundaryBridgeValue::Unit),
        ReplValue::Int(value) => Ok(NativeBoundaryBridgeValue::Int(*value)),
        ReplValue::Float(value) => {
            Ok(value
                .parse::<f64>()
                .map(NativeBoundaryBridgeValue::Float)
                .map_err(|error| format!("error[native_boundary.direct_std]: {error}"))?)
        }
        ReplValue::String(value) => Ok(NativeBoundaryBridgeValue::Text(value.clone())),
        ReplValue::StringBytes(value) => Ok(std::str::from_utf8(value)
            .map(|value| NativeBoundaryBridgeValue::Text(value.to_string()))
            .map_err(|error| format!("error[native_boundary.direct_std]: {error}"))?),
        ReplValue::Bytes(value) => Ok(NativeBoundaryBridgeValue::Bytes(value.to_vec())),
        ReplValue::Atom(value) => Ok(NativeBoundaryBridgeValue::Atom(value.clone())),
        ReplValue::Bool(value) => Ok(NativeBoundaryBridgeValue::Bool(*value)),
        ReplValue::Record { name: _, fields } if native_handle(fields).is_some() => {
            let (handle, type_name, owner) = native_handle(fields)
                .expect("native handle presence was checked before conversion")?;
            let expected_owner = owner_process_id.to_string();
            if owner != expected_owner {
                return Err(format!(
                    "error[native_boundary.resource_owner]: handle owner `{owner}` does not match process `{expected_owner}`"
                ).into());
            }
            if !supported_handle_type(type_name) {
                return Err(format!(
                    "error[native_boundary.direct_std]: operation received unsupported handle type `{type_name}`"
                ).into());
            }
            Ok(NativeBoundaryBridgeValue::Handle(handle))
        }
        ReplValue::Record { name, fields } => Ok(NativeBoundaryBridgeValue::Record {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(name, value)| {
                    repl_to_bridge(value, owner_process_id).map(|value| (name.clone(), value))
                })
                .collect::<Result<Vec<_>, _>>()?,
        }),
        ReplValue::Tuple(values) => values
            .iter()
            .map(|value| repl_to_bridge(value, owner_process_id))
            .collect::<Result<Vec<_>, _>>()
            .map(NativeBoundaryBridgeValue::Tuple),
        ReplValue::List(values) => Ok(NativeBoundaryBridgeValue::List(
            values
                .iter()
                .map(|value| repl_to_bridge(value, owner_process_id))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        unsupported => Err(format!(
            "error[native_boundary.direct_std]: unsupported argument {unsupported:?}"
        )
        .into()),
    }
}

fn supported_handle_type(type_name: &str) -> bool {
    matches!(
        type_name,
        "std.data.Json.Json"
            | "std.regex.Regex.Regex"
            | "std.http.Response.Response"
            | "std.io.Path.Path"
            | "std.random.Random.Generator"
            | "std.native.collections.Vector.Vector"
    )
}

fn bridge_to_repl(
    resources: &ResourceStore,
    owner_process_id: u64,
    value: NativeBoundaryBridgeValue,
) -> VmRuntimeResult<ReplValue> {
    match value {
        NativeBoundaryBridgeValue::Tuple(values) => values
            .into_iter()
            .map(|value| bridge_to_repl(resources, owner_process_id, value))
            .collect::<Result<Vec<_>, _>>()
            .map(ReplValue::Tuple),
        NativeBoundaryBridgeValue::Map(entries) => entries
            .into_iter()
            .map(|(key, value)| {
                Ok((
                    bridge_to_repl(resources, owner_process_id, key)?,
                    bridge_to_repl(resources, owner_process_id, value)?,
                ))
            })
            .collect::<VmRuntimeResult<_>>()
            .map(ReplValue::Map),
        NativeBoundaryBridgeValue::Unit => Ok(ReplValue::Unit),
        NativeBoundaryBridgeValue::Text(value) => Ok(ReplValue::String(value)),
        NativeBoundaryBridgeValue::Bytes(value) => Ok(ReplValue::Bytes(value.into())),
        NativeBoundaryBridgeValue::Int(value) => Ok(ReplValue::Int(value)),
        NativeBoundaryBridgeValue::Float(value) => Ok(ReplValue::Float(value.to_string())),
        NativeBoundaryBridgeValue::Bool(value) => Ok(ReplValue::Bool(value)),
        NativeBoundaryBridgeValue::Atom(value) => Ok(ReplValue::Atom(value)),
        NativeBoundaryBridgeValue::Record { name, fields } => Ok(ReplValue::Record {
            name,
            fields: fields
                .into_iter()
                .map(|(name, value)| {
                    bridge_to_repl(resources, owner_process_id, value).map(|value| (name, value))
                })
                .collect::<Result<Vec<_>, _>>()?,
        }),
        NativeBoundaryBridgeValue::Handle(handle) => {
            native_handle_from_store(resources, owner_process_id, handle)
        }
        NativeBoundaryBridgeValue::List(values) => Ok(ReplValue::List(
            values
                .into_iter()
                .map(|value| bridge_to_repl(resources, owner_process_id, value))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        NativeBoundaryBridgeValue::OptionalText(value) => Ok(match value {
            Some(value) => ReplValue::Record {
                name: "Some".to_string(),
                fields: vec![("value".to_string(), ReplValue::String(value))],
            },
            None => ReplValue::Record {
                name: "None".to_string(),
                fields: Vec::new(),
            },
        }),
        NativeBoundaryBridgeValue::OptionalHandle(value) => Ok(match value {
            Some(handle) => ReplValue::Record {
                name: "Some".to_string(),
                fields: vec![(
                    "value".to_string(),
                    native_handle_from_store(resources, owner_process_id, handle)?,
                )],
            },
            None => ReplValue::Record {
                name: "None".to_string(),
                fields: Vec::new(),
            },
        }),
        unsupported => Err(format!(
            "error[native_boundary.direct_std]: unsupported return value {unsupported:?}"
        )
        .into()),
    }
}

fn native_handle_from_store(
    resources: &ResourceStore,
    owner_process_id: u64,
    handle: NativeBoundaryHandle,
) -> VmRuntimeResult<ReplValue> {
    let type_name = match resources
        .kind(handle)
        .map_err(|error| format!("error[{}]: {}", error.code(), error.message()))?
    {
        ResourceKind::Json => "std.data.Json.Json",
        ResourceKind::RandomGenerator => "std.random.Random.Generator",
        ResourceKind::Regex => "std.regex.Regex.Regex",
        ResourceKind::Path => "std.io.Path.Path",
        ResourceKind::HttpResponse => "std.http.Response.Response",
        ResourceKind::NativeVector => "std.native.collections.Vector.Vector",
        kind => {
            return Err(format!(
                "error[native_boundary.direct_std]: unsupported resource kind {kind:?}"
            )
            .into())
        }
    };
    native_handle_value(owner_process_id, handle, type_name)
}

pub(super) type ParsedNativeHandle<'a> = VmRuntimeResult<(NativeBoundaryHandle, &'a str, &'a str)>;

pub(super) fn native_handle(fields: &[(String, ReplValue)]) -> Option<ParsedNativeHandle<'_>> {
    let owner = text_field(fields, "$native_owner")?;
    let id = int_field(fields, "$native_id")?;
    let generation = int_field(fields, "$native_generation")?;
    let type_name = text_field(fields, "$native_type")?;
    Some(
        u64::try_from(id)
            .and_then(|id| u64::try_from(generation).map(|generation| (id, generation)))
            .map(|(id, generation)| (NativeBoundaryHandle { id, generation }, type_name, owner))
            .map_err(|_| {
                VmRuntimeError::from(
                    "error[native_boundary.direct_std]: native handle fields must be nonnegative",
                )
            }),
    )
}

fn text_field<'a>(fields: &'a [(String, ReplValue)], name: &str) -> Option<&'a str> {
    fields.iter().find_map(|(field, value)| {
        (field == name)
            .then_some(value)
            .and_then(|value| match value {
                ReplValue::String(value) => Some(value.as_str()),
                _ => None,
            })
    })
}

fn int_field(fields: &[(String, ReplValue)], name: &str) -> Option<i64> {
    fields.iter().find_map(|(field, value)| {
        (field == name)
            .then_some(value)
            .and_then(|value| match value {
                ReplValue::Int(value) => Some(*value),
                _ => None,
            })
    })
}

pub(super) fn native_handle_value(
    owner_process_id: u64,
    handle: NativeBoundaryHandle,
    type_name: &str,
) -> VmRuntimeResult<ReplValue> {
    Ok(ReplValue::Record {
        name: type_name
            .rsplit('.')
            .next()
            .unwrap_or(type_name)
            .to_string(),
        fields: vec![
            (
                "$native_owner".to_string(),
                ReplValue::String(owner_process_id.to_string()),
            ),
            (
                "$native_id".to_string(),
                ReplValue::Int(i64::try_from(handle.id).map_err(|_| {
                    "error[native_boundary.direct_std]: resource id exceeds Int".to_string()
                })?),
            ),
            (
                "$native_generation".to_string(),
                ReplValue::Int(i64::try_from(handle.generation).map_err(|_| {
                    "error[native_boundary.direct_std]: resource generation exceeds Int".to_string()
                })?),
            ),
            (
                "$native_type".to_string(),
                ReplValue::String(type_name.to_string()),
            ),
        ],
    })
}

fn result_ok(value: ReplValue) -> ReplValue {
    ReplValue::Record {
        name: "Ok".to_string(),
        fields: vec![("value".to_string(), value)],
    }
}

pub(super) fn typed_result_error(error: DispatchError, error_name: &str) -> ReplValue {
    ReplValue::Record {
        name: "Err".to_string(),
        fields: vec![(
            "reason".to_string(),
            ReplValue::Record {
                name: error_name.to_string(),
                fields: vec![
                    (
                        "code".to_string(),
                        ReplValue::Atom(error.code().to_string()),
                    ),
                    (
                        "message".to_string(),
                        ReplValue::String(error.message().to_string()),
                    ),
                    (
                        "offset".to_string(),
                        ReplValue::Int(i64::try_from(error.offset()).unwrap_or(i64::MAX)),
                    ),
                ],
            },
        )],
    }
}

fn dispatch_error(error: DispatchError) -> String {
    format!("error[{}]: {}", error.code(), error.message())
}

#[cfg(test)]
#[path = "direct_std_test.rs"]
mod tests;
