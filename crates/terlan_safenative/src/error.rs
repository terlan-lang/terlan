//! Stable error conversion for SafeNative adapter boundaries.
//!
//! Native adapters must not leak backend-specific panic strings, exception
//! payloads, or transient runtime details across the Terlan boundary. This
//! module captures the pure part of that contract: each admitted error kind
//! maps to a stable code and message pair.

/// Closed set of proof-track SafeNative error categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    /// A caller supplied a stale or mismatched resource handle.
    StaleHandle,
    /// A caller attempted to reserve work beyond the configured bridge limit.
    BackpressureLimit,
    /// A command reply did not match the pending request lifecycle slot.
    InvalidRequest,
}

/// Stable SafeNative error shape returned across adapter boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafeNativeError {
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
/// - `kind`: closed SafeNative error category.
///
/// Output:
/// - A `SafeNativeError` with stable `kind`, `code`, and `message` fields.
///
/// Transformation:
/// - Maps each closed error kind to a static code/message pair without
///   allocation, panic paths, or backend-specific runtime data.
pub fn error_for(kind: ErrorKind) -> SafeNativeError {
    match kind {
        ErrorKind::StaleHandle => SafeNativeError {
            kind,
            code: "safe_native.stale_handle",
            message: "SafeNative handle is stale or does not match the resource slot.",
        },
        ErrorKind::BackpressureLimit => SafeNativeError {
            kind,
            code: "safe_native.backpressure_limit",
            message: "SafeNative bridge backpressure limit was exceeded.",
        },
        ErrorKind::InvalidRequest => SafeNativeError {
            kind,
            code: "safe_native.invalid_request",
            message: "SafeNative request lifecycle did not match the reply.",
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
pub fn is_canonical_error(error: SafeNativeError, kind: ErrorKind) -> bool {
    error == error_for(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies stale-handle errors have stable code and message fields.
    ///
    /// Inputs:
    /// - `ErrorKind::StaleHandle`.
    ///
    /// Output:
    /// - Test passes when the mapped fields match the stable contract.
    ///
    /// Transformation:
    /// - Exercises the conversion branch used by handle-liveness rejection.
    #[test]
    fn stale_handle_error_has_stable_fields() {
        assert_eq!(
            error_for(ErrorKind::StaleHandle),
            SafeNativeError {
                kind: ErrorKind::StaleHandle,
                code: "safe_native.stale_handle",
                message: "SafeNative handle is stale or does not match the resource slot.",
            }
        );
    }

    /// Verifies backpressure errors have stable code and message fields.
    ///
    /// Inputs:
    /// - `ErrorKind::BackpressureLimit`.
    ///
    /// Output:
    /// - Test passes when the mapped fields match the stable contract.
    ///
    /// Transformation:
    /// - Exercises the conversion branch used by credit reservation rejection.
    #[test]
    fn backpressure_error_has_stable_fields() {
        assert_eq!(
            error_for(ErrorKind::BackpressureLimit),
            SafeNativeError {
                kind: ErrorKind::BackpressureLimit,
                code: "safe_native.backpressure_limit",
                message: "SafeNative bridge backpressure limit was exceeded.",
            }
        );
    }

    /// Verifies invalid-request errors have stable code and message fields.
    ///
    /// Inputs:
    /// - `ErrorKind::InvalidRequest`.
    ///
    /// Output:
    /// - Test passes when the mapped fields match the stable contract.
    ///
    /// Transformation:
    /// - Exercises the conversion branch used by request lifecycle rejection.
    #[test]
    fn invalid_request_error_has_stable_fields() {
        assert_eq!(
            error_for(ErrorKind::InvalidRequest),
            SafeNativeError {
                kind: ErrorKind::InvalidRequest,
                code: "safe_native.invalid_request",
                message: "SafeNative request lifecycle did not match the reply.",
            }
        );
    }

    /// Verifies canonical error comparison checks all fields.
    ///
    /// Inputs:
    /// - Canonical and non-canonical boundary errors.
    ///
    /// Output:
    /// - Test passes when only the exact canonical mapping is accepted.
    ///
    /// Transformation:
    /// - Guards against changing message or code independently from the kind.
    #[test]
    fn canonical_error_check_rejects_changed_fields() {
        let canonical = error_for(ErrorKind::InvalidRequest);
        let changed = SafeNativeError {
            message: "changed",
            ..canonical
        };

        assert!(is_canonical_error(canonical, ErrorKind::InvalidRequest));
        assert!(!is_canonical_error(changed, ErrorKind::InvalidRequest));
    }
}
