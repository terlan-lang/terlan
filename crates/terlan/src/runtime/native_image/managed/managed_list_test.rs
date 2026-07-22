use super::*;
use crate::runtime::native_image::managed::{
    ActorId, HeapLimits, ManagedBytes, ManagedRoot, RootLocation,
};

fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(91).expect("actor"),
        HeapLimits::new(4096, 16 * 1024 * 1024).expect("limits"),
    )
    .expect("heap")
}

fn ints(count: usize) -> Vec<ManagedFieldValue> {
    (0..count)
        .map(|value| ManagedFieldValue::Int(value as i64))
        .collect()
}

#[test]
fn adaptive_list_selects_empty_inline_regular_and_relaxed_profiles() {
    let descriptor =
        ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).expect("descriptor");
    let mut heap = heap();
    let empty = heap
        .list_from_elements(&descriptor, &[])
        .expect("empty list");
    let inline = heap
        .list_from_elements(&descriptor, &ints(8))
        .expect("inline list");
    let regular = heap
        .list_from_elements(&descriptor, &ints(1024))
        .expect("regular list");
    let relaxed = heap
        .list_from_elements(&descriptor, &ints(33))
        .expect("relaxed list");

    assert_eq!(
        heap.list_profile(&descriptor, empty),
        Ok(ManagedListProfile::Empty)
    );
    assert_eq!(
        heap.list_profile(&descriptor, inline),
        Ok(ManagedListProfile::Inline)
    );
    assert_eq!(
        heap.list_profile(&descriptor, regular),
        Ok(ManagedListProfile::RegularTree)
    );
    assert_eq!(
        heap.list_profile(&descriptor, relaxed),
        Ok(ManagedListProfile::RelaxedTree)
    );
    assert_eq!(heap.list_is_empty(&descriptor, empty), Ok(true));
    assert_eq!(heap.list_is_empty(&descriptor, inline), Ok(false));
}

#[test]
fn rrb_lookup_handles_leaf_and_multilevel_boundaries() {
    let descriptor =
        ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).expect("descriptor");
    let mut heap = heap();
    let list = heap
        .list_from_elements(&descriptor, &ints(2050))
        .expect("multilevel list");

    assert_eq!(heap.list_length(&descriptor, list), Ok(2050));
    assert_eq!(
        heap.list_first(&descriptor, list),
        Ok(Some(ManagedFieldValue::Int(0)))
    );
    for index in [0, 7, 8, 31, 32, 1023, 1024, 2048, 2049] {
        assert_eq!(
            heap.list_get(&descriptor, list, index),
            Ok(ManagedFieldValue::Int(index as i64))
        );
    }
    assert_eq!(
        heap.list_get(&descriptor, list, 2050),
        Err(ManagedMemoryError::CollectionIndexOutOfBounds)
    );
}

#[test]
fn persistent_update_append_and_concat_leave_prior_versions_unchanged() {
    let descriptor =
        ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).expect("descriptor");
    let mut heap = heap();
    let original = heap
        .list_from_elements(&descriptor, &ints(40))
        .expect("original");
    let updated = heap
        .list_update(&descriptor, original, 20, ManagedFieldValue::Int(999))
        .expect("updated");
    let appended = heap
        .list_append(&descriptor, updated, ManagedFieldValue::Int(40))
        .expect("appended");
    let suffix = heap
        .list_from_elements(
            &descriptor,
            &[ManagedFieldValue::Int(41), ManagedFieldValue::Int(42)],
        )
        .expect("suffix");
    let concatenated = heap
        .list_concat(&descriptor, appended, suffix)
        .expect("concatenated");

    assert_eq!(
        heap.list_get(&descriptor, original, 20),
        Ok(ManagedFieldValue::Int(20))
    );
    assert_eq!(
        heap.list_get(&descriptor, updated, 20),
        Ok(ManagedFieldValue::Int(999))
    );
    assert_eq!(heap.list_length(&descriptor, original), Ok(40));
    assert_eq!(heap.list_length(&descriptor, appended), Ok(41));
    assert_eq!(heap.list_length(&descriptor, concatenated), Ok(43));
    assert_eq!(
        heap.list_get(&descriptor, concatenated, 42),
        Ok(ManagedFieldValue::Int(42))
    );
    assert_eq!(
        heap.list_update(&descriptor, original, 40, ManagedFieldValue::Int(0)),
        Err(ManagedMemoryError::CollectionIndexOutOfBounds)
    );
}

