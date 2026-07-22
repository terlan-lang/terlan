//! Tests for image-local pointer-free managed closures.

use std::sync::Arc;

use crate::runtime::native_image::TvmBoundaryType;

use super::*;

fn generation(byte: u8) -> ManagedClosureImageGeneration {
    ManagedClosureImageGeneration::new([byte; 32]).expect("image generation")
}

fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(41).expect("actor"),
        HeapLimits::new(64, 8192).expect("limits"),
    )
    .expect("heap")
}

#[test]
fn owned_closure_survives_collection_and_continuation_parking() {
    let mut heap = heap();
    let capture_semantic = SemanticTypeId::from_canonical("example.Captured").expect("semantic");
    let capture_descriptor = Arc::new(
        ManagedTypeDescriptor::new(capture_semantic, 8, 8, Vec::new(), AllocationClass::Young)
            .expect("capture layout"),
    );
    let capture = heap
        .allocate::<u64>(capture_descriptor, &99_u64.to_le_bytes(), &[])
        .expect("capture");
    let descriptor = ManagedClosureDescriptor::new(
        generation(7),
        42,
        vec![TvmBoundaryType::Int],
        vec![TvmBoundaryType::Int],
        vec![
            TvmBoundaryType::Int,
            TvmBoundaryType::Managed(capture_semantic.bytes()),
        ],
    )
    .expect("closure descriptor");
    let closure = heap
        .allocate_closure(
            &descriptor,
            &[
                17,
                i64::from_ne_bytes(capture.encoded_abi_word().to_ne_bytes()),
            ],
        )
        .expect("closure");
    let old_closure = closure;
    let old_capture = capture;
    let mut parked = ManagedContinuation::capture(heap.owner(), 91, vec![closure.erase()])
        .expect("park closure");

    heap.collect(parked.captures_mut(), 8192)
        .expect("collect closure graph");
    let relocated = parked.captures()[0].reference().cast::<ManagedClosure>();
    let view = heap.closure_view(relocated).expect("relocated closure");
    let relocated_capture = u64::from_ne_bytes(view.capture_words[1].to_ne_bytes());

    assert_eq!(view.generation, generation(7));
    assert_eq!(view.callable_id, 42);
    assert_eq!(view.parameters, [TvmBoundaryType::Int]);
    assert_eq!(view.results, [TvmBoundaryType::Int]);
    assert_eq!(view.capture_words[0], 17);
    assert_eq!(
        view.validate_invocation(
            generation(7),
            &[TvmBoundaryType::Int],
            &[TvmBoundaryType::Int],
        ),
        Ok(())
    );
    assert_ne!(relocated, old_closure);
    assert_ne!(relocated_capture, old_capture.encoded_abi_word());
    assert_eq!(
        heap.read(old_closure),
        Err(ManagedMemoryError::StaleReference)
    );
    assert_eq!(
        heap.read(old_capture),
        Err(ManagedMemoryError::StaleReference)
    );
}

#[test]
fn closure_generation_and_signature_fail_closed() {
    let descriptor = ManagedClosureDescriptor::new(
        generation(3),
        99,
        vec![TvmBoundaryType::Int, TvmBoundaryType::String],
        vec![TvmBoundaryType::Bool],
        Vec::new(),
    )
    .expect("closure descriptor");
    assert_eq!(
        descriptor.validate_invocation(
            generation(4),
            &[TvmBoundaryType::Int, TvmBoundaryType::String],
            &[TvmBoundaryType::Bool],
        ),
        Err(ManagedMemoryError::StaleClosureGeneration)
    );
    assert_eq!(
        descriptor.validate_invocation(
            generation(3),
            &[TvmBoundaryType::Int],
            &[TvmBoundaryType::Bool],
        ),
        Err(ManagedMemoryError::ClosureSignatureMismatch)
    );
}

#[test]
fn closure_shape_rejects_unowned_or_unbounded_state() {
    assert_eq!(
        ManagedClosureImageGeneration::new([0; 32]),
        Err(ManagedMemoryError::InvalidClosure)
    );
    assert_eq!(
        ManagedClosureDescriptor::new(
            generation(1),
            0,
            Vec::new(),
            vec![TvmBoundaryType::Unit],
            Vec::new(),
        ),
        Err(ManagedMemoryError::InvalidClosure)
    );
    assert_eq!(
        ManagedClosureDescriptor::new(
            generation(1),
            1,
            Vec::new(),
            vec![TvmBoundaryType::Unit],
            vec![TvmBoundaryType::Json],
        ),
        Err(ManagedMemoryError::InvalidClosure)
    );
}

#[test]
fn closure_metadata_is_send_sync_static_and_pointer_free() {
    fn require_send_sync_static<T: Send + Sync + 'static>() {}
    require_send_sync_static::<ManagedClosureImageGeneration>();
    require_send_sync_static::<ManagedClosureDescriptor>();
    require_send_sync_static::<ManagedClosureView>();

    let source = include_str!("closures.rs");
    for forbidden in ["*const", "*mut", "NonNull", "ThreadId", "TcpStream"] {
        assert!(
            !source.contains(forbidden),
            "forbidden closure state: {forbidden}"
        );
    }
}
