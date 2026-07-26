use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use super::hash::VmStableHashError;
use super::{ReplValue, VmBitString};

fn stable_hash(value: &ReplValue) -> u64 {
    value.stable_hash().expect("portable value must hash")
}

#[test]
fn portable_value_hashes_have_stable_golden_vectors_and_type_separation() {
    let values = [
        ReplValue::Unit,
        ReplValue::Int(0),
        ReplValue::Int(-1),
        ReplValue::Bool(false),
        ReplValue::Bool(true),
        ReplValue::String(String::new()),
        ReplValue::Atom(String::new()),
        ReplValue::Bytes(Arc::from([])),
        ReplValue::Tuple(Vec::new()),
        ReplValue::List(Vec::new()),
        ReplValue::Map(Vec::new()),
        ReplValue::Set(Vec::new()),
    ];
    let actual = values.iter().map(stable_hash).collect::<Vec<_>>();

    assert_eq!(
        actual,
        [
            12_638_152_016_183_539_244,
            925_820_630_484_784_613,
            12_651_931_728_328_052_317,
            598_336_668_751_268_149,
            598_335_569_239_639_938,
            10_978_417_736_201_556_339,
            3_154_070_194_012_243_846,
            356_021_204_681_317_216,
            14_346_266_151_335_950_366,
            9_889_767_024_281_031_900,
            4_863_468_471_422_646_037,
            12_687_816_013_611_958_530,
        ]
    );
    let mut distinct = actual;
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), values.len());
}

#[test]
fn ordered_and_unordered_aggregate_hash_contracts_are_distinct() {
    let one = ReplValue::Int(1);
    let two = ReplValue::Int(2);
    let tuple = ReplValue::Tuple(vec![one.clone(), two.clone()]);
    let reversed_tuple = ReplValue::Tuple(vec![two.clone(), one.clone()]);
    let list = ReplValue::List(vec![one.clone(), two.clone()]);
    assert_ne!(stable_hash(&tuple), stable_hash(&reversed_tuple));
    assert_ne!(stable_hash(&tuple), stable_hash(&list));

    let entries = vec![
        (one.clone(), ReplValue::String("one".into())),
        (two.clone(), ReplValue::String("two".into())),
    ];
    let reversed_entries = entries.iter().cloned().rev().collect();
    assert_eq!(
        stable_hash(&ReplValue::Map(entries.clone())),
        stable_hash(&ReplValue::Map(reversed_entries))
    );

    let changed_entries = vec![
        (one.clone(), ReplValue::String("one".into())),
        (two.clone(), ReplValue::String("changed".into())),
    ];
    assert_ne!(
        stable_hash(&ReplValue::Map(entries)),
        stable_hash(&ReplValue::Map(changed_entries))
    );
    assert_eq!(
        stable_hash(&ReplValue::Set(vec![one.clone(), two.clone()])),
        stable_hash(&ReplValue::Set(vec![two, one]))
    );
}

#[test]
fn bitstring_hashing_uses_canonical_logical_bits() {
    let padded = VmBitString::from_bytes([0b1010_1111, 0xff], 4).expect("four represented bits");
    let canonical = VmBitString::from_bytes([0b1010_0000], 4).expect("canonical four bits");
    assert_eq!(padded, canonical);
    assert_eq!(
        stable_hash(&ReplValue::BitString(padded)),
        stable_hash(&ReplValue::BitString(canonical.clone()))
    );
    assert_ne!(
        stable_hash(&ReplValue::BitString(canonical)),
        stable_hash(&ReplValue::Bytes(Arc::from([0b1010_0000])))
    );
}

#[test]
fn large_binary_hashing_is_deterministic_and_content_sensitive() {
    let bytes = (0..10 * 1024 * 1024)
        .map(|index| (index as u8).wrapping_mul(31))
        .collect::<Vec<_>>();
    let value = ReplValue::Bytes(Arc::from(bytes.clone()));
    assert_eq!(stable_hash(&value), stable_hash(&value));

    let mut changed = bytes;
    let last = changed.last_mut().expect("ten-megabyte value is non-empty");
    *last ^= 1;
    assert_ne!(
        stable_hash(&value),
        stable_hash(&ReplValue::Bytes(Arc::from(changed)))
    );
}

#[test]
fn deeply_nested_values_hash_without_using_the_host_call_stack() {
    const DEPTH: usize = 50_000;

    let mut value = ReplValue::Int(42);
    for index in 0..DEPTH {
        value = if index.is_multiple_of(2) {
            ReplValue::Tuple(vec![value])
        } else {
            ReplValue::List(vec![value])
        };
    }
    let first = stable_hash(&value);
    assert_eq!(first, stable_hash(&value));

    // Recursive destruction of this intentionally adversarial value is a
    // separate host-stack concern and would obscure the traversal assertion.
    std::mem::forget(value);
}

#[test]
fn unsupported_runtime_service_values_fail_loudly() {
    let iterator = ReplValue::Iterator {
        items: vec![ReplValue::Int(1)],
        index: 0,
    };
    assert_eq!(
        iterator.stable_hash(),
        Err(VmStableHashError::UnsupportedValue("Iterator"))
    );
}

#[test]
fn rust_hash_consumers_receive_the_stable_vm_fingerprint() {
    let left = ReplValue::Map(vec![
        (ReplValue::Int(1), ReplValue::Atom("one".into())),
        (ReplValue::Int(2), ReplValue::Atom("two".into())),
    ]);
    let right = ReplValue::Map(vec![
        (ReplValue::Int(2), ReplValue::Atom("two".into())),
        (ReplValue::Int(1), ReplValue::Atom("one".into())),
    ]);
    let hash_with_rust = |value: &ReplValue| {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    };

    assert_eq!(stable_hash(&left), stable_hash(&right));
    assert_eq!(hash_with_rust(&left), hash_with_rust(&right));
}
