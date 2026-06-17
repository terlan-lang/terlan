//! Bridge-facing term contract for SafeNative runtime calls.
//!
//! The real BEAM/NIF boundary will eventually encode and decode Erlang terms.
//! This module defines the stable Rust-side shape for that boundary without
//! depending on Rustler, NIF APIs, async runtimes, or generated adapter stubs.

use crate::dispatch::{DispatchError, SafeNativeBridgeValue};
use crate::handle::SafeNativeHandle;

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
mod tests {
    use super::*;
    use crate::dispatch::DispatchError;

    /// Builds a stable handle fixture for term codec tests.
    ///
    /// Inputs:
    /// - No external input.
    ///
    /// Output:
    /// - A deterministic handle with id `42` and generation `7`.
    ///
    /// Transformation:
    /// - Constructs fixed resource identity data for round-trip assertions.
    fn handle_fixture() -> SafeNativeHandle {
        SafeNativeHandle {
            id: 42,
            generation: 7,
        }
    }

    /// Verifies primitive bridge values round-trip through terms.
    ///
    /// Inputs:
    /// - Representative unit, text, integer, float, and bool values.
    ///
    /// Output:
    /// - Test passes when decoded values match the original inputs.
    ///
    /// Transformation:
    /// - Exercises lossless primitive conversion without resource access.
    #[test]
    fn primitive_values_round_trip_through_terms() {
        let values = [
            SafeNativeBridgeValue::Unit,
            SafeNativeBridgeValue::Text(String::from("hello")),
            SafeNativeBridgeValue::Int(7),
            SafeNativeBridgeValue::Float(1.5),
            SafeNativeBridgeValue::Bool(true),
        ];

        for value in values {
            let term = encode_bridge_value(value.clone());
            assert_eq!(decode_bridge_value(&term), value);
        }
    }

    /// Verifies opaque handles round-trip through explicit id/generation terms.
    ///
    /// Inputs:
    /// - A deterministic resource handle.
    ///
    /// Output:
    /// - Test passes when the handle survives encode/decode unchanged.
    ///
    /// Transformation:
    /// - Confirms the bridge term contract never carries adapter-owned Rust
    ///   values directly.
    #[test]
    fn handle_value_round_trips_through_term_fields() {
        let handle = handle_fixture();
        let value = SafeNativeBridgeValue::Handle(handle);

        assert_eq!(
            encode_bridge_value(value.clone()),
            SafeNativeTerm::Handle {
                id: handle.id,
                generation: handle.generation,
            }
        );
        assert_eq!(
            decode_bridge_value(&encode_bridge_value(value.clone())),
            value
        );
    }

    /// Verifies optional text and handle values round-trip through terms.
    ///
    /// Inputs:
    /// - `Some` and `None` optional bridge values.
    ///
    /// Output:
    /// - Test passes when optional payload shape is preserved.
    ///
    /// Transformation:
    /// - Exercises the optional result shapes used by path and URI accessors.
    #[test]
    fn optional_values_round_trip_through_terms() {
        let values = [
            SafeNativeBridgeValue::OptionalText(Some(String::from("name"))),
            SafeNativeBridgeValue::OptionalText(None),
            SafeNativeBridgeValue::OptionalHandle(Some(handle_fixture())),
            SafeNativeBridgeValue::OptionalHandle(None),
        ];

        for value in values {
            let term = encode_bridge_value(value.clone());
            assert_eq!(decode_bridge_value(&term), value);
        }
    }

    /// Verifies argument lists preserve order through term encoding.
    ///
    /// Inputs:
    /// - A mixed bridge argument list.
    ///
    /// Output:
    /// - Test passes when decoded arguments equal the original ordered list.
    ///
    /// Transformation:
    /// - Exercises bulk argument conversion used before resource dispatch.
    #[test]
    fn argument_lists_round_trip_in_order() {
        let args = vec![
            SafeNativeBridgeValue::Handle(handle_fixture()),
            SafeNativeBridgeValue::Text(String::from("key")),
            SafeNativeBridgeValue::Bool(false),
        ];

        let terms = encode_bridge_args(&args);

        assert_eq!(decode_bridge_args(&terms), args);
    }

    /// Verifies command terms expose their request ids consistently.
    ///
    /// Inputs:
    /// - A call command and a dispose command with different request ids.
    ///
    /// Output:
    /// - Test passes when each command reports its own request id.
    ///
    /// Transformation:
    /// - Exercises the common request-id accessor used by transport-neutral
    ///   worker dispatch.
    #[test]
    fn command_terms_expose_request_ids() {
        let call = SafeNativeCommandTerm::Call {
            request_id: 11,
            operation: String::from("std.encoding.base64.encode"),
            args: vec![SafeNativeTerm::Text(String::from("hello"))],
        };
        let dispose = SafeNativeCommandTerm::Dispose {
            request_id: 12,
            handle: handle_fixture(),
        };

        assert_eq!(call.request_id(), 11);
        assert_eq!(dispose.request_id(), 12);
    }

    /// Verifies successful dispatch replies encode as `Ok` terms.
    ///
    /// Inputs:
    /// - A successful bridge dispatch result.
    ///
    /// Output:
    /// - Test passes when the reply decodes back to the original value.
    ///
    /// Transformation:
    /// - Converts a result into the reply contract and back through the success
    ///   decoder.
    #[test]
    fn successful_dispatch_reply_decodes_to_value() {
        let value = SafeNativeBridgeValue::Text(String::from("done"));
        let reply = encode_dispatch_reply(Ok(value.clone()));

        assert_eq!(decode_success_reply(&reply), Ok(value));
    }

    /// Verifies failed dispatch replies preserve stable error fields.
    ///
    /// Inputs:
    /// - A dispatch error with code, message, and offset.
    ///
    /// Output:
    /// - Test passes when decoding returns the same stable error payload.
    ///
    /// Transformation:
    /// - Converts dispatch errors into bridge reply terms without backend
    ///   exception leakage.
    #[test]
    fn failed_dispatch_reply_decodes_to_stable_term_error() {
        let reply =
            encode_dispatch_reply(Err(DispatchError::new("dispatch.type", "wrong value", 12)));
        let error = decode_success_reply(&reply)
            .err()
            .unwrap_or_else(|| TermError::new("missing", "missing", 0));

        assert_eq!(error.code(), "dispatch.type");
        assert_eq!(error.message(), "wrong value");
        assert_eq!(error.offset(), 12);
    }
}
