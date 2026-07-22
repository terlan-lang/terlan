use super::*;

#[test]
fn native_boundary_worker_panic_becomes_typed_error_without_payload_leak() {
    let error = catch_native_boundary_panic::<()>("std.test.panics", || {
        std::panic::panic_any(String::from("secret native payload"));
    })
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "expected panic error", 0));

    assert_eq!(error.code(), "native_boundary.worker_panic");
    assert_eq!(
        error.message(),
        "NativeBoundary worker failed while executing `std.test.panics`; the panic payload was suppressed."
    );
    assert!(!error.message().contains("secret native payload"));
}

#[test]
fn native_boundary_panic_guard_preserves_success_and_typed_error() {
    assert_eq!(
        catch_native_boundary_panic("std.test.success", || Ok(42)),
        Ok(42)
    );

    let expected = DispatchError::new("native.typed", "typed adapter failure", 7);
    assert_eq!(
        catch_native_boundary_panic::<()>("std.test.typed", || Err(expected.clone())),
        Err(expected)
    );
}