#[test]
fn update_path_copies_only_the_selected_leaf_and_ancestors() {
    let descriptor =
        ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).expect("descriptor");
    let mut heap = heap();
    let original = heap
        .list_from_elements(&descriptor, &ints(1024))
        .expect("original");
    let before = heap.object_count();
    let updated = heap
        .list_update(&descriptor, original, 500, ManagedFieldValue::Int(-1))
        .expect("updated");

    assert_eq!(heap.object_count() - before, 3);
    assert_eq!(
        heap.list_get(&descriptor, original, 500),
        Ok(ManagedFieldValue::Int(500))
    );
    assert_eq!(
        heap.list_get(&descriptor, updated, 500),
        Ok(ManagedFieldValue::Int(-1))
    );
    assert_eq!(
        heap.list_get(&descriptor, updated, 499),
        Ok(ManagedFieldValue::Int(499))
    );
}

#[test]
fn append_grows_a_full_tree_with_bounded_path_copying() {
    let descriptor =
        ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).expect("descriptor");
    let mut heap = heap();
    let original = heap
        .list_from_elements(&descriptor, &ints(1024))
        .expect("original");
    let before = heap.object_count();
    let appended = heap
        .list_append(&descriptor, original, ManagedFieldValue::Int(1024))
        .expect("appended");

    assert!(heap.object_count() - before <= 4);
    assert_eq!(heap.list_length(&descriptor, original), Ok(1024));
    assert_eq!(heap.list_length(&descriptor, appended), Ok(1025));
    assert_eq!(
        heap.list_get(&descriptor, appended, 1024),
        Ok(ManagedFieldValue::Int(1024))
    );
}

#[test]
fn concat_rebalances_only_the_two_tree_fringes() {
    let descriptor =
        ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).expect("descriptor");
    let mut heap = heap();
    let left = heap
        .list_from_elements(&descriptor, &ints(1024))
        .expect("left");
    let right_values = (1024..2048)
        .map(|value| ManagedFieldValue::Int(value as i64))
        .collect::<Vec<_>>();
    let right = heap
        .list_from_elements(&descriptor, &right_values)
        .expect("right");
    let before = heap.object_count();
    let concatenated = heap
        .list_concat(&descriptor, left, right)
        .expect("concatenated");

    assert!(heap.object_count() - before <= 6);
    assert_eq!(heap.list_length(&descriptor, concatenated), Ok(2048));
    for index in [0, 1023, 1024, 2047] {
        assert_eq!(
            heap.list_get(&descriptor, concatenated, index),
            Ok(ManagedFieldValue::Int(index as i64))
        );
    }
    assert_eq!(heap.list_length(&descriptor, left), Ok(1024));
    assert_eq!(heap.list_length(&descriptor, right), Ok(1024));
}

#[test]
fn subtract_removes_first_structural_matches_and_shares_unchanged_inputs() {
    let descriptor =
        ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).expect("descriptor");
    let mut heap = heap();
    let original = heap
        .list_from_elements(
            &descriptor,
            &[
                ManagedFieldValue::Int(1),
                ManagedFieldValue::Int(2),
                ManagedFieldValue::Int(1),
                ManagedFieldValue::Int(3),
            ],
        )
        .expect("original");
    let removals = heap
        .list_from_elements(
            &descriptor,
            &[
                ManagedFieldValue::Int(1),
                ManagedFieldValue::Int(3),
                ManagedFieldValue::Int(9),
            ],
        )
        .expect("removals");
    let result = heap
        .list_subtract(&descriptor, original, removals, |_heap, left, right| {
            Ok(left == right)
        })
        .expect("subtract");

    assert_eq!(
        heap.list_elements(&descriptor, result),
        Ok(vec![ManagedFieldValue::Int(2), ManagedFieldValue::Int(1)])
    );
    assert_eq!(heap.list_length(&descriptor, original), Ok(4));

    let missing = heap
        .list_from_elements(&descriptor, &[ManagedFieldValue::Int(99)])
        .expect("missing");
    let before = heap.object_count();
    let unchanged = heap
        .list_subtract(&descriptor, original, missing, |_heap, left, right| {
            Ok(left == right)
        })
        .expect("unchanged subtraction");
    assert_eq!(unchanged, original);
    assert_eq!(heap.object_count(), before);
}

