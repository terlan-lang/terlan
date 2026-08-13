//! Stable error conversion for NativeBoundary adapter boundaries.
//!
//! Native adapters must not leak backend-specific panic strings, exception
//! payloads, or transient runtime details across the Terlan boundary. This
//! module captures the pure part of that contract: each admitted error kind
//! maps to a stable code and message pair.

/// Closed set of proof-track NativeBoundary error categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    /// A caller supplied a stale or mismatched resource handle.
    StaleHandle,
    /// A caller attempted to reserve work beyond the configured bridge limit.
    BackpressureLimit,
    /// A command reply did not match the pending request lifecycle slot.
    InvalidRequest,
    /// A pending request was cancelled before completion.
    Cancelled,
    /// A pending request exceeded its allowed wait interval.
    Timeout,
}

/// Stable NativeBoundary error shape returned across adapter boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeBoundaryError {
    /// Closed error category used by compiler/runtime policy.
    pub kind: ErrorKind,
    /// Stable machine-readable error code.
    pub code: &'static str,
    /// Stable human-readable diagnostic message.
    pub message: &'static str,
}

/// Converts an error kind into a stable boundary error.
///
/// Inputs:
/// - `kind`: closed NativeBoundary error category.
///
/// Output:
/// - A `NativeBoundaryError` with stable `kind`, `code`, and `message` fields.
///
/// Transformation:
/// - Maps each closed error kind to a static code/message pair without
///   allocation, panic paths, or backend-specific runtime data.
pub fn error_for(kind: ErrorKind) -> NativeBoundaryError {
    match kind {
        ErrorKind::StaleHandle => NativeBoundaryError {
            kind,
            code: "native_boundary.stale_handle",
            message: "NativeBoundary handle is stale or does not match the resource slot.",
        },
        ErrorKind::BackpressureLimit => NativeBoundaryError {
            kind,
            code: "native_boundary.backpressure_limit",
            message: "NativeBoundary backpressure limit was exceeded.",
        },
        ErrorKind::InvalidRequest => NativeBoundaryError {
            kind,
            code: "native_boundary.invalid_request",
            message: "NativeBoundary request lifecycle did not match the reply.",
        },
        ErrorKind::Cancelled => NativeBoundaryError {
            kind,
            code: "native_boundary.cancelled",
            message: "NativeBoundary request was cancelled before the native reply completed.",
        },
        ErrorKind::Timeout => NativeBoundaryError {
            kind,
            code: "native_boundary.timeout",
            message: "NativeBoundary request timed out before the native reply completed.",
        },
    }
}

/// Returns whether an error is the canonical mapping for a kind.
///
/// Inputs:
/// - `error`: boundary error to inspect.
/// - `kind`: expected closed error category.
///
/// Output:
/// - `true` when `error` exactly equals `error_for(kind)`.
///
/// Transformation:
/// - Compares all stable fields, so changed code/message values are detected.
pub fn is_canonical_error(error: NativeBoundaryError, kind: ErrorKind) -> bool {
    error == error_for(kind)
}

#[cfg(test)]
#[path = "error_test.rs"]
#[cfg(test)]
mod error_test;
