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
        NativeBoundaryError {
            kind: ErrorKind::StaleHandle,
            code: "native_boundary.stale_handle",
            message: "NativeBoundary handle is stale or does not match the resource slot.",
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
        NativeBoundaryError {
            kind: ErrorKind::BackpressureLimit,
            code: "native_boundary.backpressure_limit",
            message: "NativeBoundary backpressure limit was exceeded.",
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
        NativeBoundaryError {
            kind: ErrorKind::InvalidRequest,
            code: "native_boundary.invalid_request",
            message: "NativeBoundary request lifecycle did not match the reply.",
        }
    );
}

/// Verifies cancellation errors have stable code and message fields.
///
/// Inputs:
/// - `ErrorKind::Cancelled`.
///
/// Output:
/// - Test passes when the mapped fields match the stable contract.
///
/// Transformation:
/// - Exercises the conversion branch used by async request cancellation.
#[test]
fn cancelled_error_has_stable_fields() {
    assert_eq!(
        error_for(ErrorKind::Cancelled),
        NativeBoundaryError {
            kind: ErrorKind::Cancelled,
            code: "native_boundary.cancelled",
            message: "NativeBoundary request was cancelled before the native reply completed.",
        }
    );
}

/// Verifies timeout errors have stable code and message fields.
///
/// Inputs:
/// - `ErrorKind::Timeout`.
///
/// Output:
/// - Test passes when the mapped fields match the stable contract.
///
/// Transformation:
/// - Exercises the conversion branch used by async request timeout.
#[test]
fn timeout_error_has_stable_fields() {
    assert_eq!(
        error_for(ErrorKind::Timeout),
        NativeBoundaryError {
            kind: ErrorKind::Timeout,
            code: "native_boundary.timeout",
            message: "NativeBoundary request timed out before the native reply completed.",
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
    let changed = NativeBoundaryError {
        message: "changed",
        ..canonical
    };

    assert!(is_canonical_error(canonical, ErrorKind::InvalidRequest));
    assert!(!is_canonical_error(changed, ErrorKind::InvalidRequest));
}
