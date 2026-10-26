use super::*;
use crate::runtime::native_image::managed::{
    encode_collection_layout, ActorId, HeapLimits, ManagedCollectionDescriptor,
    ManagedListDescriptor, ManagedRoot, RootLocation,
};
use crate::runtime::native_image::TvmManagedCollectionDescriptor;

#[path = "string_join_profile_test.rs"]
mod profile;

fn fixture() -> (ActorHeap, ManagedLayoutRegistry, ManagedListDescriptor) {
    let collection = ManagedCollectionDescriptor::list(
        "List(String)",
        ManagedFieldType::Reference(managed_string_semantic_id()),
    )
    .unwrap();
    let descriptor = collection.list_descriptor().unwrap().clone();
    let layouts = ManagedLayoutRegistry::from_image(
        &[],
        &[TvmManagedCollectionDescriptor {
            semantic_id: collection.semantic_id().bytes(),
            encoded_layout: encode_collection_layout(&collection).unwrap(),
        }],
        &[],
    )
    .unwrap();
    let heap = ActorHeap::new(
        ActorId::new(942).unwrap(),
        HeapLimits::new(4096, 4 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    (heap, layouts, descriptor)
}

/// The compact reference batch must match an independent owned-string join.
#[test]
fn joined_batches_preserve_unicode_nul_empty_and_large_fragments() {
    for count in [0, 1, 8, 9, 32, 33, 1025] {
        let (mut heap, layouts, descriptor) = fixture();
        let expected = (0..count)
            .map(|index| format!("λ\0{index}"))
            .collect::<Vec<_>>();
        let values = expected
            .iter()
            .map(|value| ManagedFieldValue::Reference(heap.allocate_string(value).unwrap().erase()))
            .collect::<Vec<_>>();
        let list = heap.list_from_elements(&descriptor, &values).unwrap();
        let before = heap.object_count();
        let joined = join_string_list(&mut heap, &layouts, list.encoded_abi_word() as i64).unwrap();
        assert_eq!(heap.read_string(joined), Ok(expected.concat().as_str()));
        assert_eq!(heap.object_count() - before, usize::from(count != 1));
        let mut roots = [ManagedRoot::new(
            heap.owner(),
            RootLocation::ActorState { slot: 0 },
            joined.erase(),
        )];
        heap.collect(&mut roots, usize::MAX).unwrap();
        assert_eq!(
            heap.read_string(roots[0].reference().cast()),
            Ok(expected.concat().as_str())
        );
    }
}

/// External storage can contain a short string; length alone cannot choose a path.
#[test]
fn joined_batches_accept_short_external_storage() {
    let (mut heap, layouts, descriptor) = fixture();
    let external = heap
        .allocate_shared_string(bytes::Bytes::from_static(b"external"))
        .unwrap();
    let local = heap.allocate_string(" λ").unwrap();
    let list = heap
        .list_from_elements(
            &descriptor,
            &[
                ManagedFieldValue::Reference(external.erase()),
                ManagedFieldValue::Reference(local.erase()),
            ],
        )
        .unwrap();
    let joined = join_string_list(&mut heap, &layouts, list.encoded_abi_word() as i64).unwrap();
    assert_eq!(heap.read_string(joined), Ok("external λ"));
    let prefixed = heap.prepend_string_literal("prefix ", external).unwrap();
    assert_eq!(heap.read_string(prefixed), Ok("prefix external"));
}

/// Borrowed fragments are validated before any result is published.
#[test]
fn joining_rejects_wrong_collection_without_allocating() {
    let (mut heap, layouts, _) = fixture();
    let integers = ManagedListDescriptor::new("List(Int)", ManagedFieldType::Int).unwrap();
    let list = heap
        .list_from_elements(&integers, &[ManagedFieldValue::Int(7)])
        .unwrap();
    let before = (heap.allocated_bytes(), heap.object_count());
    assert_eq!(
        join_string_list(&mut heap, &layouts, list.encoded_abi_word() as i64),
        Err(ManagedMemoryError::ManagedTypeMismatch)
    );
    assert_eq!((heap.allocated_bytes(), heap.object_count()), before);
}

#[test]
fn singleton_and_batch_joins_reject_invalid_utf8_before_publication() {
    use crate::runtime::native_image::managed::{AllocationClass, ManagedTypeDescriptor};
    let (mut heap, layouts, descriptor) = fixture();
    let string = std::sync::Arc::new(
        ManagedTypeDescriptor::new(
            managed_string_semantic_id(),
            9,
            8,
            Vec::new(),
            AllocationClass::Young,
        )
        .unwrap(),
    );
    let malformed = heap
        .allocate::<ManagedString>(string, &[1, 0, 0, 0, 0, 0, 0, 0, 255], &[])
        .unwrap();
    for count in [1, 2] {
        let list = heap
            .list_from_elements(
                &descriptor,
                &vec![ManagedFieldValue::Reference(malformed.erase()); count],
            )
            .unwrap();
        let before = (heap.allocated_bytes(), heap.object_count());
        assert_eq!(
            join_string_list(&mut heap, &layouts, list.encoded_abi_word() as i64),
            Err(ManagedMemoryError::InvalidUtf8)
        );
        assert_eq!((heap.allocated_bytes(), heap.object_count()), before);
    }
}
