use super::*;
use crate::session_test_support::{failure, Probe};
use terlan_runtime_abi::NativeValue;

fn binding<S: SessionStorage + ?Sized>(operation: &str) -> Option<NativeContextBinding<S>> {
    bindings()
        .into_iter()
        .find(|binding| binding.operation == operation)
}

fn operations() -> [(&'static str, usize, NativeValue); 8] {
    [
        (
            "lookup",
            1,
            Some(" \u{2003}identity=unchanged;\u{2003} ").into(),
        ),
        ("try_create", 2, true.into()),
        ("get", 2, Some("stored").into()),
        ("set", 3, NativeValue::Unit),
        ("delete", 2, NativeValue::Unit),
        ("try_rotate", 3, true.into()),
        ("expire", 1, NativeValue::Unit),
        ("is_live", 1, true.into()),
    ]
}

#[test]
fn every_operation_preserves_exact_arguments_results_and_backend_errors() {
    for (name, arity, expected) in operations() {
        let operation = format!("std.http.session.{name}");
        let binding = binding::<dyn SessionStorage>(&operation).unwrap();
        assert_eq!(binding.operation, operation);
        let timed = matches!(name, "try_create" | "try_rotate");
        let mut texts = [
            " \u{2003}identity=unchanged;\u{2003} ",
            " key\0 ",
            "\nvalue\0",
        ];
        if timed {
            texts[arity - 1] = "13";
        }
        let mut args = texts[..arity]
            .iter()
            .map(|value| NativeValue::from(*value))
            .collect::<Vec<_>>();
        if timed {
            args[arity - 1] = 13_i64.into();
        }
        let mut storage = Probe {
            value: Some("stored".into()),
            live: true,
            ..Probe::default()
        };
        assert_eq!(binding.call(&mut storage, &args).unwrap(), expected);
        let expected_call = std::iter::once(name)
            .chain(texts[..arity].iter().copied())
            .map(String::from)
            .collect::<Vec<_>>();
        assert_eq!(storage.calls, std::slice::from_ref(&expected_call));
        storage.fail = true;
        assert_eq!(binding.call(&mut storage, &args), Err(failure()));
        assert_eq!(storage.calls, [expected_call.clone(), expected_call]);
    }
}

#[test]
fn every_argument_is_checked_before_backend_access() {
    let invalid_values = [
        NativeValue::Unit,
        NativeValue::Bool(false),
        NativeValue::Int(0),
        NativeValue::String("13".into()),
        NativeValue::Float(0.0),
        NativeValue::Atom("identity".into()),
        NativeValue::Bytes(b"identity".to_vec()),
        NativeValue::List(vec![]),
        NativeValue::Tuple(vec![]),
        NativeValue::Map(vec![]),
        None::<String>.into(),
    ];
    for (name, arity, _) in operations() {
        let binding = binding::<dyn SessionStorage>(&format!("std.http.session.{name}")).unwrap();
        let timed = matches!(name, "try_create" | "try_rotate");
        let mut valid = vec![NativeValue::from(""); arity];
        if timed {
            valid[arity - 1] = 13_i64.into();
        }
        let mut storage = Probe::default();
        for count in 0..=4 {
            if count != arity {
                assert_eq!(
                    binding
                        .call(&mut storage, &vec!["".into(); count])
                        .unwrap_err()
                        .code(),
                    "native_package.arguments"
                );
            }
        }
        for index in 0..arity {
            for value in &invalid_values {
                if if timed && index == arity - 1 {
                    matches!(value, NativeValue::Int(_))
                } else {
                    matches!(value, NativeValue::String(_))
                } {
                    continue;
                }
                let mut args = valid.clone();
                args[index] = value.clone();
                assert_eq!(
                    binding.call(&mut storage, &args).unwrap_err().code(),
                    "dispatch.type"
                );
            }
        }
        assert!(storage.calls.is_empty());
    }
}

#[test]
fn lifetime_is_positive_and_validated_before_storage_access() {
    for (operation, mut args) in [
        (
            "std.http.session.try_create",
            vec!["id".into(), NativeValue::Unit],
        ),
        (
            "std.http.session.try_rotate",
            vec!["id".into(), "next".into(), NativeValue::Unit],
        ),
    ] {
        let binding = binding::<dyn SessionStorage>(operation).unwrap();
        let mut storage = Probe::default();
        for seconds in [i64::MIN, -1, 0] {
            *args.last_mut().unwrap() = seconds.into();
            let error = binding.call(&mut storage, &args).unwrap_err();
            assert!(error.to_string().contains("TTL must be greater than 0"));
            assert!(storage.calls.is_empty());
        }
        *args.last_mut().unwrap() = i64::MAX.into();
        binding.call(&mut storage, &args).unwrap();
        assert_eq!(storage.calls[0].last().unwrap(), &i64::MAX.to_string());
    }
}

#[test]
fn missing_and_empty_values_remain_distinct_and_contexts_are_isolated() {
    let get = binding::<dyn SessionStorage>("std.http.session.get").unwrap();
    let live = binding::<dyn SessionStorage>("std.http.session.is_live").unwrap();
    let mut missing = Probe::default();
    let mut present = Probe {
        value: Some(String::new()),
        live: true,
        ..Probe::default()
    };
    assert_eq!(
        get.call(&mut missing, &["same".into(), "".into()]),
        Ok(None::<String>.into())
    );
    assert_eq!(
        get.call(&mut present, &["same".into(), "".into()]),
        Ok(Some("").into())
    );
    assert_eq!(live.call(&mut missing, &["same".into()]), Ok(false.into()));
    assert_eq!(live.call(&mut present, &["same".into()]), Ok(true.into()));
    assert_eq!(missing.calls.len(), 2);
    assert_eq!(present.calls.len(), 2);
}

#[test]
fn unknown_and_legacy_names_are_not_admitted() {
    for name in [
        "",
        "get",
        "std.http.session.GET",
        "std.http.session.get.extra",
        "std.http.session.cookie",
        "std.http.session.candidate",
        "std.http.session.create",
        "std.http.session.rotate",
        "std.http.session.current",
        "std.http.session.current ",
        "std.http.sessions.get",
    ] {
        assert!(binding::<dyn SessionStorage>(name).is_none());
    }
}
