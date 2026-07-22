//! Tests for admitted image-local managed closure dispatch.

use crate::runtime::native_image::{TvmBoundaryType, TvmCallableDescriptor};

use super::*;

fn generation(byte: u8) -> ManagedClosureImageGeneration {
    ManagedClosureImageGeneration::new([byte; 32]).expect("image generation")
}

fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(73).expect("actor"),
        HeapLimits::new(64, 8192).expect("limits"),
    )
    .expect("heap")
}

fn callable(id: u64) -> TvmCallableDescriptor {
    TvmCallableDescriptor {
        id,
        parameters: vec![TvmBoundaryType::Int],
        results: vec![TvmBoundaryType::Int],
        captures: vec![TvmBoundaryType::Int],
    }
}

#[test]
fn admitted_closure_dispatch_prepends_owned_captures() {
    let mut heap = heap();
    let table =
        ManagedClosureDispatchTable::admit(generation(7), &[callable(41)]).expect("dispatch table");
    let descriptor = table.closure_descriptor(41).expect("closure descriptor");
    let closure = heap.allocate_closure(&descriptor, &[40]).expect("closure");

    let invocation = heap
        .prepare_closure_invocation(
            closure,
            &table,
            generation(7),
            &[TvmBoundaryType::Int],
            &[2],
            &[TvmBoundaryType::Int],
        )
        .expect("validated dispatch");

    assert_eq!(invocation.target().callable_id(), 41);
    assert_eq!(invocation.target().capture_count(), 1);
    assert_eq!(invocation.target().parameter_count(), 1);
    assert_eq!(invocation.words(), [40, 2]);
}

#[test]
fn closure_allocation_abi_uses_only_admitted_callable_shape() {
    let mut heap = heap();
    let table =
        ManagedClosureDispatchTable::admit(generation(8), &[callable(51)]).expect("dispatch table");
    let encoded = encode_closure_allocation(51).expect("closure ABI");
    let word =
        execute_closure_allocation(&mut heap, &table, &encoded, &[37]).expect("closure allocation");
    let closure = heap
        .validate_abi_reference(
            word,
            table
                .closure_descriptor(51)
                .expect("descriptor")
                .semantic_id(),
        )
        .expect("closure reference")
        .cast::<ManagedClosure>();
    let view = heap.closure_view(closure).expect("closure view");
    assert_eq!(view.callable_id, 51);
    assert_eq!(view.capture_words, [37]);

    let unknown = encode_closure_allocation(52).expect("unknown closure ABI");
    assert_eq!(
        execute_closure_allocation(&mut heap, &table, &unknown, &[37]),
        Err(ManagedMemoryError::UnknownClosureCallable)
    );
    assert_eq!(
        execute_closure_allocation(&mut heap, &table, &encoded, &[]),
        Err(ManagedMemoryError::InvalidClosure)
    );
}

#[test]
fn closure_dispatch_rejects_foreign_generation_and_signature() {
    let mut heap = heap();
    let table =
        ManagedClosureDispatchTable::admit(generation(3), &[callable(19)]).expect("dispatch table");
    let closure = heap
        .allocate_closure(&table.closure_descriptor(19).expect("descriptor"), &[1])
        .expect("closure");

    assert_eq!(
        heap.prepare_closure_invocation(
            closure,
            &table,
            generation(4),
            &[TvmBoundaryType::Int],
            &[1],
            &[TvmBoundaryType::Int],
        ),
        Err(ManagedMemoryError::StaleClosureGeneration)
    );
    assert_eq!(
        heap.prepare_closure_invocation(
            closure,
            &table,
            generation(3),
            &[TvmBoundaryType::Bool],
            &[1],
            &[TvmBoundaryType::Int],
        ),
        Err(ManagedMemoryError::ClosureSignatureMismatch)
    );
}

#[test]
fn closure_dispatch_rejects_unadmitted_target_and_capture_shape() {
    let mut heap = heap();
    let table =
        ManagedClosureDispatchTable::admit(generation(5), &[callable(11)]).expect("dispatch table");
    let foreign = ManagedClosureDescriptor::new(
        generation(5),
        12,
        vec![TvmBoundaryType::Int],
        vec![TvmBoundaryType::Int],
        vec![TvmBoundaryType::Int],
    )
    .expect("foreign descriptor");
    let foreign = heap
        .allocate_closure(&foreign, &[1])
        .expect("foreign closure");
    assert_eq!(
        heap.prepare_closure_invocation(
            foreign,
            &table,
            generation(5),
            &[TvmBoundaryType::Int],
            &[1],
            &[TvmBoundaryType::Int],
        ),
        Err(ManagedMemoryError::UnknownClosureCallable)
    );

    let wrong_capture = ManagedClosureDescriptor::new(
        generation(5),
        11,
        vec![TvmBoundaryType::Int],
        vec![TvmBoundaryType::Int],
        vec![TvmBoundaryType::Bool],
    )
    .expect("wrong capture descriptor");
    let wrong_capture = heap
        .allocate_closure(&wrong_capture, &[1])
        .expect("wrong capture closure");
    assert_eq!(
        heap.prepare_closure_invocation(
            wrong_capture,
            &table,
            generation(5),
            &[TvmBoundaryType::Int],
            &[1],
            &[TvmBoundaryType::Int],
        ),
        Err(ManagedMemoryError::ClosureCaptureMismatch)
    );
}

#[test]
fn callable_table_is_bounded_canonical_and_pointer_free() {
    assert_eq!(
        ManagedClosureDispatchTable::admit(generation(1), &[callable(2), callable(1)]),
        Err(ManagedMemoryError::InvalidClosure)
    );
    let mut json = callable(1);
    json.captures = vec![TvmBoundaryType::Json];
    assert_eq!(
        ManagedClosureDispatchTable::admit(generation(1), &[json]),
        Err(ManagedMemoryError::InvalidClosure)
    );

    fn require_send_sync_static<T: Send + Sync + 'static>() {}
    require_send_sync_static::<ManagedClosureDispatchTable>();
    require_send_sync_static::<ManagedClosureInvocation>();
    let source = include_str!("closure_dispatch.rs");
    for forbidden in ["*const", "*mut", "NonNull", "ThreadId", "TcpStream"] {
        assert!(
            !source.contains(forbidden),
            "forbidden dispatch state: {forbidden}"
        );
    }
}
