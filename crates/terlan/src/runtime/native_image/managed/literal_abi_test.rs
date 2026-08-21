//! Tests for compiler-owned immutable managed literals.

use super::*;
use crate::runtime::native_image::managed::{ActorId, HeapLimits, TvmRef};

/// Creates one bounded actor heap for literal ABI tests.
fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(1).expect("actor"),
        HeapLimits::new(4096, 8192).expect("limits"),
    )
    .expect("heap")
}

/// Verifies generated UTF-8 data becomes an actor-owned managed string.
#[test]
fn string_literal_round_trips_through_actor_heap() {
    let encoded = encode_string_literal("hello \u{2603}").expect("encode");
    let mut heap = heap();
    let word = heap
        .allocate_managed_words_abi(&encoded, &[])
        .expect("allocate");
    let reference = TvmRef::from_encoded(
        usize::try_from(word)
            .ok()
            .and_then(std::num::NonZeroUsize::new)
            .expect("reference"),
    );
    assert_eq!(heap.read_string(reference).expect("read"), "hello \u{2603}");
}

/// Verifies typed Binary literals retain Binary rather than String semantics.
#[test]
fn binary_literal_round_trips_through_actor_heap() {
    let encoded = encode_binary_literal(b"hello").expect("encode");
    let mut heap = heap();
    let word = heap
        .allocate_managed_words_abi(&encoded, &[])
        .expect("allocate");
    let reference = TvmRef::from_encoded(
        usize::try_from(word)
            .ok()
            .and_then(std::num::NonZeroUsize::new)
            .expect("reference"),
    );
    assert_eq!(
        heap.read_binary(reference).expect("read").aligned_bytes(),
        Some(b"hello".as_slice())
    );
    assert!(heap.read_string(reference.cast()).is_err());
}

/// Verifies malformed, oversized, and field-bearing literal calls are rejected.
#[test]
fn string_literal_rejects_invalid_abi_inputs() {
    assert!(encode_string_literal(&"x".repeat(MAX_MANAGED_LITERAL_ABI_BYTES)).is_err());
    assert!(decode_string_literal(b"TVMS\x01\x00\x02\x00\x00\x00x").is_err());
    assert!(decode_string_literal(b"TVMS\x02\x00\x00\x00\x00\x00").is_err());
    assert!(decode_string_literal(b"TVMS\x01\x00\x01\x00\x00\x00\xff").is_err());

    let encoded = encode_string_literal("x").expect("encode");
    assert!(heap().allocate_managed_words_abi(&encoded, &[1]).is_err());
}
