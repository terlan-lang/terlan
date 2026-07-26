//! Synchronous client for package-owned native helper processes.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::runtime::native_image::control::TvmTransitionOperation;

use super::pure_native::{
    PureNativeCapabilityRequest, PureNativeExecution, PureNativeExecutionShard,
};
use super::ReplValue;

const HELPER_PATH_ENV: &str = "TERLAN_NATIVE_BOUNDARY_HELPER_PATH";
const MAX_FRAME_BYTES: usize = 1_048_576;

/// One live package helper with monotonic request correlation.
pub(crate) struct VmPackageNativeHelper {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_request_id: u64,
}

impl VmPackageNativeHelper {
    /// Starts the helper selected by the package runtime environment.
    pub(crate) fn from_environment() -> Result<Self, String> {
        let path = std::env::var_os(HELPER_PATH_ENV).ok_or_else(|| {
            "error[native_helper_unavailable]: TERLAN_NATIVE_BOUNDARY_HELPER_PATH is not set for compiler-native package operation"
                .to_string()
        })?;
        Self::spawn(path)
    }

    fn spawn(path: impl AsRef<OsStr>) -> Result<Self, String> {
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
    ) -> Result<ReplValue, String> {
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
                "error[native_helper_protocol]: package request exceeds one helper frame"
                    .to_string(),
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
            return Err("error[native_helper_io]: helper exited without replying".to_string());
        }
        if reply.len() > MAX_FRAME_BYTES {
            return Err("error[native_helper_protocol]: helper reply is oversized".to_string());
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
    helper: &mut Option<VmPackageNativeHelper>,
    function: &str,
    arguments: &[ReplValue],
) -> Result<ReplValue, String> {
    let (owner, mut execution) = shard.begin_call(function, arguments)?;
    loop {
        execution = match execution {
            PureNativeExecution::Complete(value) => {
                shard.finish_completed_call(owner)?;
                return Ok(value);
            }
            PureNativeExecution::HttpResponse(_) => {
                shard.cancel_call(owner, "package call returned an HTTP response")?;
                return Err(
                    "error[execution_shard.result_projection]: package call returned an HTTP response"
                        .to_string(),
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
                    shard.resume_call(owner, suspension)?
                } else {
                    if helper.is_none() {
                        match VmPackageNativeHelper::from_environment() {
                            Ok(started) => *helper = Some(started),
                            Err(error) => return cancel_with_error(shard, owner, error),
                        }
                    }
                    let value = match helper
                        .as_mut()
                        .expect("package helper was initialized")
                        .call(wait.request())
                    {
                        Ok(value) => value,
                        Err(error) => return cancel_with_error(shard, owner, error),
                    };
                    shard.resume_capability_value_call(owner, suspension, wait, value)?
                }
            }
            PureNativeExecution::Suspended(suspension) => shard.resume_call(owner, suspension)?,
        };
    }
}

fn cancel_with_error(
    shard: &mut PureNativeExecutionShard,
    owner: crate::runtime::vm::process::VmProcessId,
    error: String,
) -> Result<ReplValue, String> {
    match shard.cancel_call(owner, error.clone()) {
        Ok(()) => Err(error),
        Err(cleanup) => Err(format!(
            "{error}; error[execution_shard.cleanup]: {cleanup}"
        )),
    }
}

fn encode_argument(value: &ReplValue) -> Result<String, String> {
    match value {
        ReplValue::Int(value) => Ok(format!("i:{value}")),
        ReplValue::Float(value) => value
            .parse::<f64>()
            .map(|value| format!("f:{value}"))
            .map_err(|error| format!("error[native_helper_argument]: {error}")),
        ReplValue::Bool(value) => Ok(format!("b:{value}")),
        ReplValue::String(value) => Ok(format!("s:{}", STANDARD.encode(value.as_bytes()))),
        ReplValue::StringBytes(value) => Ok(format!("s:{}", STANDARD.encode(value))),
        ReplValue::Bytes(value) => Ok(format!("x:{}", STANDARD.encode(value))),
        ReplValue::Atom(value) => Ok(format!("a:{}", STANDARD.encode(value.as_bytes()))),
        ReplValue::List(values) if values.is_empty() => Ok("ls:".to_string()),
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
        ReplValue::Record { name, fields } => encode_record(name, fields),
        unsupported => Err(format!(
            "error[native_helper_argument]: unsupported package argument {unsupported:?}"
        )),
    }
}

fn encode_record(name: &str, fields: &[(String, ReplValue)]) -> Result<String, String> {
    let field_names = fields
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<HashSet<_>>();
    if [
        "$native_owner",
        "$native_id",
        "$native_generation",
        "$native_type",
    ]
    .iter()
    .all(|name| field_names.contains(name))
    {
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
        return Ok(format!(
            "h:{}:{}:{}:{}",
            STANDARD.encode(text("$native_owner")?.as_bytes()),
            integer("$native_id")?,
            integer("$native_generation")?,
            STANDARD.encode(text("$native_type")?.as_bytes())
        ));
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
                    ))
                }
            };
            Ok(format!("{}:{value}", STANDARD.encode(field.as_bytes())))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join(",");
    Ok(format!(
        "r:{}:{encoded_fields}",
        STANDARD.encode(name.as_bytes())
    ))
}

