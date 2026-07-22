use crate::runtime::native_boundary::error::{ErrorKind, NativeBoundaryError};

use super::*;

/// Verifies stale native handles map into the portable core error type.
///
/// Inputs:
/// - `ErrorKind::StaleHandle`.
///
/// Output:
/// - `std.core.Error.Error` metadata with stable code and message.
///
/// Transformation:
/// - Projects a native-boundary runtime category into the Terlan error shape.
#[test]
fn mobile_native_error_maps_stale_handle_to_core_error() {
    let error = mobile_native_error_for_kind(ErrorKind::StaleHandle);

    assert_eq!(error.module, "std.core.Error");
    assert_eq!(error.type_name, "Error");
    assert_eq!(error.code, "native_boundary.stale_handle");
    assert_eq!(
        error.message,
        "NativeBoundary handle is stale or does not match the resource slot."
    );
}

/// Verifies every current NativeBoundary kind has a Terlan error projection.
///
/// Inputs:
/// - Backpressure, invalid-request, cancelled, and timeout native error
///   categories.
///
/// Output:
/// - Stable Terlan error code mapping.
///
/// Transformation:
/// - Keeps mobile native errors aligned with the current NativeBoundary taxonomy.
#[test]
fn mobile_native_error_maps_all_current_native_boundary_kinds() {
    assert_eq!(
        mobile_native_error_for_kind(ErrorKind::BackpressureLimit).code,
        "native_boundary.backpressure_limit"
    );
    assert_eq!(
        mobile_native_error_for_kind(ErrorKind::InvalidRequest).code,
        "native_boundary.invalid_request"
    );
    assert_eq!(
        mobile_native_error_for_kind(ErrorKind::Cancelled).code,
        "native_boundary.cancelled"
    );
    assert_eq!(
        mobile_native_error_for_kind(ErrorKind::Timeout).code,
        "native_boundary.timeout"
    );
}

/// Verifies canonical NativeBoundary errors are admitted.
///
/// Inputs:
/// - Canonical invalid-request NativeBoundary error.
///
/// Output:
/// - Typed Terlan error metadata.
///
/// Transformation:
/// - Exercises the runtime-error entrypoint used by native worker replies.
#[test]
fn mobile_native_error_accepts_canonical_native_boundary_error() {
    let error = crate::runtime::native_boundary::error::error_for(ErrorKind::InvalidRequest);
    let mapped = mobile_native_error_from_native_boundary(error).expect("canonical mapping");

    assert_eq!(mapped.module, "std.core.Error");
    assert_eq!(mapped.type_name, "Error");
    assert_eq!(mapped.code, "native_boundary.invalid_request");
}

/// Verifies non-canonical native error strings are rejected.
///
/// Inputs:
/// - NativeBoundary error with the right kind but changed message.
///
/// Output:
/// - Stable non-canonical diagnostic.
///
/// Transformation:
/// - Prevents backend-specific native panic strings from being treated as
///   portable Terlan errors.
#[test]
fn mobile_native_error_rejects_noncanonical_native_boundary_error() {
    let canonical = crate::runtime::native_boundary::error::error_for(ErrorKind::BackpressureLimit);
    let diagnostic = mobile_native_error_from_native_boundary(NativeBoundaryError {
        message: "native worker said too much",
        ..canonical
    })
    .expect_err("noncanonical error");

    assert_eq!(diagnostic.code, "mobile_native_error_noncanonical");
    assert!(diagnostic
        .message
        .contains("native_boundary.backpressure_limit"));
}
