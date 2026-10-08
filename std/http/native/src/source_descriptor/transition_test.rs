use super::*;
use terlan_runtime_abi::NativeValue as V;

#[test]
fn match_payloads_move_without_copying_and_preserve_empty_frames() {
    for values in [["first", "second"], ["", ""], ["", "second"], ["first", ""]] {
        let [first, second] = values.map(String::from);
        let pointers = [first.as_ptr(), second.as_ptr()];
        let output = paired_match(V::Tuple(vec![first.into(), second.into()])).unwrap();
        assert_eq!([output.0.as_str(), output.1.as_str()], values);
        assert_eq!([output.0.as_ptr(), output.1.as_ptr()], pointers);
    }
}

#[test]
fn match_admission_rejects_wrong_shapes_and_either_malformed_payload() {
    for value in [
        V::Unit,
        V::List(vec!["one".into(), "two".into()]),
        "one".into(),
    ] {
        assert!(paired_match(value).is_err());
    }
    for size in [0, 1, 3, 4] {
        assert!(paired_match(V::Tuple(vec!["text".into(); size])).is_err());
    }
    for value in [
        V::Int(1),
        V::Unit,
        V::Atom("text".into()),
        some("text".into()),
        V::Tuple(vec![]),
    ] {
        assert!(paired_match(V::Tuple(vec![value.clone(), "valid".into()])).is_err());
        assert!(paired_match(V::Tuple(vec!["valid".into(), value])).is_err());
    }
}

fn some(value: V) -> V {
    V::Record {
        name: "Some".into(),
        fields: vec![("value".into(), value)],
    }
}

fn transition(first: V, second: V) -> V {
    V::Tuple(vec!["next".into(), first, second])
}

#[test]
fn source_result_preserves_deliveries_and_propagates_rejection() {
    let value = transition(some("".into()), V::Atom("none".into()));
    assert_eq!(
        paired_callback_transition(V::from(Ok::<_, String>(value))).unwrap(),
        ("next".into(), Some(String::new()), None)
    );
    let error = paired_callback_transition(V::from(Err::<V, _>("source rejection".to_string())))
        .unwrap_err();
    assert_eq!(error.message(), "source rejection");
}

#[test]
fn source_result_rejects_legacy_tuples_and_forged_wrappers() {
    let valid = transition(some("first".into()), some("second".into()));
    for value in [V::Unit, valid.clone(), V::from(Ok::<_, String>(V::Unit))] {
        assert!(paired_callback_transition(value).is_err());
    }
    for (name, fields) in [
        ("Ok", vec![]),
        (
            "Ok",
            vec![("value", valid.clone()), ("value", valid.clone())],
        ),
        ("Ok", vec![("other", valid.clone())]),
        ("Other", vec![("value", valid)]),
        ("Err", vec![("value", "reason".into())]),
        ("Err", vec![("reason", V::Int(1))]),
    ] {
        assert!(paired_callback_transition(V::Record {
            name: name.into(),
            fields: fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        })
        .is_err());
    }
}

#[test]
fn transfers_paired_output_without_copying_text() {
    let [next, first, second] = ["next state", "first payload", "second payload"].map(String::from);
    let pointers = [next.as_ptr(), first.as_ptr(), second.as_ptr()];
    let output = paired_transition(V::Tuple(vec![
        next.into(),
        some(first.into()),
        some(second.into()),
    ]))
    .unwrap();
    assert_eq!(
        [
            output.0.as_ptr(),
            output.1.as_ref().unwrap().as_ptr(),
            output.2.as_ref().unwrap().as_ptr()
        ],
        pointers
    );
    assert_eq!(
        output,
        (
            "next state".into(),
            Some("first payload".into()),
            Some("second payload".into())
        )
    );
}

#[test]
fn optional_deliveries_distinguish_empty_frames_from_no_frame() {
    for none in [
        V::Atom("none".into()),
        V::Record {
            name: "None".into(),
            fields: vec![],
        },
    ] {
        assert_eq!(
            paired_transition(transition(none.clone(), none.clone())).unwrap(),
            ("next".into(), None, None)
        );
        assert_eq!(
            paired_transition(transition(some("".into()), none.clone())).unwrap(),
            ("next".into(), Some(String::new()), None)
        );
        assert_eq!(
            paired_transition(transition(none, some("".into()))).unwrap(),
            ("next".into(), None, Some(String::new()))
        );
    }
}

#[test]
fn rejects_non_tuple_wrong_length_and_non_string_state() {
    for value in [V::Unit, V::List(vec!["".into(); 3]), "tuple".into()] {
        assert!(paired_transition(value).is_err());
    }
    for length in [0, 1, 2, 4, 5] {
        assert!(paired_transition(V::Tuple(vec!["".into(); length])).is_err());
    }
    for invalid in [V::Unit, V::Int(1), V::Atom("text".into()), V::Tuple(vec![])] {
        assert!(
            paired_transition(V::Tuple(vec![invalid, some("".into()), some("".into())])).is_err()
        );
    }
}

#[test]
fn rejects_malformed_delivery_in_either_position() {
    for invalid in [
        V::Unit,
        V::Int(1),
        "".into(),
        V::Atom("None".into()),
        V::Tuple(vec![]),
        some(V::Int(1)),
        some(V::Unit),
        some(some("nested".into())),
        V::Record {
            name: "Some".into(),
            fields: vec![],
        },
        V::Record {
            name: "Some".into(),
            fields: vec![("other".into(), "text".into())],
        },
        V::Record {
            name: "Some".into(),
            fields: vec![
                ("value".into(), "one".into()),
                ("value".into(), "two".into()),
            ],
        },
        V::Record {
            name: "None".into(),
            fields: vec![("value".into(), "text".into())],
        },
        V::Record {
            name: "Other".into(),
            fields: vec![],
        },
    ] {
        assert!(paired_transition(transition(invalid.clone(), some("valid".into()))).is_err());
        assert!(paired_transition(transition(some("valid".into()), invalid)).is_err());
    }
}
