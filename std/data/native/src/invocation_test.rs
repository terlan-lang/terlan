use super::{invoke, mutate};
use crate::{Json, RESOURCE_OPERATIONS};
use std::collections::BTreeSet;
use terlan_runtime_abi::{NativeResourceValue as R, NativeValue as V};

fn parsed(text: &str) -> Json {
    crate::parse(text).unwrap()
}
fn value<T: Into<V>>(value: T) -> R<&'static Json> {
    R::Value(value.into())
}
fn resource(text: &str) -> R<Json> {
    R::Resource(parsed(text))
}

fn invoke_registered(
    operation: &str,
    args: &[R<&Json>],
) -> Result<R<Json>, terlan_runtime_abi::NativeAdapterError> {
    let mut store = terlan_runtime_abi::ResourceRegistry::new();
    let args = args
        .iter()
        .map(|arg| match arg {
            R::Value(value) => R::Value(value.clone()),
            R::Resource(value) => R::Resource(store.insert_for_owner(7, (*value).clone()).unwrap()),
        })
        .collect::<Vec<_>>();
    crate::RESOURCE_ADAPTER
        .call(&mut store, 7, operation, &args)
        .map(|result| match result {
            R::Value(value) => R::Value(value),
            R::Resource(handle) => {
                assert_eq!(
                    store.validate_owner(handle, 8).unwrap_err().code(),
                    "resource.owner"
                );
                R::Resource(store.get_for_owner(handle, 7).unwrap().clone())
            }
        })
}

