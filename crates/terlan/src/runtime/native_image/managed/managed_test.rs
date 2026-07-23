use std::sync::Arc;

use super::*;

fn actor(value: u64) -> ActorId {
    ActorId::new(value).expect("valid actor")
}

fn limits() -> HeapLimits {
    HeapLimits::new(128, 4096).expect("valid limits")
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
fn managed_layout_identity_is_deterministic_and_physical() {
    let semantic = SemanticTypeId::from_canonical("example.Node[Int]").expect("semantic id");
    let first = ManagedTypeDescriptor::new(semantic, 16, 8, vec![0], AllocationClass::Young)
        .expect("first descriptor");
    let replay = ManagedTypeDescriptor::new(semantic, 16, 8, vec![0], AllocationClass::Young)
        .expect("replayed descriptor");
    let large = ManagedTypeDescriptor::new(semantic, 16, 8, vec![0], AllocationClass::Large)
        .expect("large descriptor");

    assert_eq!(first.semantic_id(), semantic);
    assert_eq!(first.fingerprint(), replay.fingerprint());
    assert_ne!(first.fingerprint(), large.fingerprint());
    assert_eq!(first.size(), 16);
    assert_eq!(first.alignment(), 8);
    assert_eq!(first.reference_offsets(), &[0]);
    assert_eq!(large.allocation_class(), AllocationClass::Large);
    assert_ne!(semantic.bytes(), [0; 16]);
    assert_ne!(first.fingerprint().bytes(), [0; 32]);
}

#[test]
fn managed_layout_rejects_invalid_shapes() {
    let semantic = SemanticTypeId::from_canonical("example.Bad").expect("semantic id");
    assert_eq!(
        SemanticTypeId::from_canonical(""),
        Err(ManagedMemoryError::EmptySemanticIdentity)
    );
    assert_eq!(
        ManagedTypeDescriptor::new(semantic, 0, 8, vec![], AllocationClass::Young),
        Err(ManagedMemoryError::InvalidLayoutSize)
    );
    assert_eq!(
        ManagedTypeDescriptor::new(semantic, 8, 3, vec![], AllocationClass::Young),
        Err(ManagedMemoryError::InvalidLayoutAlignment)
    );
    assert_eq!(
        ManagedTypeDescriptor::new(semantic, 16, 8, vec![8, 0], AllocationClass::Young),
        Err(ManagedMemoryError::InvalidReferenceMap)
    );
    assert_eq!(
        ManagedTypeDescriptor::new(semantic, 16, 8, vec![1], AllocationClass::Young),
        Err(ManagedMemoryError::InvalidReferenceMap)
    );
    assert_eq!(
        ManagedTypeDescriptor::new(semantic, 8, 8, vec![8], AllocationClass::Young),
        Err(ManagedMemoryError::InvalidReferenceMap)
    );
}

#[test]
fn minimal_managed_object_survives_relocation_and_typed_continuation() {
    let owner = actor(7);
    let mut heap = ActorHeap::new(owner, limits()).expect("actor heap");
    let leaf = heap
        .allocate::<u64>(
            descriptor("example.Leaf", 8, vec![]),
            &42_u64.to_le_bytes(),
            &[],
        )
        .expect("allocate leaf");
    let old_leaf = leaf;
    let mut continuation =
        ManagedContinuation::capture(owner, 91, vec![leaf.erase()]).expect("capture continuation");

    let stats = heap
        .collect(continuation.captures_mut(), 4096)
        .expect("collect managed heap");
    let relocated = continuation.captures()[0].reference();

    assert_eq!(continuation.owner(), owner);
    assert_eq!(continuation.continuation_id(), 91);
    assert_ne!(old_leaf.erase(), relocated);
    assert_eq!(
        heap.read(relocated).expect("read relocated leaf"),
        42_u64.to_le_bytes()
    );
    assert_eq!(stats.objects_before, 1);
    assert_eq!(stats.objects_after, 1);
    assert_eq!(heap.collection_count(), 1);
    assert_eq!(heap.read(old_leaf), Err(ManagedMemoryError::StaleReference));
}

#[test]
fn precise_collection_relocates_graph_fields_and_reclaims_unreachable_objects() {
    let owner = actor(8);
    let mut heap = ActorHeap::new(owner, limits()).expect("actor heap");
    let leaf_descriptor = descriptor("example.GraphLeaf", 8, vec![]);
    let leaf = heap
        .allocate::<u64>(leaf_descriptor.clone(), &11_u64.to_le_bytes(), &[])
        .expect("allocate live leaf");
    let dead = heap
        .allocate::<u64>(leaf_descriptor, &99_u64.to_le_bytes(), &[])
        .expect("allocate dead leaf");
    let parent_descriptor = descriptor("example.Parent", 16, vec![0]);
    let mut payload = [0_u8; 16];
    payload[8..].copy_from_slice(&77_u64.to_le_bytes());
    let parent = heap
        .allocate::<u64>(parent_descriptor, &payload, &[(0, leaf.erase())])
        .expect("allocate parent");
    let mut roots = [ManagedRoot::new(
        owner,
        RootLocation::ActorState { slot: 0 },
        parent.erase(),
    )];

    let stats = heap.collect(&mut roots, 4096).expect("collect graph");
    let relocated_parent = roots[0].reference();
    let relocated_leaf = heap
        .reference_field(relocated_parent, 0)
        .expect("read relocated child");

    assert_eq!(stats.objects_before, 3);
    assert_eq!(stats.objects_after, 2);
    assert_eq!(heap.object_count(), 2);
    assert_eq!(
        heap.read(relocated_leaf).expect("live child"),
        11_u64.to_le_bytes()
    );
    assert_eq!(heap.read(dead), Err(ManagedMemoryError::StaleReference));
    assert_eq!(
        heap.read(relocated_parent).expect("parent")[8..],
        77_u64.to_le_bytes()
    );
}

#[test]
fn bounded_collection_fails_atomically_before_relocation() {
    let owner = actor(9);
    let mut heap = ActorHeap::new(owner, limits()).expect("actor heap");
    let value = heap
        .allocate::<u64>(descriptor("example.Budget", 64, vec![]), &[3; 64], &[])
        .expect("allocate value");
    let mut roots = [ManagedRoot::new(
        owner,
        RootLocation::RuntimeFrame {
            frame_id: 4,
            slot: 0,
        },
        value.erase(),
    )];
    let original_root = roots[0].reference();

    assert_eq!(
        heap.collect(&mut roots, 1),
        Err(ManagedMemoryError::CollectionBudgetExceeded)
    );
    assert_eq!(roots[0].reference(), original_root);
    assert_eq!(heap.collection_count(), 0);
    assert_eq!(heap.read(value).expect("value remains live"), &[3; 64]);
}

#[test]
fn actor_local_collection_budget_cannot_pause_or_mutate_another_heap() {
    let first_owner = actor(90);
    let second_owner = actor(91);
    let mut first = ActorHeap::new(first_owner, limits()).expect("first heap");
    let mut second = ActorHeap::new(second_owner, limits()).expect("second heap");
    let first_value = first
        .allocate::<u64>(descriptor("example.First", 64, vec![]), &[1; 64], &[])
        .expect("first value");
    let second_value = second
        .allocate::<u64>(descriptor("example.Second", 8, vec![]), &[2; 8], &[])
        .expect("second value");
    let mut first_roots = [ManagedRoot::new(
        first_owner,
        RootLocation::ActorState { slot: 0 },
        first_value.erase(),
    )];
    let mut second_roots = [ManagedRoot::new(
        second_owner,
        RootLocation::ActorState { slot: 0 },
        second_value.erase(),
    )];
    let second_root_before = second_roots[0].reference();

    assert_eq!(
        first.collect(&mut first_roots, 1),
        Err(ManagedMemoryError::CollectionBudgetExceeded)
    );
    assert_eq!(second.collection_count(), 0);
    assert_eq!(second_roots[0].reference(), second_root_before);
    assert_eq!(
        second
            .read(second_value)
            .expect("second actor remains live"),
        &[2; 8]
    );

    second
        .collect(&mut second_roots, 4096)
        .expect("second actor collects independently");
    assert_eq!(second.collection_count(), 1);
    assert_eq!(first.collection_count(), 0);
    assert_eq!(
        first.read(first_value).expect("first actor remains live"),
        &[1; 64]
    );
}

#[test]
fn heap_rejects_cross_actor_stale_and_unknown_references() {
    let first_owner = actor(10);
    let second_owner = actor(11);
    let mut first = ActorHeap::new(first_owner, limits()).expect("first heap");
    let second = ActorHeap::new(second_owner, limits()).expect("second heap");
    let value = first
        .allocate::<u64>(descriptor("example.Owner", 8, vec![]), &[0; 8], &[])
        .expect("allocate first value");

    assert_eq!(
        second.read(value),
        Err(ManagedMemoryError::CrossActorReference)
    );
    let mut wrong_owner = [ManagedRoot::new(
        second_owner,
        RootLocation::ActorState { slot: 0 },
        value.erase(),
    )];
    assert_eq!(
        first.collect(&mut wrong_owner, 4096),
        Err(ManagedMemoryError::CrossActorReference)
    );

    let mut roots = [ManagedRoot::new(
        first_owner,
        RootLocation::ActorState { slot: 0 },
        value.erase(),
    )];
    first.collect(&mut roots, 4096).expect("relocate value");
    assert_eq!(first.read(value), Err(ManagedMemoryError::StaleReference));
}

#[test]
fn allocation_enforces_layout_reference_map_and_hard_limit() {
    let owner = actor(12);
    let mut heap =
        ActorHeap::new(owner, HeapLimits::new(16, 24).expect("limits")).expect("actor heap");
    let leaf = heap
        .allocate::<u64>(descriptor("example.Small", 8, vec![]), &[1; 8], &[])
        .expect("leaf");
    assert!(!heap.should_collect());
    assert_eq!(
        heap.allocate::<u64>(descriptor("example.WrongSize", 8, vec![]), &[0; 7], &[]),
        Err(ManagedMemoryError::LayoutMismatch)
    );
    assert_eq!(
        heap.allocate::<u64>(descriptor("example.Ref", 8, vec![0]), &[0; 8], &[]),
        Err(ManagedMemoryError::InvalidReferenceMap)
    );
    heap.allocate::<u64>(descriptor("example.Second", 8, vec![]), &[2; 8], &[])
        .expect("second allocation");
    assert!(heap.should_collect());
    assert_eq!(
        heap.allocate::<u64>(descriptor("example.TooLarge", 16, vec![]), &[0; 16], &[]),
        Err(ManagedMemoryError::AllocationLimitExceeded)
    );
    assert_eq!(heap.descriptor(leaf).expect("descriptor").size(), 8);
    assert_eq!(
        heap.reference_field(leaf, 0),
        Err(ManagedMemoryError::InvalidReferenceMap)
    );
}

#[test]
fn direct_heap_initialization_rolls_back_failed_payloads() {
    let owner = actor(16);
    let mut heap = ActorHeap::new(owner, limits()).expect("actor heap");
    heap.allocate::<u64>(descriptor("example.Keep", 8, vec![]), &[7; 8], &[])
        .expect("retained object");
    let bytes_before = heap.allocated_bytes();
    let objects_before = heap.object_count();

    let result =
        heap.allocate_initialized::<u64>(descriptor("example.Direct", 8, vec![]), &[], |payload| {
            payload.copy_from_slice(&[9; 8]);
            Err(ManagedMemoryError::CorruptedCollection)
        });

    assert_eq!(result, Err(ManagedMemoryError::CorruptedCollection));
    assert_eq!(heap.allocated_bytes(), bytes_before);
    assert_eq!(heap.object_count(), objects_before);
}

#[test]
fn precise_stack_maps_reject_missing_duplicate_derived_and_borrowed_roots() {
    let record = StackMapRecord::new(
        17,
        3,
        4,
        vec![
            StackMapEntry {
                slot: 0,
                kind: StackRootKind::ActorLocal,
            },
            StackMapEntry {
                slot: 1,
                kind: StackRootKind::Derived {
                    base_slot: 0,
                    byte_offset: 8,
                },
            },
            StackMapEntry {
                slot: 2,
                kind: StackRootKind::Shared,
            },
        ],
    )
    .expect("valid stack map");
    assert_eq!(record.function_id(), 17);
    assert_eq!(record.safepoint_id(), 3);
    assert_eq!(record.frame_words(), 4);
    assert_eq!(record.entries().len(), 3);

    let table = StackMapTable::new(vec![record.clone()]).expect("stack map table");
    assert_eq!(table.len(), 1);
    assert!(!table.is_empty());
    assert_eq!(table.require(17, 3), Ok(&record));
    assert_eq!(
        table.require(17, 4),
        Err(ManagedMemoryError::MissingStackMap)
    );
    assert_eq!(
        StackMapTable::new(vec![record.clone(), record]),
        Err(ManagedMemoryError::InvalidStackMap)
    );
    assert_eq!(
        StackMapRecord::new(
            17,
            4,
            2,
            vec![StackMapEntry {
                slot: 0,
                kind: StackRootKind::Borrowed,
            }]
        ),
        Err(ManagedMemoryError::BorrowedValueAtSafepoint)
    );
    assert_eq!(
        StackMapRecord::new(
            17,
            4,
            2,
            vec![StackMapEntry {
                slot: 1,
                kind: StackRootKind::Derived {
                    base_slot: 0,
                    byte_offset: 4,
                },
            }]
        ),
        Err(ManagedMemoryError::InvalidStackMap)
    );
}

#[test]
fn corrupted_embedded_reference_is_rejected_before_heap_mutation() {
    let owner = actor(13);
    let mut heap = ActorHeap::new(owner, limits()).expect("actor heap");
    let leaf = heap
        .allocate::<u64>(descriptor("example.CorruptLeaf", 8, vec![]), &[5; 8], &[])
        .expect("leaf");
    let parent = heap
        .allocate::<u64>(
            descriptor("example.CorruptParent", 8, vec![0]),
            &[0; 8],
            &[(0, leaf.erase())],
        )
        .expect("parent");
    heap.corrupt_reference(parent, 0, 1);
    let mut roots = [ManagedRoot::new(
        owner,
        RootLocation::Mailbox {
            fragment: 2,
            slot: 0,
        },
        parent.erase(),
    )];

    assert_eq!(
        heap.collect(&mut roots, 4096),
        Err(ManagedMemoryError::CorruptedRelocationMetadata)
    );
    assert_eq!(heap.object_count(), 2);
    assert_eq!(heap.collection_count(), 0);
}

#[test]
fn repeated_collection_and_actor_exit_reclaim_all_managed_storage() {
    let owner = actor(14);
    let mut heap = ActorHeap::new(owner, limits()).expect("actor heap");
    let initial = heap
        .allocate::<u64>(descriptor("example.Churn", 8, vec![]), &[7; 8], &[])
        .expect("initial object");
    let mut roots = [ManagedRoot::new(
        owner,
        RootLocation::NativeStack {
            function_id: 1,
            slot: 0,
        },
        initial.erase(),
    )];
    for _ in 0..32 {
        heap.allocate::<u64>(descriptor("example.Garbage", 8, vec![]), &[9; 8], &[])
            .expect("garbage object");
        heap.collect(&mut roots, 4096).expect("bounded collection");
        assert_eq!(heap.object_count(), 1);
        assert_eq!(heap.read(roots[0].reference()).expect("live root"), &[7; 8]);
    }
    let last = roots[0].reference();
    assert_eq!(heap.collection_count(), 32);
    heap.reclaim_all();
    assert_eq!(heap.allocated_bytes(), 0);
    assert_eq!(heap.object_count(), 0);
    assert_eq!(heap.read(last), Err(ManagedMemoryError::StaleReference));
}

#[test]
fn public_managed_reference_is_pointer_width_and_opaque() {
    let owner = actor(15);
    let mut heap = ActorHeap::new(owner, limits()).expect("actor heap");
    let value = heap
        .allocate::<u64>(descriptor("example.Opaque", 8, vec![]), &[0; 8], &[])
        .expect("managed value");
    assert_eq!(
        std::mem::size_of::<TvmRef<u64>>(),
        std::mem::size_of::<usize>()
    );
    assert_eq!(format!("{value:?}"), "TvmRef(<opaque>)");
    assert_eq!(ActorId::new(0), Err(ManagedMemoryError::InvalidActorId));
    assert_eq!(actor(15).get(), 15);
    assert_eq!(
        HeapLimits::new(0, 1),
        Err(ManagedMemoryError::AllocationLimitExceeded)
    );
    assert_eq!(
        HeapLimits::new(2, 1),
        Err(ManagedMemoryError::AllocationLimitExceeded)
    );
    assert_eq!(
        ManagedContinuation::capture(owner, 0, vec![value.erase()]),
        Err(ManagedMemoryError::InvalidContinuation)
    );
}
