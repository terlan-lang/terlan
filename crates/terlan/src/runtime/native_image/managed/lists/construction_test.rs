use super::*;
use crate::runtime::native_image::managed::{ActorId, HeapLimits, ManagedRoot, RootLocation};

fn heap(hard: usize) -> ActorHeap {
    ActorHeap::new(
        ActorId::new(941).unwrap(),
        HeapLimits::new(hard.min(4096), hard).unwrap(),
    )
    .unwrap()
}

/// A failed late leaf must not retain earlier nodes or disturb an existing value.
#[test]
fn failed_list_construction_rolls_back_all_nodes() {
    let descriptor = ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).unwrap();
    let mut heap = heap(16384);
    let retained = heap
        .list_from_elements(&descriptor, &[ManagedFieldValue::Int(42)])
        .unwrap();
    let before = (heap.allocated_bytes(), heap.object_count());
    let mut values = vec![ManagedFieldValue::Int(7); 65];
    values[64] = ManagedFieldValue::Bool(true);
    assert_eq!(
        heap.list_from_elements(&descriptor, &values),
        Err(ManagedMemoryError::InvalidAggregateField)
    );
    assert_eq!((heap.allocated_bytes(), heap.object_count()), before);
    assert_eq!(
        heap.list_get(&descriptor, retained, 0),
        Ok(ManagedFieldValue::Int(42))
    );
}

/// Running out of space between leaves also rolls back the entire construction.
#[test]
fn failed_list_allocation_preserves_heap_accounting() {
    let descriptor = ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).unwrap();
    let mut heap = heap(512);
    assert_eq!(
        heap.list_from_elements(&descriptor, &vec![ManagedFieldValue::Int(7); 65]),
        Err(ManagedMemoryError::AllocationLimitExceeded)
    );
    assert_eq!((heap.allocated_bytes(), heap.object_count()), (0, 0));
    let list = heap
        .list_from_elements(&descriptor, &[ManagedFieldValue::Int(42)])
        .unwrap();
    assert_eq!(
        heap.list_elements(&descriptor, list),
        Ok(vec![ManagedFieldValue::Int(42)])
    );
}

/// Directly written reference leaves retain their precise maps across moving GC.
#[test]
fn constructed_reference_lists_survive_collection() {
    let mut heap = heap(65536);
    let value = heap.allocate_string("retained").unwrap();
    let descriptor = ManagedListDescriptor::new(
        "List[String]",
        ManagedFieldType::Reference(heap.descriptor(value).unwrap().semantic_id()),
    )
    .unwrap();
    for length in [0, 1, 8, 9, 32, 33, 65] {
        let values = vec![ManagedFieldValue::Reference(value.erase()); length];
        let list = heap.list_from_elements(&descriptor, &values).unwrap();
        assert_eq!(heap.list_elements(&descriptor, list), Ok(values));
    }
    let list = heap
        .list_from_elements(
            &descriptor,
            &vec![ManagedFieldValue::Reference(value.erase()); 65],
        )
        .unwrap();
    let mut roots = [ManagedRoot::new(
        heap.owner(),
        RootLocation::ActorState { slot: 0 },
        list.erase(),
    )];
    heap.collect(&mut roots, usize::MAX).unwrap();
    let elements = heap
        .list_elements(&descriptor, roots[0].reference().cast())
        .unwrap();
    assert_eq!(elements.len(), 65);
    for element in elements {
        let ManagedFieldValue::Reference(reference) = element else {
            panic!("reference element")
        };
        assert_eq!(heap.read_string(reference.cast()), Ok("retained"));
    }
}

/// Traversal agrees with checked indexing after shape changes and prefix slicing.
#[test]
fn leaf_traversal_matches_checked_indexing() {
    let descriptor = ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).unwrap();
    for length in [0, 1, 8, 9, 31, 32, 33, 64, 65, 1024, 1025, 4097] {
        let mut heap = heap(4 * 1024 * 1024);
        let values = (0..length)
            .map(|index| ManagedFieldValue::Int(index as i64))
            .collect::<Vec<_>>();
        let original = heap.list_from_elements(&descriptor, &values).unwrap();
        let extra = heap
            .list_from_elements(&descriptor, &[ManagedFieldValue::Int(-1); 9])
            .unwrap();
        let mut list = heap.list_concat(&descriptor, original, extra).unwrap();
        for _ in 0..(length + 9).min(35) {
            let expected = (0..heap.list_length(&descriptor, list).unwrap())
                .map(|index| heap.list_get(&descriptor, list, index).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(heap.list_elements(&descriptor, list), Ok(expected));
            list = heap.list_rest(&descriptor, list).unwrap().unwrap();
        }
    }
}

/// A visitor error stops traversal immediately and never mutates the source heap.
#[test]
fn traversal_stops_on_consumer_error() {
    let descriptor = ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).unwrap();
    let mut heap = heap(65536);
    let list = heap
        .list_from_elements(&descriptor, &vec![ManagedFieldValue::Int(7); 65])
        .unwrap();
    let before = (heap.allocated_bytes(), heap.object_count());
    let mut calls = 0;
    assert_eq!(
        heap.visit_list(&descriptor, list, |_| {
            calls += 1;
            Err(ManagedMemoryError::InvalidAggregateField)
        }),
        Err(ManagedMemoryError::InvalidAggregateField)
    );
    assert_eq!(calls, 1);
    assert_eq!((heap.allocated_bytes(), heap.object_count()), before);
}

/// Sparse valid RRB shapes need not have the minimum height for their length.
#[test]
fn leaf_traversal_accepts_tall_sparse_trees() {
    let descriptor = ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).unwrap();
    let mut heap = heap(65536);
    let values = vec![ManagedFieldValue::Int(7); 9];
    let mut tree = allocate_leaf(&mut heap, &descriptor, &values).unwrap();
    for _ in 0..8 {
        tree = allocate_internal(&mut heap, &descriptor, &[tree]).unwrap();
    }
    let list = allocate_tree_root(&mut heap, &descriptor, tree, 0, 9).unwrap();
    assert_eq!(heap.list_elements(&descriptor, list), Ok(values));
}

/// Rejects a forged size before a materializing consumer reserves its output vector.
#[test]
fn oversized_root_is_rejected_before_materialization() {
    let descriptor = ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).unwrap();
    let mut heap = heap(65536);
    let leaf = allocate_leaf(&mut heap, &descriptor, &[ManagedFieldValue::Int(7); 9]).unwrap();
    let managed = root_descriptor(
        &mut heap,
        &descriptor,
        FORM_TREE,
        &[],
        &[TREE_REFERENCE_OFFSET],
    )
    .unwrap();
    let mut payload = [0; TREE_ROOT_BYTES];
    write_root_header(&mut payload, FORM_TREE, MAX_LIST_ELEMENTS + 1, 0).unwrap();
    let list = heap
        .allocate::<ManagedList>(
            managed,
            &payload,
            &[(TREE_REFERENCE_OFFSET, leaf.reference.erase())],
        )
        .unwrap();
    assert_eq!(
        heap.list_length(&descriptor, list),
        Err(ManagedMemoryError::CollectionTooLarge)
    );
    assert_eq!(
        heap.list_elements(&descriptor, list),
        Err(ManagedMemoryError::CollectionTooLarge)
    );
}
