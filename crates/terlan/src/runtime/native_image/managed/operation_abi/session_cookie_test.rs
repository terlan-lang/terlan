//! All retired session descriptors are rejected by the generic managed boundary.

use super::{execute_managed_operation, is_managed_operation};
use crate::runtime::native_image::managed::{
    ActorHeap, ActorId, HeapLimits, ManagedLayoutRegistry, ManagedMemoryError,
};

#[test]
fn retired_session_opcodes_cannot_read_or_allocate_heap_values() {
    let mut heap = ActorHeap::new(
        ActorId::new(121).unwrap(),
        HeapLimits::new(1024 * 1024, 16 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    let layouts = ManagedLayoutRegistry::default();
    let payload = heap.allocate_string("unchanged").unwrap();
    let before = heap.allocated_bytes();
    // Include every former state, cookie, predicate and acquisition opcode,
    // old/current/future versions, truncation, extra payload and reserved bits.
    for version in [0_u16, 1, 2, 3, u16::MAX] {
        for opcode in 0..=16 {
            for reserved in [0, 1, 255] {
                let mut encoded = b"TVHS".to_vec();
                encoded.extend_from_slice(&version.to_le_bytes());
                encoded.extend_from_slice(&[opcode, reserved]);
                encoded.resize(77, 0);
                for length in 0..=encoded.len() {
                    let encoded = &encoded[..length];
                    assert!(!is_managed_operation(encoded));
                    for words in [
                        &[][..],
                        &[0],
                        &[-1],
                        &[i64::MAX; 3],
                        &[payload.encoded_abi_word() as i64],
                    ] {
                        assert_eq!(
                            execute_managed_operation(&mut heap, &layouts, encoded, words),
                            Err(ManagedMemoryError::InvalidAggregateAbi),
                        );
                        assert_eq!(heap.allocated_bytes(), before);
                        assert_eq!(heap.read_string(payload), Ok("unchanged"));
                    }
                }
            }
        }
    }
}
