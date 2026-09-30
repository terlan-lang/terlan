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

#[test]
fn rendering_fallback_and_equality_preserve_diagnostic_identity() {
    for text in [
        "plain failure",
        "error[unfinished",
        "prefix error[code]: message",
    ] {
        let error = BoundaryError::message(ErrorDomain::NativeBoundary, "parse", text);
        assert_eq!(error.code(), "terlan.boundary");
        assert_eq!(error.to_string(), text);
        assert!(error.source().is_none());
        assert!(format!("{error:?}").contains("has_source: false"));
        assert_eq!(String::from(error), text);
    }
    let make = |domain, code, operation, context, source| {
        BoundaryError::sourced(
            domain,
            code,
            operation,
            context,
            std::io::Error::other(source),
        )
    };
    let base = make(
        ErrorDomain::NativeBoundary,
        "code",
        "parse",
        "failure",
        "cause",
    );
    assert_eq!(
        base,
        make(
            ErrorDomain::NativeBoundary,
            "code",
            "parse",
            "failure",
            "cause"
        )
    );
    for other in [
        make(ErrorDomain::VmRuntime, "code", "parse", "failure", "cause"),
        make(
            ErrorDomain::NativeBoundary,
            "other",
            "parse",
            "failure",
            "cause",
        ),
        make(
            ErrorDomain::NativeBoundary,
            "code",
            "other",
            "failure",
            "cause",
        ),
        make(
            ErrorDomain::NativeBoundary,
            "code",
            "parse",
            "other",
            "cause",
        ),
        make(
            ErrorDomain::NativeBoundary,
            "code",
            "parse",
            "failure",
            "other",
        ),
    ] {
        assert_ne!(base, other);
    }
    assert!(format!("{base:?}").contains("has_source: true"));
}
