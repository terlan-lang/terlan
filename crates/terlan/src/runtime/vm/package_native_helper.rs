//! Synchronous client for package-owned native helper processes.

#[cfg(test)]
#[path = "package_native_helper_test.rs"]
#[cfg(test)]
mod package_native_helper_test;

#[path = "package_native_helper/capability.rs"]
mod capability;
#[path = "package_native_helper/direct_std.rs"]
mod direct_std;
#[cfg(any(
    test,
    all(not(feature = "serve-runtime-bin"), feature = "postgres-libpq")
))]
#[path = "package_native_helper/sql.rs"]
mod sql;
#[path = "package_native_helper/support.rs"]
mod support;
use support::*;

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::accelerator_contract::{
    AcceleratorAddressSpace, AcceleratorResourceClass, AcceleratorResourceHandle,
    AcceleratorResourceId, AcceleratorResourcePrincipal, AcceleratorResourceRole,
};
use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::vm::native_exchange::{NativeExchangeBroker, NativeExchangeToken};
use crate::terlan_native_boundary::resource::ResourceStore;

use super::pure_native::{
    PureNativeCapabilityRequest, PureNativeExecution, PureNativeExecutionShard,
};
use super::{ReplValue, VmRuntimeError, VmRuntimeResult};

const MAX_FRAME_BYTES: usize = 1_048_576;

/// Package helpers isolated by compiler-native operation namespace.
#[derive(Default)]
pub(crate) struct VmPackageNativeHelpers {
    helpers: BTreeMap<String, VmPackageNativeHelper>,
    helper_paths: BTreeMap<String, std::ffi::OsString>,
    exchanges: NativeExchangeBroker,
    direct_std_resources: ResourceStore,
    program_arguments: Vec<String>,
}

impl VmPackageNativeHelpers {
    /// Builds one helper set with immutable arguments owned by this VM run.
    pub(crate) fn with_program_arguments(program_arguments: Vec<String>) -> Self {
        Self {
            helpers: BTreeMap::new(),
            helper_paths: BTreeMap::new(),
            exchanges: NativeExchangeBroker::default(),
            direct_std_resources: ResourceStore::new(),
            program_arguments,
        }
    }

    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    pub(crate) fn from_helper_environment(
        bindings: &[(String, std::path::PathBuf)],
    ) -> VmRuntimeResult<Self> {
        let mut helpers = Self::default();
        for (environment, path) in bindings {
            let namespace = helper_environment_namespace(environment)?;
            if helpers
                .helper_paths
                .insert(namespace.clone(), path.as_os_str().to_os_string())
                .is_some()
            {
                return Err(format!(
                    "error[native_helper_namespace]: duplicate helper binding for namespace `{namespace}`"
                ).into());
            }
        }
        Ok(helpers)
    }

    fn call(
        &mut self,
        owner_process_id: u64,
        request: &PureNativeCapabilityRequest,
    ) -> VmRuntimeResult<ReplValue> {
        let namespace = package_operation_namespace(&request.operation)?;
        if direct_std::supports(&request.operation) {
            return direct_std::call(&mut self.direct_std_resources, owner_process_id, request);
        }
        let mut routed = request.clone();
        let mut claims = Vec::<NativeExchangeToken>::new();
        if let Some(arguments) = routed.package_arguments.as_mut() {
            for argument in arguments.iter_mut() {
                let ReplValue::Bytes(bytes) = argument else {
                    continue;
                };
                if let Some((token, packet)) = self
                    .exchanges
                    .claim_tensor_packet(bytes, owner_process_id, namespace)
                    .map_err(native_exchange_error)?
                {
                    *argument = ReplValue::Bytes(packet.into());
                    claims.push(token);
                }
            }
        }
        if !self.helpers.contains_key(namespace) {
            let helper = match self.helper_paths.get(namespace) {
                Some(path) => VmPackageNativeHelper::spawn(path)?,
                None => VmPackageNativeHelper::from_environment(namespace).map_err(|error| {
                    format!(
                        "{error}; error[native_helper_operation]: operation `{}` requires namespace `{namespace}`",
                        request.operation
                    )
                })?,
            };
            self.helpers.insert(namespace.to_string(), helper);
        }
        let result = self
            .helpers
            .get_mut(namespace)
            .expect("helper was inserted for the requested namespace")
            .call(&routed);
        for claim in claims {
            self.exchanges
                .close_claim(claim)
                .map_err(native_exchange_error)?;
        }
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                self.exchanges.close_producer(namespace);
                return Err(error);
            }
        };
        if !request.operation.ends_with(".export_tensor_packet") {
            return Ok(value);
        }
        let consumer = request
            .package_arguments
            .as_ref()
            .and_then(|arguments| arguments.last())
            .and_then(|argument| match argument {
                ReplValue::String(value) => Some(value.as_str()),
                _ => None,
            })
            .ok_or_else(|| {
                "error[native_exchange.consumer]: tensor packet export requires a final consumer namespace String"
                    .to_string()
            })?;
        let ReplValue::Bytes(packet) = value else {
            return Err(
                "error[native_exchange.payload]: tensor packet export did not return Bytes".into(),
            );
        };
        Ok(self
            .exchanges
            .publish_tensor_packet(owner_process_id, namespace, consumer, &packet)
            .map(|token| ReplValue::Bytes(token.into()))
            .map_err(native_exchange_error)?)
    }

    fn close_owner(&mut self, owner_process_id: u64) {
        self.exchanges.close_owner(owner_process_id);
        self.direct_std_resources.dispose_owner(owner_process_id);
    }
}

