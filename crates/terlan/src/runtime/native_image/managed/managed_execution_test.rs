//! Public sequence allocation tests for the shard-owned managed runtime.

use std::ffi::c_void;
use std::sync::Arc;

use crate::runtime::native_image::{TvmBoundaryType, TvmCallableDescriptor};

use super::*;

type TestAllocator =
    unsafe extern "C" fn(*mut c_void, *const u8, u64, *const i64, u64, *mut u64) -> i32;
type TestClosureResolver = unsafe extern "C" fn(
    *mut c_void,
    i64,
    *const i64,
    u64,
    *const i64,
    *const i64,
    u64,
    *mut u64,
    *mut i64,
    u64,
    *mut u64,
) -> i32;

/// Pins authenticated callable membership into every empty shard fork.
#[test]
fn executable_metadata_installs_generation_scoped_closure_dispatch() {
    let callable = TvmCallableDescriptor {
        id: 71,
        parameters: vec![TvmBoundaryType::Int],
        results: vec![TvmBoundaryType::Int],
        captures: Vec::new(),
    };
    let runtime = ManagedExecutionRuntime::with_executable_image_metadata(
        &[],
        &[],
        &[],
        [9; 32],
        &[callable],
    )
    .expect("executable managed metadata");
    let dispatch = runtime.closure_dispatch().expect("closure dispatch");
    assert_eq!(dispatch.generation().digest(), [9; 32]);
    assert_eq!(
        dispatch
            .closure_descriptor(71)
            .expect("callable descriptor")
            .callable_id(),
        71
    );

    let fork = runtime.fork_empty();
    assert_eq!(
        fork.closure_dispatch()
            .expect("fork closure dispatch")
            .generation()
            .digest(),
        [9; 32]
    );
    assert!(ManagedExecutionRuntime::runtime_default()
        .expect("metadata-only runtime")
        .closure_dispatch()
        .expect_err("no executable generation")
        .contains("no admitted executable generation"));
}

/// Exercises the exact generated-code seam for validating and unpacking a closure.
#[test]
#[allow(unsafe_code)]
fn dispatch_context_resolves_owned_closure_without_code_pointers() {
    let callable = TvmCallableDescriptor {
        id: 73,
        parameters: vec![TvmBoundaryType::Int],
        results: vec![TvmBoundaryType::Int],
        captures: vec![TvmBoundaryType::Int],
    };
    let mut runtime = ManagedExecutionRuntime::with_executable_image_metadata(
        &[],
        &[],
        &[],
        [11; 32],
        &[callable],
    )
    .expect("executable managed metadata");
    let layout = encode_closure_allocation(73).expect("closure allocation ABI");
    let type_words = TvmBoundaryType::Int.transition_words();
    let resolved = runtime.with_dispatch(72, |context, allocator, resolver| {
        // SAFETY: `with_dispatch` supplies these exact call-scoped callback ABIs.
        let allocator: TestAllocator = unsafe { std::mem::transmute(allocator) };
        // SAFETY: same fixed callback contract as above.
        let resolver: TestClosureResolver = unsafe { std::mem::transmute(resolver) };
        let mut closure = 0_u64;
        // SAFETY: every pointer references live bounded local storage.
        let status = unsafe {
            allocator(
                context,
                layout.as_ptr(),
                layout.len() as u64,
                [40_i64].as_ptr(),
                1,
                &mut closure,
            )
        };
        assert_eq!(status, 0);
        let mut target = 0_u64;
        let mut words = [0_i64; 2];
        let mut word_count = 0_u64;
        // SAFETY: closure and all type/value/output buffers remain live for the call.
        let status = unsafe {
            resolver(
                context,
                i64::from_ne_bytes(closure.to_ne_bytes()),
                type_words.as_ptr(),
                1,
                [2_i64].as_ptr(),
                type_words.as_ptr(),
                1,
                &mut target,
                words.as_mut_ptr(),
                words.len() as u64,
                &mut word_count,
            )
        };
        (status, target, words, word_count)
    });

    assert_eq!(resolved, (0, 73, [40, 2], 2));
    assert!(runtime.take_allocation_error().is_none());
}

/// Rolls back the backing Bytes allocation when a Binary range is invalid.
#[test]
fn binary_argument_allocation_is_atomic_on_invalid_bit_length() {
    let mut runtime = ManagedExecutionRuntime::runtime_default().expect("managed runtime");
    let error = runtime
        .allocate_binary_value(81, &[0], 9)
        .expect_err("out-of-range Binary");
    assert!(error.contains("binary slice exceeds its backing bytes"));
    assert_eq!(runtime.heap_usage(81), Some((0, 0)));
}

/// Rejects zero actor identities before publishing public managed data.
#[test]
fn public_sequence_allocation_rejects_zero_owner() {
    let mut runtime = ManagedExecutionRuntime::runtime_default().expect("managed runtime");
    let error = runtime
        .allocate_string_value(0, "not-owned")
        .expect_err("zero owner");
    assert!(error.contains("actor identity must be nonzero"));
    assert_eq!(runtime.actor_count(), 0);
}

