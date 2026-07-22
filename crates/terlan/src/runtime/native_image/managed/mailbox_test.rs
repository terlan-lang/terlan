use std::sync::Arc;

use super::{
    ActorHeap, ActorId, HeapLimits, ManagedAggregateDescriptor, ManagedFieldType,
    ManagedFieldValue, ManagedMemoryError, RootLocation, SemanticTypeId,
};

/// Creates one actor heap with explicit transfer-test limits.
fn heap(owner: u64, soft_bytes: usize, hard_bytes: usize) -> ActorHeap {
    ActorHeap::new(
        ActorId::new(owner).expect("actor"),
        HeapLimits::new(soft_bytes, hard_bytes).expect("limits"),
    )
    .expect("heap")
}

/// Builds a two-object graph whose parent references one shared leaf twice.
fn shared_graph(
    source: &mut ActorHeap,
) -> (
    Arc<ManagedAggregateDescriptor>,
    Arc<ManagedAggregateDescriptor>,
    super::TvmRef<super::ManagedAggregate>,
) {
    let leaf = Arc::new(
        ManagedAggregateDescriptor::tuple("app.Leaf", vec![ManagedFieldType::Int])
            .expect("leaf descriptor"),
    );
    let parent = Arc::new(
        ManagedAggregateDescriptor::record(
            "app.Shared",
            vec![
                (
                    "left".to_owned(),
                    ManagedFieldType::Reference(leaf.managed().semantic_id()),
                ),
                (
                    "right".to_owned(),
                    ManagedFieldType::Reference(leaf.managed().semantic_id()),
                ),
            ],
        )
        .expect("parent descriptor"),
    );
    let leaf_value = source
        .allocate_aggregate(leaf.clone(), &[ManagedFieldValue::Int(41)])
        .expect("leaf");
    let parent_value = source
        .allocate_aggregate(
            parent.clone(),
            &[
                ManagedFieldValue::Reference(leaf_value.erase()),
                ManagedFieldValue::Reference(leaf_value.erase()),
            ],
        )
        .expect("parent");
    (leaf, parent, parent_value)
}

/// Copies a shared graph once and publishes one receiver-owned precise root.
#[test]
fn mailbox_transfer_preserves_sharing_and_receiver_ownership() {
    let mut source = heap(61, 256, 1024 * 1024);
    let mut receiver = heap(62, 256, 1024 * 1024);
    let (leaf, parent, source_root) = shared_graph(&mut source);

    let fragment = source
        .copy_message_graph_to(
            source_root.erase(),
            parent.managed().semantic_id(),
            &mut receiver,
            7,
            4096,
        )
        .expect("copy graph");

    assert_eq!(fragment.sender(), source.owner());
    assert_eq!(fragment.receiver(), receiver.owner());
    assert_eq!(fragment.fragment_id(), 7);
    assert_eq!(fragment.copied_objects(), 2);
    assert_eq!(fragment.copied_payload_bytes(), 24);
    assert_eq!(source.object_count(), 2);
    assert_eq!(receiver.object_count(), 2);
    assert_eq!(fragment.root().owner(), receiver.owner());
    assert_eq!(
        fragment.root().location(),
        &RootLocation::Mailbox {
            fragment: 7,
            slot: 0,
        }
    );
    let left = receiver
        .reference_field(fragment.root_reference(), parent.fields()[0].offset())
        .expect("left reference");
    let right = receiver
        .reference_field(fragment.root_reference(), parent.fields()[1].offset())
        .expect("right reference");
    assert_eq!(left, right);
    assert_eq!(
        receiver
            .descriptor(left)
            .expect("leaf descriptor")
            .semantic_id(),
        leaf.managed().semantic_id()
    );
    assert_eq!(
        source.read(fragment.root_reference()),
        Err(ManagedMemoryError::CrossActorReference)
    );
}

