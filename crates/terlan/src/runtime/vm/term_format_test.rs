use super::{
    encode_tetf, encode_tetf_distribution_envelope, encode_tetf_vm_ref, TetfDistributionEnvelope,
    TetfVmRef, TetfVmRefKind,
};
use crate::runtime::vm::ReplValue;

#[test]
fn tetf_encodes_primitive_values_with_header() {
    let bytes = encode_tetf(
        &ReplValue::Tuple(vec![
            ReplValue::Unit,
            ReplValue::Bool(true),
            ReplValue::Int(42),
            ReplValue::String("hello".to_string()),
        ]),
        &[],
    )
    .expect("encode TETF");

    assert_eq!(&bytes[0..4], b"TETF");
    assert_eq!(bytes[4], 1);
    assert_eq!(bytes[5], 1);
    assert_eq!(bytes[6], 0x09);
}

#[test]
fn tetf_rejects_atoms_missing_from_manifest() {
    let error = encode_tetf(&ReplValue::Atom("ready".to_string()), &[])
        .expect_err("undeclared atom should fail");

    assert!(error.starts_with("error[tetf_atom]:"));
    assert!(error.contains("ready"));
}

#[test]
fn tetf_accepts_declared_atoms() {
    let bytes = encode_tetf(
        &ReplValue::Atom("ready".to_string()),
        &[String::from("ready")],
    )
    .expect("declared atom should encode");

    assert_eq!(bytes, b"TETF\x01\x01\x07\0\0\0\x05ready".to_vec());
}

#[test]
fn tetf_map_encoding_is_deterministic_by_encoded_key() {
    let left = ReplValue::Map(vec![
        (ReplValue::String("b".to_string()), ReplValue::Int(2)),
        (ReplValue::String("a".to_string()), ReplValue::Int(1)),
    ]);
    let right = ReplValue::Map(vec![
        (ReplValue::String("a".to_string()), ReplValue::Int(1)),
        (ReplValue::String("b".to_string()), ReplValue::Int(2)),
    ]);

    assert_eq!(
        encode_tetf(&left, &[]).expect("encode left"),
        encode_tetf(&right, &[]).expect("encode right")
    );
}

#[test]
fn tetf_set_encoding_sorts_and_deduplicates_items() {
    let value = ReplValue::Set(vec![
        ReplValue::Int(3),
        ReplValue::Int(1),
        ReplValue::Int(3),
        ReplValue::Int(2),
    ]);

    let encoded = encode_tetf(&value, &[]).expect("encode set");

    assert_eq!(&encoded[0..7], b"TETF\x01\x01\x0c");
    assert_eq!(&encoded[7..11], 3u32.to_be_bytes().as_slice());
}

#[test]
fn tetf_rejects_runtime_only_values() {
    let error = encode_tetf(
        &ReplValue::Iterator {
            items: vec![ReplValue::Int(1)],
            index: 0,
        },
        &[],
    )
    .expect_err("iterator should fail");

    assert!(error.starts_with("error[tetf_unsupported]:"));
}

#[test]
fn tetf_encodes_vm_refs_with_kind_node_local_id_and_epoch() {
    let reference = TetfVmRef::new(TetfVmRefKind::Process, "node-a", 42, 7);

    let encoded = encode_tetf_vm_ref(&reference).expect("reference should encode");

    assert_eq!(&encoded[0..4], b"TETF");
    assert_eq!(encoded[4], 1);
    assert_eq!(encoded[5], 2);
    assert_eq!(encoded[6], 0x20);
    assert_eq!(encoded[7], 1);
    assert!(contains_bytes(&encoded, b"node-a"));
    assert!(encoded.ends_with(&7u64.to_be_bytes()));
}

#[test]
fn tetf_encodes_distribution_envelope_with_refs_and_payload() {
    let envelope = TetfDistributionEnvelope::new(
        "trace-1",
        "node-a",
        "node-b",
        9,
        vec![
            TetfVmRef::new(TetfVmRefKind::Monitor, "node-a", 1, 9),
            TetfVmRef::new(TetfVmRefKind::Timer, "node-b", 2, 9),
            TetfVmRef::new(TetfVmRefKind::Resource, "node-b", 3, 9),
        ],
        ReplValue::Tuple(vec![
            ReplValue::Atom("ready".to_string()),
            ReplValue::String("payload".to_string()),
        ]),
    );

    let encoded = encode_tetf_distribution_envelope(&envelope, &[String::from("ready")])
        .expect("distribution envelope should encode");

    assert_eq!(&encoded[0..4], b"TETF");
    assert_eq!(encoded[4], 1);
    assert_eq!(encoded[5], 2);
    assert_eq!(encoded[6], 0x21);
    assert!(contains_bytes(&encoded, b"trace-1"));
    assert!(contains_bytes(&encoded, b"node-a"));
    assert!(contains_bytes(&encoded, b"node-b"));
    assert!(contains_bytes(&encoded, b"payload"));
}

#[test]
fn tetf_distribution_envelope_rejects_payload_atoms_missing_from_manifest() {
    let envelope = TetfDistributionEnvelope::new(
        "trace-1",
        "node-a",
        "node-b",
        9,
        Vec::new(),
        ReplValue::Atom("dynamic".to_string()),
    );

    let error = encode_tetf_distribution_envelope(&envelope, &[])
        .expect_err("undeclared payload atom should fail");

    assert!(error.starts_with("error[tetf_atom]:"));
    assert!(error.contains("dynamic"));
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|candidate| candidate == needle)
}