#[test]
fn every_read_operation_executes_its_package_adapter() {
    let object = parsed(r#"{"name":"Ada","count":9,"items":[1,2]}"#);
    let rows = parsed(
        r#"[{"name":"first","count":1,"active":true},{"name":"second","count":2,"active":false}]"#,
    );
    let nested =
        parsed(r#"[{"name":"first","kids":[]},{"name":"second","kids":[{"label":"child"}]}]"#);
    let array = parsed("[1,2]");
    let string = parsed(r#""text""#);
    let int = crate::int(42);
    let float = crate::float(1.5).unwrap();
    let boolean = crate::r#bool(true);
    let null = crate::null();
    let cases = vec![
        ("null", vec![], R::Resource(crate::null())),
        (
            "bool",
            vec![value(false)],
            R::Resource(crate::r#bool(false)),
        ),
        (
            "int",
            vec![value(i64::MIN)],
            R::Resource(crate::int(i64::MIN)),
        ),
        (
            "float",
            vec![value(1.5)],
            R::Resource(crate::float(1.5).unwrap()),
        ),
        (
            "string",
            vec![value("text")],
            R::Resource(crate::string("text")),
        ),
        ("array", vec![], resource("[]")),
        ("object", vec![], resource("{}")),
        ("parse", vec![value("[1,2]")], resource("[1,2]")),
        (
            "to_string",
            vec![R::Resource(&array)],
            R::Value("[1,2]".into()),
        ),
        (
            "stringify",
            vec![R::Resource(&array)],
            R::Value("[1,2]".into()),
        ),
        (
            "stringify_pretty",
            vec![R::Resource(&array)],
            R::Value("[\n  1,\n  2\n]".into()),
        ),
        (
            "get",
            vec![R::Resource(&object), value("count")],
            R::Resource(crate::int(9)),
        ),
        (
            "keys",
            vec![R::Resource(&object)],
            R::Value(vec!["count", "items", "name"].into()),
        ),
        (
            "object_length",
            vec![R::Resource(&object)],
            R::Value(V::Int(3)),
        ),
        ("length", vec![R::Resource(&array)], R::Value(V::Int(2))),
        (
            "at",
            vec![R::Resource(&array), value(1_i64)],
            R::Resource(crate::int(2)),
        ),
        (
            "as_string",
            vec![R::Resource(&string)],
            R::Value("text".into()),
        ),
        ("as_int", vec![R::Resource(&int)], R::Value(V::Int(42))),
        (
            "as_float",
            vec![R::Resource(&float)],
            R::Value(V::Float(1.5)),
        ),
        (
            "as_bool",
            vec![R::Resource(&boolean)],
            R::Value(V::Bool(true)),
        ),
        ("is_null", vec![R::Resource(&null)], R::Value(V::Bool(true))),
        (
            "string_field_rows",
            vec![R::Resource(&rows), value(vec!["name", "missing"])],
            R::Value(
                crate::string_field_rows(&rows, &["name", "missing"])
                    .unwrap()
                    .into(),
            ),
        ),
        (
            "required_fields",
            vec![
                R::Resource(&object),
                value(vec!["name"]),
                value(vec!["count"]),
                value(vec!["items"]),
            ],
            R::Value(
                crate::required_fields(&object, &["name"], &["count"], &["items"])
                    .unwrap()
                    .into(),
            ),
        ),
        (
            "required_field_rows",
            vec![
                R::Resource(&rows),
                value(vec!["name"]),
                value(vec!["count"]),
                value(vec!["active"]),
            ],
            R::Value(
                crate::required_field_rows(&rows, &["name"], &["count"], &["active"])
                    .unwrap()
                    .into(),
            ),
        ),
        (
            "required_field_rows_page",
            vec![
                R::Resource(&rows),
                value(1_i64),
                value(1_i64),
                value(vec!["name"]),
                value(vec!["count"]),
                value(vec!["active"]),
            ],
            R::Value(
                crate::required_field_rows_page(&rows, 1, 1, &["name"], &["count"], &["active"])
                    .unwrap()
                    .into(),
            ),
        ),
        (
            "nested_string_field_rows",
            vec![
                R::Resource(&nested),
                value(vec!["name"]),
                value("kids"),
                value(vec!["label"]),
            ],
            R::Value(
                crate::nested_string_field_rows(&nested, &["name"], "kids", &["label"])
                    .unwrap()
                    .into(),
            ),
        ),
        (
            "nested_string_field_rows_page",
            vec![
                R::Resource(&nested),
                value(1_i64),
                value(1_i64),
                value(vec!["name"]),
                value("kids"),
                value(vec!["label"]),
            ],
            R::Value(
                crate::nested_string_field_rows_page(&nested, 1, 1, &["name"], "kids", &["label"])
                    .unwrap()
                    .into(),
            ),
        ),
        (
            "string_object_rows",
            vec![value(vec!["name"]), value(vec![vec!["Ada"]])],
            resource(r#"[{"name":"Ada"}]"#),
        ),
    ];
    let mut seen = BTreeSet::new();
    for (method, mut args, expected) in cases {
        let operation = format!("std.data.json.{method}");
        assert!(seen.insert(operation.clone()));
        assert_eq!(invoke(&operation, &args).unwrap(), expected, "{method}");
        assert_eq!(
            crate::operation(&operation).unwrap().returns_resource,
            matches!(&expected, R::Resource(_)),
            "{method}"
        );
        assert_eq!(
            invoke_registered(&operation, &args).unwrap(),
            expected,
            "{method}"
        );
        // Corrupt every argument independently, not only the leading receiver.
        for index in 0..args.len() {
            let saved = std::mem::replace(&mut args[index], R::Value(V::Unit));
            let error = invoke(&operation, &args).unwrap_err();
            assert_eq!(invoke_registered(&operation, &args).unwrap_err(), error);
            assert_eq!(error.code(), "dispatch.type", "{method} argument {index}");
            assert!(error.message().contains(&format!("argument {index}")));
            args[index] = saved;
        }
    }
    assert_eq!(
        seen,
        RESOURCE_OPERATIONS
            .iter()
            .filter(|op| !op.mutates_receiver)
            .map(|op| op.operation.to_string())
            .collect()
    );
}

#[test]
fn all_mutations_preserve_source_snapshots_and_are_atomic_on_invalid_inputs() {
    let cases = [
        ("array_push", "[1]", vec![resource("2")], "[1,2]"),
        ("array_extend", "[1]", vec![resource("[2,3]")], "[1,2,3]"),
        (
            "array_set",
            "[1,2]",
            vec![R::Value(V::Int(1)), resource("3")],
            "[1,3]",
        ),
        (
            "object_put",
            "{\"a\":1}",
            vec![R::Value("b".into()), resource("2")],
            "{\"a\":1,\"b\":2}",
        ),
        (
            "object_remove",
            "{\"a\":1,\"b\":2}",
            vec![R::Value("a".into())],
            "{\"b\":2}",
        ),
    ];
    let mut seen = BTreeSet::new();
    for (method, initial, args, expected) in cases {
        let operation = format!("std.data.json.{method}");
        assert!(seen.insert(operation.clone()));
        let mut receiver = parsed(initial);
        let original = receiver.clone();
        let mut registered_args = vec![R::Resource(&original)];
        registered_args.extend(args.iter().map(|arg| match arg {
            R::Value(value) => R::Value(value.clone()),
            R::Resource(value) => R::Resource(value),
        }));
        assert_eq!(
            invoke_registered(&operation, &registered_args).unwrap(),
            resource(expected)
        );
        for index in 0..args.len() {
            let malformed = args
                .iter()
                .enumerate()
                .map(|(current, value)| {
                    if index == current {
                        R::Value(V::Unit)
                    } else {
                        match value {
                            R::Value(value) => R::Value(value.clone()),
                            R::Resource(value) => R::Resource(value.clone()),
                        }
                    }
                })
                .collect();
            let error = mutate(&operation, &mut receiver, malformed).unwrap_err();
            assert_eq!(error.code(), "dispatch.type");
            assert!(error.message().contains(&format!("argument {}", index + 1)));
            assert_eq!(receiver, original);
        }
        mutate(&operation, &mut receiver, args).unwrap();
        assert_eq!(receiver, parsed(expected), "{method}");
        assert_eq!(original, parsed(initial));
    }
    assert_eq!(
        seen,
        RESOURCE_OPERATIONS
            .iter()
            .filter(|op| op.mutates_receiver)
            .map(|op| op.operation.to_string())
            .collect()
    );
}

#[test]
fn arity_unknown_operations_and_execution_modes_fail_before_execution() {
    for contract in RESOURCE_OPERATIONS {
        for arity in 0..=contract.arity + 1 {
            if arity == contract.arity {
                continue;
            }
            let args = (0..arity).map(|_| R::Value(V::Unit)).collect::<Vec<_>>();
            assert_eq!(
                invoke(contract.operation, &args).unwrap_err().code(),
                "dispatch.arity"
            );
        }
        if contract.mutates_receiver {
            let args = (0..contract.arity)
                .map(|_| R::Value(V::Unit))
                .collect::<Vec<_>>();
            assert_eq!(
                invoke(contract.operation, &args).unwrap_err().code(),
                "dispatch.mutable_receiver_requires_direct_lowering"
            );
        }
    }
    let mut receiver = crate::null();
    assert_eq!(
        mutate("std.data.json.is_null", &mut receiver, vec![])
            .unwrap_err()
            .code(),
        "dispatch.immutable_operation"
    );
    assert_eq!(
        mutate("std.data.json.array_push", &mut receiver, vec![])
            .unwrap_err()
            .code(),
        "dispatch.arity"
    );
    for name in [
        "",
        "std.data.json.unknown",
        "std.data.json.parse.extra",
        "app.data.json.parse",
    ] {
        assert_eq!(
            invoke(name, &[]).unwrap_err().code(),
            "dispatch.unknown_operation"
        );
        assert_eq!(
            mutate(name, &mut receiver, vec![]).unwrap_err().code(),
            "dispatch.unknown_operation"
        );
    }
    assert_eq!(receiver, crate::null());
}

#[test]
fn malformed_nested_values_and_domain_errors_preserve_typed_diagnostics() {
    for rows in [
        V::Bool(false),
        V::List(vec![V::Bool(false)]),
        V::List(vec![V::List(vec![V::Int(1)])]),
    ] {
        let error = invoke(
            "std.data.json.string_object_rows",
            &[value(vec!["name"]), R::Value(rows)],
        )
        .unwrap_err();
        assert_eq!(error.code(), "dispatch.type");
        assert_eq!(
            error.message(),
            "Operation `std.data.json.string_object_rows` argument 1 must be `List[List[String]]`."
        );
    }
    for number in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        assert_eq!(
            invoke("std.data.json.float", &[value(number)])
                .unwrap_err()
                .code(),
            crate::float(number).unwrap_err().code()
        );
    }
    let source = "{\n \"a\": }";
    let expected = crate::parse(source).unwrap_err();
    let actual = invoke("std.data.json.parse", &[value(source)]).unwrap_err();
    assert_eq!(
        (actual.code(), actual.message(), actual.offset()),
        (expected.code(), expected.message(), expected.offset())
    );
    let mut receiver = parsed("[1]");
    let original = receiver.clone();
    assert!(mutate(
        "std.data.json.array_set",
        &mut receiver,
        vec![R::Value(V::Int(-1)), resource("2")]
    )
    .is_err());
    assert!(mutate(
        "std.data.json.array_extend",
        &mut receiver,
        vec![resource("{}")]
    )
    .is_err());
    assert_eq!(receiver, original);
}
