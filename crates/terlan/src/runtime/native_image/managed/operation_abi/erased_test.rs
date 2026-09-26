//! Compact union values must retain their exact type when boxed and collected.

use super::*;
use crate::runtime::native_image::managed::{
    encode_aggregate_layout, ActorId, HeapLimits, ManagedAggregate, ManagedAggregateDescriptor,
    ManagedFieldType, ManagedRoot, RootLocation,
};
use crate::runtime::native_image::TvmManagedLayoutDescriptor;

const OPTION: &str = "Option(Int)";

#[test]
fn erased_type_queries_distinguish_mismatch_from_invalid_or_foreign_boxes() {
    let mut heap = heap(8192);
    let layouts = registry(false);
    let boxed = heap
        .allocate_erased_value(&TvmBoundaryType::Int, 42)
        .unwrap();
    let word = i64::from_ne_bytes(boxed.encoded_abi_word().to_ne_bytes());
    let integer = encode_erased_value_is_type_operation(&TvmBoundaryType::Int).unwrap();
    let boolean = encode_erased_value_is_type_operation(&TvmBoundaryType::Bool).unwrap();
    assert_eq!(execute(&mut heap, &layouts, &integer, &[word]), Ok(1));
    assert_eq!(execute(&mut heap, &layouts, &boolean, &[word]), Ok(0));
    for invalid in [0, 42, -1] {
        assert!(execute(&mut heap, &layouts, &boolean, &[invalid]).is_err());
    }
    let mut foreign = ActorHeap::new(
        ActorId::new(85).unwrap(),
        HeapLimits::new(8192, 8192).unwrap(),
    )
    .unwrap();
    assert!(execute(&mut foreign, &layouts, &boolean, &[word]).is_err());
    heap.collect(&mut [], 8192).unwrap();
    assert!(execute(&mut heap, &layouts, &boolean, &[word]).is_err());
}

fn heap(limit: usize) -> ActorHeap {
    ActorHeap::new(
        ActorId::new(84).unwrap(),
        HeapLimits::new(limit, limit).unwrap(),
    )
    .unwrap()
}

fn registry(ambiguous: bool) -> ManagedLayoutRegistry {
    let mut descriptors = vec![
        ManagedAggregateDescriptor::constructor(OPTION, "None", 0, 2, vec![]).unwrap(),
        ManagedAggregateDescriptor::constructor(
            OPTION,
            "Some",
            1,
            2,
            vec![(None, ManagedFieldType::Int)],
        )
        .unwrap(),
    ];
    if ambiguous {
        descriptors
            .push(ManagedAggregateDescriptor::constructor(OPTION, "none", 1, 2, vec![]).unwrap());
    }
    let descriptors = descriptors
        .iter()
        .map(|descriptor| TvmManagedLayoutDescriptor {
            semantic_id: descriptor.managed().semantic_id().bytes(),
            encoded_layout: encode_aggregate_layout(descriptor).unwrap(),
        })
        .collect::<Vec<_>>();
    ManagedLayoutRegistry::from_image(&descriptors, &[], &["none".into(), "some".into()]).unwrap()
}

fn option_type() -> TvmBoundaryType {
    TvmBoundaryType::Managed(SemanticTypeId::from_canonical(OPTION).unwrap().bytes())
}

fn none_word(layouts: &ManagedLayoutRegistry) -> i64 {
    i64::from(layouts.atom_index("none").unwrap().get())
}

