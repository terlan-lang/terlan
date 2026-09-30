//! Retired HTTP opcodes cannot allocate or reinterpret source-owned response values.

use super::{execute_managed_operation, is_managed_operation};
use crate::runtime::native_image::managed::{
    ActorHeap, ActorId, HeapLimits, ManagedLayoutRegistry, ManagedMemoryError, SemanticTypeId,
};

#[test]
fn retired_http_operations_reject_before_reading_or_allocating_heap_values() {
    let mut heap = ActorHeap::new(
        ActorId::new(97).unwrap(),
        HeapLimits::new(1024 * 1024, 16 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    let layouts = ManagedLayoutRegistry::from_image(&[], &[], &[]).unwrap();
    let payload = heap.allocate_string("body").unwrap();
    let before = heap.allocated_bytes();
    for (opcode, length) in [(1, 8), (2, 96), (3, 76), (4, 40)] {
        for subtype in 0..=5 {
            let mut encoded = b"TVHO\x01\x00\x00\x00".to_vec();
            encoded[6] = opcode;
            encoded[7] = subtype;
            if opcode == 4 {
                for name in ["Named(Response)", "std.http.Response.Headers"] {
                    encoded
                        .extend_from_slice(&SemanticTypeId::from_canonical(name).unwrap().bytes());
                }
            } else {
                encoded.resize(length, 0);
            }
            let mut variants = (0..=encoded.len())
                .map(|length| encoded[..length].to_vec())
                .collect::<Vec<_>>();
            encoded.push(0);
            variants.push(encoded);
            for encoded in variants {
                assert!(!is_managed_operation(&encoded));
                for words in [
                    vec![],
                    vec![0],
                    vec![i64::MAX; 10],
                    vec![payload.encoded_abi_word() as i64, 200],
                ] {
                    assert_eq!(
                        execute_managed_operation(&mut heap, &layouts, &encoded, &words),
                        Err(ManagedMemoryError::InvalidAggregateAbi)
                    );
                    assert_eq!(heap.allocated_bytes(), before);
                    assert_eq!(heap.read_string(payload), Ok("body"));
                }
            }
        }
    }
}