impl Drop for VmPackageNativeHelpers {
    fn drop(&mut self) {
        self.exchanges.shutdown();
    }
}

/// One live package helper with monotonic request correlation.
struct VmPackageNativeHelper {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_request_id: u64,
}

impl VmPackageNativeHelper {
    /// Starts the helper selected by the package runtime environment.
    fn from_environment(namespace: &str) -> VmRuntimeResult<Self> {
        let env_name = package_helper_environment(namespace)?;
        let path = std::env::var_os(&env_name).ok_or_else(|| {
            format!(
                "error[native_helper_unavailable]: {env_name} is not set for native package namespace `{namespace}`"
            )
        })?;
        Self::spawn(path)
    }

    fn spawn(path: impl AsRef<OsStr>) -> VmRuntimeResult<Self> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("error[native_helper_unavailable]: {error}"))?;
        let input = child.stdin.take().ok_or_else(|| {
            "error[native_helper_unavailable]: helper stdin is closed".to_string()
        })?;
        let output = child.stdout.take().ok_or_else(|| {
            "error[native_helper_unavailable]: helper stdout is closed".to_string()
        })?;
        Ok(Self {
            child,
            input,
            output: BufReader::new(output),
            next_request_id: 0,
        })
    }

    /// Executes one compiler-native package request.
    pub(crate) fn call(
        &mut self,
        request: &PureNativeCapabilityRequest,
    ) -> VmRuntimeResult<ReplValue> {
        let arguments = request.package_arguments.as_ref().ok_or_else(|| {
            "error[native_helper_protocol]: built-in capability was sent to a package helper"
                .to_string()
        })?;
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or_else(|| "error[native_helper_protocol]: request id overflow".to_string())?;
        let mut fields = vec![
            "call".to_string(),
            self.next_request_id.to_string(),
            STANDARD.encode(request.operation.as_bytes()),
        ];
        fields.extend(
            arguments
                .iter()
                .map(encode_argument)
                .collect::<Result<Vec<_>, _>>()?,
        );
        let line = fields.join(" ");
        if line.len().saturating_add(1) > MAX_FRAME_BYTES {
            return Err(
                "error[native_helper_protocol]: package request exceeds one helper frame".into(),
            );
        }
        writeln!(self.input, "{line}")
            .and_then(|()| self.input.flush())
            .map_err(|error| format!("error[native_helper_io]: {error}"))?;
        let mut reply = String::new();
        let read = self
            .output
            .by_ref()
            .take((MAX_FRAME_BYTES + 1) as u64)
            .read_line(&mut reply)
            .map_err(|error| format!("error[native_helper_io]: {error}"))?;
        if read == 0 {
            return Err("error[native_helper_io]: helper exited without replying".into());
        }
        if reply.len() > MAX_FRAME_BYTES {
            return Err("error[native_helper_protocol]: helper reply is oversized".into());
        }
        decode_reply(reply.trim_end_matches(['\r', '\n']), self.next_request_id)
    }
}

