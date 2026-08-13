use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use crate::terlan_native_boundary::request::RequestId;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};
use crate::terlan_native_boundary::worker::NativeBoundaryWorker;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// Executes the private native vector runtime helper.
///
/// Inputs:
/// - `args`: no public arguments are accepted.
///
/// Output:
/// - Exit success while serving line-oriented vector requests on stdin/stdout.
/// - Exit failure for malformed helper startup only.
///
/// Transformation:
/// - Creates one Rust-owned NativeBoundary worker and keeps vector resources alive
///   across bridge calls until the process exits.
pub(crate) fn run(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("terlc __native-vector-runtime does not accept arguments");
        return ExitCode::from(2);
    }
    run_loop(io::stdin().lock(), io::stdout())
}

/// Runs the native vector helper request loop.
///
/// Inputs:
/// - `input`: newline-delimited helper commands.
/// - `output`: response writer consumed by the VM bridge.
///
/// Output:
/// - Success after EOF or a write failure.
///
/// Transformation:
/// - Parses each command, dispatches it through one persistent NativeBoundary
///   worker, and writes one line-oriented response per request.
fn run_loop(input: impl BufRead, mut output: impl Write) -> ExitCode {
    let mut worker = NativeBoundaryWorker::new(32);
    let mut next_request_id = 1_u64;
    for line in input.lines() {
        let response = match line {
            Ok(line) => {
                let response = execute_line(&mut worker, next_request_id, line.trim_end());
                next_request_id = next_request_id.saturating_add(1);
                response
            }
            Err(error) => error_response(
                "native_vector_runtime_read_error",
                &format!("failed to read native vector request: {error}"),
            ),
        };
        if writeln!(output, "{response}").is_err() {
            return ExitCode::SUCCESS;
        }
        let _ = output.flush();
    }
    ExitCode::SUCCESS
}

/// Executes one helper protocol line.
///
/// Inputs:
/// - `worker`: persistent NativeBoundary worker that owns vector resources.
/// - `request_id`: monotonic request id for worker accounting.
/// - `line`: one command line from the VM bridge.
///
/// Output:
/// - One encoded response line.
///
/// Transformation:
/// - Converts the compact bridge protocol into NativeBoundary term calls and
///   converts the worker reply back into the compact protocol.
fn execute_line(worker: &mut NativeBoundaryWorker, request_id: u64, line: &str) -> String {
    let mut parts = line.split_whitespace();
    let Some(command) = parts.next() else {
        return error_response("native_vector_empty_command", "empty native vector command");
    };
    match command {
        "new" => match reject_extra_args(&mut parts, command) {
            Ok(()) => call_worker(
                worker,
                request_id,
                "std.native.collections.vector.new",
                Vec::new(),
            ),
            Err(response) => response,
        },
        "from_list" => {
            let encoded = parts.next().unwrap_or("");
            match reject_extra_args(&mut parts, command).and_then(|()| encoded_terms(encoded)) {
                Ok(values) => {
                    let values = values
                        .into_iter()
                        .map(NativeBoundaryTerm::Text)
                        .collect::<Vec<_>>();
                    call_worker(
                        worker,
                        request_id,
                        "std.native.collections.vector.from_list",
                        vec![NativeBoundaryTerm::List(values)],
                    )
                }
                Err(response) => response,
            }
        }
        "length" => match handle_args(&mut parts)
            .and_then(|handle| reject_extra_args(&mut parts, command).map(|()| handle))
        {
            Ok(handle) => call_worker(
                worker,
                request_id,
                "std.native.collections.vector.length",
                vec![handle],
            ),
            Err(response) => response,
        },
        "get_at" => match handle_args(&mut parts).and_then(|handle| {
            let index = parse_i64_arg(parts.next(), "index")?;
            reject_extra_args(&mut parts, command)?;
            Ok(vec![handle, NativeBoundaryTerm::Int(index)])
        }) {
            Ok(args) => call_worker(
                worker,
                request_id,
                "std.native.collections.vector.get_at",
                args,
            ),
            Err(response) => response,
        },
        "get" => match handle_args(&mut parts).and_then(|handle| {
            let index = parse_i64_arg(parts.next(), "index")?;
            reject_extra_args(&mut parts, command)?;
            Ok(vec![handle, NativeBoundaryTerm::Int(index)])
        }) {
            Ok(args) => call_worker(
                worker,
                request_id,
                "std.native.collections.vector.get",
                args,
            ),
            Err(response) => response,
        },
        "set_at" => match handle_args(&mut parts).and_then(|handle| {
            let index = parse_i64_arg(parts.next(), "index")?;
            let value = parse_encoded_term_arg(parts.next(), "set_at value")?;
            reject_extra_args(&mut parts, command)?;
            Ok(vec![
                handle,
                NativeBoundaryTerm::Int(index),
                NativeBoundaryTerm::Text(value),
            ])
        }) {
            Ok(args) => call_worker(
                worker,
                request_id,
                "std.native.collections.vector.set_at",
                args,
            ),
            Err(response) => response,
        },
        "swap" => match handle_args(&mut parts).and_then(|handle| {
            let left = parse_i64_arg(parts.next(), "left index")?;
            let right = parse_i64_arg(parts.next(), "right index")?;
            reject_extra_args(&mut parts, command)?;
            Ok(vec![
                handle,
                NativeBoundaryTerm::Int(left),
                NativeBoundaryTerm::Int(right),
            ])
        }) {
            Ok(args) => call_worker(
                worker,
                request_id,
                "std.native.collections.vector.swap",
                args,
            ),
            Err(response) => response,
        },
        "push" => match handle_args(&mut parts).and_then(|handle| {
            let value = parse_encoded_term_arg(parts.next(), "push value")?;
            reject_extra_args(&mut parts, command)?;
            Ok(vec![handle, NativeBoundaryTerm::Text(value)])
        }) {
            Ok(args) => call_worker(
                worker,
                request_id,
                "std.native.collections.vector.push",
                args,
            ),
            Err(response) => response,
        },
        "to_list" => match handle_args(&mut parts)
            .and_then(|handle| reject_extra_args(&mut parts, command).map(|()| handle))
        {
            Ok(handle) => call_worker(
                worker,
                request_id,
                "std.native.collections.vector.to_list",
                vec![handle],
            ),
            Err(response) => response,
        },
        other => error_response(
            "native_vector_unknown_command",
            &format!("unknown native vector command `{other}`"),
        ),
    }
}

