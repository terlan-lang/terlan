//! Retired std JSON opcodes cannot bypass package resource dispatch.

use super::{execute_managed_operation_with_context, is_managed_operation};
use crate::runtime::native_image::managed::{
    ActorHeap, ActorId, HeapLimits, ManagedLayoutRegistry, ManagedMemoryError, SemanticTypeId,
};

#[test]
fn retired_json_operations_reject_before_reading_or_allocating_heap_values() {
    let mut heap = ActorHeap::new(
        ActorId::new(111).unwrap(),
        HeapLimits::new(1024 * 1024, 16 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    let layouts = ManagedLayoutRegistry::from_image(&[], &[], &[]).unwrap();
    let text = heap
        .allocate_string(r#"{"enabled":true,"count":2}"#)
        .unwrap();
    let before = heap.allocated_bytes();
    for (opcode, semantics) in [
        (
            1,
            vec![
                "Named(Json)",
                "Apply(Result;Named(Json),Named(Error))",
                "Named(Error)",
            ],
        ),
        (2, vec!["Apply(Result;Named(Json),Named(Error))"]),
    ] {
        // Exact legacy v1 envelopes, followed by truncated and trailing-byte variants.
        let mut encoded = b"TVMJ\x01\x00\x00\x00".to_vec();
        encoded[6] = opcode;
        for name in semantics {
            encoded.extend_from_slice(&SemanticTypeId::from_canonical(name).unwrap().bytes());
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
                vec![-1],
                vec![text.encoded_abi_word() as i64],
                vec![0, 0],
            ] {
                assert_eq!(
                    execute_managed_operation_with_context(
                        &mut heap, &layouts, None, &encoded, &words,
                    ),
                    Err(ManagedMemoryError::InvalidAggregateAbi)
                );
                assert_eq!(heap.allocated_bytes(), before);
                assert_eq!(heap.read_string(text), Ok(r#"{"enabled":true,"count":2}"#));
            }
        }
    }
}