impl Drop for VmPackageNativeHelper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Executes one shard call while servicing package-native capabilities through
/// one lazily started helper process.
pub(crate) fn execute_call(
    shard: &mut PureNativeExecutionShard,
    helpers: &mut VmPackageNativeHelpers,
    function: &str,
    arguments: &[ReplValue],
) -> VmRuntimeResult<ReplValue> {
    let (owner, mut execution) = shard.begin_call(function, arguments)?;
    loop {
        if let Err(error) = service_resident_capability(shard, helpers) {
            return cancel_with_error(shard, owner, error);
        }
        execution = match execution {
            PureNativeExecution::Complete(value) => {
                shard.finish_completed_call(owner)?;
                helpers.close_owner(owner.as_u64());
                return Ok(value);
            }
            PureNativeExecution::HttpResponse(_) => {
                shard.cancel_call(owner, "package call returned an HTTP response")?;
                helpers.close_owner(owner.as_u64());
                return Err(
                    "error[execution_shard.result_projection]: package call returned an HTTP response"
                        .into(),
                );
            }
            PureNativeExecution::Suspended(suspension)
                if suspension.operation() == TvmTransitionOperation::Capability =>
            {
                let wait = match shard.begin_capability_call(owner, &suspension) {
                    Ok(wait) => wait,
                    Err(error) => return cancel_with_error(shard, owner, error),
                };
                if wait.request().package_arguments.is_none() {
                    let reply = match dispatch_vm_capability_with_program_arguments(
                        wait.request(),
                        &helpers.program_arguments,
                    ) {
                        Ok(reply) => reply,
                        Err(error) => return cancel_with_error(shard, owner, error),
                    };
                    shard.resume_capability_call(owner, *suspension, wait, reply)?
                } else {
                    let operation = wait.request().operation.clone();
                    let value = match helpers.call(owner.as_u64(), wait.request()) {
                        Ok(value) => value,
                        Err(error) => {
                            helpers.close_owner(owner.as_u64());
                            return cancel_with_error(shard, owner, error);
                        }
                    };
                    let handles = match accelerator_resource_handles(&value) {
                        Ok(handles) => handles,
                        Err(error) => return cancel_with_error(shard, owner, error),
                    };
                    if let Err(error) = shard.register_accelerator_resources(owner, handles) {
                        return cancel_with_error(shard, owner, error);
                    }
                    shard
                        .resume_capability_value_call(owner, *suspension, wait, value)
                        .map_err(|error| {
                            format!(
                                "{error}; error[native_helper_resume]: operation `{operation}` returned an incompatible value"
                            )
                        })?
                }
            }
            PureNativeExecution::Suspended(suspension) => shard.resume_call(owner, *suspension)?,
        };
    }
}

/// Services one runnable capability wait owned by a spawned actor.
fn service_resident_capability(
    shard: &mut PureNativeExecutionShard,
    helpers: &mut VmPackageNativeHelpers,
) -> VmRuntimeResult<()> {
    let Some((owner, suspension, wait)) = shard.take_resident_capability_call()? else {
        return Ok(());
    };
    let completed = if wait.request().package_arguments.is_none() {
        let reply = dispatch_vm_capability_with_program_arguments(
            wait.request(),
            &helpers.program_arguments,
        )
        .map_err(|error| fail_resident_capability(shard, helpers, owner, error))?;
        shard.resume_resident_capability_call(owner, suspension, wait, reply)?
    } else {
        let value = helpers
            .call(owner.as_u64(), wait.request())
            .map_err(|error| fail_resident_capability(shard, helpers, owner, error))?;
        let handles = accelerator_resource_handles(&value)
            .map_err(|error| fail_resident_capability(shard, helpers, owner, error))?;
        shard
            .register_accelerator_resources(owner, handles)
            .map_err(|error| fail_resident_capability(shard, helpers, owner, error))?;
        shard.resume_resident_capability_value_call(owner, suspension, wait, value)?
    };
    if completed {
        helpers.close_owner(owner.as_u64());
    }
    Ok(())
}

