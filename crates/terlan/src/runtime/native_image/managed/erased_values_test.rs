use super::*;
use crate::runtime::native_image::managed::{
    encode_erased_value_box_operation, encode_erased_value_unbox_operation,
    execute_managed_operation_with_context, managed_abi_result_is_reference, ActorId, HeapLimits,
    ManagedClosureDescriptor, ManagedClosureImageGeneration, ManagedLayoutRegistry, ManagedRoot,
    RootLocation,
};

fn heap(actor: u64) -> ActorHeap {
    ActorHeap::new(
        ActorId::new(actor).unwrap(),
        HeapLimits::new(64, 8192).unwrap(),
    )
    .unwrap()
}

fn abi_word(reference: TvmRef<impl Sized>) -> i64 {
    i64::from_ne_bytes(reference.encoded_abi_word().to_ne_bytes())
}

#[test]
fn erased_values_preserve_scalar_bits_and_reject_type_confusion() {
    let mut heap = heap(73);
    for (ty, word) in [
        (TvmBoundaryType::Unit, 0),
        (TvmBoundaryType::Bool, 1),
        (TvmBoundaryType::Int, i64::MIN),
        (TvmBoundaryType::Float, 0x7ff8_0000_0000_1234),
        (TvmBoundaryType::Atom, 42),
    ] {
        let boxed = heap.allocate_erased_value(&ty, word).unwrap();
        assert_eq!(heap.unbox_erased_value(boxed, &ty), Ok(word));
        let other = if ty == TvmBoundaryType::Int {
            TvmBoundaryType::Float
        } else {
            TvmBoundaryType::Int
        };
        assert_eq!(
            heap.unbox_erased_value(boxed, &other),
            Err(ManagedMemoryError::ManagedTypeMismatch)
        );
    }
    for (ty, word) in [
        (TvmBoundaryType::Unit, 1),
        (TvmBoundaryType::Bool, 2),
        (TvmBoundaryType::Atom, -1),
        (TvmBoundaryType::Atom, i64::MAX),
    ] {
        assert_eq!(
            heap.allocate_erased_value(&ty, word),
            Err(ManagedMemoryError::InvalidManagedScalar)
        );
    }
    assert!(heap
        .allocate_erased_value(&TvmBoundaryType::Json, 0)
        .is_err());
    assert!(heap
        .allocate_erased_value(&TvmBoundaryType::NativeResource(1), 1)
        .is_err());
}

#[test]
fn erased_values_trace_nested_references_and_reject_foreign_or_stale_inputs() {
    let mut heap = heap(74);
    let mut foreign = self::heap(75);
    let foreign_string = foreign.allocate_string("foreign").unwrap();
    assert!(heap
        .allocate_erased_value(&TvmBoundaryType::String, abi_word(foreign_string))
        .is_err());
    let string = heap.allocate_string("captured λ").unwrap();
    assert_eq!(
        heap.allocate_erased_value(&TvmBoundaryType::Bytes, abi_word(string)),
        Err(ManagedMemoryError::ManagedTypeMismatch)
    );
    let boxed = heap
        .allocate_erased_value(&TvmBoundaryType::String, abi_word(string))
        .unwrap();
    let box_type = TvmBoundaryType::Managed(managed_erased_value_semantic_id().unwrap().bytes());
    let outer = heap
        .allocate_erased_value(&box_type, abi_word(boxed))
        .unwrap();
    let mut roots = [ManagedRoot::new(
        heap.owner(),
        RootLocation::ActorState { slot: 0 },
        outer.erase(),
    )];
    heap.collect(&mut roots, 8192).unwrap();
    let new_outer = roots[0].reference().cast();
    let inner_word = heap.unbox_erased_value(new_outer, &box_type).unwrap();
    let inner = heap
        .validate_abi_reference(
            inner_word as u64,
            managed_erased_value_semantic_id().unwrap(),
        )
        .unwrap();
    let string_word = heap
        .unbox_erased_value(inner.cast(), &TvmBoundaryType::String)
        .unwrap();
    let string_reference = heap
        .validate_abi_reference(
            string_word as u64,
            super::super::managed_string_semantic_id(),
        )
        .unwrap();
    assert_eq!(heap.read_string(string_reference.cast()), Ok("captured λ"));
    assert!(heap.unbox_erased_value(outer, &box_type).is_err());
    assert!(heap
        .allocate_erased_value(&TvmBoundaryType::String, abi_word(string))
        .is_err());
}