/// Calls the NativeBoundary worker and formats the reply.
///
/// Inputs:
/// - `worker`: persistent NativeBoundary worker.
/// - `request_id`: request id for this call.
/// - `operation`: NativeBoundary operation id.
/// - `args`: stable term arguments.
///
/// Output:
/// - Encoded helper response line.
///
/// Transformation:
/// - Delegates resource ownership and mutation to NativeBoundary, then renders only
///   primitive protocol fields for the Vm bridge.
fn call_worker(
    worker: &mut NativeBoundaryWorker,
    request_id: u64,
    operation: &str,
    args: Vec<NativeBoundaryTerm>,
) -> String {
    let reply = worker.call(RequestId { value: request_id }, operation, &args);
    encode_reply(reply.result)
}

/// Encodes one NativeBoundary reply for the helper protocol.
///
/// Inputs:
/// - `reply`: stable NativeBoundary reply term.
///
/// Output:
/// - One response line with no embedded whitespace.
///
/// Transformation:
/// - Converts handles, ints, encoded Vm term strings, and lists into the
///   private bridge protocol used by generated Vm runtime code.
fn encode_reply(reply: NativeBoundaryReplyTerm) -> String {
    match reply {
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Unit) => "ok_unit".to_string(),
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Int(value)) => format!("ok_int {value}"),
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Text(value)) => format!("ok_term {value}"),
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Handle { id, generation }) => {
            format!("ok_handle {id} {generation}")
        }
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::List(values)) => {
            let encoded = values
                .into_iter()
                .filter_map(|value| match value {
                    NativeBoundaryTerm::Text(text) => Some(text),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("ok_terms {encoded}")
        }
        NativeBoundaryReplyTerm::Ok(_) => error_response(
            "native_vector_unexpected_reply",
            "native vector runtime returned an unsupported reply value",
        ),
        NativeBoundaryReplyTerm::Error { code, message, .. } => error_response(&code, &message),
    }
}

/// Parses a vector handle from protocol arguments.
///
/// Inputs:
/// - `parts`: remaining command fields.
///
/// Output:
/// - NativeBoundary handle term or encoded error response.
///
/// Transformation:
/// - Reads id and generation as unsigned integers and keeps them opaque for
///   NativeBoundary resource validation.
fn handle_args<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
) -> Result<NativeBoundaryTerm, String> {
    let id = parse_u64_arg(parts.next(), "handle id")?;
    let generation = parse_u64_arg(parts.next(), "handle generation")?;
    Ok(NativeBoundaryTerm::Handle { id, generation })
}

