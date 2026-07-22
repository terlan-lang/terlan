//! Panic containment for NativeBoundary adapter execution.

use std::panic::{catch_unwind, AssertUnwindSafe};

use super::DispatchError;

pub(super) fn catch_native_boundary_panic<T>(
    operation: &str,
    execute: impl FnOnce() -> Result<T, DispatchError>,
) -> Result<T, DispatchError> {
    match catch_unwind(AssertUnwindSafe(execute)) {
        Ok(result) => result,
        Err(_) => Err(DispatchError::new(
            "native_boundary.worker_panic",
            format!(
                "NativeBoundary worker failed while executing `{operation}`; the panic payload was suppressed."
            ),
            0,
        )),
    }
}

#[cfg(test)]
#[path = "panic_boundary_test.rs"]
mod panic_boundary_test;
