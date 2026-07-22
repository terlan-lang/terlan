//! Typed Terlan error projection for mobile native-boundary failures.
#![allow(dead_code)]
//!
//! Inputs:
//! - Stable NativeBoundary runtime error categories.
//!
//! Outputs:
//! - Compiler-owned metadata describing the `std.core.Error.Error` value that
//!   native failures must become at the Terlan boundary.
//!
//! Transformation:
//! - Rejects non-canonical native error code/message drift and keeps mobile
//!   native failures aligned with the portable core error type.

use crate::runtime::native_boundary::error::{
    error_for, is_canonical_error, ErrorKind, NativeBoundaryError,
};

/// Canonical Terlan base error module for native-boundary failures.
pub(crate) const TERLAN_CORE_ERROR_MODULE: &str = "std.core.Error";

/// Canonical Terlan base error type for native-boundary failures.
pub(crate) const TERLAN_CORE_ERROR_TYPE: &str = "Error";

/// Typed Terlan error metadata produced from a native-boundary failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MobileNativeTypedError {
    pub(crate) module: &'static str,
    pub(crate) type_name: &'static str,
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
}

/// Diagnostic returned when native error mapping rejects an input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MobileNativeErrorDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

/// Maps a NativeBoundary error kind into the Terlan base error shape.
///
/// Inputs:
/// - `kind`: closed NativeBoundary error category.
///
/// Output:
/// - `MobileNativeTypedError` targeting `std.core.Error.Error`.
///
/// Transformation:
/// - Converts the canonical NativeBoundary code/message pair into a compiler-owned
///   Terlan error projection without backend-specific payloads.
pub(crate) fn mobile_native_error_for_kind(kind: ErrorKind) -> MobileNativeTypedError {
    mobile_native_error_from_canonical(error_for(kind))
}

/// Maps a NativeBoundary error into the Terlan base error shape.
///
/// Inputs:
/// - `error`: NativeBoundary error received from a native worker or adapter.
///
/// Output:
/// - Typed Terlan error metadata when the error is canonical for its kind.
/// - Stable diagnostic when code or message fields have drifted.
///
/// Transformation:
/// - Prevents backend-specific native error strings from crossing the Terlan
///   boundary as if they were standard-library errors.
pub(crate) fn mobile_native_error_from_native_boundary(
    error: NativeBoundaryError,
) -> Result<MobileNativeTypedError, MobileNativeErrorDiagnostic> {
    if is_canonical_error(error, error.kind) {
        Ok(mobile_native_error_from_canonical(error))
    } else {
        Err(MobileNativeErrorDiagnostic {
            code: "mobile_native_error_noncanonical",
            message: format!(
                "native error `{}` does not match the canonical mapping for {:?}",
                error.code, error.kind
            ),
        })
    }
}

/// Projects one canonical NativeBoundary error into the Terlan error metadata.
fn mobile_native_error_from_canonical(error: NativeBoundaryError) -> MobileNativeTypedError {
    MobileNativeTypedError {
        module: TERLAN_CORE_ERROR_MODULE,
        type_name: TERLAN_CORE_ERROR_TYPE,
        code: error.code,
        message: error.message,
    }
}

#[cfg(test)]
#[path = "mobile_native_error_test.rs"]
mod mobile_native_error_test;
