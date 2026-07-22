//! Shared bounded wire contract between the VM and capability workers.

use std::io::{self, BufRead, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::terlan_native::postgres;
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

/// Current capability-worker protocol version.
pub(crate) const CAPABILITY_PROTOCOL_VERSION: u16 = 2;

/// Maximum recursively nested values admitted in one request.
pub(crate) const MAX_CAPABILITY_TERM_COUNT: usize = 4_096;

/// Opaque resource identity carried by the capability protocol.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CapabilityHandle {
    /// Worker-local resource slot.
    pub(crate) id: u64,
    /// Generation used to reject stale slot reuse.
    pub(crate) generation: u64,
}

impl From<NativeBoundaryHandle> for CapabilityHandle {
    /// Converts a runtime handle into its transport identity.
    fn from(value: NativeBoundaryHandle) -> Self {
        Self {
            id: value.id,
            generation: value.generation,
        }
    }
}

impl From<CapabilityHandle> for NativeBoundaryHandle {
    /// Converts a transport handle into the runtime's opaque identity.
    fn from(value: CapabilityHandle) -> Self {
        Self {
            id: value.id,
            generation: value.generation,
        }
    }
}

/// Stable owned values admitted by the capability-worker protocol.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub(crate) enum CapabilityValue {
    /// Terlan Unit.
    Unit,
    /// Owned UTF-8 text.
    Text(String),
    /// Owned arbitrary bytes.
    Bytes(Vec<u8>),
    /// Signed integer.
    Int(i64),
    /// Floating-point value.
    Float(f64),
    /// Boolean value.
    Bool(bool),
    /// Opaque worker resource.
    Handle(CapabilityHandle),
    /// Optional owned text.
    OptionalText(Option<String>),
    /// Optional worker resource.
    OptionalHandle(Option<CapabilityHandle>),
    /// Owned Postgres configuration.
    PostgresConfig(postgres::Config),
    /// Ordered recursively bounded values.
    List(Vec<CapabilityValue>),
}

impl CapabilityValue {
    /// Converts one bounded wire value into the adapter term contract.
    pub(crate) fn into_term(self) -> NativeBoundaryTerm {
        match self {
            Self::Unit => NativeBoundaryTerm::Unit,
            Self::Text(value) => NativeBoundaryTerm::Text(value),
            Self::Bytes(value) => NativeBoundaryTerm::Bytes(value),
            Self::Int(value) => NativeBoundaryTerm::Int(value),
            Self::Float(value) => NativeBoundaryTerm::Float(value),
            Self::Bool(value) => NativeBoundaryTerm::Bool(value),
            Self::Handle(value) => NativeBoundaryTerm::Handle {
                id: value.id,
                generation: value.generation,
            },
            Self::OptionalText(value) => NativeBoundaryTerm::OptionalText(value),
            Self::OptionalHandle(value) => {
                NativeBoundaryTerm::OptionalHandle(value.map(Into::into))
            }
            Self::PostgresConfig(value) => NativeBoundaryTerm::PostgresConfig(value),
            Self::List(values) => NativeBoundaryTerm::List(
                values.into_iter().map(Self::into_term).collect::<Vec<_>>(),
            ),
        }
    }

    /// Converts one adapter term into an owned wire value.
    pub(crate) fn from_term(value: NativeBoundaryTerm) -> Self {
        match value {
            NativeBoundaryTerm::Unit => Self::Unit,
            NativeBoundaryTerm::Text(value) => Self::Text(value),
            NativeBoundaryTerm::Bytes(value) => Self::Bytes(value),
            NativeBoundaryTerm::Int(value) => Self::Int(value),
            NativeBoundaryTerm::Float(value) => Self::Float(value),
            NativeBoundaryTerm::Bool(value) => Self::Bool(value),
            NativeBoundaryTerm::Handle { id, generation } => {
                Self::Handle(CapabilityHandle { id, generation })
            }
            NativeBoundaryTerm::OptionalText(value) => Self::OptionalText(value),
            NativeBoundaryTerm::OptionalHandle(value) => {
                Self::OptionalHandle(value.map(Into::into))
            }
            NativeBoundaryTerm::PostgresConfig(value) => Self::PostgresConfig(value),
            NativeBoundaryTerm::List(values) => {
                Self::List(values.into_iter().map(Self::from_term).collect::<Vec<_>>())
            }
        }
    }
}

