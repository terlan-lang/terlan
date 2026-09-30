#![forbid(unsafe_code)]

//! Host clock reads owned by std.time, independent of compiler and VM internals.

use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use terlan_runtime_abi::{BoundaryError, ErrorDomain, NativeBinding, NativeValue};

/// Nonblocking wall-clock observation. It is not a pure or cacheable operation.
pub const UNIX_TIME_NS: NativeBinding = NativeBinding {
    operation: "std.time.clock.unix_time_ns",
    arity: 0,
    invoke: |args| {
        UNIX_TIME_NS.validate_arity(args.len())?;
        unix_time_ns().map(NativeValue::Int)
    },
};

/// Nonblocking observation relative to one shared host-process origin.
pub const MONOTONIC_TIME_NS: NativeBinding = NativeBinding {
    operation: "std.time.clock.monotonic_time_ns",
    arity: 0,
    invoke: |args| {
        MONOTONIC_TIME_NS.validate_arity(args.len())?;
        monotonic_time_ns().map(NativeValue::Int)
    },
};

/// Observes host wall time, rejecting pre-epoch values and integer overflow.
pub fn unix_time_ns() -> Result<i64, BoundaryError> {
    unix_time_ns_at(SystemTime::now())
}

/// Observes elapsed time from one origin shared by every caller in this process.
pub fn monotonic_time_ns() -> Result<i64, BoundaryError> {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    nanoseconds(
        ORIGIN.get_or_init(Instant::now).elapsed(),
        "monotonic timestamp",
    )
}

fn unix_time_ns_at(now: SystemTime) -> Result<i64, BoundaryError> {
    let elapsed = now.duration_since(UNIX_EPOCH).map_err(|error| {
        BoundaryError::message(
            ErrorDomain::NativeBoundary,
            "wall clock",
            format!("error[dispatch.clock_before_unix_epoch]: {error}"),
        )
    })?;
    nanoseconds(elapsed, "Unix timestamp")
}

fn nanoseconds(elapsed: Duration, label: &str) -> Result<i64, BoundaryError> {
    i64::try_from(elapsed.as_nanos()).map_err(|_| {
        BoundaryError::message(
            ErrorDomain::NativeBoundary,
            "clock range",
            format!("error[dispatch.clock_overflow]: {label} does not fit Terlan Int"),
        )
    })
}

#[cfg(test)]
#[path = "clock_test.rs"]
mod tests;
