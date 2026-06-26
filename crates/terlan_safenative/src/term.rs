//! Bridge-facing term contract for SafeNative runtime calls.
//!
//! The real BEAM/NIF boundary will eventually encode and decode Erlang terms.
//! This module defines the stable Rust-side shape for that boundary without
//! depending on Rustler, NIF APIs, async runtimes, or generated adapter stubs.

use crate::dispatch::{DispatchError, SafeNativeBridgeValue};
use crate::handle::SafeNativeHandle;
use crate::postgres;

/// Stable term shape accepted by the SafeNative bridge.
#[derive(Clone, Debug, PartialEq)]
pub enum SafeNativeTerm {
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
    /// Opaque resource handle encoded as id and generation.
    Handle {
        /// Stable resource slot id.
        id: u64,
        /// Resource generation tag used to reject stale handles.
        generation: u64,
    },
    /// Optional `String` result.
    OptionalText(Option<String>),
    /// Optional opaque resource handle result.
    OptionalHandle(Option<SafeNativeHandle>),
    /// Postgres connection configuration accepted by `std.db.postgres.connect`.
    PostgresConfig(postgres::Config),
    /// Terlan list carrying bridge-stable element terms.
    List(Vec<SafeNativeTerm>),
}

/// Stable reply shape returned by a SafeNative bridge call.
#[derive(Clone, Debug, PartialEq)]
pub enum SafeNativeReplyTerm {
    /// Successful call result.
    Ok(SafeNativeTerm),
    /// Failed call with stable diagnostic fields.
    Error {
        /// Stable machine-readable error code.
        code: String,
        /// Human-readable error message.
        message: String,
        /// Source/input byte offset when supplied by an adapter, or `0`.
        offset: usize,
    },
}

/// Stable command shape accepted by the SafeNative bridge.
#[derive(Clone, Debug, PartialEq)]
pub enum SafeNativeCommandTerm {
    /// Calls one compiler-native operation with stable bridge terms.
    Call {
        /// Request id assigned by the bridge caller.
        request_id: u64,
        /// Compiler-native operation id.
        operation: String,
        /// Operation arguments encoded as stable terms.
        args: Vec<SafeNativeTerm>,
    },
    /// Disposes one opaque resource handle.
    Dispose {
        /// Request id assigned by the bridge caller.
        request_id: u64,
        /// Opaque resource handle to dispose.
        handle: SafeNativeHandle,
    },
}

impl SafeNativeCommandTerm {
    /// Returns the request id carried by this command.
    ///
    /// Inputs:
    /// - `self`: command term received by the bridge.
    ///
    /// Output:
    /// - Request id supplied by the caller.
    ///
    /// Transformation:
    /// - Reads the request id from either command variant without mutating the
    ///   command payload.
    pub fn request_id(&self) -> u64 {
        match self {
            SafeNativeCommandTerm::Call { request_id, .. }
            | SafeNativeCommandTerm::Dispose { request_id, .. } => *request_id,
        }
    }
}

/// Error returned while interpreting bridge reply terms.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TermError {
    code: String,
    message: String,
    offset: usize,
}

impl TermError {
    /// Builds a term decoding error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    /// - `offset`: source/input byte offset when available, or `0`.
    ///
    /// Output:
    /// - A `TermError` suitable for callers that decode bridge replies.
    ///
    /// Transformation:
    /// - Stores backend-neutral diagnostic fields without exposing BEAM or NIF
    ///   implementation details.
    pub fn new(code: impl Into<String>, message: impl Into<String>, offset: usize) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            offset,
        }
    }

    /// Returns the stable machine-readable error code.
    ///
    /// Inputs:
    /// - `self`: term decoding error.
    ///
    /// Output:
    /// - Borrowed error code.
    ///
    /// Transformation:
    /// - Reads the code field without allocation or mutation.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the human-readable error message.
    ///
    /// Inputs:
    /// - `self`: term decoding error.
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
    /// - `self`: term decoding error.
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

/// Encodes one bridge value into the stable term contract.
///
/// Inputs:
/// - `value`: resource-aware SafeNative value returned by bridge dispatch.
///
/// Output:
/// - `SafeNativeTerm` carrying only primitives or opaque handles.
///
/// Transformation:
/// - Removes Rust adapter value types from the boundary representation while
///   preserving resource handle identity.
pub fn encode_bridge_value(value: SafeNativeBridgeValue) -> SafeNativeTerm {
    match value {
        SafeNativeBridgeValue::Unit => SafeNativeTerm::Unit,
        SafeNativeBridgeValue::Text(value) => SafeNativeTerm::Text(value),
        SafeNativeBridgeValue::Int(value) => SafeNativeTerm::Int(value),
        SafeNativeBridgeValue::Float(value) => SafeNativeTerm::Float(value),
        SafeNativeBridgeValue::Bool(value) => SafeNativeTerm::Bool(value),
        SafeNativeBridgeValue::Handle(handle) => SafeNativeTerm::Handle {
            id: handle.id,
            generation: handle.generation,
        },
        SafeNativeBridgeValue::OptionalText(value) => SafeNativeTerm::OptionalText(value),
        SafeNativeBridgeValue::OptionalHandle(value) => SafeNativeTerm::OptionalHandle(value),
        SafeNativeBridgeValue::PostgresConfig(value) => SafeNativeTerm::PostgresConfig(value),
        SafeNativeBridgeValue::List(values) => SafeNativeTerm::List(
            values
                .into_iter()
                .map(encode_bridge_value)
                .collect::<Vec<_>>(),
        ),
    }
}

