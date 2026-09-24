//! Random adapter execution and recursive resource ownership regressions.

use super::*;
use crate::terlan_native_boundary::dispatch::dispatch_with_resources_for_process as call;
use crate::terlan_native_boundary::resource::ResourceKind;

const OWNER: u64 = 7;

fn seed(store: &mut ResourceStore) -> Value {
    call(store, OWNER, "std.random.random.seed", &[Value::Int(42)]).unwrap()
}

fn draw(
    store: &mut ResourceStore,
    generator: &Value,
    method: &str,
    extra: &[Value],
) -> (Value, Value) {
    let mut args = vec![generator.clone()];
    args.extend_from_slice(extra);
    let value = call(store, OWNER, &format!("std.random.random.{method}"), &args).unwrap();
    let Value::Tuple(fields) = value else {
        panic!("expected tuple draw")
    };
    let [next, result]: [Value; 2] = fields.try_into().unwrap();
    let Value::Handle(handle) = next else {
        panic!("expected generator handle")
    };
    assert_eq!(store.kind(handle).unwrap(), ResourceKind::RandomGenerator);
    store.validate_owner(handle, OWNER).unwrap();
    (Value::Handle(handle), result)
}

#[test]
fn random_draws_replay_without_mutating_source_generators() {
    let mut store = ResourceStore::new();
    let initial = seed(&mut store);
    let (next, first) = draw(&mut store, &initial, "int", &[]);
    assert_eq!(draw(&mut store, &initial, "int", &[]).1, first);
    let independent = seed(&mut store);
    let (other_next, _) = draw(&mut store, &independent, "int", &[]);
    assert_eq!(
        draw(&mut store, &next, "int", &[]).1,
        draw(&mut store, &other_next, "int", &[]).1
    );
    let Value::Handle(handle) = initial else {
        unreachable!()
    };
    assert_eq!(
        store.random_generator(handle).unwrap(),
        &random::Generator::from_seed(42)
    );
    let Value::Int(value) = draw(
        &mut store,
        &initial,
        "bounded_int",
        &[Value::Int(-10), Value::Int(10)],
    )
    .1
    else {
        panic!("integer")
    };
    assert!((-10..10).contains(&value));
    let Value::Float(value) = draw(&mut store, &initial, "float", &[]).1 else {
        panic!("float")
    };
    assert!((0.0..1.0).contains(&value));
    assert!(matches!(
        draw(&mut store, &initial, "bool", &[]).1,
        Value::Bool(_)
    ));
    assert!(matches!(
        call(&mut store, OWNER, "std.random.random.entropy", &[]).unwrap(),
        Value::Handle(_)
    ));
}

#[test]
fn random_collections_preserve_generic_values_and_persistent_state() {
    let mut store = ResourceStore::new();
    let generator = seed(&mut store);
    let values = vec![
        Value::Tuple(vec![Value::Int(1), Value::Text("one".into())]),
        Value::Record {
            name: "Entry".into(),
            fields: vec![("value".into(), Value::Bool(true))],
        },
    ];
    let input = Value::List(values.clone());
    assert!(values.contains(
        &draw(
            &mut store,
            &generator,
            "choice",
            std::slice::from_ref(&input)
        )
        .1
    ));
    let Value::List(shuffled) = draw(
        &mut store,
        &generator,
        "shuffle",
        std::slice::from_ref(&input),
    )
    .1
    else {
        panic!("list")
    };
    assert_eq!(shuffled.len(), values.len());
    for value in &values {
        assert!(shuffled.contains(value));
    }
    let Value::List(sampled) = draw(&mut store, &generator, "sample", &[input, Value::Int(1)]).1
    else {
        panic!("list")
    };
    assert_eq!(sampled.len(), 1);
    assert!(values.contains(&sampled[0]));
    assert_eq!(
        draw(&mut store, &generator, "shuffle", &[Value::List(vec![])]).1,
        Value::List(vec![])
    );
}

#[test]
fn random_errors_retain_stable_codes_without_advancing_inputs() {
    let mut store = ResourceStore::new();
    let generator = seed(&mut store);
    for (method, args, code) in [
        ("seed", vec![Value::Int(-1)], "random.invalid_seed"),
        (
            "bounded_int",
            vec![generator.clone(), Value::Int(5), Value::Int(5)],
            "random.invalid_bounds",
        ),
        (
            "choice",
            vec![generator.clone(), Value::List(vec![])],
            "random.empty_choice",
        ),
        (
            "sample",
            vec![generator.clone(), Value::List(vec![]), Value::Int(-1)],
            "random.invalid_sample_size",
        ),
        (
            "sample",
            vec![generator.clone(), Value::List(vec![]), Value::Int(1)],
            "random.sample_too_large",
        ),
    ] {
        let error = call(
            &mut store,
            OWNER,
            &format!("std.random.random.{method}"),
            &args,
        )
        .unwrap_err();
        assert_eq!(error.code(), code);
        assert_eq!(error.offset(), 0);
    }
    let independent = seed(&mut store);
    assert_eq!(
        draw(&mut store, &generator, "int", &[]).1,
        draw(&mut store, &independent, "int", &[]).1
    );
}

#[test]
fn random_dispatch_rejects_foreign_nested_stale_and_wrong_kind_handles() {
    let mut store = ResourceStore::new();
    let generator = seed(&mut store);
    let Value::Handle(handle) = generator else {
        unreachable!()
    };
    assert_eq!(
        call(
            &mut store,
            OWNER + 1,
            "std.random.random.int",
            &[generator.clone()]
        )
        .unwrap_err()
        .code(),
        "resource.owner"
    );
    let foreign = store
        .insert_for_owner(
            OWNER + 1,
            ResourceValue::RandomGenerator(random::Generator::from_seed(3)),
        )
        .unwrap();
    for nested in [
        Value::Tuple(vec![Value::Handle(foreign)]),
        Value::Record {
            name: "Box".into(),
            fields: vec![("value".into(), Value::Handle(foreign))],
        },
    ] {
        assert_eq!(
            call(
                &mut store,
                OWNER,
                "std.random.random.choice",
                &[generator.clone(), Value::List(vec![nested])]
            )
            .unwrap_err()
            .code(),
            "resource.owner"
        );
    }
    let wrong = store
        .insert_for_owner(
            OWNER,
            ResourceValue::Json(crate::terlan_native::json::null()),
        )
        .unwrap();
    assert_eq!(
        call(
            &mut store,
            OWNER,
            "std.random.random.int",
            &[Value::Handle(wrong)]
        )
        .unwrap_err()
        .code(),
        "resource.kind"
    );
    store.dispose_for_owner(handle, OWNER).unwrap();
    assert_eq!(
        call(&mut store, OWNER, "std.random.random.int", &[generator])
            .unwrap_err()
            .code(),
        "resource.stale_handle"
    );
}