#[test]
fn subtract_uses_content_equality_for_managed_reference_elements() {
    let bytes_semantic = SemanticTypeId::from_canonical("std.binary.Bytes").expect("semantic");
    let descriptor =
        ManagedListDescriptor::new("List[Bytes]", ManagedFieldType::Reference(bytes_semantic))
            .expect("descriptor");
    let mut heap = heap();
    let first = heap.allocate_bytes(b"same").expect("first");
    let equal_content = heap.allocate_bytes(b"same").expect("equal content");
    assert_ne!(first.erase(), equal_content.erase());
    let values = heap
        .list_from_elements(&descriptor, &[ManagedFieldValue::Reference(first.erase())])
        .expect("values");
    let removals = heap
        .list_from_elements(
            &descriptor,
            &[ManagedFieldValue::Reference(equal_content.erase())],
        )
        .expect("removals");

    let result = heap
        .list_subtract(&descriptor, values, removals, |heap, left, right| {
            let (ManagedFieldValue::Reference(left), ManagedFieldValue::Reference(right)) =
                (left, right)
            else {
                return Err(ManagedMemoryError::InvalidAggregateField);
            };
            Ok(heap.read_bytes(left.cast())? == heap.read_bytes(right.cast())?)
        })
        .expect("structural subtraction");

    assert_eq!(heap.list_length(&descriptor, result), Ok(0));
    assert_eq!(heap.list_length(&descriptor, values), Ok(1));
}

#[test]
fn swap_copies_the_union_of_changed_paths_and_preserves_source_values() {
    let descriptor =
        ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).expect("descriptor");
    let mut heap = heap();
    let original = heap
        .list_from_elements(&descriptor, &ints(1024))
        .expect("original");
    let before = heap.object_count();
    let swapped = heap.list_swap(&descriptor, original, 31, 32).expect("swap");

    assert_eq!(heap.object_count() - before, 4);
    assert_eq!(
        heap.list_get(&descriptor, swapped, 31),
        Ok(ManagedFieldValue::Int(32))
    );
    assert_eq!(
        heap.list_get(&descriptor, swapped, 32),
        Ok(ManagedFieldValue::Int(31))
    );
    assert_eq!(
        heap.list_get(&descriptor, original, 31),
        Ok(ManagedFieldValue::Int(31))
    );

    let unchanged = heap
        .list_swap(&descriptor, original, 500, 500)
        .expect("identity swap");
    assert_eq!(unchanged, original);
    assert_eq!(
        heap.list_swap(&descriptor, original, 0, 1024),
        Err(ManagedMemoryError::CollectionIndexOutOfBounds)
    );
}

#[test]
fn transient_builder_batches_values_into_one_canonical_rrb_publication() {
    let descriptor =
        ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).expect("descriptor");
    let mut heap = heap();
    let mut builder = heap.list_builder(&descriptor, 2050).expect("builder");
    assert!(builder.is_empty());
    for value in ints(2050) {
        builder.push(value).expect("push");
    }
    assert_eq!(builder.len(), 2050);
    let list = builder.finish().expect("finish");

    assert_eq!(heap.object_count(), 70);
    assert_eq!(heap.list_length(&descriptor, list), Ok(2050));
    for index in [0, 31, 32, 1023, 1024, 2049] {
        assert_eq!(
            heap.list_get(&descriptor, list, index),
            Ok(ManagedFieldValue::Int(index as i64))
        );
    }
}

#[test]
fn transient_builder_rejects_invalid_batches_before_mutating_its_buffer() {
    let descriptor =
        ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).expect("descriptor");
    let mut heap = heap();
    let mut builder = heap.list_builder(&descriptor, 3).expect("builder");

    assert_eq!(
        builder.extend_from_slice(&[
            ManagedFieldValue::Int(1),
            ManagedFieldValue::Bool(true),
            ManagedFieldValue::Int(2),
        ]),
        Err(ManagedMemoryError::InvalidAggregateField)
    );
    assert!(builder.is_empty());
    builder
        .extend_from_slice(&[ManagedFieldValue::Int(7), ManagedFieldValue::Int(8)])
        .expect("valid batch");
    let list = builder.finish().expect("finish");
    assert_eq!(
        heap.list_elements(&descriptor, list),
        Ok(vec![ManagedFieldValue::Int(7), ManagedFieldValue::Int(8)])
    );

    assert!(matches!(
        heap.list_builder(&descriptor, (1 << 24) + 1),
        Err(ManagedMemoryError::CollectionTooLarge)
    ));
}

#[test]
fn transient_builder_validates_managed_reference_ownership_at_admission() {
    let semantic = SemanticTypeId::from_canonical("std.binary.Bytes").expect("semantic");
    let descriptor =
        ManagedListDescriptor::new("List[Bytes]", ManagedFieldType::Reference(semantic))
            .expect("descriptor");
    let mut owner = heap();
    let mut foreign = ActorHeap::new(
        ActorId::new(92).expect("actor"),
        HeapLimits::new(4096, 16 * 1024 * 1024).expect("limits"),
    )
    .expect("foreign heap");
    let foreign_value = foreign.allocate_bytes(b"foreign").expect("foreign bytes");
    let mut builder = owner.list_builder(&descriptor, 1).expect("builder");

    assert_eq!(
        builder.push(ManagedFieldValue::Reference(foreign_value.erase())),
        Err(ManagedMemoryError::CrossActorReference)
    );
    assert!(builder.is_empty());
    let list = builder.finish().expect("empty list");
    assert_eq!(owner.list_length(&descriptor, list), Ok(0));
}