/// Closes helper state and commits a resident capability failure to actor exit.
fn fail_resident_capability(
    shard: &mut PureNativeExecutionShard,
    helpers: &mut VmPackageNativeHelpers,
    owner: crate::runtime::vm::process::VmProcessId,
    error: impl Into<String>,
) -> String {
    let error = error.into();
    helpers.close_owner(owner.as_u64());
    match shard.cancel_call(owner, error.clone()) {
        Ok(()) => error,
        Err(cleanup) => format!("{error}; error[execution_shard.cleanup]: {cleanup}"),
    }
}

fn native_exchange_error(
    error: crate::runtime::vm::native_exchange::NativeExchangeError,
) -> String {
    format!("error[{}]: {}", error.code(), error.message())
}

fn package_operation_namespace(operation: &str) -> VmRuntimeResult<&str> {
    let namespace = operation.split_once('.').map(|(value, _)| value).ok_or_else(|| {
        format!(
            "error[native_helper_namespace]: package-native operation `{operation}` has no namespace"
        )
    })?;
    let mut chars = namespace.chars();
    let valid_start = chars.next().is_some_and(|value| value.is_ascii_lowercase());
    let valid_rest =
        chars.all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == '_');
    if !valid_start || !valid_rest {
        return Err(format!(
            "error[native_helper_namespace]: package-native operation `{operation}` has noncanonical namespace `{namespace}`"
        ).into());
    }
    Ok(namespace)
}

