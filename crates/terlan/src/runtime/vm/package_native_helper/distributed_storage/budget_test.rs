//! Ownership budgets cannot be bypassed by creating more adapters or retained views.

use super::*;

#[test]
fn owner_and_runtime_byte_limits_are_independent_and_reclaimed() {
    let mut budget = Budget::default();
    for owner in 0..4 {
        budget.check(owner, OWNER_BYTES).unwrap();
        budget.charge(owner, OWNER_BYTES);
        assert!(budget.check(owner, 1).is_err());
    }
    assert!(budget.check(5, 1).is_err());
    budget.release(1);
    budget.check(5, OWNER_BYTES).unwrap();
    budget.charge(5, OWNER_BYTES);
    assert!(budget.check(1, 1).is_err());
    budget.release(1); // Repeated cleanup cannot release another owner's capacity.
    assert!(budget.check(1, 1).is_err());
    budget.release(5);
    budget.check(1, OWNER_BYTES).unwrap();
    assert!(budget.check(1, usize::MAX).is_err());
}

#[test]
fn tiny_resources_still_have_owner_and_runtime_count_bounds() {
    let mut budget = Budget::default();
    for owner in 0..4 {
        for _ in 0..OWNER_RESOURCES {
            budget.check(owner, 1).unwrap();
            budget.charge(owner, 1);
        }
        assert!(budget.check(owner, 0).is_err());
    }
    assert!(budget.check(5, 0).is_err());
    budget.release(0);
    budget.check(5, 1).unwrap();
}