#[test]
fn boxed_compact_variant_becomes_a_precisely_traced_reference() {
    let mut heap = heap(8192);
    let layouts = registry(false);
    let ty = option_type();
    let boxing = encode_erased_value_box_operation(&ty).unwrap();
    let boxed = execute(&mut heap, &layouts, &boxing, &[none_word(&layouts)]).unwrap();
    let boxed = heap
        .validate_abi_reference(boxed, managed_erased_value_semantic_id().unwrap())
        .unwrap();
    let mut roots = [ManagedRoot::new(
        heap.owner(),
        RootLocation::ActorState { slot: 0 },
        boxed,
    )];
    heap.collect(&mut roots, 8192).unwrap();
    let moved = roots[0].reference().cast::<ManagedErasedValue>();
    let word = heap.unbox_erased_value(moved, &ty).unwrap();
    assert!(!is_immediate_union_word(word));
    let semantic = SemanticTypeId::from_canonical(OPTION).unwrap();
    let reference = heap.validate_abi_reference(word as u64, semantic).unwrap();
    let descriptor = layouts
        .layout_for_reference(&heap, semantic, reference)
        .unwrap();
    assert_eq!(descriptor.variant_name(), Some("None"));
    assert_eq!(
        heap.read_aggregate(reference.cast::<ManagedAggregate>(), descriptor)
            .unwrap()
            .discriminant(),
        Some(0)
    );
    assert!(heap.unbox_erased_value(boxed.cast(), &ty).is_err());
    let equality = super::super::equality::encode_managed_value_equal_operation(semantic);
    assert_eq!(
        super::super::equality::execute_equality_operation(
            &heap,
            &layouts,
            &equality,
            &[word, none_word(&layouts)]
        ),
        Ok(1)
    );
}

#[test]
fn boxed_compact_variants_require_an_unambiguous_zero_field_image_layout() {
    let mut heap = heap(8192);
    let layouts = registry(false);
    let boxing = encode_erased_value_box_operation(&option_type()).unwrap();
    let some = i64::from(layouts.atom_index("some").unwrap().get());
    for word in [some, i64::from(u32::MAX)] {
        assert!(execute(&mut heap, &layouts, &boxing, &[word]).is_err());
        assert_eq!(heap.object_count(), 0);
    }
    let other = TvmBoundaryType::Managed(SemanticTypeId::from_canonical("Other").unwrap().bytes());
    let other = encode_erased_value_box_operation(&other).unwrap();
    assert!(execute(&mut heap, &layouts, &other, &[none_word(&layouts)]).is_err());
    let ambiguous = registry(true);
    assert!(execute(&mut heap, &ambiguous, &boxing, &[none_word(&ambiguous)]).is_err());
    assert_eq!(heap.object_count(), 0);
}

#[test]
fn compact_variant_boxing_rolls_back_both_allocations_on_exhaustion() {
    let mut heap = heap(40);
    let layouts = registry(false);
    let boxing = encode_erased_value_box_operation(&option_type()).unwrap();
    for _ in 0..3 {
        assert_eq!(
            execute(&mut heap, &layouts, &boxing, &[none_word(&layouts)]),
            Err(ManagedMemoryError::AllocationLimitExceeded)
        );
        assert_eq!(heap.object_count(), 0);
        assert_eq!(heap.allocated_bytes(), 0);
    }
    heap.allocate_erased_value(&TvmBoundaryType::Int, 42)
        .unwrap();
}

#[test]
fn compact_atoms_are_not_reinterpreted_without_a_managed_union_type() {
    let mut heap = heap(8192);
    let layouts = registry(false);
    let atom = none_word(&layouts);
    for ty in [TvmBoundaryType::Atom, TvmBoundaryType::Int] {
        let boxing = encode_erased_value_box_operation(&ty).unwrap();
        let boxed = execute(&mut heap, &layouts, &boxing, &[atom]).unwrap();
        let boxed = heap
            .validate_abi_reference(boxed, managed_erased_value_semantic_id().unwrap())
            .unwrap();
        assert_eq!(heap.unbox_erased_value(boxed.cast(), &ty), Ok(atom));
        assert_eq!(
            heap.unbox_erased_value(boxed.cast(), &option_type()),
            Err(ManagedMemoryError::ManagedTypeMismatch)
        );
    }
    let boxing = encode_erased_value_box_operation(&TvmBoundaryType::String).unwrap();
    assert!(execute(&mut heap, &layouts, &boxing, &[atom]).is_err());
}
