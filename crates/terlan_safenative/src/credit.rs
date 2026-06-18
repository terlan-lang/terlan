//! Credit accounting for SafeNative command/reply bridges.
//!
//! The BEAM/native bridge uses credits as a simple backpressure contract:
//! callers may reserve work up to a configured limit, completed work releases
//! credits, and neither operation may overflow or underflow the accounting
//! state.

/// Normalizes a caller-provided credit limit.
///
/// Inputs:
/// - `limit`: requested maximum number of outstanding credits.
///
/// Output:
/// - A non-zero credit limit.
///
/// Transformation:
/// - Converts `0` to `1` so every SafeNative worker has at least one usable
///   credit slot; leaves every other limit unchanged.
pub fn normalize_limit(limit: u64) -> u64 {
    if limit == 0 {
        1
    } else {
        limit
    }
}

/// Returns whether a reservation fits inside the configured limit.
///
/// Inputs:
/// - `current`: currently reserved credits.
/// - `requested`: additional credits requested by the caller.
/// - `limit`: configured maximum outstanding credits.
///
/// Output:
/// - `true` when `current + requested <= normalize_limit(limit)`.
///
/// Transformation:
/// - Uses checked addition so overflow is rejected instead of wrapping.
pub fn can_reserve(current: u64, requested: u64, limit: u64) -> bool {
    match current.checked_add(requested) {
        Some(next) => next <= normalize_limit(limit),
        None => false,
    }
}

/// Attempts to reserve additional credits.
///
/// Inputs:
/// - `current`: currently reserved credits.
/// - `requested`: additional credits requested by the caller.
/// - `limit`: configured maximum outstanding credits.
///
/// Output:
/// - `Some(next)` when the reservation fits.
/// - `None` when the reservation would exceed the normalized limit or overflow.
///
/// Transformation:
/// - Computes the next reserved count only through checked addition and returns
///   it only if it remains within `normalize_limit(limit)`.
pub fn reserve_credit(current: u64, requested: u64, limit: u64) -> Option<u64> {
    let next = current.checked_add(requested)?;
    if next <= normalize_limit(limit) {
        Some(next)
    } else {
        None
    }
}

/// Releases previously reserved credits.
///
/// Inputs:
/// - `current`: currently reserved credits.
/// - `released`: credits completed by the worker.
///
/// Output:
/// - `Some(next)` when enough credits were reserved.
/// - `None` when releasing would underflow the reserved count.
///
/// Transformation:
/// - Uses checked subtraction so underflow is rejected instead of wrapping.
pub fn release_credit(current: u64, released: u64) -> Option<u64> {
    current.checked_sub(released)
}

#[cfg(test)]
#[path = "credit_test.rs"]
mod credit_test;