/// Decodes one stable term into a bridge value.
///
/// Inputs:
/// - `term`: stable bridge term produced by the BEAM-facing codec.
///
/// Output:
/// - `SafeNativeBridgeValue` suitable for resource-backed dispatch.
///
/// Transformation:
/// - Reconstructs opaque handle structs from their term-level id/generation
///   fields and clones owned primitive payloads.
pub fn decode_bridge_value(term: &SafeNativeTerm) -> SafeNativeBridgeValue {
    match term {
        SafeNativeTerm::Unit => SafeNativeBridgeValue::Unit,
        SafeNativeTerm::Text(value) => SafeNativeBridgeValue::Text(value.clone()),
        SafeNativeTerm::Int(value) => SafeNativeBridgeValue::Int(*value),
        SafeNativeTerm::Float(value) => SafeNativeBridgeValue::Float(*value),
        SafeNativeTerm::Bool(value) => SafeNativeBridgeValue::Bool(*value),
        SafeNativeTerm::Handle { id, generation } => {
            SafeNativeBridgeValue::Handle(SafeNativeHandle {
                id: *id,
                generation: *generation,
            })
        }
        SafeNativeTerm::OptionalText(value) => SafeNativeBridgeValue::OptionalText(value.clone()),
        SafeNativeTerm::OptionalHandle(value) => SafeNativeBridgeValue::OptionalHandle(*value),
        SafeNativeTerm::PostgresConfig(value) => {
            SafeNativeBridgeValue::PostgresConfig(value.clone())
        }
        SafeNativeTerm::List(values) => {
            SafeNativeBridgeValue::List(values.iter().map(decode_bridge_value).collect::<Vec<_>>())
        }
    }
}

/// Encodes bridge call arguments into stable terms.
///
/// Inputs:
/// - `args`: bridge-facing operation arguments.
///
/// Output:
/// - Stable term vector with the same argument order.
///
/// Transformation:
/// - Applies `encode_bridge_value` to each argument without interpreting the
///   operation id or mutating resource state.
pub fn encode_bridge_args(args: &[SafeNativeBridgeValue]) -> Vec<SafeNativeTerm> {
    args.iter().cloned().map(encode_bridge_value).collect()
}

/// Decodes stable terms into bridge call arguments.
///
/// Inputs:
/// - `terms`: stable terms supplied by a BEAM-facing codec.
///
/// Output:
/// - Bridge value vector with the same argument order.
///
/// Transformation:
/// - Applies `decode_bridge_value` to each term without touching resource
///   ownership or adapter logic.
pub fn decode_bridge_args(terms: &[SafeNativeTerm]) -> Vec<SafeNativeBridgeValue> {
    terms.iter().map(decode_bridge_value).collect()
}

/// Encodes a dispatch result into a stable reply term.
///
/// Inputs:
/// - `result`: resource-backed dispatch result.
///
/// Output:
/// - `SafeNativeReplyTerm::Ok` for success or `SafeNativeReplyTerm::Error` for
///   stable dispatch failures.
///
/// Transformation:
/// - Converts dispatch errors into owned term fields and successful bridge
///   values into the stable term contract.
pub fn encode_dispatch_reply(
    result: Result<SafeNativeBridgeValue, DispatchError>,
) -> SafeNativeReplyTerm {
    match result {
        Ok(value) => SafeNativeReplyTerm::Ok(encode_bridge_value(value)),
        Err(error) => SafeNativeReplyTerm::Error {
            code: error.code().to_string(),
            message: error.message().to_string(),
            offset: error.offset(),
        },
    }
}

/// Decodes a successful reply term into a bridge value.
///
/// Inputs:
/// - `reply`: stable reply term returned by the bridge.
///
/// Output:
/// - `Ok(SafeNativeBridgeValue)` for successful replies.
/// - `Err(TermError)` carrying the stable error payload for failed replies.
///
/// Transformation:
/// - Reuses `decode_bridge_value` for success and preserves error code,
///   message, and offset for failure.
pub fn decode_success_reply(
    reply: &SafeNativeReplyTerm,
) -> Result<SafeNativeBridgeValue, TermError> {
    match reply {
        SafeNativeReplyTerm::Ok(value) => Ok(decode_bridge_value(value)),
        SafeNativeReplyTerm::Error {
            code,
            message,
            offset,
        } => Err(TermError::new(code.clone(), message.clone(), *offset)),
    }
}

#[cfg(test)]
#[path = "term_test.rs"]
mod term_test;
