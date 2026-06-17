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
mod tests {
    use super::*;

    /// Verifies zero limits still admit one outstanding operation.
    ///
    /// Inputs:
    /// - A zero configured limit and one requested credit.
    ///
    /// Output:
    /// - Test passes when the normalized limit is one and reservation succeeds.
    ///
    /// Transformation:
    /// - Exercises the SafeNative rule that `0` cannot create a permanently
    ///   unusable worker.
    #[test]
    fn normalize_limit_makes_zero_limit_usable() {
        assert_eq!(normalize_limit(0), 1);
        assert_eq!(reserve_credit(0, 1, 0), Some(1));
    }

    /// Verifies reservations never exceed the configured limit.
    ///
    /// Inputs:
    /// - Current, requested, and limit values around the boundary.
    ///
    /// Output:
    /// - Test passes when in-bound reservations succeed and out-of-bound
    ///   reservations fail.
    ///
    /// Transformation:
    /// - Exercises the same invariant the Lean proof mirrors: every successful
    ///   reservation is less than or equal to the normalized limit.
    #[test]
    fn reserve_credit_respects_limit() {
        assert_eq!(reserve_credit(1, 2, 3), Some(3));
        assert_eq!(reserve_credit(1, 3, 3), None);
    }

    /// Verifies reservation overflow is rejected.
    ///
    /// Inputs:
    /// - `u64::MAX` plus one requested credit.
    ///
    /// Output:
    /// - Test passes when overflow returns `None`.
    ///
    /// Transformation:
    /// - Proves the implementation does not use wrapping arithmetic for credit
    ///   accounting.
    #[test]
    fn reserve_credit_rejects_overflow() {
        assert_eq!(reserve_credit(u64::MAX, 1, u64::MAX), None);
        assert!(!can_reserve(u64::MAX, 1, u64::MAX));
    }

    /// Verifies release underflow is rejected.
    ///
    /// Inputs:
    /// - A current reservation count smaller than the requested release.
    ///
    /// Output:
    /// - Test passes when underflow returns `None`.
    ///
    /// Transformation:
    /// - Exercises the SafeNative rule that completed work cannot release more
    ///   credits than the bridge has reserved.
    #[test]
    fn release_credit_rejects_underflow() {
        assert_eq!(release_credit(1, 2), None);
        assert_eq!(release_credit(3, 2), Some(1));
    }
}
