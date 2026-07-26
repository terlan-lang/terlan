//! Loud boundary used by the compiler-free AOT serve runtime.
//!
//! The VM process must not load or call `libpq` directly. Database access from
//! this process belongs behind the asynchronous capability-worker protocol.

use super::PostgresError;

/// Placeholder that prevents command code from silently creating an in-process
/// database readiness loop in the AOT serve runtime.
#[derive(Clone, Debug)]
pub(crate) struct DriverReadinessPoller;

impl DriverReadinessPoller {
    pub(crate) fn new() -> Result<Self, PostgresError> {
        Err(PostgresError::new(
            "postgres.capability_worker.required",
            "The AOT serve runtime cannot execute Postgres in-process; route this operation \
             through the asynchronous capability-worker protocol.",
        ))
    }
}

#[cfg(test)]
#[path = "libpq_unavailable_test.rs"]
mod tests;
