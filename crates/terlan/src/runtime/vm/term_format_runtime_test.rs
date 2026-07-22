use super::{
    decode_tetf, encode_tetf, encode_tetf_distribution_envelope, ReplValue,
    TetfDistributionEnvelope,
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