/// Versioned requests accepted by a capability worker.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum CapabilityRequest {
    /// Invokes one manifest-declared adapter operation.
    Call {
        /// Protocol version.
        version: u16,
        /// Monotonic request identity.
        request_id: u64,
        /// VM process resource owner.
        owner_id: u64,
        /// Explicit manifest capability identity.
        capability: String,
        /// Compiler-native operation identity.
        operation: String,
        /// Owned operation arguments.
        arguments: Vec<CapabilityValue>,
    },
    /// Disposes one process-owned resource.
    Dispose {
        /// Protocol version.
        version: u16,
        /// Monotonic request identity.
        request_id: u64,
        /// VM process resource owner.
        owner_id: u64,
        /// Explicit capability that owns the resource.
        capability: String,
        /// Resource to dispose.
        handle: CapabilityHandle,
    },
    /// Delivers cooperative cancellation for one request.
    Cancel {
        /// Protocol version.
        version: u16,
        /// Request being cancelled.
        request_id: u64,
        /// VM process that owns the request.
        owner_id: u64,
    },
    /// Requests orderly process termination.
    Shutdown {
        /// Protocol version.
        version: u16,
    },
}

/// Stable operation outcome returned by a capability worker.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum CapabilityOutcome {
    /// Successful adapter result.
    Ok {
        /// Owned result value.
        value: CapabilityValue,
    },
    /// Typed adapter or admission failure.
    Error {
        /// Stable machine-readable error code.
        code: String,
        /// Redacted human-readable message.
        message: String,
        /// Input offset supplied by the adapter, or zero.
        offset: usize,
    },
}

impl CapabilityOutcome {
    /// Converts a worker outcome into the VM's stable reply term.
    pub(crate) fn into_reply(self) -> NativeBoundaryReplyTerm {
        match self {
            Self::Ok { value } => NativeBoundaryReplyTerm::Ok(value.into_term()),
            Self::Error {
                code,
                message,
                offset,
            } => NativeBoundaryReplyTerm::Error {
                code,
                message,
                offset,
            },
        }
    }
}

/// Versioned responses emitted by a capability worker.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum CapabilityResponse {
    /// Operation or disposal completion.
    Reply {
        /// Protocol version.
        version: u16,
        /// Echoed request identity.
        request_id: u64,
        /// Credits still reserved after completion.
        reserved_credits: u64,
        /// Credits available after completion.
        available_credits: u64,
        /// Typed request result.
        outcome: CapabilityOutcome,
    },
    /// Cooperative cancellation acknowledgement.
    CancelAck {
        /// Protocol version.
        version: u16,
        /// Echoed request identity.
        request_id: u64,
        /// Whether cancellation reached a pending worker request.
        accepted: bool,
    },
    /// Orderly shutdown acknowledgement.
    ShutdownAck {
        /// Protocol version.
        version: u16,
    },
}

/// Reads and deserializes one newline-delimited frame within a hard byte cap.
pub(crate) fn read_json_frame<T: DeserializeOwned>(
    reader: &mut impl BufRead,
    max_bytes: usize,
) -> Result<Option<T>, String> {
    let Some(frame) = read_frame_bytes(reader, max_bytes)? else {
        return Ok(None);
    };
    serde_json::from_slice(&frame)
        .map(Some)
        .map_err(|error| format!("error[capability_worker.frame]: {error}"))
}

