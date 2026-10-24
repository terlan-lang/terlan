use super::*;
use crate::{native_record, DescriptorView, NativeValue as V};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn record(fields: Vec<(&str, V)>) -> V {
    V::Record {
        name: "Message".into(),
        fields: fields
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    }
}

#[test]
fn fields_move_in_schema_order_for_every_wire_permutation() {
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let text = "allocated payload".to_owned();
        let pointer = text.as_ptr();
        let mut fields = vec![
            ("text", V::String(text)),
            ("count", V::Int(7)),
            ("items", V::List(vec![])),
        ];
        fields.sort_by_key(|(name, _)| {
            let index = ["text", "count", "items"]
                .iter()
                .position(|n| n == name)
                .unwrap();
            order.iter().position(|i| *i == index).unwrap()
        });
        let mut evaluations = 0;
        let [text, count, items] = native_record_fields!(
            {
                evaluations += 1;
                record(fields)
            },
            Message { text, count, items }
        )
        .unwrap();
        assert_eq!(evaluations, 1);
        let V::String(text) = text else {
            panic!("text");
        };
        assert_eq!(text.as_ptr(), pointer);
        assert_eq!(count, V::Int(7));
        assert_eq!(items, V::List(vec![]));
    }
}

#[test]
fn malformed_shapes_never_project_a_partial_record_or_expose_payloads() {
    for value in [
        V::Unit,
        V::String("secret-payload".into()),
        V::Tuple(vec![]),
        V::Record {
            name: "Other".into(),
            fields: vec![],
        },
        record(vec![]),
        record(vec![("text", V::Unit)]),
        record(vec![
            ("text", V::Unit),
            ("count", V::Unit),
            ("extra", V::Unit),
        ]),
        record(vec![("text", V::Unit), ("text", V::Unit)]),
        record(vec![("count", V::Unit), ("count", V::Unit)]),
        record(vec![("Text", V::Unit), ("count", V::Unit)]),
        record(vec![("text", V::Unit), ("unknown", V::Unit)]),
    ] {
        let error = native_record_fields!(value, Message { text, count }).unwrap_err();
        assert_eq!(error.code(), "native.record");
        assert_eq!(error.message(), "record shape mismatch");
    }
    assert!(into_record_fields(
        record(vec![("text", V::Unit), ("count", V::Unit)]),
        "Message",
        ["text", "text"]
    )
    .is_err());
}

#[test]
fn exact_shapes_reject_each_field_position_at_multiple_record_widths() {
    fn check<const N: usize>() {
        let names: [String; N] = std::array::from_fn(|index| format!("field_{index}"));
        let schema = names.each_ref().map(String::as_str);
        let original = names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.as_str(), V::Int(index as i64)))
            .collect::<Vec<_>>();
        for index in 0..N {
            let mut permuted = original.clone();
            permuted.rotate_left(index);
            assert_eq!(
                into_record_fields(record(permuted), "Message", schema).unwrap(),
                std::array::from_fn(|index| V::Int(index as i64))
            );
            let mut missing = original.clone();
            missing.remove(index);
            let mut unknown = original.clone();
            unknown[index].0 = "unknown";
            let mut extra = original.clone();
            extra.push(original[index].clone());
            for fields in [missing, unknown, extra] {
                assert!(into_record_fields(record(fields), "Message", schema).is_err());
            }
            if N > 1 {
                let mut duplicate = original.clone();
                duplicate[index].0 = schema[(index + 1) % N];
                assert!(into_record_fields(record(duplicate), "Message", schema).is_err());
                let mut repeated = schema;
                repeated[index] = schema[(index + 1) % N];
                assert!(into_record_fields(record(original.clone()), "Message", repeated).is_err());
            }
        }
    }
    check::<1>();
    check::<2>();
    check::<3>();
    check::<9>();
    check::<32>();
}

#[test]
fn empty_and_singleton_records_preserve_exact_identity() {
    assert_eq!(
        native_record_fields!(record(vec![]), Message {}).unwrap(),
        []
    );
    assert!(native_record_fields!(V::Unit, Message {}).is_err());
    assert!(native_record_fields!(record(vec![("extra", V::Unit)]), Message {}).is_err());
    let text = String::from("one");
    assert_eq!(
        native_record_fields!(native_record!(Message, { text }), Message { text }).unwrap(),
        [V::String("one".into())]
    );
}

#[test]
fn declaration_bindings_do_not_shadow_input_expressions_or_caller_locals() {
    let text = "caller";
    let __native_record_source = record(vec![("text", text.into())]);
    let [extracted] = native_record_fields!(__native_record_source, Message { text }).unwrap();
    assert_eq!(extracted, V::String(text.into()));
    assert_eq!(text, "caller");
}

struct DropCount(Arc<AtomicUsize>);

impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

enum Probe {
    Record(Vec<(String, Probe)>),
    Opaque(DropCount),
}

impl DescriptorValue for Probe {
    fn descriptor_view(&self) -> DescriptorView<'_, Self> {
        panic!("owned projection must not borrow or convert leaf values");
    }

    fn into_descriptor(self) -> OwnedDescriptor<Self> {
        match self {
            Self::Record(fields) => OwnedDescriptor::Record("Message".into(), fields),
            Self::Opaque(resource) => {
                drop(resource);
                panic!("owned projection must leave resources opaque");
            }
        }
    }
}

#[test]
fn opaque_noncloneable_values_move_or_drop_exactly_once() {
    for names in [
        ["one", "two"],
        ["two", "one"],
        ["one", "one"],
        ["one", "other"],
    ] {
        let drops = Arc::new(AtomicUsize::new(0));
        let fields = names
            .into_iter()
            .map(|name| (name.into(), Probe::Opaque(DropCount(Arc::clone(&drops)))))
            .collect();
        let result = native_record_fields!(Probe::Record(fields), Message { one, two });
        if names == ["one", "two"] || names == ["two", "one"] {
            assert!(result.is_ok());
            assert_eq!(drops.load(Ordering::Relaxed), 0);
        } else {
            assert!(result.is_err());
            assert_eq!(drops.load(Ordering::Relaxed), 2);
        }
        drop(result);
        assert_eq!(drops.load(Ordering::Relaxed), 2);
    }
}