#[test]
fn completed_actor_heap_capacity_is_reused_with_a_fresh_owner_token() {
    let mut runtime = ManagedExecutionRuntime::runtime_default().expect("managed runtime");
    let stale = runtime
        .allocate_string_value(81, "first owner")
        .expect("first actor string");

    runtime.release_owner(81);
    assert_eq!(runtime.actor_count(), 0);
    assert_eq!(runtime.recycled_heap_count(), 1);

    let current = runtime
        .allocate_string_value(82, "second owner")
        .expect("second actor string");
    assert_eq!(runtime.recycled_heap_count(), 0);
    assert_eq!(
        runtime
            .materialize_string_value(82, current)
            .expect("current owner reference"),
        "second owner"
    );
    let error = runtime
        .materialize_string_value(82, stale)
        .expect_err("recycled heap must reject prior-owner reference");
    assert!(error.contains("reference"), "{error}");
}

#[test]
fn fixed_owner_heap_resets_in_place_with_stale_token_protection() {
    let mut runtime = ManagedExecutionRuntime::runtime_default().expect("managed runtime");
    let stale = runtime
        .allocate_string_value(81, "first request")
        .expect("first request string");

    runtime.reset_owner(81);
    assert_eq!(runtime.actor_count(), 1);
    assert_eq!(runtime.recycled_heap_count(), 0);

    let current = runtime
        .allocate_string_value(81, "second request")
        .expect("second request string");
    assert_eq!(
        runtime
            .materialize_string_value(81, current)
            .expect("current request reference"),
        "second request"
    );
    let error = runtime
        .materialize_string_value(81, stale)
        .expect_err("reset heap must reject prior-request reference");
    assert!(error.contains("reference"), "{error}");
}

/// Copies a native aggregate directly between actor heaps and rolls it back atomically.
#[test]
fn managed_mailbox_copy_preserves_type_owner_and_rollback_boundary() {
    let mut runtime = ManagedExecutionRuntime::runtime_default().expect("managed runtime");
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::tuple(
            "app.Pair",
            vec![ManagedFieldType::Int, ManagedFieldType::Bool],
        )
        .expect("Pair descriptor"),
    );
    let semantic = descriptor.managed().semantic_id();
    let source = runtime
        .with_public_allocation(83, |heap, _| {
            heap.allocate_aggregate(
                descriptor,
                &[ManagedFieldValue::Int(41), ManagedFieldValue::Bool(true)],
            )
            .map_err(|error| error.to_string())
        })
        .expect("source Pair");
    let source_word = i64::from_ne_bytes(source.encoded_abi_word().to_ne_bytes());
    let boundary = TvmBoundaryType::Managed(semantic.bytes());

    let fragment = runtime
        .copy_mailbox_value(83, 84, &boundary, source_word)
        .expect("copy Pair into receiver heap");

    assert_eq!(fragment.sender().get(), 83);
    assert_eq!(fragment.receiver().get(), 84);
    assert_eq!(fragment.copied_objects(), 1);
    assert_ne!(fragment.root_reference(), source.erase());
    runtime
        .validate_boundary_reference(
            84,
            &boundary,
            i64::from_ne_bytes(fragment.root_reference().encoded_abi_word().to_ne_bytes()),
        )
        .expect("receiver owns copied root");
    assert!(runtime
        .validate_boundary_reference(84, &boundary, source_word)
        .is_err());

    runtime
        .rollback_mailbox_value(fragment.fragment_id())
        .expect("rollback unpublished receiver graph");
    assert_eq!(runtime.heap_usage(84), Some((0, 0)));
    assert_eq!(runtime.heap_usage(83).map(|usage| usage.1), Some(1));
}

/// Retains immutable self-send graphs without allocating a duplicate object.
#[test]
fn managed_mailbox_self_send_adds_only_a_precise_root() {
    let mut runtime = ManagedExecutionRuntime::runtime_default().expect("managed runtime");
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::tuple("app.Single", vec![ManagedFieldType::Int])
            .expect("Single descriptor"),
    );
    let semantic = descriptor.managed().semantic_id();
    let source = runtime
        .with_public_allocation(85, |heap, _| {
            heap.allocate_aggregate(descriptor, &[ManagedFieldValue::Int(47)])
                .map_err(|error| error.to_string())
        })
        .expect("source Single");
    let before = runtime.heap_usage(85);

    let fragment = runtime
        .copy_mailbox_value(
            85,
            85,
            &TvmBoundaryType::Managed(semantic.bytes()),
            i64::from_ne_bytes(source.encoded_abi_word().to_ne_bytes()),
        )
        .expect("retain self-send root");

    assert_eq!(fragment.root_reference(), source.erase());
    assert_eq!(fragment.receiver_heap_bytes(), 0);
    assert_eq!(runtime.heap_usage(85), before);
}
