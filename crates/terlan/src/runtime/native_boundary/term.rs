//! Bridge-facing term contract for NativeBoundary runtime calls.
//!
//! The real VM/NIF boundary will eventually encode and decode Vm terms.
//! This module defines the stable Rust-side shape for that boundary without
//! depending on Rustler, NIF APIs, async runtimes, or generated adapter stubs.

use crate::terlan_native::postgres;
use crate::terlan_native_boundary::dispatch::{DispatchError, NativeBoundaryBridgeValue};
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;

/// Stable term shape accepted by the NativeBoundary bridge.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeBoundaryTerm {
    /// Terlan `Unit`.
    Unit,
    /// Terlan `String`.
    Text(String),
    /// Terlan VM-owned `Bytes`.
    Bytes(Vec<u8>),
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
    OptionalHandle(Option<NativeBoundaryHandle>),
    /// Postgres connection configuration accepted by `std.db.postgres.connect`.
    PostgresConfig(postgres::Config),
    /// Terlan list carrying bridge-stable element terms.
    List(Vec<NativeBoundaryTerm>),
}

/// Stable reply shape returned by a NativeBoundary bridge call.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeBoundaryReplyTerm {
    /// Successful call result.
    Ok(NativeBoundaryTerm),
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

/// Stable command shape accepted by the NativeBoundary bridge.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeBoundaryCommandTerm {
    /// Calls one compiler-native operation with stable bridge terms.
    Call {
        /// Request id assigned by the bridge caller.
        request_id: u64,
        /// Compiler-native operation id.
        operation: String,
        /// Operation arguments encoded as stable terms.
        args: Vec<NativeBoundaryTerm>,
    },
    /// Disposes one opaque resource handle.
    Dispose {
        /// Request id assigned by the bridge caller.
        request_id: u64,
        /// Opaque resource handle to dispose.
        handle: NativeBoundaryHandle,
    },
}