#[test]
fn erased_values_preserve_callable_signature_and_image_generation() {
    let mut heap = heap(76);
    let generation = ManagedClosureImageGeneration::new([9; 32]).unwrap();
    let descriptor = ManagedClosureDescriptor::new(
        generation,
        11,
        vec![TvmBoundaryType::Int],
        vec![TvmBoundaryType::Int],
        vec![TvmBoundaryType::Int],
    )
    .unwrap();
    let closure = heap.allocate_closure(&descriptor, &[42]).unwrap();
    let ty = TvmBoundaryType::Managed(descriptor.semantic_id().bytes());
    let boxed = heap.allocate_erased_value(&ty, abi_word(closure)).unwrap();
    let word = heap.unbox_erased_value(boxed, &ty).unwrap();
    let view = heap
        .closure_view(
            heap.validate_abi_reference(word as u64, descriptor.semantic_id())
                .unwrap()
                .cast(),
        )
        .unwrap();
    assert_eq!(view.generation, generation);
    assert_eq!(view.callable_id, 11);
    assert_eq!(view.capture_words, [42]);
    assert_eq!(view.parameters, [TvmBoundaryType::Int]);
    assert_eq!(view.results, [TvmBoundaryType::Int]);
}

#[test]
fn erased_value_abi_preserves_reference_classification_and_validates_descriptors() {
    let mut heap = heap(77);
    let layouts = ManagedLayoutRegistry::default();
    let boxing = encode_erased_value_box_operation(&TvmBoundaryType::Int).unwrap();
    let unboxing = encode_erased_value_unbox_operation(&TvmBoundaryType::Int).unwrap();
    let reference_unbox = encode_erased_value_unbox_operation(&TvmBoundaryType::String).unwrap();
    assert!(managed_abi_result_is_reference(&boxing));
    assert!(!managed_abi_result_is_reference(&unboxing));
    assert!(managed_abi_result_is_reference(&reference_unbox));
    let boxed =
        execute_managed_operation_with_context(&mut heap, &layouts, None, &boxing, &[42]).unwrap();
    assert_eq!(
        execute_managed_operation_with_context(
            &mut heap,
            &layouts,
            None,
            &unboxing,
            &[boxed as i64]
        ),
        Ok(42)
    );
    assert!(
        execute_managed_operation_with_context(&mut heap, &layouts, None, &boxing, &[]).is_err()
    );
    for offset in [0, 4, 6, 7, 8, 16] {
        let mut malformed = boxing.clone();
        malformed[offset] = 255;
        assert!(execute_managed_operation_with_context(
            &mut heap,
            &layouts,
            None,
            &malformed,
            &[42]
        )
        .is_err());
    }
    let mut oversized = boxing.clone();
    oversized.push(0);
    for malformed in [&boxing[..31], &oversized] {
        assert!(execute_managed_operation_with_context(
            &mut heap,
            &layouts,
            None,
            malformed,
            &[42]
        )
        .is_err());
    }
    assert!(encode_erased_value_box_operation(&TvmBoundaryType::Json).is_err());
}

#[test]
fn erased_values_reject_forged_layouts_and_obey_heap_limits() {
    let mut heap = heap(78);
    let ty = TvmBoundaryType::Int;
    let mut payload = shape(&ty);
    payload.extend_from_slice(&42_i64.to_ne_bytes());
    let forged = heap
        .allocate::<ManagedErasedValue>(
            Arc::new(
                ManagedTypeDescriptor::new(
                    managed_erased_value_semantic_id().unwrap(),
                    OBJECT_BYTES,
                    8,
                    vec![],
                    AllocationClass::Young,
                )
                .unwrap(),
            ),
            &payload,
            &[],
        )
        .unwrap();
    assert_eq!(
        heap.unbox_erased_value(forged, &ty),
        Err(ManagedMemoryError::LayoutMismatch)
    );
    let mut limited =
        ActorHeap::new(ActorId::new(79).unwrap(), HeapLimits::new(40, 40).unwrap()).unwrap();
    limited.allocate_erased_value(&ty, 1).unwrap();
    assert_eq!(
        limited.allocate_erased_value(&ty, 2),
        Err(ManagedMemoryError::AllocationLimitExceeded)
    );
}
