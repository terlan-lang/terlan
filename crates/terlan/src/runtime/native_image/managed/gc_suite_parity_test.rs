//! Portable managed-memory contracts retained from OTP `gc_SUITE`.

use std::sync::Arc;

use super::{
    ActorHeap, ActorId, AllocationClass, HeapLimits, ManagedMemoryError, ManagedRoot,
    ManagedTypeDescriptor, RootLocation, SemanticTypeId,
};

fn actor(value: u64) -> ActorId {
    ActorId::new(value).expect("valid actor")
}

fn descriptor(
    name: &str,
    size: usize,
    reference_offsets: Vec<usize>,
) -> Arc<ManagedTypeDescriptor> {
    Arc::new(
        ManagedTypeDescriptor::new(
            SemanticTypeId::from_canonical(name).expect("semantic identity"),
            size,
            8,
            reference_offsets,
            AllocationClass::Young,
        )
        .expect("managed descriptor"),
    )
}

#[test]
fn gc_suite_deep_heap_growth_relocates_the_live_chain_then_reclaims_it() {
    const DEPTH: u64 = 2_048;

    let owner = actor(201);
    let mut heap = ActorHeap::new(
        owner,
        HeapLimits::new(4 * 1024, 256 * 1024).expect("limits"),
    )
    .expect("heap");
    let leaf = heap
        .allocate::<u64>(descriptor("gc.Leaf", 8, vec![]), &42_u64.to_le_bytes(), &[])
        .expect("leaf");
    let node = descriptor("gc.Node", 16, vec![0]);
    let mut head = leaf.erase();
    for value in 1..=DEPTH {
        let mut payload = [0_u8; 16];
        payload[8..].copy_from_slice(&value.to_le_bytes());
        head = heap
            .allocate::<u64>(node.clone(), &payload, &[(0, head)])
            .expect("chain node")
            .erase();
    }
    assert!(heap.should_collect());
    assert_eq!(heap.object_count(), DEPTH as usize + 1);

    let old_head = head;
    let mut roots = [ManagedRoot::new(
        owner,
        RootLocation::ActorState { slot: 0 },
        head,
    )];
    let stats = heap
        .collect(&mut roots, 256 * 1024)
        .expect("collect deep live chain");

    assert_eq!(stats.objects_before, DEPTH as usize + 1);
    assert_eq!(stats.objects_after, DEPTH as usize + 1);
    assert_eq!(heap.read(old_head), Err(ManagedMemoryError::StaleReference));
    let relocated_head = roots[0].reference();
    let mut cursor = relocated_head;
    for expected in (1..=DEPTH).rev() {
        let payload = heap.read(cursor).expect("live node");
        assert_eq!(
            u64::from_le_bytes(payload[8..].try_into().expect("node value")),
            expected
        );
        cursor = heap.reference_field(cursor, 0).expect("next node");
    }
    assert_eq!(heap.read(cursor).expect("live leaf"), 42_u64.to_le_bytes());

    let mut stack_root = [ManagedRoot::new(
        owner,
        RootLocation::NativeStack {
            function_id: 7,
            slot: 0,
        },
        cursor,
    )];
    let collapsed = heap
        .collect(&mut stack_root, 4 * 1024)
        .expect("drop the unwound chain");
    assert_eq!(collapsed.objects_before, DEPTH as usize + 1);
    assert_eq!(collapsed.objects_after, 1);
    assert_eq!(heap.object_count(), 1);
    assert_eq!(
        heap.read(relocated_head),
        Err(ManagedMemoryError::StaleReference)
    );
    assert_eq!(
        heap.read(stack_root[0].reference()).expect("retained leaf"),
        42_u64.to_le_bytes()
    );
}

#[test]
fn gc_suite_all_precise_root_classes_survive_stack_churn_and_empty_collection() {
    let owner = actor(202);
    let mut heap =
        ActorHeap::new(owner, HeapLimits::new(64, 16 * 1024).expect("limits")).expect("heap");
    let scalar = descriptor("gc.RootValue", 8, vec![]);
    let locations = [
        RootLocation::ActorState { slot: 0 },
        RootLocation::NativeStack {
            function_id: 11,
            slot: 3,
        },
        RootLocation::Continuation {
            continuation_id: 13,
            slot: 1,
        },
        RootLocation::RuntimeFrame {
            frame_id: 17,
            slot: 2,
        },
        RootLocation::Mailbox {
            fragment: 19,
            slot: 0,
        },
    ];
    let mut roots = locations
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, location)| {
            let value = (index as u64 + 1) * 10;
            let reference = heap
                .allocate::<u64>(scalar.clone(), &value.to_le_bytes(), &[])
                .expect("root value");
            ManagedRoot::new(owner, location, reference.erase())
        })
        .collect::<Vec<_>>();
    for _ in 0..128 {
        heap.allocate::<u64>(scalar.clone(), &999_u64.to_le_bytes(), &[])
            .expect("temporary stack value");
    }

    let stats = heap
        .collect(&mut roots, 16 * 1024)
        .expect("collect all precise root classes");
    assert_eq!(stats.objects_before, 133);
    assert_eq!(stats.objects_after, 5);
    for (index, root) in roots.iter().enumerate() {
        assert_eq!(root.location(), &locations[index]);
        assert_eq!(
            heap.read(root.reference()).expect("relocated root"),
            ((index as u64 + 1) * 10).to_le_bytes()
        );
    }

    let prior_roots = roots.iter().map(ManagedRoot::reference).collect::<Vec<_>>();
    let released = heap.collect(&mut [], 1).expect("empty root collection");
    assert_eq!(released.objects_before, 5);
    assert_eq!(released.objects_after, 0);
    assert_eq!(heap.allocated_bytes(), 0);
    assert_eq!(heap.object_count(), 0);
    for reference in prior_roots {
        assert_eq!(
            heap.read(reference),
            Err(ManagedMemoryError::StaleReference)
        );
    }
}

#[test]
fn gc_suite_hard_limit_and_collection_budget_fail_without_partial_mutation() {
    let owner = actor(203);
    let mut heap = ActorHeap::new(owner, HeapLimits::new(64, 256).expect("limits")).expect("heap");
    let block = descriptor("gc.Block", 128, vec![]);
    let live = heap
        .allocate::<u64>(block.clone(), &[7; 128], &[])
        .expect("live block");
    let before = (heap.allocated_bytes(), heap.object_count());

    let second = heap
        .allocate::<u64>(block, &[9; 128], &[])
        .expect("allocation exactly at the hard limit");
    let after_two = (heap.allocated_bytes(), heap.object_count());
    assert_eq!(after_two, (256, 2));
    assert_ne!(after_two, before);
    assert_eq!(heap.read(second).expect("second block"), &[9; 128]);

    let overflow = heap.allocate::<u64>(
        descriptor("gc.Overflow", 8, vec![]),
        &8_u64.to_le_bytes(),
        &[],
    );
    assert_eq!(overflow, Err(ManagedMemoryError::AllocationLimitExceeded));
    assert_eq!((heap.allocated_bytes(), heap.object_count()), after_two);

    let mut roots = [ManagedRoot::new(
        owner,
        RootLocation::ActorState { slot: 0 },
        live.erase(),
    )];
    let original_root = roots[0].reference();
    assert_eq!(
        heap.collect(&mut roots, 1),
        Err(ManagedMemoryError::CollectionBudgetExceeded)
    );
    assert_eq!(roots[0].reference(), original_root);
    assert_eq!((heap.allocated_bytes(), heap.object_count()), after_two);
    assert_eq!(heap.collection_count(), 0);
}