#[test]
fn front_views_are_constant_shape_then_trim_excluded_leaf_retention() {
    let descriptor =
        ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).expect("descriptor");
    let mut heap = heap();
    let mut list = heap
        .list_from_elements(&descriptor, &ints(40))
        .expect("list");

    for expected in 1..32 {
        list = heap
            .list_rest(&descriptor, list)
            .expect("rest")
            .expect("nonempty rest");
        assert_eq!(
            heap.list_first(&descriptor, list),
            Ok(Some(ManagedFieldValue::Int(expected)))
        );
    }
    list = heap
        .list_rest(&descriptor, list)
        .expect("trim rest")
        .expect("nonempty rest");
    assert_eq!(heap.list_length(&descriptor, list), Ok(8));
    assert_eq!(
        heap.list_profile(&descriptor, list),
        Ok(ManagedListProfile::Inline)
    );

    let mut roots = [ManagedRoot::new(
        heap.owner(),
        RootLocation::ActorState { slot: 0 },
        list.erase(),
    )];
    let stats = heap.collect(&mut roots, usize::MAX).expect("collect");
    assert_eq!(stats.objects_after, 1);
}

#[test]
fn reference_elements_use_precise_leaf_maps_and_relocate_with_the_list() {
    let bytes_semantic = SemanticTypeId::from_canonical("std.binary.Bytes").expect("semantic");
    let descriptor =
        ManagedListDescriptor::new("List[Bytes]", ManagedFieldType::Reference(bytes_semantic))
            .expect("descriptor");
    let mut heap = heap();
    let values = (0..40)
        .map(|index| {
            heap.allocate_bytes(&[index as u8])
                .map(|value| ManagedFieldValue::Reference(value.erase()))
        })
        .collect::<Result<Vec<_>, _>>()
        .expect("byte values");
    let list = heap
        .list_from_elements(&descriptor, &values)
        .expect("reference list");
    let old_list = list;
    let mut roots = [ManagedRoot::new(
        heap.owner(),
        RootLocation::Mailbox {
            fragment: 2,
            slot: 0,
        },
        list.erase(),
    )];

    heap.collect(&mut roots, usize::MAX).expect("collect");
    let relocated: TvmRef<ManagedList> = roots[0].reference().cast();
    for index in [0, 31, 32, 39] {
        let ManagedFieldValue::Reference(value) = heap
            .list_get(&descriptor, relocated, index)
            .expect("element")
        else {
            panic!("expected reference element");
        };
        let value: TvmRef<ManagedBytes> = value.cast();
        assert_eq!(heap.read_bytes(value), Ok(&[index as u8][..]));
    }
    assert_eq!(heap.read(old_list), Err(ManagedMemoryError::StaleReference));
}

#[test]
fn list_admission_rejects_wrong_types_nonfinite_values_and_descriptors() {
    let int_descriptor =
        ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).expect("int descriptor");
    let float_descriptor = ManagedListDescriptor::new("List[Float]", ManagedFieldType::Float)
        .expect("float descriptor");
    let other_descriptor = ManagedListDescriptor::new("app.OtherList", ManagedFieldType::Int)
        .expect("other descriptor");
    let mut heap = heap();

    assert_eq!(
        heap.list_from_elements(&int_descriptor, &[ManagedFieldValue::Bool(true)]),
        Err(ManagedMemoryError::InvalidAggregateField)
    );
    assert_eq!(
        heap.list_from_elements(
            &float_descriptor,
            &[ManagedFieldValue::Float(f64::INFINITY)]
        ),
        Err(ManagedMemoryError::InvalidManagedScalar)
    );
    let list = heap
        .list_from_elements(&int_descriptor, &ints(2))
        .expect("list");
    assert_eq!(
        heap.list_length(&other_descriptor, list),
        Err(ManagedMemoryError::ManagedTypeMismatch)
    );
    let empty = heap
        .list_from_elements(&int_descriptor, &[])
        .expect("empty");
    assert_eq!(heap.list_first(&int_descriptor, empty), Ok(None));
    assert_eq!(heap.list_rest(&int_descriptor, empty), Ok(None));
}
