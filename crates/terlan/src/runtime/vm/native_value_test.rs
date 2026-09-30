use super::*;

#[test]
fn maps_roundtrip_nested_keys_and_values_without_reinterpretation() {
    let value = ReplValue::Map(vec![
        (
            ReplValue::Tuple(vec![ReplValue::Int(1)]),
            ReplValue::Map(vec![]),
        ),
        (
            ReplValue::String("key".into()),
            ReplValue::List(vec![ReplValue::Bool(true)]),
        ),
    ]);
    assert_eq!(from_native(to_native(&value).unwrap()), value);
}

#[test]
fn replacement_preserves_storage_and_removes_old_collection_entries() {
    let mut value = ReplValue::Record {
        name: "Envelope".into(),
        fields: vec![(
            "items".into(),
            ReplValue::Map(vec![(ReplValue::Int(1), ReplValue::Int(2))]),
        )],
    };
    let ReplValue::Record { fields, .. } = &value else {
        unreachable!()
    };
    let pointer = fields.as_ptr();
    let next = NativeValue::Record {
        name: "Envelope".into(),
        fields: vec![("items".into(), NativeValue::Map(vec![]))],
    };
    replace_native(&mut value, next.clone());
    let ReplValue::Record { fields, .. } = &value else {
        unreachable!()
    };
    assert_eq!(pointer, fields.as_ptr());
    assert_eq!(value, from_native(next));
}

#[test]
fn replacement_repairs_changed_shapes_and_clears_sequences() {
    let targets = [
        ReplValue::Unit,
        ReplValue::List(vec![ReplValue::Int(42)]),
        ReplValue::Tuple(vec![ReplValue::Int(42)]),
        ReplValue::Record {
            name: "Wrong".into(),
            fields: vec![],
        },
    ];
    let sources = [
        NativeValue::Unit,
        NativeValue::List(vec![]),
        NativeValue::Tuple(vec![]),
        NativeValue::Map(vec![]),
        NativeValue::Record {
            name: "Right".into(),
            fields: vec![("value".into(), NativeValue::Int(1))],
        },
    ];
    for target in targets {
        for source in &sources {
            let mut value = target.clone();
            replace_native(&mut value, source.clone());
            assert_eq!(value, from_native(source.clone()));
        }
    }
}