fn package_helper_environment(namespace: &str) -> VmRuntimeResult<String> {
    if package_operation_namespace(&format!("{namespace}.probe"))? != namespace {
        return Err(format!(
            "error[native_helper_namespace]: package namespace `{namespace}` is not canonical"
        )
        .into());
    }
    Ok(format!(
        "TERLAN_{}_NATIVE_BOUNDARY_HELPER_PATH",
        namespace.to_ascii_uppercase()
    ))
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
fn helper_environment_namespace(environment: &str) -> VmRuntimeResult<String> {
    let namespace = environment
        .strip_prefix("TERLAN_")
        .and_then(|value| value.strip_suffix("_NATIVE_BOUNDARY_HELPER_PATH"))
        .ok_or_else(|| {
            format!(
                "error[native_helper_namespace]: helper environment `{environment}` is not namespaced"
            )
        })?;
    if namespace.is_empty()
        || !namespace
            .chars()
            .all(|value| value.is_ascii_uppercase() || value.is_ascii_digit() || value == '_')
    {
        return Err(format!(
            "error[native_helper_namespace]: helper environment `{environment}` is not canonical"
        )
        .into());
    }
    Ok(namespace.to_ascii_lowercase())
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use capability::dispatch_vm_capability;
pub(crate) use capability::dispatch_vm_capability_with_program_arguments;

fn cancel_with_error(
    shard: &mut PureNativeExecutionShard,
    owner: crate::runtime::vm::process::VmProcessId,
    error: impl Into<String>,
) -> VmRuntimeResult<ReplValue> {
    let error = error.into();
    match shard.cancel_call(owner, error.clone()) {
        Ok(()) => Err(error.into()),
        Err(cleanup) => Err(format!("{error}; error[execution_shard.cleanup]: {cleanup}").into()),
    }
}

fn encode_argument(value: &ReplValue) -> VmRuntimeResult<String> {
    if let ReplValue::List(values) = value {
        if let Some(encoded) = encode_nullable_list(values)? {
            return Ok(encoded);
        }
        if !values.is_empty()
            && values
                .iter()
                .all(|value| matches!(value, ReplValue::Record { fields, .. } if is_native_handle(fields)))
        {
            let handles = values
                .iter()
                .map(|value| {
                    let ReplValue::Record { fields, .. } = value else {
                        unreachable!("native resource list shape was checked");
                    };
                    encode_native_handle(fields)
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(format!("lh:{}", handles.join(",")));
        }
    }
    match value {
        ReplValue::Int(value) => Ok(format!("i:{value}")),
        ReplValue::Float(value) => value
            .parse::<f64>()
            .map(|value| format!("f:{value}"))
            .map_err(|error| {
                VmRuntimeError::message(format!("error[native_helper_argument]: {error}"))
            }),
        ReplValue::Bool(value) => Ok(format!("b:{value}")),
        ReplValue::String(value) => Ok(format!("s:{}", STANDARD.encode(value.as_bytes()))),
        ReplValue::StringBytes(value) => Ok(format!("s:{}", STANDARD.encode(value))),
        ReplValue::Bytes(value) => Ok(format!("x:{}", STANDARD.encode(value))),
        ReplValue::Atom(value) => Ok(format!("a:{}", STANDARD.encode(value.as_bytes()))),
        ReplValue::List(values) if values.is_empty() => Ok("ls:".to_string()),
        ReplValue::List(rows)
            if rows.iter().all(|row| {
                matches!(
                    row,
                    ReplValue::List(values)
                        if values.iter().all(|value| {
                            matches!(value, ReplValue::String(_) | ReplValue::StringBytes(_))
                        })
                )
            }) =>
        {
            let rows = rows
                .iter()
                .map(|row| {
                    let ReplValue::List(values) = row else {
                        unreachable!("nested list shape was checked");
                    };
                    let encoded = values
                        .iter()
                        .map(|value| match value {
                            ReplValue::String(value) => STANDARD.encode(value.as_bytes()),
                            ReplValue::StringBytes(value) => STANDARD.encode(value),
                            _ => unreachable!("nested string shape was checked"),
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    STANDARD.encode(format!("{}|{encoded}", values.len()))
                })
                .collect::<Vec<_>>()
                .join(",");
            Ok(format!("lss:{rows}"))
        }
        ReplValue::List(values)
            if values
                .iter()
                .all(|value| matches!(value, ReplValue::Int(_))) =>
        {
            Ok(format!(
                "li:{}",
                values
                    .iter()
                    .map(|value| match value {
                        ReplValue::Int(value) => value.to_string(),
                        _ => unreachable!("list shape was checked"),
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            ))
        }
        ReplValue::List(values)
            if values
                .iter()
                .all(|value| matches!(value, ReplValue::Float(_))) =>
        {
            Ok(format!(
                "lf:{}",
                values
                    .iter()
                    .map(|value| match value {
                        ReplValue::Float(value) => value.as_str(),
                        _ => unreachable!("list shape was checked"),
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            ))
        }
        ReplValue::List(values)
            if values
                .iter()
                .all(|value| matches!(value, ReplValue::Bool(_))) =>
        {
            Ok(format!(
                "lb:{}",
                values
                    .iter()
                    .map(|value| match value {
                        ReplValue::Bool(value) => value.to_string(),
                        _ => unreachable!("list shape was checked"),
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            ))
        }
        ReplValue::List(values)
            if values
                .iter()
                .all(|value| matches!(value, ReplValue::String(_) | ReplValue::StringBytes(_))) =>
        {
            Ok(format!(
                "ls:{}",
                values
                    .iter()
                    .map(|value| match value {
                        ReplValue::String(value) => STANDARD.encode(value.as_bytes()),
                        ReplValue::StringBytes(value) => STANDARD.encode(value),
                        _ => unreachable!("list shape was checked"),
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            ))
        }
        ReplValue::Record { name, fields } => encode_record(name, fields),
        unsupported => Err(format!(
            "error[native_helper_argument]: unsupported package argument {unsupported:?}"
        )
        .into()),
    }
}

/// Encodes one canonical managed `List[Option[T]]` when its payload is primitive.
fn encode_nullable_list(values: &[ReplValue]) -> VmRuntimeResult<Option<String>> {
    let mut kind = None;
    let mut encoded = Vec::with_capacity(values.len());
    for value in values {
        let ReplValue::Record { name, fields } = value else {
            return Ok(None);
        };
        match (name.as_str(), fields.as_slice()) {
            ("None", []) => encoded.push("n".to_string()),
            ("Some", [(field, value)]) if field == "value" => {
                let (next_kind, value) = encode_nullable_value(value)?;
                if kind.is_some_and(|kind| kind != next_kind) {
                    return Err(
                        "error[native_helper_argument]: nullable list mixes payload types".into(),
                    );
                }
                kind = Some(next_kind);
                encoded.push(value);
            }
            _ => return Ok(None),
        }
    }
    let Some(kind) = kind else {
        return Ok((!values.is_empty()).then(|| format!("lon:{}", values.len())));
    };
    Ok(Some(format!("{}:{}", kind.prefix(), encoded.join(","))))
}

/// Primitive payload families accepted by the package-helper nullable protocol.
#[derive(Clone, Copy, Eq, PartialEq)]
enum NullableKind {
    String,
    Int,
    Float,
    Bool,
}

impl NullableKind {
    /// Returns the established helper-protocol prefix for this payload family.
    fn prefix(self) -> &'static str {
        match self {
            Self::String => "los",
            Self::Int => "loi",
            Self::Float => "lof",
            Self::Bool => "lob",
        }
    }
}

/// Encodes one present primitive payload with its nullable element tag.
fn encode_nullable_value(value: &ReplValue) -> VmRuntimeResult<(NullableKind, String)> {
    match value {
        ReplValue::String(value) => Ok((
            NullableKind::String,
            format!("s{}", STANDARD.encode(value.as_bytes())),
        )),
        ReplValue::StringBytes(value) => {
            Ok((NullableKind::String, format!("s{}", STANDARD.encode(value))))
        }
        ReplValue::Int(value) => Ok((NullableKind::Int, format!("v{value}"))),
        ReplValue::Float(value) => Ok((NullableKind::Float, format!("v{value}"))),
        ReplValue::Bool(value) => Ok((NullableKind::Bool, format!("v{value}"))),
        unsupported => Err(format!(
            "error[native_helper_argument]: unsupported nullable payload {unsupported:?}"
        )
        .into()),
    }
}

fn encode_record(name: &str, fields: &[(String, ReplValue)]) -> VmRuntimeResult<String> {
    if is_native_handle(fields) {
        return Ok(format!("h:{}", encode_native_handle(fields)?));
    }
    let encoded_fields = fields
        .iter()
        .map(|(field, value)| {
            let value = match value {
                ReplValue::Int(value) => format!("i:{value}"),
                ReplValue::Float(value) => format!("f:{value}"),
                ReplValue::Bool(value) => format!("b:{value}"),
                _ => {
                    return Err(format!(
                        "error[native_helper_argument]: record field `{field}` is not primitive"
                    )
                    .into())
                }
            };
            Ok(format!("{}:{value}", STANDARD.encode(field.as_bytes())))
        })
        .collect::<VmRuntimeResult<Vec<_>>>()?
        .join(",");
    Ok(format!(
        "r:{}:{encoded_fields}",
        STANDARD.encode(name.as_bytes())
    ))
}

/// Returns whether a record contains the complete native resource identity.
fn is_native_handle(fields: &[(String, ReplValue)]) -> bool {
    let field_names = fields
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<HashSet<_>>();
    [
        "$native_owner",
        "$native_id",
        "$native_generation",
        "$native_type",
    ]
    .iter()
    .all(|name| field_names.contains(name))
}

/// Encodes one validated native resource without the scalar/list wire prefix.
fn encode_native_handle(fields: &[(String, ReplValue)]) -> VmRuntimeResult<String> {
    let text = |name: &str| match fields.iter().find(|(field, _)| field == name) {
        Some((_, ReplValue::String(value))) => Ok(value.as_str()),
        _ => Err(format!(
            "error[native_helper_argument]: resource field `{name}` is invalid"
        )),
    };
    let integer = |name: &str| match fields.iter().find(|(field, _)| field == name) {
        Some((_, ReplValue::Int(value))) => u64::try_from(*value).map_err(|_| {
            format!("error[native_helper_argument]: resource field `{name}` is invalid")
        }),
        _ => Err(format!(
            "error[native_helper_argument]: resource field `{name}` is invalid"
        )),
    };
    Ok(format!(
        "{}:{}:{}:{}",
        STANDARD.encode(text("$native_owner")?.as_bytes()),
        integer("$native_id")?,
        integer("$native_generation")?,
        STANDARD.encode(text("$native_type")?.as_bytes())
    ))
}

fn decode_reply(line: &str, expected_request_id: u64) -> VmRuntimeResult<ReplValue> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    let ["reply", request_id, "1", payload @ ..] = fields.as_slice() else {
        return Err(
            format!("error[native_helper_protocol]: malformed helper reply `{line}`").into(),
        );
    };
    let request_id = request_id
        .parse::<u64>()
        .map_err(|error| format!("error[native_helper_protocol]: {error}"))?;
    if request_id != expected_request_id {
        return Err(format!(
            "error[native_helper_protocol]: helper replied to request {request_id}, expected {expected_request_id}"
        ).into());
    }
    decode_payload(payload)
}

fn decode_payload(fields: &[&str]) -> VmRuntimeResult<ReplValue> {
    match fields {
        ["ok_unit"] => Ok(ReplValue::Unit),
        ["ok_int", value] => value.parse::<i64>().map(ReplValue::Int).map_err(|error| {
            VmRuntimeError::message(format!("error[native_helper_protocol]: {error}"))
        }),
        ["ok_float", value] => value
            .parse::<f64>()
            .map(|value| ReplValue::Float(value.to_string()))
            .map_err(|error| {
                VmRuntimeError::message(format!("error[native_helper_protocol]: {error}"))
            }),
        ["ok_bool", value] => value.parse::<bool>().map(ReplValue::Bool).map_err(|error| {
            VmRuntimeError::message(format!("error[native_helper_protocol]: {error}"))
        }),
        ["ok_atom", value] => decode_text(value).map(ReplValue::Atom),
        ["ok_string", value] => decode_text(value).map(ReplValue::String),
        ["ok_bytes"] => Ok(ReplValue::Bytes(Vec::new().into())),
        ["ok_bytes", value] => STANDARD
            .decode(value)
            .map(|value| ReplValue::Bytes(value.into()))
            .map_err(|error| {
                VmRuntimeError::message(format!("error[native_helper_protocol]: {error}"))
            }),
        ["ok_ints"] => Ok(ReplValue::List(Vec::new())),
        ["ok_ints", values] => parse_list(values, |value| value.parse::<i64>().map(ReplValue::Int)),
        ["ok_floats"] => Ok(ReplValue::List(Vec::new())),
        ["ok_floats", values] => parse_list(values, |value| {
            value
                .parse::<f64>()
                .map(|value| ReplValue::Float(value.to_string()))
        }),
        ["ok_bools"] => Ok(ReplValue::List(Vec::new())),
        ["ok_bools", values] => {
            parse_list(values, |value| value.parse::<bool>().map(ReplValue::Bool))
        }
        ["ok_strings"] => Ok(ReplValue::List(Vec::new())),
        ["ok_strings", values] => values
            .split(',')
            .map(|value| decode_text(value).map(ReplValue::String))
            .collect::<Result<Vec<_>, _>>()
            .map(ReplValue::List),
        ["ok_schema"] => Ok(ReplValue::List(Vec::new())),
        ["ok_schema", values] => decode_schema_entries(values).map(ReplValue::List),
        ["ok_record", name, fields] => Ok(ReplValue::Record {
            name: decode_text(name)?,
            fields: decode_record_fields(fields)?,
        }),
        ["ok_handle", owner, id, generation, type_name] => {
            decode_handle(owner, id, generation, type_name)
        }
        ["ok_none"] => Ok(ReplValue::Record {
            name: "None".to_string(),
            fields: Vec::new(),
        }),
        ["ok_some_handle", owner, id, generation, type_name] => {
            decode_handle(owner, id, generation, type_name).map(|value| ReplValue::Record {
                name: "Some".to_string(),
                fields: vec![("value".to_string(), value)],
            })
        }
        ["ok_handles"] => Ok(ReplValue::List(Vec::new())),
        ["ok_handles", handles] => decode_handles(handles).map(ReplValue::List),
        ["result_ok_unit"] => Ok(result_ok(ReplValue::Unit)),
        ["result_ok_int", value] => value
            .parse::<i64>()
            .map(ReplValue::Int)
            .map(result_ok)
            .map_err(|error| {
                VmRuntimeError::message(format!("error[native_helper_protocol]: {error}"))
            }),
        ["result_ok_float", value] => value
            .parse::<f64>()
            .map(|value| ReplValue::Float(value.to_string()))
            .map(result_ok)
            .map_err(|error| {
                VmRuntimeError::message(format!("error[native_helper_protocol]: {error}"))
            }),
        ["result_ok_bool", value] => value
            .parse::<bool>()
            .map(ReplValue::Bool)
            .map(result_ok)
            .map_err(|error| {
                VmRuntimeError::message(format!("error[native_helper_protocol]: {error}"))
            }),
        ["result_ok_string", value] => decode_text(value).map(ReplValue::String).map(result_ok),
        ["result_ok_bytes"] => Ok(result_ok(ReplValue::Bytes(Vec::new().into()))),
        ["result_ok_bytes", value] => STANDARD
            .decode(value)
            .map(|value| ReplValue::Bytes(value.into()))
            .map(result_ok)
            .map_err(|error| {
                VmRuntimeError::message(format!("error[native_helper_protocol]: {error}"))
            }),
        ["result_ok_handle", owner, id, generation, type_name] => {
            decode_handle(owner, id, generation, type_name).map(result_ok)
        }
        ["result_ok_string_rows"] => Ok(result_ok(ReplValue::List(Vec::new()))),
        ["result_ok_string_rows", rows] => {
            decode_string_rows(rows).map(ReplValue::List).map(result_ok)
        }
        ["result_ok_ints"] => Ok(result_ok(ReplValue::List(Vec::new()))),
        ["result_ok_ints", values] => {
            parse_list(values, |value| value.parse::<i64>().map(ReplValue::Int)).map(result_ok)
        }
        ["result_ok_floats"] => Ok(result_ok(ReplValue::List(Vec::new()))),
        ["result_ok_floats", values] => parse_list(values, |value| {
            value
                .parse::<f64>()
                .map(|value| ReplValue::Float(value.to_string()))
        })
        .map(result_ok),
        ["result_ok_bools"] => Ok(result_ok(ReplValue::List(Vec::new()))),
        ["result_ok_bools", values] => {
            parse_list(values, |value| value.parse::<bool>().map(ReplValue::Bool)).map(result_ok)
        }
        ["result_ok_strings"] => Ok(result_ok(ReplValue::List(Vec::new()))),
        ["result_ok_strings", values] => values
            .split(',')
            .map(|value| decode_text(value).map(ReplValue::String))
            .collect::<Result<Vec<_>, _>>()
            .map(ReplValue::List)
            .map(result_ok),
        ["result_err", code, message] => {
            Ok(result_error(decode_text(code)?, decode_text(message)?))
        }
        ["err", code, message] => {
            Err(format!("error[{}]: {}", decode_text(code)?, decode_text(message)?).into())
        }
        _ => Err(format!(
            "error[native_helper_protocol]: unsupported helper payload `{}`",
            fields.join(" ")
        )
        .into()),
    }
}

/// Decodes a comma-separated list of complete opaque resource identities.
fn decode_handles(value: &str) -> VmRuntimeResult<Vec<ReplValue>> {
    value
        .split(',')
        .map(|handle| {
            let fields = handle.split(':').collect::<Vec<_>>();
            let [owner, id, generation, type_name] = fields.as_slice() else {
                return Err("error[native_helper_protocol]: malformed handle list".into());
            };
            decode_handle(owner, id, generation, type_name)
        })
        .collect()
}

fn decode_schema_entries(value: &str) -> VmRuntimeResult<Vec<ReplValue>> {
    value
        .split(',')
        .map(|entry| {
            let (name, data_type) = entry.split_once(':').ok_or_else(|| {
                "error[native_helper_protocol]: malformed schema entry".to_string()
            })?;
            Ok(ReplValue::Record {
                name: "ColumnSchema".to_string(),
                fields: vec![
                    ("name".to_string(), ReplValue::String(decode_text(name)?)),
                    (
                        "data_type".to_string(),
                        ReplValue::String(decode_text(data_type)?),
                    ),
                ],
            })
        })
        .collect()
}

fn decode_handle(
    owner: &str,
    id: &str,
    generation: &str,
    type_name: &str,
) -> VmRuntimeResult<ReplValue> {
    let owner = decode_text(owner)?;
    let id = parse_u64(id)?;
    let generation = parse_u64(generation)?;
    let type_name = decode_text(type_name)?;
    Ok(ReplValue::Record {
        name: type_name
            .rsplit('.')
            .next()
            .unwrap_or(type_name.as_str())
            .to_string(),
        fields: vec![
            ("$native_owner".to_string(), ReplValue::String(owner)),
            (
                "$native_id".to_string(),
                ReplValue::Int(i64::try_from(id).map_err(|_| {
                    "error[native_helper_protocol]: handle id exceeds i64".to_string()
                })?),
            ),
            (
                "$native_generation".to_string(),
                ReplValue::Int(i64::try_from(generation).map_err(|_| {
                    "error[native_helper_protocol]: handle generation exceeds i64".to_string()
                })?),
            ),
            ("$native_type".to_string(), ReplValue::String(type_name)),
        ],
    })
}
