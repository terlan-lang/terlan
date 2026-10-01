use super::*;
use terlan_runtime_abi::NativeValue as V;

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
