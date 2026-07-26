use super::super::bitstring::VmBitString;
use super::{
    decode_tetf, decode_tetf_distribution_envelope, encode_tetf, encode_tetf_distribution_envelope,
    encode_tetf_vm_ref, ReplValue, TetfDistributionEnvelope, TetfVmRef, TetfVmRefKind, MAGIC,
    PROFILE_RUNTIME_TERM, TAG_INT, TAG_RECORD, VERSION,
};

#[test]
fn tetf_runtime_term_roundtrips_nested_portable_values() {
    let atoms = vec![String::from("sample")];
    let value = ReplValue::Tuple(vec![
        ReplValue::Atom("sample".to_string()),
        ReplValue::List(vec![ReplValue::Int(1), ReplValue::Int(2)]),
        ReplValue::Map(vec![(
            ReplValue::Atom("sample".to_string()),
            ReplValue::Bytes(vec![1, 2, 3].into()),
        )]),
    ]);

    let encoded = encode_tetf(&value, &atoms).expect("runtime term should encode");
    let decoded = decode_tetf(&encoded, &atoms).expect("runtime term should decode");

    assert_eq!(decoded, value);
}

#[test]
fn tetf_encodes_bitstrings_with_exact_logical_length() {
    let bitstring =
        VmBitString::from_bytes([0b1010_1010, 0b1100_1000], 13).expect("valid bitstring");
    let value = ReplValue::BitString(bitstring);

    let encoded = encode_tetf(&value, &[]).expect("bitstring should encode");
    let decoded = decode_tetf(&encoded, &[]).expect("bitstring should decode");

    assert_eq!(decoded, value);
}

#[test]
fn tetf_runtime_term_decoder_rejects_wrong_profile_atoms_and_trailing_data() {
    let atoms = vec![String::from("sample")];
    let value = ReplValue::Atom("sample".to_string());
    let mut encoded = encode_tetf(&value, &atoms).expect("runtime term should encode");

    assert_eq!(
        decode_tetf(&encoded, &[]).expect_err("undeclared atom must fail"),
        "error[tetf_atom]: atom `sample` is not in the declared atom manifest"
    );

    encoded.push(0);
    assert_eq!(
        decode_tetf(&encoded, &atoms).expect_err("trailing bytes must fail"),
        "error[tetf_trailing]: 1 unconsumed payload bytes"
    );

    let envelope =
        TetfDistributionEnvelope::new("trace-1", "node-a", "node-b", 1, vec![], ReplValue::Int(1));
    let envelope_bytes =
        encode_tetf_distribution_envelope(&envelope, &[]).expect("envelope should encode");
    assert_eq!(
        decode_tetf(&envelope_bytes, &[]).expect_err("envelope profile must fail"),
        "error[tetf_profile]: expected profile 1, found 2"
    );
}

#[test]
fn tetf_distribution_envelope_roundtrips_refs_and_rejects_invalid_metadata() {
    let refs = [
        TetfVmRefKind::Process,
        TetfVmRefKind::Monitor,
        TetfVmRefKind::Timer,
        TetfVmRefKind::Resource,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, kind)| TetfVmRef::new(kind, "node-a", index as u64 + 1, 9))
    .collect::<Vec<_>>();
    for reference in &refs {
        let bytes = encode_tetf_vm_ref(reference).expect("standalone reference should encode");
        assert_eq!(&bytes[..MAGIC.len()], MAGIC);
    }
    let envelope = TetfDistributionEnvelope::new(
        "trace-ref",
        "node-a",
        "node-b",
        9,
        refs,
        ReplValue::Tuple(vec![ReplValue::Int(0), ReplValue::String("ok".into())]),
    );
    let bytes = encode_tetf_distribution_envelope(&envelope, &[]).expect("envelope should encode");
    assert_eq!(
        decode_tetf_distribution_envelope(&bytes, &[]).expect("envelope should decode"),
        envelope
    );

    for invalid in [
        TetfDistributionEnvelope::new("", "node-a", "node-b", 9, vec![], ReplValue::Unit),
        TetfDistributionEnvelope::new("trace", "", "node-b", 9, vec![], ReplValue::Unit),
        TetfDistributionEnvelope::new("trace", "node-a", "", 9, vec![], ReplValue::Unit),
        TetfDistributionEnvelope::new("trace", "node-a", "node-b", 0, vec![], ReplValue::Unit),
        TetfDistributionEnvelope::new(
            "trace",
            "node-a",
            "node-b",
            9,
            vec![TetfVmRef::new(TetfVmRefKind::Process, "node-a", 0, 9)],
            ReplValue::Unit,
        ),
    ] {
        assert!(encode_tetf_distribution_envelope(&invalid, &[]).is_err());
    }
}

