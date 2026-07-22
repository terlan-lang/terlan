use super::{VmReferenceAllocationError, VmReferenceAllocator};

#[test]
fn reference_allocator_validates_namespace_and_allocates_stable_identities() {
    assert_eq!(
        VmReferenceAllocator::new("   ", 1).expect_err("blank node must fail"),
        "VM reference node id must not be empty"
    );
    assert_eq!(
        VmReferenceAllocator::new("node-a", 0).expect_err("zero epoch must fail"),
        "VM reference epoch must be non-zero"
    );
    assert_eq!(
        VmReferenceAllocator::with_limits("node-a", 1, 1, -1)
            .expect_err("negative integer limit must fail"),
        "VM unique-integer limit must not be negative"
    );

    let mut first_runtime = VmReferenceAllocator::new("node-a", 3).expect("first runtime");
    let mut remote_runtime = VmReferenceAllocator::new("node-b", 3).expect("remote runtime");
    let mut rebooted_runtime = VmReferenceAllocator::new("node-a", 4).expect("rebooted runtime");
    let first = first_runtime.allocate_reference().expect("first reference");
    let second = first_runtime
        .allocate_reference()
        .expect("second reference");
    let remote = remote_runtime
        .allocate_reference()
        .expect("remote reference");
    let rebooted = rebooted_runtime
        .allocate_reference()
        .expect("rebooted reference");

    assert_eq!(first.node_id(), "node-a");
    assert_eq!(first.epoch(), 3);
    assert_eq!(first.as_u64(), 1);
    assert_eq!(second.as_u64(), 2);
    assert_ne!(first, remote);
    assert_ne!(first, rebooted);
    assert_eq!(first_runtime.allocate_unique_integer(), Ok(1));
    assert_eq!(first_runtime.allocate_unique_integer(), Ok(2));
}

#[test]
fn reference_allocator_reports_exhaustion_without_wrapping_or_reuse() {
    let mut references =
        VmReferenceAllocator::with_limits("node-a", 4, 2, 2).expect("bounded allocator");

    assert_eq!(references.allocate_reference().map(|id| id.as_u64()), Ok(1));
    assert_eq!(references.allocate_reference().map(|id| id.as_u64()), Ok(2));
    for _ in 0..2 {
        let error = references
            .allocate_reference()
            .expect_err("reference sequence must not wrap");
        assert_eq!(
            error,
            VmReferenceAllocationError::ReferenceSequenceExhausted
        );
        assert_eq!(error.to_string(), "VM reference sequence exhausted");
    }

    assert_eq!(references.allocate_unique_integer(), Ok(1));
    assert_eq!(references.allocate_unique_integer(), Ok(2));
    for _ in 0..2 {
        let error = references
            .allocate_unique_integer()
            .expect_err("integer sequence must not wrap");
        assert_eq!(
            error,
            VmReferenceAllocationError::UniqueIntegerSequenceExhausted
        );
        assert_eq!(error.to_string(), "VM unique-integer sequence exhausted");
    }
}

#[test]
fn reference_allocator_remains_unique_and_ordered_under_high_churn() {
    const ALLOCATIONS: u64 = 65_536;

    let mut references = VmReferenceAllocator::new("churn-node", 17).expect("reference namespace");
    let mut previous = references.allocate_reference().expect("first reference");

    for expected in 2..=ALLOCATIONS {
        let current = references
            .allocate_reference()
            .expect("reference allocation must not wrap");
        assert_eq!(current.as_u64(), expected);
        assert!(current > previous);
        previous = current;
    }
}

#[test]
fn unique_integer_sequence_is_monotonic_and_independent_under_high_churn() {
    const ALLOCATIONS: i64 = 65_536;

    let mut references = VmReferenceAllocator::new("integer-node", 23).expect("integer namespace");

    for expected in 1..=ALLOCATIONS {
        if expected % 257 == 0 {
            references
                .allocate_reference()
                .expect("reference allocation must use an independent sequence");
        }
        assert_eq!(references.allocate_unique_integer(), Ok(expected));
    }

    assert_eq!(
        references
            .allocate_reference()
            .expect("reference sequence remains independent")
            .as_u64(),
        u64::try_from(ALLOCATIONS / 257).expect("allocation count fits u64") + 1
    );
}