/// Keeps the mailbox root valid when the receiver performs moving collection.
#[test]
fn mailbox_fragment_root_relocates_with_receiver_collection() {
    let mut source = heap(63, 256, 1024 * 1024);
    let mut receiver = heap(64, 256, 1024 * 1024);
    let (_, parent, source_root) = shared_graph(&mut source);
    let mut fragment = source
        .copy_message_graph_to(
            source_root.erase(),
            parent.managed().semantic_id(),
            &mut receiver,
            8,
            4096,
        )
        .expect("copy graph");
    let before = fragment.root_reference();

    let stats = receiver
        .collect(fragment.roots_mut(), 4096)
        .expect("collect mailbox graph");

    assert_eq!(stats.objects_after, 2);
    assert_ne!(fragment.root_reference(), before);
    assert_eq!(
        receiver.read(before),
        Err(ManagedMemoryError::StaleReference)
    );
    assert_eq!(
        receiver
            .descriptor(fragment.root_reference())
            .expect("relocated root")
            .semantic_id(),
        parent.managed().semantic_id()
    );
}

/// Leaves both heaps unchanged when admission or staged allocation fails.
#[test]
fn mailbox_transfer_failure_is_atomic_for_budget_limit_type_and_owner() {
    let mut source = heap(65, 256, 1024 * 1024);
    let mut receiver = heap(66, 8, 8);
    let (_, parent, source_root) = shared_graph(&mut source);
    let source_before = (source.allocated_bytes(), source.object_count());
    let receiver_before = (receiver.allocated_bytes(), receiver.object_count());

    assert_eq!(
        source.copy_message_graph_to(
            source_root.erase(),
            parent.managed().semantic_id(),
            &mut receiver,
            9,
            1,
        ),
        Err(ManagedMemoryError::MessageTransferBudgetExceeded)
    );
    assert_eq!(
        source.copy_message_graph_to(
            source_root.erase(),
            parent.managed().semantic_id(),
            &mut receiver,
            9,
            4096,
        ),
        Err(ManagedMemoryError::AllocationLimitExceeded)
    );
    assert_eq!(
        source.copy_message_graph_to(
            source_root.erase(),
            SemanticTypeId::from_canonical("app.Wrong").expect("wrong type"),
            &mut receiver,
            9,
            4096,
        ),
        Err(ManagedMemoryError::ManagedTypeMismatch)
    );
    assert_eq!(
        source_before,
        (source.allocated_bytes(), source.object_count())
    );
    assert_eq!(
        receiver_before,
        (receiver.allocated_bytes(), receiver.object_count())
    );

    let mut foreign = heap(67, 256, 1024 * 1024);
    let (_, foreign_parent, foreign_root) = shared_graph(&mut foreign);
    assert_eq!(
        source.copy_message_graph_to(
            foreign_root.erase(),
            foreign_parent.managed().semantic_id(),
            &mut receiver,
            9,
            4096,
        ),
        Err(ManagedMemoryError::CrossActorReference)
    );
}

/// Traverses deep recursive algebraic graphs without native recursion.
#[test]
fn mailbox_transfer_handles_deep_recursive_graph_iteratively() {
    const DEPTH: usize = 2_000;
    let mut source = heap(68, 4096, 8 * 1024 * 1024);
    let mut receiver = heap(69, 4096, 8 * 1024 * 1024);
    let semantic = SemanticTypeId::from_canonical("app.Node").expect("node semantic");
    let empty = Arc::new(
        ManagedAggregateDescriptor::constructor("app.Node", "Empty", 0, 2, Vec::new())
            .expect("empty descriptor"),
    );
    let next = Arc::new(
        ManagedAggregateDescriptor::constructor(
            "app.Node",
            "Next",
            1,
            2,
            vec![(
                Some("tail".to_owned()),
                ManagedFieldType::Reference(semantic),
            )],
        )
        .expect("next descriptor"),
    );
    let mut root = source.allocate_aggregate(empty, &[]).expect("empty node");
    for _ in 0..DEPTH {
        root = source
            .allocate_aggregate(next.clone(), &[ManagedFieldValue::Reference(root.erase())])
            .expect("next node");
    }

    let fragment = source
        .copy_message_graph_to(root.erase(), semantic, &mut receiver, 10, 1024 * 1024)
        .expect("copy deep graph");

    assert_eq!(fragment.copied_objects(), DEPTH + 1);
    assert_eq!(receiver.object_count(), DEPTH + 1);
    assert_eq!(
        receiver
            .descriptor(fragment.root_reference())
            .expect("deep root")
            .semantic_id(),
        semantic
    );
}
