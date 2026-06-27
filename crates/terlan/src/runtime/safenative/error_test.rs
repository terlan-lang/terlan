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
