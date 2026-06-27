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
