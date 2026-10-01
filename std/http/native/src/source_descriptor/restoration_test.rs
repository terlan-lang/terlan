use super::*;
use terlan_runtime_abi::NativeValue as V;

fn record_value(name: &str, fields: Vec<(&str, V)>) -> V {
    V::Record {
        name: name.into(),
        fields: fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    }
}

fn ok(value: V) -> V {
    record_value("Ok", vec![("value", value)])
}

fn some(value: V) -> V {
    record_value("Some", vec![("value", value)])
}

#[test]
fn admits_fresh_and_both_restored_roles() {
    for none in [V::Atom("none".into()), record_value("None", vec![])] {
        assert_eq!(restoration_identity(&ok(none)).unwrap(), None);
    }
    for role in [1, 2] {
        let value = ok(some(V::Tuple(vec!["room".into(), V::Int(role)])));
        assert_eq!(
            restoration_identity(&value).unwrap(),
            Some(("room".into(), role))
        );
    }
    let rejected = record_value("Err", vec![("reason", "source policy rejection".into())]);
    assert_eq!(
        restoration_identity(&rejected).unwrap_err().message(),
        "source policy rejection"
    );
}

#[test]
fn rejects_malformed_or_forged_callback_results() {
    for value in [
        V::Unit,
        record_value("Unknown", vec![]),
        record_value("Ok", vec![]),
        record_value("Ok", vec![("value", V::Unit), ("value", V::Unit)]),
        record_value("Ok", vec![("wrong", V::Unit)]),
        record_value("Err", vec![("reason", V::Int(1))]),
        ok(record_value("None", vec![("value", V::Unit)])),
        ok(V::Atom("some".into())),
        ok(record_value("Some", vec![])),
        ok(record_value("Some", vec![("wrong", V::Unit)])),
        ok(some(V::Unit)),
        ok(some(V::Tuple(vec![]))),
        ok(some(V::Tuple(vec!["room".into(), V::Int(1), V::Unit]))),
        ok(some(V::Tuple(vec![V::Unit, V::Int(1)]))),
        ok(some(V::Tuple(vec!["".into(), V::Int(1)]))),
        ok(some(V::Tuple(vec!["room".into(), "1".into()]))),
        ok(some(V::Tuple(vec!["room".into(), V::Int(0)]))),
        ok(some(V::Tuple(vec!["room".into(), V::Int(3)]))),
        ok(some(V::Tuple(vec!["room".into(), V::Int(i64::MIN)]))),
        ok(some(V::Tuple(vec!["room".into(), V::Int(i64::MAX)]))),
    ] {
        assert!(restoration_identity(&value).is_err(), "{value:?}");
    }
}
