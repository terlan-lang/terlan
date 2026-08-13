use super::*;

#[test]
fn release_policy_covers_exceptional_floating_values_and_determinism() {
    let strict = AcceleratorNumericPolicy::release(AcceleratorDeterminism::Strict);
    assert_eq!(strict.validate(), Ok(()));
    assert!(strict.floating.equivalent(f64::NAN, f64::NAN));
    assert!(strict.floating.equivalent(f64::INFINITY, f64::INFINITY));
    assert!(!strict.floating.equivalent(f64::INFINITY, f64::NEG_INFINITY));
    assert!(!strict.floating.equivalent(0.0, -0.0));
    assert!(strict.floating.equivalent(1.0, 1.0 + 1.0e-7));
    assert!(!strict.floating.equivalent(1.0, 1.1));
    assert_eq!(strict.reduction_order, AcceleratorReductionOrder::FixedTree);
}

#[test]
fn malformed_policies_fail_closed() {
    let mut policy = AcceleratorNumericPolicy::release(AcceleratorDeterminism::Strict);
    policy.floating.absolute_tolerance = f64::NAN;
    assert!(policy.validate().is_err());
    let mut policy = AcceleratorNumericPolicy::release(AcceleratorDeterminism::Strict);
    policy.canonical_boolean_storage = false;
    assert!(policy.validate().is_err());
    let mut policy = AcceleratorNumericPolicy::release(AcceleratorDeterminism::Strict);
    policy.reduction_order = AcceleratorReductionOrder::Unspecified;
    assert!(policy.validate().is_err());
}
