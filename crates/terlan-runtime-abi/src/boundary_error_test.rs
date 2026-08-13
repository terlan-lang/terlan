use std::error::Error;

use super::{BoundaryError, ErrorDomain};

#[test]
fn message_preserves_stable_diagnostic_fields() {
    let error = BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "decode capability frame",
        "error[native.frame]: invalid tag",
    );
    assert_eq!(error.domain(), ErrorDomain::NativeBoundary);
    assert_eq!(error.code(), "native.frame");
    assert_eq!(error.operation(), "decode capability frame");
    assert_eq!(error.context(), "error[native.frame]: invalid tag");
}

#[test]
fn sourced_error_preserves_source_chain() {
    let error = BoundaryError::sourced(
        ErrorDomain::VmRuntime,
        "vm.io",
        "read actor stream",
        "actor stream read failed",
        std::io::Error::other("closed"),
    );
    assert_eq!(
        error.source().map(ToString::to_string).as_deref(),
        Some("closed")
    );
}
