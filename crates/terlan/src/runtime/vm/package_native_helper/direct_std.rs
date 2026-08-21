//! Direct VM-owned execution for safe Rust-backed standard-library adapters.
//!
//! These operations use the same typed NativeBoundary dispatcher as external
//! workers, but retain their resources inside the execution-shard process.
//! Unsafe or blocking package code remains on the supervised helper protocol.

use crate::runtime::vm::pure_native::PureNativeCapabilityRequest;
use crate::runtime::vm::{ReplValue, VmRuntimeError, VmRuntimeResult};
use crate::terlan_native_boundary::dispatch::{
    dispatch_with_resources_for_process, DispatchError, NativeBoundaryBridgeValue,
};
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;
use crate::terlan_native_boundary::resource::{ResourceKind, ResourceStore};

/// Returns whether an operation belongs to the closed direct-safe adapter set.
pub(super) fn supports(operation: &str) -> bool {
    operation.starts_with("std.data.json.")
        || operation.starts_with("std.encoding.base64.")
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
        || operation == "std.http.request.body_json"
        || operation == "std.http.request.body_file_path"
        || operation == "std.http.request.body_text"
        || operation == "std.http.request.method"
        || operation == "std.http.request.path"
        || operation == "std.http.request.param"
        || operation == "std.http.request.query"
        || operation == "std.http.request.query_string"
        || operation == "std.http.request.header"
        || operation == "std.http.request.cookie"
        || operation == "std.http.request.cookies"
        || operation == "std.http.response.json"
        || operation == "std.http.response.json_text"
        || operation == "std.http.response.text"
        || operation == "std.http.response.html"
        || operation == "std.http.response.file"
        || operation == "std.http.response.redirect"
        || operation == "std.http.cookies.get"
        || operation == "std.http.cookies.set_header"
        || operation == "std.http.cookies.set_header_with_options"
        || operation == "std.http.cookies.delete_header"
        || operation == "std.net.uri.parse"
        || operation == "std.net.uri.to_string"
        || operation == "std.net.uri.scheme"
        || operation == "std.net.uri.host"
        || operation == "std.net.uri.path"
        || operation == "std.net.uri.query"
        || operation == "std.net.uri.fragment"
        || operation == "std.crypto.hash.sha256_framed"
        || operation == "std.crypto.hash.sha256_domain_framed"
        || operation == "std.crypto.hash.sha256_nul_separated"
        || operation == "std.crypto.hash.sha256_bytes"
        || operation == "std.crypto.ed25519.verify"
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
    match dispatch_with_resources_for_process(
        resources,
        owner_process_id,
        &request.operation,
        &arguments,
    ) {
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
    if operation == "std.data.toml.parse" {
        return Some("TomlError");
    }
    if matches!(
        operation,
        "std.package.registry.parse_publish_request" | "std.package.registry.parse_yank_request"
    ) {
        return Some("RegistryProtocolError");
    }
    if matches!(
        operation,
        "std.encoding.base64.decode"
            | "std.encoding.base64.decode_url"
            | "std.encoding.base64.decode_bytes"
            | "std.encoding.base64.decode_url_bytes"
    ) {
        return Some("Base64Error");
    }
    if matches!(
        operation,
        "std.data.json.float"
            | "std.data.json.parse"
            | "std.data.json.stringify"
            | "std.data.json.stringify_pretty"
            | "std.data.json.get"
            | "std.data.json.keys"
            | "std.data.json.object_length"
            | "std.data.json.required_fields"
            | "std.data.json.required_field_rows"
            | "std.data.json.required_field_rows_page"
            | "std.data.json.nested_string_field_rows"
            | "std.data.json.nested_string_field_rows_page"
            | "std.data.json.string_field_rows"
            | "std.data.json.string_fields"
            | "std.data.json.string_object_rows"
            | "std.data.json.length"
            | "std.data.json.at"
            | "std.data.json.as_string"
            | "std.data.json.as_int"
            | "std.data.json.as_float"
            | "std.data.json.as_bool"
    ) {
        return Some("JsonError");
    }
    if matches!(operation, "std.io.path.from_string" | "std.io.path.join") {
        return Some("PathError");
    }
    if matches!(
        operation,
        "std.http.request.body_json" | "std.http.cookies.set_header"
    ) {
        return Some("HttpError");
    }
    if matches!(operation, "std.http.cookies.set_header_with_options") {
        return Some("HttpError");
    }
    if matches!(operation, "std.http.cookies.delete_header") {
        return Some("HttpError");
    }
    if operation == "std.net.uri.parse" {
        return Some("UriError");
    }
    (operation == "std.regex.regex.compile").then_some("RegexError")
}

fn repl_to_bridge(
    value: &ReplValue,
    owner_process_id: u64,
) -> VmRuntimeResult<NativeBoundaryBridgeValue> {
    match value {
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
        ReplValue::Record { name, fields } if native_handle(fields).is_some() => {
            let (handle, type_name, owner) = native_handle(fields)
                .expect("native handle presence was checked before conversion")?;
            let expected_owner = owner_process_id.to_string();
            if owner != expected_owner {
                return Err(format!(
                    "error[native_boundary.resource_owner]: handle owner `{owner}` does not match process `{expected_owner}`"
                ).into());
            }
            if !matches!(
                (type_name, name.as_str()),
                ("std.data.Json.Json", "Json")
                    | ("std.regex.Regex.Regex", "Regex")
                    | ("std.http.Request.Request", "Request")
                    | ("std.http.Response.Response", "Response")
                    | ("std.http.Cookies.Jar", "Jar")
                    | ("std.net.Uri.Uri", "Uri")
                    | ("std.io.Path.Path", "Path")
            ) {
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

fn bridge_to_repl(
    resources: &ResourceStore,
    owner_process_id: u64,
    value: NativeBoundaryBridgeValue,
) -> VmRuntimeResult<ReplValue> {
    match value {
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
        ResourceKind::Regex => "std.regex.Regex.Regex",
        ResourceKind::Path => "std.io.Path.Path",
        ResourceKind::HttpRequest => "std.http.Request.Request",
        ResourceKind::HttpResponse => "std.http.Response.Response",
        ResourceKind::HttpCookieJar => "std.http.Cookies.Jar",
        ResourceKind::Uri => "std.net.Uri.Uri",
        kind => {
            return Err(format!(
                "error[native_boundary.direct_std]: unsupported resource kind {kind:?}"
            )
            .into())
        }
    };
    native_handle_value(owner_process_id, handle, type_name)
}

type ParsedNativeHandle<'a> = VmRuntimeResult<(NativeBoundaryHandle, &'a str, &'a str)>;

fn native_handle(fields: &[(String, ReplValue)]) -> Option<ParsedNativeHandle<'_>> {
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

fn native_handle_value(
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

fn typed_result_error(error: DispatchError, error_name: &str) -> ReplValue {
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
