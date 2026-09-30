use crate::{NestedStringFieldRow, RequiredFieldProjection, RequiredFieldRow, StringFieldRow};
use terlan_runtime_abi::NativeValue as V;

fn record(name: &str, fields: Vec<(&str, V)>) -> V {
    V::Record {
        name: name.into(),
        fields: fields
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    }
}

#[test]
fn optional_string_rows_preserve_absence_empty_strings_and_order() {
    let row = StringFieldRow {
        object: true,
        values: vec![None, Some(String::new()), Some("x\0y".into()), None],
    };
    assert_eq!(
        V::from(row),
        record(
            "StringFieldRow",
            vec![
                ("object", V::Bool(true)),
                (
                    "values",
                    V::List(vec![
                        record("None", vec![]),
                        record("Some", vec![("value", V::String(String::new()))]),
                        record("Some", vec![("value", V::String("x\0y".into()))]),
                        record("None", vec![]),
                    ])
                ),
            ]
        )
    );
}

#[test]
fn nested_rows_keep_both_presence_flags_and_nonobject_children() {
    for object in [false, true] {
        for child_array in [false, true] {
            let row = NestedStringFieldRow {
                object,
                values: vec![],
                child_array,
                children: vec![StringFieldRow {
                    object: false,
                    values: vec![],
                }],
            };
            assert_eq!(
                V::from(row),
                record(
                    "NestedStringFieldRow",
                    vec![
                        ("object", V::Bool(object)),
                        ("values", V::List(vec![])),
                        ("child_array", V::Bool(child_array)),
                        (
                            "children",
                            V::List(vec![record(
                                "StringFieldRow",
                                vec![("object", V::Bool(false)), ("values", V::List(vec![])),]
                            )])
                        ),
                    ]
                )
            );
        }
    }
}

#[test]
fn required_projections_preserve_scalar_extremes_and_empty_columns() {
    let projection = RequiredFieldProjection {
        strings: vec![],
        ints: vec![i64::MIN, 0, i64::MAX],
        array_lengths: vec![0, 256],
    };
    assert_eq!(
        V::from(projection),
        record(
            "RequiredFieldProjection",
            vec![
                ("strings", V::List(vec![])),
                (
                    "ints",
                    V::List(vec![V::Int(i64::MIN), V::Int(0), V::Int(i64::MAX)])
                ),
                ("array_lengths", V::List(vec![V::Int(0), V::Int(256)])),
            ]
        )
    );
    let row = RequiredFieldRow {
        strings: vec!["second".into(), "first".into()],
        ints: vec![],
        bools: vec![false, true, false],
    };
    assert_eq!(
        V::from(row),
        record(
            "RequiredFieldRow",
            vec![
                (
                    "strings",
                    V::List(vec![V::String("second".into()), V::String("first".into())])
                ),
                ("ints", V::List(vec![])),
                (
                    "bools",
                    V::List(vec![V::Bool(false), V::Bool(true), V::Bool(false)])
                ),
            ]
        )
    );
}