/// Serializes one newline-delimited frame within a hard byte cap.
pub(crate) fn write_json_frame<T: Serialize>(
    output: &mut impl Write,
    value: &T,
    max_bytes: usize,
) -> Result<(), String> {
    let mut frame = BoundedBuffer::new(max_bytes);
    serde_json::to_writer(&mut frame, value)
        .map_err(|error| format!("error[capability_worker.reply_limit]: {error}"))?;
    output
        .write_all(&frame.bytes)
        .and_then(|()| output.write_all(b"\n"))
        .and_then(|()| output.flush())
        .map_err(|error| format!("error[capability_worker.write]: {error}"))
}

/// Enforces the exact protocol version before state mutation.
pub(crate) fn validate_protocol_version(version: u16) -> Result<(), String> {
    if version == CAPABILITY_PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(format!(
            "error[capability_worker.version]: expected {CAPABILITY_PROTOCOL_VERSION}, received {version}"
        ))
    }
}

/// Bounds recursive value work independently from byte framing.
pub(crate) fn validate_capability_term_budget(values: &[CapabilityValue]) -> Result<(), String> {
    let mut pending = values.iter().collect::<Vec<_>>();
    let mut count = 0_usize;
    while let Some(value) = pending.pop() {
        count = count.checked_add(1).ok_or_else(|| {
            "error[capability_worker.term_limit]: term counter overflow".to_string()
        })?;
        if count > MAX_CAPABILITY_TERM_COUNT {
            return Err(format!(
                "error[capability_worker.term_limit]: request exceeds {MAX_CAPABILITY_TERM_COUNT} terms"
            ));
        }
        if let CapabilityValue::List(items) = value {
            pending.extend(items);
        }
    }
    Ok(())
}

/// Reads one raw frame without permitting unbounded allocation.
fn read_frame_bytes(
    reader: &mut impl BufRead,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, String> {
    let allocation_limit = max_bytes.saturating_add(2);
    let mut frame = Vec::new();
    let mut terminated = false;
    loop {
        let available = reader
            .fill_buf()
            .map_err(|error| format!("error[capability_worker.read]: {error}"))?;
        if available.is_empty() {
            break;
        }
        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        let next = frame.len().checked_add(consumed).ok_or_else(|| {
            "error[capability_worker.payload_limit]: frame size overflow".to_string()
        })?;
        if next > allocation_limit {
            return Err(format!(
                "error[capability_worker.payload_limit]: frame exceeds {max_bytes} bytes"
            ));
        }
        frame.extend_from_slice(&available[..consumed]);
        let complete = available[consumed - 1] == b'\n';
        reader.consume(consumed);
        if complete {
            terminated = true;
            break;
        }
    }
    if frame.is_empty() {
        return Ok(None);
    }
    if !terminated {
        return Err("error[capability_worker.frame]: truncated frame".to_string());
    }
    if frame.last() == Some(&b'\n') {
        frame.pop();
    }
    if frame.last() == Some(&b'\r') {
        frame.pop();
    }
    if frame.len() > max_bytes {
        return Err(format!(
            "error[capability_worker.payload_limit]: frame exceeds {max_bytes} bytes"
        ));
    }
    if frame.is_empty() {
        return Err("error[capability_worker.frame]: empty frame".to_string());
    }
    Ok(Some(frame))
}

/// Buffer that refuses to allocate beyond one configured frame.
struct BoundedBuffer {
    /// Serialized bytes accepted so far.
    bytes: Vec<u8>,
    /// Hard byte limit for the frame.
    maximum: usize,
}

impl BoundedBuffer {
    /// Creates an empty bounded response buffer.
    fn new(maximum: usize) -> Self {
        Self {
            bytes: Vec::new(),
            maximum,
        }
    }
}

impl Write for BoundedBuffer {
    /// Appends bytes only when the complete write remains within the limit.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|next| *next <= self.maximum)
            .ok_or_else(|| io::Error::other("frame exceeds configured payload limit"))?;
        self.bytes.reserve(next.saturating_sub(self.bytes.len()));
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    /// Flushes the in-memory buffer without side effects.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "capability_wire_test.rs"]
mod capability_wire_test;
