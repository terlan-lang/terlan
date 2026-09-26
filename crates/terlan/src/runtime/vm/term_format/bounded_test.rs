//! Byte-exact transport limits apply before output/canonical-sort allocation.

use super::*;

fn envelope(payload: ReplValue) -> TetfDistributionEnvelope {
    TetfDistributionEnvelope::new("t", "a", "b", 1, vec![], payload)
}

#[test]
fn bounded_codec_preserves_the_existing_wire_format() {
    let mut golden = b"TETF\x01\x02\x21".to_vec();
    for text in [b't', b'a', b'b'] {
        golden.extend_from_slice(&1_u32.to_be_bytes());
        golden.push(text);
    }
    golden.extend_from_slice(&1_u64.to_be_bytes());
    golden.extend_from_slice(&0_u32.to_be_bytes());
    golden.push(TAG_UNIT);
    assert_eq!(
        encode_tetf_distribution_envelope_bounded(&envelope(ReplValue::Unit), &[], golden.len())
            .unwrap(),
        golden
    );
}

#[test]
fn every_shorter_limit_rejects_nested_values_and_exact_limits_succeed() {
    let atoms = vec!["ready".into()];
    let payloads = vec![
        ReplValue::Unit,
        ReplValue::Bytes(vec![42; 256].into()),
        ReplValue::Tuple(vec![ReplValue::Atom("ready".into()), ReplValue::Int(42)]),
        ReplValue::List(vec![ReplValue::String("payload".into()); 8]),
        ReplValue::Map(vec![
            (
                ReplValue::String("b".into()),
                ReplValue::Bytes(vec![1; 64].into()),
            ),
            (ReplValue::String("a".into()), ReplValue::Int(3)),
        ]),
        ReplValue::Record {
            name: "app.Checkpoint".into(),
            fields: vec![
                ("b".into(), ReplValue::Int(2)),
                ("a".into(), ReplValue::Int(1)),
            ],
        },
        ReplValue::Set(vec![
            ReplValue::Int(2),
            ReplValue::Int(1),
            ReplValue::Int(2),
        ]),
        ReplValue::Set(vec![
            ReplValue::Set(vec![
                ReplValue::Int(2),
                ReplValue::Int(1)
            ]);
            3
        ]),
    ];
    for payload in payloads {
        let value = envelope(payload);
        let expected = encode_tetf_distribution_envelope(&value, &atoms).unwrap();
        for maximum in 0..expected.len() {
            let error = encode_tetf_distribution_envelope_bounded(&value, &atoms, maximum)
                .expect_err("short output budget must fail");
            assert!(error.starts_with("error[tetf_size]"), "{error}");
        }
        assert_eq!(
            encode_tetf_distribution_envelope_bounded(&value, &atoms, expected.len()).unwrap(),
            expected
        );
        let decoded = decode_tetf_distribution_envelope(&expected, &atoms).unwrap();
        assert_eq!(
            encode_tetf_distribution_envelope(&decoded, &atoms).unwrap(),
            expected
        );
    }
}

#[test]
fn oversized_sort_keys_and_values_fail_under_the_shared_byte_budget() {
    let large = ReplValue::String("x".repeat(4096));
    for value in [
        ReplValue::Map(vec![(large.clone(), ReplValue::Unit)]),
        ReplValue::Map(vec![(ReplValue::Unit, large.clone())]),
        ReplValue::Set(vec![large.clone()]),
        ReplValue::Record {
            name: "app.Data".into(),
            fields: vec![("value".into(), large)],
        },
    ] {
        assert!(
            encode_tetf_distribution_envelope_bounded(&envelope(value), &[], 128)
                .unwrap_err()
                .starts_with("error[tetf_size]")
        );
    }
}

#[test]
fn oversized_metadata_and_nested_native_handles_are_not_admitted() {
    let mut value = envelope(ReplValue::Unit);
    value.trace_id = "x".repeat(4096);
    assert!(encode_tetf_distribution_envelope_bounded(&value, &[], 128).is_err());
    let value = envelope(ReplValue::List(vec![ReplValue::Record {
        name: "app.Resource".into(),
        fields: vec![("$native_id".into(), ReplValue::Int(1))],
    }]));
    assert!(encode_tetf_distribution_envelope_bounded(&value, &[], 1024)
        .unwrap_err()
        .starts_with("error[tetf_unsupported]"));
}