/// Parses a required unsigned integer protocol argument.
///
/// Inputs:
/// - `value`: optional text argument.
/// - `name`: human-readable argument name.
///
/// Output:
/// - Parsed `u64` or encoded error response.
///
/// Transformation:
/// - Converts helper protocol text into the handle integer shape used by
///   NativeBoundary terms.
fn parse_u64_arg(value: Option<&str>, name: &str) -> Result<u64, String> {
    value
        .ok_or_else(|| {
            error_response(
                "native_vector_missing_argument",
                &format!("native vector command missing {name}"),
            )
        })?
        .parse::<u64>()
        .map_err(|error| {
            error_response(
                "native_vector_invalid_integer",
                &format!("native vector {name} is not an integer: {error}"),
            )
        })
}

/// Parses a required signed integer protocol argument.
///
/// Inputs:
/// - `value`: optional text argument.
/// - `name`: human-readable argument name.
///
/// Output:
/// - Parsed `i64` or encoded error response.
///
/// Transformation:
/// - Converts helper protocol text into Terlan `Int` bridge values.
fn parse_i64_arg(value: Option<&str>, name: &str) -> Result<i64, String> {
    value
        .ok_or_else(|| {
            error_response(
                "native_vector_missing_argument",
                &format!("native vector command missing {name}"),
            )
        })?
        .parse::<i64>()
        .map_err(|error| {
            error_response(
                "native_vector_invalid_integer",
                &format!("native vector {name} is not an integer: {error}"),
            )
        })
}

/// Parses a required encoded Vm term protocol argument.
///
/// Inputs:
/// - `value`: optional encoded text argument.
/// - `name`: human-readable argument name.
///
/// Output:
/// - Validated encoded term text or encoded error response.
///
/// Transformation:
/// - Requires a payload and validates it as Base64 before it can enter the
///   Rust-owned vector store.
fn parse_encoded_term_arg(value: Option<&str>, name: &str) -> Result<String, String> {
    let value = value.ok_or_else(|| {
        error_response(
            "native_vector_missing_value",
            &format!("native vector {name} requires an encoded value"),
        )
    })?;
    validate_encoded_term(value)?;
    Ok(value.to_string())
}

/// Rejects trailing protocol fields after a command has parsed its arguments.
///
/// Inputs:
/// - `parts`: remaining command fields.
/// - `command`: command name used in diagnostics.
///
/// Output:
/// - `Ok(())` when no fields remain.
/// - Encoded error response when trailing fields are present.
///
/// Transformation:
/// - Turns ignored trailing input into a stable protocol error so malformed
///   bridge calls cannot be accepted accidentally.
fn reject_extra_args<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    command: &str,
) -> Result<(), String> {
    match parts.next() {
        None => Ok(()),
        Some(extra) => Err(error_response(
            "native_vector_unexpected_argument",
            &format!("native vector command `{command}` received unexpected argument `{extra}`"),
        )),
    }
}

/// Validates one encoded Vm term payload.
///
/// Inputs:
/// - `value`: encoded term text from the helper protocol.
///
/// Output:
/// - `Ok(())` when `value` is Base64.
/// - Encoded error response otherwise.
///
/// Transformation:
/// - Uses the maintained Base64 decoder to reject malformed payloads before
///   storing them as bridge-neutral vector values.
fn validate_encoded_term(value: &str) -> Result<(), String> {
    STANDARD.decode(value).map(|_| ()).map_err(|error| {
        error_response(
            "native_vector_invalid_encoded_term",
            &format!("native vector payload is not valid Base64: {error}"),
        )
    })
}

/// Splits and validates comma-separated encoded Vm terms.
///
/// Inputs:
/// - `encoded`: comma-separated base64 term payloads.
///
/// Output:
/// - Ordered encoded terms or encoded error response.
///
/// Transformation:
/// - Treats an empty payload as an empty list, validates each non-empty term as
///   Base64, and preserves every encoded field verbatim for Rust-owned vector
///   storage.
fn encoded_terms(encoded: &str) -> Result<Vec<String>, String> {
    if encoded.is_empty() {
        Ok(Vec::new())
    } else {
        encoded
            .split(',')
            .map(|value| {
                validate_encoded_term(value)?;
                Ok(value.to_string())
            })
            .collect()
    }
}

/// Builds an encoded helper error response.
///
/// Inputs:
/// - `code`: stable machine-readable error code.
/// - `message`: human-readable diagnostic.
///
/// Output:
/// - Error line accepted by the Vm bridge parser.
///
/// Transformation:
/// - Base64-encodes the message so the line protocol stays single-line and
///   whitespace-safe.
fn error_response(code: &str, message: &str) -> String {
    format!("err {code} {}", STANDARD.encode(message.as_bytes()))
}

#[cfg(test)]
#[path = "native_vector_runtime_test.rs"]
#[cfg(test)]
mod native_vector_runtime_test;