impl NativeBoundaryCommandTerm {
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
            NativeBoundaryCommandTerm::Call { request_id, .. }
            | NativeBoundaryCommandTerm::Dispose { request_id, .. } => *request_id,
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
    /// - Stores backend-neutral diagnostic fields without exposing VM or NIF
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
/// - `value`: resource-aware NativeBoundary value returned by bridge dispatch.
///
/// Output:
/// - `NativeBoundaryTerm` carrying only primitives or opaque handles.
///
/// Transformation:
/// - Removes Rust adapter value types from the boundary representation while
///   preserving resource handle identity.
pub fn encode_bridge_value(value: NativeBoundaryBridgeValue) -> NativeBoundaryTerm {
    match value {
        NativeBoundaryBridgeValue::Unit => NativeBoundaryTerm::Unit,
        NativeBoundaryBridgeValue::Text(value) => NativeBoundaryTerm::Text(value),
        NativeBoundaryBridgeValue::Bytes(value) => NativeBoundaryTerm::Bytes(value),
        NativeBoundaryBridgeValue::Int(value) => NativeBoundaryTerm::Int(value),
        NativeBoundaryBridgeValue::Float(value) => NativeBoundaryTerm::Float(value),
        NativeBoundaryBridgeValue::Bool(value) => NativeBoundaryTerm::Bool(value),
        NativeBoundaryBridgeValue::Handle(handle) => NativeBoundaryTerm::Handle {
            id: handle.id,
            generation: handle.generation,
        },
        NativeBoundaryBridgeValue::OptionalText(value) => NativeBoundaryTerm::OptionalText(value),
        NativeBoundaryBridgeValue::OptionalHandle(value) => {
            NativeBoundaryTerm::OptionalHandle(value)
        }
        NativeBoundaryBridgeValue::PostgresConfig(value) => {
            NativeBoundaryTerm::PostgresConfig(value)
        }
        NativeBoundaryBridgeValue::List(values) => NativeBoundaryTerm::List(
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
/// - `term`: stable bridge term produced by the VM-facing codec.
///
/// Output:
/// - `NativeBoundaryBridgeValue` suitable for resource-backed dispatch.
///
/// Transformation:
/// - Reconstructs opaque handle structs from their term-level id/generation
///   fields and clones owned primitive payloads.
pub fn decode_bridge_value(term: &NativeBoundaryTerm) -> NativeBoundaryBridgeValue {
    match term {
        NativeBoundaryTerm::Unit => NativeBoundaryBridgeValue::Unit,
        NativeBoundaryTerm::Text(value) => NativeBoundaryBridgeValue::Text(value.clone()),
        NativeBoundaryTerm::Bytes(value) => NativeBoundaryBridgeValue::Bytes(value.clone()),
        NativeBoundaryTerm::Int(value) => NativeBoundaryBridgeValue::Int(*value),
        NativeBoundaryTerm::Float(value) => NativeBoundaryBridgeValue::Float(*value),
        NativeBoundaryTerm::Bool(value) => NativeBoundaryBridgeValue::Bool(*value),
        NativeBoundaryTerm::Handle { id, generation } => {
            NativeBoundaryBridgeValue::Handle(NativeBoundaryHandle {
                id: *id,
                generation: *generation,
            })
        }
        NativeBoundaryTerm::OptionalText(value) => {
            NativeBoundaryBridgeValue::OptionalText(value.clone())
        }
        NativeBoundaryTerm::OptionalHandle(value) => {
            NativeBoundaryBridgeValue::OptionalHandle(*value)
        }
        NativeBoundaryTerm::PostgresConfig(value) => {
            NativeBoundaryBridgeValue::PostgresConfig(value.clone())
        }
        NativeBoundaryTerm::List(values) => NativeBoundaryBridgeValue::List(
            values.iter().map(decode_bridge_value).collect::<Vec<_>>(),
        ),
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
pub fn encode_bridge_args(args: &[NativeBoundaryBridgeValue]) -> Vec<NativeBoundaryTerm> {
    args.iter().cloned().map(encode_bridge_value).collect()
}

/// Decodes stable terms into bridge call arguments.
///
/// Inputs:
/// - `terms`: stable terms supplied by a VM-facing codec.
///
/// Output:
/// - Bridge value vector with the same argument order.
///
/// Transformation:
/// - Applies `decode_bridge_value` to each term without touching resource
///   ownership or adapter logic.
pub fn decode_bridge_args(terms: &[NativeBoundaryTerm]) -> Vec<NativeBoundaryBridgeValue> {
    terms.iter().map(decode_bridge_value).collect()
}

/// Encodes a dispatch result into a stable reply term.
///
/// Inputs:
/// - `result`: resource-backed dispatch result.
///
/// Output:
/// - `NativeBoundaryReplyTerm::Ok` for success or `NativeBoundaryReplyTerm::Error` for
///   stable dispatch failures.
///
/// Transformation:
/// - Converts dispatch errors into owned term fields and successful bridge
///   values into the stable term contract.
pub fn encode_dispatch_reply(
    result: Result<NativeBoundaryBridgeValue, DispatchError>,
) -> NativeBoundaryReplyTerm {
    match result {
        Ok(value) => NativeBoundaryReplyTerm::Ok(encode_bridge_value(value)),
        Err(error) => NativeBoundaryReplyTerm::Error {
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
/// - `Ok(NativeBoundaryBridgeValue)` for successful replies.
/// - `Err(TermError)` carrying the stable error payload for failed replies.
///
/// Transformation:
/// - Reuses `decode_bridge_value` for success and preserves error code,
///   message, and offset for failure.
pub fn decode_success_reply(
    reply: &NativeBoundaryReplyTerm,
) -> Result<NativeBoundaryBridgeValue, TermError> {
    match reply {
        NativeBoundaryReplyTerm::Ok(value) => Ok(decode_bridge_value(value)),
        NativeBoundaryReplyTerm::Error {
            code,
            message,
            offset,
        } => Err(TermError::new(code.clone(), message.clone(), *offset)),
    }
}

#[cfg(test)]
#[path = "term_test.rs"]
mod term_test;
