use super::*;
use crate::runtime::native_image::managed::{ActorId, HeapLimits, ManagedScalarKeySemantics};

#[test]
fn set_reuses_map_storage_for_unique_ordered_persistent_values() {
    let descriptor =
        ManagedSetDescriptor::new("Set[Int]", ManagedFieldType::Int).expect("descriptor");
    let mut heap = ActorHeap::new(
        ActorId::new(122).expect("actor"),
        HeapLimits::new(4096, 1024 * 1024).expect("limits"),
    )
    .expect("heap");
    let mut semantics = ManagedScalarKeySemantics;
    let original = heap
        .set_from_elements(
            &descriptor,
            &[
                ManagedFieldValue::Int(3),
                ManagedFieldValue::Int(1),
                ManagedFieldValue::Int(3),
            ],
            &mut semantics,
        )
        .expect("set");

    assert_eq!(heap.set_length(&descriptor, original), Ok(2));
    assert_eq!(
        heap.set_elements(&descriptor, original),
        Ok(vec![ManagedFieldValue::Int(3), ManagedFieldValue::Int(1)])
    );
    assert_eq!(
        heap.set_contains(
            &descriptor,
            original,
            ManagedFieldValue::Int(1),
            &mut semantics,
        ),
        Ok(true)
    );

    let added = heap
        .set_add(
            &descriptor,
            original,
            ManagedFieldValue::Int(2),
            &mut semantics,
        )
        .expect("add");
    let duplicate = heap
        .set_add(
            &descriptor,
            added,
            ManagedFieldValue::Int(2),
            &mut semantics,
        )
        .expect("duplicate");
    let removed = heap
        .set_remove(
            &descriptor,
            duplicate,
            ManagedFieldValue::Int(3),
            &mut semantics,
        )
        .expect("remove");

    assert_eq!(heap.set_length(&descriptor, original), Ok(2));
    assert_eq!(
        heap.set_elements(&descriptor, removed),
        Ok(vec![ManagedFieldValue::Int(1), ManagedFieldValue::Int(2)])
    );
    let cleared = heap.set_clear(&descriptor, removed).expect("clear");
    assert_eq!(heap.set_is_empty(&descriptor, cleared), Ok(true));
}