#[test]
fn native_record_suite_roundtrips_canonical_nominal_records() {
    let value = ReplValue::Record {
        name: "app.NativeRecord".to_string(),
        fields: vec![
            ("z".to_string(), ReplValue::Int(3)),
            ("a".to_string(), ReplValue::Int(1)),
            (
                "nested".to_string(),
                ReplValue::Record {
                    name: "app.Pair".to_string(),
                    fields: vec![
                        ("right".to_string(), ReplValue::Int(22)),
                        ("left".to_string(), ReplValue::Int(20)),
                    ],
                },
            ),
        ],
    };

    let encoded = encode_tetf(&value, &[]).expect("native record should encode");
    let decoded = decode_tetf(&encoded, &[]).expect("native record should decode");
    assert_eq!(
        decoded,
        ReplValue::Record {
            name: "app.NativeRecord".to_string(),
            fields: vec![
                ("a".to_string(), ReplValue::Int(1)),
                (
                    "nested".to_string(),
                    ReplValue::Record {
                        name: "app.Pair".to_string(),
                        fields: vec![
                            ("left".to_string(), ReplValue::Int(20)),
                            ("right".to_string(), ReplValue::Int(22)),
                        ],
                    },
                ),
                ("z".to_string(), ReplValue::Int(3)),
            ],
        }
    );
    assert_eq!(
        encode_tetf(&decoded, &[]).expect("canonical record should re-encode"),
        encoded
    );
}

#[test]
fn native_record_suite_rejects_invalid_or_noncanonical_metadata() {
    for value in [
        ReplValue::Record {
            name: " ".to_string(),
            fields: Vec::new(),
        },
        ReplValue::Record {
            name: "app.Record".to_string(),
            fields: vec![("".to_string(), ReplValue::Int(1))],
        },
    ] {
        assert!(encode_tetf(&value, &[])
            .expect_err("invalid record metadata must fail")
            .starts_with("error[tetf_invalid_metadata]:"));
    }
    let duplicate = ReplValue::Record {
        name: "app.Record".to_string(),
        fields: vec![
            ("value".to_string(), ReplValue::Int(1)),
            ("value".to_string(), ReplValue::Int(2)),
        ],
    };
    assert_eq!(
        encode_tetf(&duplicate, &[]).expect_err("duplicate field must fail"),
        "error[tetf_canonical]: duplicate record field `value`"
    );

    let mut noncanonical = Vec::from(MAGIC.as_slice());
    noncanonical.extend([VERSION, PROFILE_RUNTIME_TERM, TAG_RECORD]);
    push_text(&mut noncanonical, "app.Record");
    noncanonical.extend(2_u32.to_be_bytes());
    push_text(&mut noncanonical, "z");
    noncanonical.push(TAG_INT);
    noncanonical.extend(1_i64.to_be_bytes());
    push_text(&mut noncanonical, "a");
    noncanonical.push(TAG_INT);
    noncanonical.extend(2_i64.to_be_bytes());
    assert_eq!(
        decode_tetf(&noncanonical, &[]).expect_err("unsorted fields must fail"),
        "error[tetf_canonical]: record fields must be strictly ordered"
    );
}

#[test]
fn native_record_suite_repeated_distribution_roundtrip_preserves_payload() {
    let record = ReplValue::Record {
        name: "app.DistributedRecord".to_string(),
        fields: (1..=64)
            .map(|index| (format!("f{index:02}"), ReplValue::Int(index)))
            .collect(),
    };
    let envelope =
        TetfDistributionEnvelope::new("trace-record", "node-a", "node-b", 7, vec![], record);

    for _ in 0..2 {
        let encoded =
            encode_tetf_distribution_envelope(&envelope, &[]).expect("distribution record encode");
        assert_eq!(
            decode_tetf_distribution_envelope(&encoded, &[]).expect("distribution record decode"),
            envelope
        );
    }
}

fn push_text(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend(
        u32::try_from(value.len())
            .expect("test text length")
            .to_be_bytes(),
    );
    bytes.extend(value.as_bytes());
}