fn decode_reply(line: &str, expected_request_id: u64) -> Result<ReplValue, String> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    let ["reply", request_id, "1", payload @ ..] = fields.as_slice() else {
        return Err(format!(
            "error[native_helper_protocol]: malformed helper reply `{line}`"
        ));
    };
    let request_id = request_id
        .parse::<u64>()
        .map_err(|error| format!("error[native_helper_protocol]: {error}"))?;
    if request_id != expected_request_id {
        return Err(format!(
            "error[native_helper_protocol]: helper replied to request {request_id}, expected {expected_request_id}"
        ));
    }
    decode_payload(payload)
}

fn decode_payload(fields: &[&str]) -> Result<ReplValue, String> {
    match fields {
        ["ok_unit"] => Ok(ReplValue::Unit),
        ["ok_int", value] => value
            .parse::<i64>()
            .map(ReplValue::Int)
            .map_err(|error| format!("error[native_helper_protocol]: {error}")),
        ["ok_bool", value] => value
            .parse::<bool>()
            .map(ReplValue::Bool)
            .map_err(|error| format!("error[native_helper_protocol]: {error}")),
        ["ok_bytes"] => Ok(ReplValue::Bytes(Vec::new().into())),
        ["ok_bytes", value] => STANDARD
            .decode(value)
            .map(|value| ReplValue::Bytes(value.into()))
            .map_err(|error| format!("error[native_helper_protocol]: {error}")),
        ["ok_ints"] => Ok(ReplValue::List(Vec::new())),
        ["ok_ints", values] => parse_list(values, |value| value.parse::<i64>().map(ReplValue::Int)),
        ["ok_floats"] => Ok(ReplValue::List(Vec::new())),
        ["ok_floats", values] => parse_list(values, |value| {
            value
                .parse::<f64>()
                .map(|value| ReplValue::Float(value.to_string()))
        }),
        ["ok_record", name, fields] => Ok(ReplValue::Record {
            name: decode_text(name)?,
            fields: decode_record_fields(fields)?,
        }),
        ["ok_handle", owner, id, generation, type_name] => {
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
                            "error[native_helper_protocol]: handle generation exceeds i64"
                                .to_string()
                        })?),
                    ),
                    ("$native_type".to_string(), ReplValue::String(type_name)),
                ],
            })
        }
        ["err", code, message] => Err(format!(
            "error[{}]: {}",
            decode_text(code)?,
            decode_text(message)?
        )),
        _ => Err(format!(
            "error[native_helper_protocol]: unsupported helper payload `{}`",
            fields.join(" ")
        )),
    }
}

fn parse_list<T>(
    values: &str,
    mut parse: impl FnMut(&str) -> Result<ReplValue, T>,
) -> Result<ReplValue, String>
where
    T: std::fmt::Display,
{
    values
        .split(',')
        .map(|value| {
            parse(value).map_err(|error| format!("error[native_helper_protocol]: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(ReplValue::List)
}

fn decode_record_fields(value: &str) -> Result<Vec<(String, ReplValue)>, String> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|field| {
            let mut parts = field.splitn(3, ':');
            let name = decode_text(parts.next().ok_or_else(|| {
                "error[native_helper_protocol]: record field name is missing".to_string()
            })?)?;
            let kind = parts.next().ok_or_else(|| {
                "error[native_helper_protocol]: record field kind is missing".to_string()
            })?;
            let value = parts.next().ok_or_else(|| {
                "error[native_helper_protocol]: record field value is missing".to_string()
            })?;
            let value = match kind {
                "i" => value
                    .parse::<i64>()
                    .map(ReplValue::Int)
                    .map_err(|error| format!("error[native_helper_protocol]: {error}"))?,
                "f" => value
                    .parse::<f64>()
                    .map(|value| ReplValue::Float(value.to_string()))
                    .map_err(|error| format!("error[native_helper_protocol]: {error}"))?,
                "b" => value
                    .parse::<bool>()
                    .map(ReplValue::Bool)
                    .map_err(|error| format!("error[native_helper_protocol]: {error}"))?,
                _ => {
                    return Err(format!(
                        "error[native_helper_protocol]: unsupported record field kind `{kind}`"
                    ))
                }
            };
            Ok((name, value))
        })
        .collect()
}

fn decode_text(value: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(value)
        .map_err(|error| format!("error[native_helper_protocol]: {error}"))?;
    String::from_utf8(bytes).map_err(|error| format!("error[native_helper_protocol]: {error}"))
}

fn parse_u64(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("error[native_helper_protocol]: {error}"))
}
