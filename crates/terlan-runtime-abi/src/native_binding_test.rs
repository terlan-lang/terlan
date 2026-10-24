use crate::{BoundaryError, ErrorDomain, NativeBinding, NativeValue};
use std::sync::atomic::{AtomicUsize, Ordering};

static CALLS: AtomicUsize = AtomicUsize::new(0);

const CHECKED: NativeBinding = native_binding!(
    "fixture.checked",
    |text: &str, count: i64, flag: bool, optional: Option<&str>, items: Vec<i64>| {
        CALLS.fetch_add(1, Ordering::SeqCst);
        Ok((text, count, flag, optional, items))
    }
);

#[test]
fn declaration_checks_all_inputs_before_calling_package_code() {
    let args = vec![
        "value".into(),
        7_i64.into(),
        true.into(),
        Some("label").into(),
        vec![1_i64, 2].into(),
    ];
    assert_eq!(CHECKED.arity, args.len());
    CALLS.store(0, Ordering::SeqCst);
    for count in 0..args.len() {
        let error = (CHECKED.invoke)(&args[..count]).unwrap_err();
        assert_eq!(error.code(), "native_package.arguments");
        assert!(error.to_string().contains("fixture.checked"));
    }
    let mut extra = args.clone();
    extra.push(NativeValue::Unit);
    assert_eq!(
        (CHECKED.invoke)(&extra).unwrap_err().code(),
        "native_package.arguments"
    );
    for index in 0..args.len() {
        let mut wrong = args.clone();
        wrong[index] = NativeValue::Unit;
        assert_eq!(
            (CHECKED.invoke)(&wrong).unwrap_err().code(),
            "dispatch.type"
        );
    }
    for (index, malformed) in [
        (
            3,
            NativeValue::Record {
                name: "Some".into(),
                fields: vec![],
            },
        ),
        (
            3,
            NativeValue::Record {
                name: "None".into(),
                fields: vec![("value".into(), "x".into())],
            },
        ),
        (
            4,
            vec![NativeValue::Int(1), NativeValue::Bool(false)].into(),
        ),
    ] {
        let mut wrong = args.clone();
        wrong[index] = malformed;
        assert_eq!(
            (CHECKED.invoke)(&wrong).unwrap_err().code(),
            "dispatch.type"
        );
    }
    assert_eq!(CALLS.load(Ordering::SeqCst), 0);
    let result = (CHECKED.invoke)(&args).unwrap();
    drop(args);
    assert_eq!(
        result,
        ("value", 7_i64, true, Some("label"), vec![1_i64, 2]).into()
    );
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
}

#[test]
fn empty_signature_unit_and_nested_value_results_are_owned() {
    const UNIT: NativeBinding = native_binding!("fixture.unit", || { Ok(()) });
    assert_eq!(UNIT.arity, 0);
    assert_eq!((UNIT.invoke)(&[]).unwrap(), NativeValue::Unit);
    assert_eq!(
        (UNIT.invoke)(&[NativeValue::Unit]).unwrap_err().code(),
        "native_package.arguments"
    );
    const RESULT: NativeBinding = native_binding!("fixture.result", |text: Option<&str>| {
        Ok(text.ok_or("absent"))
    });
    for text in [None, Some(""), Some("owned")] {
        assert_eq!(
            (RESULT.invoke)(&[text.into()]).unwrap(),
            NativeValue::from(text.ok_or("absent"))
        );
    }
}

#[test]
fn package_failures_preserve_the_boundary_error() {
    const FAIL: NativeBinding = native_binding!("fixture.fail", |value: &str| {
        Err::<(), _>(BoundaryError::message(
            ErrorDomain::NativeBoundary,
            "fixture",
            value,
        ))
    });
    let expected = BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "fixture",
        "error[fixture.failed]: unchanged",
    );
    assert_eq!(
        (FAIL.invoke)(&["error[fixture.failed]: unchanged".into()])
            .unwrap_err()
            .to_string(),
        expected.to_string()
    );
}

#[test]
fn context_declarations_validate_late_arguments_before_mutation() {
    use crate::NativeContextBinding;
    const APPEND: NativeContextBinding<Vec<String>> = native_binding!(
        "fixture.append", |values: &mut Vec<String>; text: &str, enabled: bool| {
            if enabled {
                values.push(text.to_owned());
            }
            Ok(())
        }
    );
    let mut values = vec![];
    assert_eq!(APPEND.arity(), 2);
    for wrong in [
        vec![],
        vec!["hello".into()],
        vec!["hello".into(), NativeValue::Unit],
        vec![NativeValue::Bool(false), NativeValue::Bool(true)],
        vec!["hello".into(), true.into(), NativeValue::Unit],
    ] {
        assert!(APPEND.call(&mut values, &wrong).is_err());
        assert!(values.is_empty());
    }
    assert_eq!(
        APPEND
            .call(&mut values, &["hello".into(), false.into()])
            .unwrap(),
        NativeValue::Unit
    );
    assert!(values.is_empty());
    assert_eq!(
        APPEND
            .call(&mut values, &["hello".into(), true.into()])
            .unwrap(),
        NativeValue::Unit
    );
    assert_eq!(values, ["hello"]);
}

#[test]
fn context_declarations_accept_unsized_hosts_and_zero_value_arguments() {
    use crate::NativeContextBinding;
    trait Store {
        fn clear(&mut self);
    }
    impl Store for String {
        fn clear(&mut self) {
            String::clear(self);
        }
    }
    fn binding<S: Store + ?Sized>() -> NativeContextBinding<S> {
        native_binding!("fixture.clear", |store: &mut S; | {
            store.clear();
            Ok(())
        })
    }
    let clear = binding::<dyn Store>();
    let mut value = String::from("secret");
    assert_eq!(clear.arity(), 0);
    assert!(clear.call(&mut value, &[NativeValue::Unit]).is_err());
    assert_eq!(value, "secret");
    assert_eq!(clear.call(&mut value, &[]).unwrap(), NativeValue::Unit);
    assert!(value.is_empty());
}
