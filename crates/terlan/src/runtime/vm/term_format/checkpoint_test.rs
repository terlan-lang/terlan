//! Portable checkpoint bytes are bounded, canonical, and independent of VM handles.

use super::*;
use crate::runtime::vm::bitstring::VmBitString;
use crate::runtime::vm::distributed_state::{
    VmDistributedStateScope as Scope, VmDistributedStateVersion as Version,
};
use crate::runtime::vm::term_format::decode_tetf_checkpoint;
use crate::runtime::vm::ReplValue;

fn entry(value: ReplValue) -> Entry {
    Entry {
        scope: Scope::new("a", "b").unwrap(),
        owner_node_id: "c".into(),
        version: Version::new(1, "d").unwrap(),
        policy: Policy::LastWriterWins,
        value,
    }
}

#[test]
fn checkpoint_golden_bytes_and_exact_limits() {
    let entry = entry(ReplValue::Int(42));
    let expected = [
        b'T', b'E', b'T', b'F', 1, 3, 0, 0, 0, 1, 0, 0, 0, 1, b'a', 0, 0, 0, 1, b'b', 0, 0, 0, 1,
        b'c', 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, b'd', 2, 4, 0, 0, 0, 0, 0, 0, 0, 42,
    ];
    for maximum in 0..expected.len() {
        assert!(encode_tetf_checkpoint(std::slice::from_ref(&entry), &[], maximum).is_err());
    }
    assert_eq!(
        encode_tetf_checkpoint(std::slice::from_ref(&entry), &[], expected.len()).unwrap(),
        expected
    );
    assert_eq!(decode_tetf_checkpoint(&expected, &[]).unwrap(), vec![entry]);
    for truncated in 0..expected.len() {
        assert!(decode_tetf_checkpoint(&expected[..truncated], &[]).is_err());
    }
    let mut trailing = expected.to_vec();
    trailing.push(0);
    assert!(decode_tetf_checkpoint(&trailing, &[]).is_err());
}

#[test]
fn checkpoint_preserves_nested_values_and_every_policy() {
    let atoms = ["ready".to_string()];
    let payload = ReplValue::Tuple(vec![
        ReplValue::Map(vec![(ReplValue::String("key".into()), ReplValue::Int(7))]),
        ReplValue::Set(vec![ReplValue::Int(1), ReplValue::Int(2)]),
        ReplValue::BitString(VmBitString::from_bytes(&[0xa0], 3).unwrap()),
        ReplValue::Atom("ready".into()),
        ReplValue::Bytes(vec![0, 255].into()),
        ReplValue::Record {
            name: "Profile".into(),
            fields: vec![("name".into(), ReplValue::String("λ".into()))],
        },
    ]);
    for policy in [
        Policy::WinnerTakesAll,
        Policy::LastWriterWins,
        Policy::Merge,
        Policy::ExplicitUserResolution,
    ] {
        let mut state = entry(payload.clone());
        state.policy = policy;
        let bytes = encode_tetf_checkpoint(std::slice::from_ref(&state), &atoms, 4096).unwrap();
        assert_eq!(decode_tetf_checkpoint(&bytes, &atoms).unwrap(), vec![state]);
        assert!(
            decode_tetf_checkpoint(&bytes, &[]).is_err(),
            "receiver atom admission"
        );
    }
}

#[test]
fn checkpoint_orders_scopes_and_rejects_duplicates() {
    let first = entry(ReplValue::Int(1));
    let mut second = entry(ReplValue::Int(2));
    second.scope.key = "c".into();
    let expected = vec![first.clone(), second.clone()];
    let bytes = encode_tetf_checkpoint(&[second.clone(), first.clone()], &[], 1024).unwrap();
    assert_eq!(decode_tetf_checkpoint(&bytes, &[]).unwrap(), expected);
    assert!(encode_tetf_checkpoint(&[first.clone(), first.clone()], &[], 1024).is_err());
    let first_bytes = encode_tetf_checkpoint(&[first], &[], 1024).unwrap();
    let mut reordered = encode_tetf_checkpoint(&[second], &[], 1024).unwrap();
    reordered[9] = 2;
    reordered.extend_from_slice(&first_bytes[10..]);
    assert!(decode_tetf_checkpoint(&reordered, &[])
        .unwrap_err()
        .contains("scope"));
}

#[test]
fn checkpoint_rejects_invalid_headers_metadata_and_tags() {
    let bytes = encode_tetf_checkpoint(&[entry(ReplValue::Int(42))], &[], 1024).unwrap();
    for (offset, value) in [(0, 0), (4, 2), (5, 2), (32, 0), (38, 0), (39, 0x20)] {
        let mut corrupt = bytes.clone();
        corrupt[offset] = value;
        assert!(
            decode_tetf_checkpoint(&corrupt, &[]).is_err(),
            "offset {offset}"
        );
    }
    let mut too_many = bytes;
    too_many[6..10].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(decode_tetf_checkpoint(&too_many, &[])
        .unwrap_err()
        .contains("count"));
    for sequence in [0, u64::MAX] {
        let mut state = entry(ReplValue::Unit);
        state.version.sequence = sequence;
        assert!(encode_tetf_checkpoint(&[state], &[], 1024).is_err());
    }
    let mut invalid = entry(ReplValue::Unit);
    invalid.owner_node_id.clear();
    assert!(encode_tetf_checkpoint(&[invalid], &[], 1024).is_err());
}

#[test]
fn checkpoint_rejects_handles_and_unknown_atoms_inside_collections() {
    let handle = ReplValue::Record {
        name: "Snapshot".into(),
        fields: vec![("$native_id".into(), ReplValue::Int(1))],
    };
    for value in [handle, ReplValue::Atom("undeclared".into())] {
        let nested = ReplValue::Map(vec![(ReplValue::Int(1), ReplValue::List(vec![value]))]);
        assert!(encode_tetf_checkpoint(&[entry(nested)], &[], 1024).is_err());
    }
}

#[test]
fn checkpoint_empty_state_has_a_real_versioned_representation() {
    let bytes = encode_tetf_checkpoint(&[], &[], 10).unwrap();
    assert_eq!(bytes, b"TETF\x01\x03\0\0\0\0");
    assert!(decode_tetf_checkpoint(&bytes, &[]).unwrap().is_empty());
}

#[test]
fn checkpoint_value_budget_limits_allocation_before_decoding_children() {
    let state = entry(ReplValue::List(vec![ReplValue::Unit; MAX_VALUES - 1]));
    let bytes = encode_tetf_checkpoint(&[state], &[], 1_000_000).unwrap();
    assert!(decode_tetf_checkpoint(&bytes, &[]).is_ok());
    let oversized = entry(ReplValue::List(vec![ReplValue::Unit; MAX_VALUES]));
    assert!(encode_tetf_checkpoint(&[oversized], &[], 1_000_000)
        .unwrap_err()
        .contains("values"));
    let mut adversarial = bytes;
    adversarial[40..44].copy_from_slice(&(MAX_VALUES as u32).to_be_bytes());
    adversarial.push(0xff);
    assert!(decode_tetf_checkpoint(&adversarial, &[])
        .unwrap_err()
        .contains("values"));
}
