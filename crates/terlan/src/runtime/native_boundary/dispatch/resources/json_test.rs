use super::*;
use crate::terlan_native::json;
use NativeBoundaryBridgeValue as V;

fn allocate(store: &mut ResourceStore, owner: u64, source: &str) -> V {
    V::Handle(
        store
            .insert_for_owner(owner, ResourceValue::Json(json::parse(source).unwrap()))
            .unwrap(),
    )
}

fn render(store: &mut ResourceStore, owner: u64, value: &V) -> V {
    dispatch_with_resources_for_process(
        store,
        owner,
        "std.data.json.to_string",
        std::slice::from_ref(value),
    )
    .unwrap()
}

#[test]
fn package_mutation_self_arguments_use_snapshots_and_retain_handle_identity() {
    for (method, source, middle, expected) in [
        ("array_push", "[1]", vec![], "[1,[1]]"),
        ("array_extend", "[1]", vec![], "[1,1]"),
        ("array_set", "[1]", vec![V::Int(0)], "[[1]]"),
        (
            "object_put",
            "{\"a\":1}",
            vec![V::Text("self".into())],
            "{\"a\":1,\"self\":{\"a\":1}}",
        ),
    ] {
        let mut store = ResourceStore::new();
        let receiver = allocate(&mut store, 7, source);
        let mut args = vec![receiver.clone()];
        args.extend(middle);
        args.push(receiver.clone());
        assert_eq!(
            dispatch_with_resources_for_process(
                &mut store,
                7,
                &format!("std.data.json.{method}"),
                &args
            )
            .unwrap(),
            receiver
        );
        assert_eq!(render(&mut store, 7, &receiver), V::Text(expected.into()));
        assert_eq!(
            store.dispose_owner(7),
            1,
            "mutation must not allocate another handle"
        );
    }
}

#[test]
fn package_mutation_rejects_foreign_and_stale_inputs_before_changing_receiver() {
    let mut store = ResourceStore::new();
    let receiver = allocate(&mut store, 7, "[1]");
    let foreign = allocate(&mut store, 8, "2");
    let stale = allocate(&mut store, 7, "3");
    let V::Handle(stale_handle) = stale else {
        panic!("handle required");
    };
    store.dispose_for_owner(stale_handle, 7).unwrap();
    for (input, code) in [
        (foreign.clone(), "resource.owner"),
        (V::Handle(stale_handle), "resource.stale_handle"),
    ] {
        let error = dispatch_with_resources_for_process(
            &mut store,
            7,
            "std.data.json.array_push",
            &[receiver.clone(), input],
        )
        .unwrap_err();
        assert_eq!(error.code(), code);
        assert_eq!(render(&mut store, 7, &receiver), V::Text("[1]".into()));
    }
    assert_eq!(render(&mut store, 8, &foreign), V::Text("2".into()));
    let error = dispatch_with_resources_for_process(
        &mut store,
        8,
        "std.data.json.array_push",
        &[receiver.clone(), foreign],
    )
    .unwrap_err();
    assert_eq!(error.code(), "resource.owner");
    assert_eq!(render(&mut store, 7, &receiver), V::Text("[1]".into()));
}

#[test]
fn invalid_package_mutation_and_wrong_resource_kind_leave_registry_unchanged() {
    let mut store = ResourceStore::new();
    let receiver = allocate(&mut store, 7, "[1]");
    let wrong_kind = V::Handle(
        store
            .insert_for_owner(7, ResourceValue::NativeVector(vector::new()))
            .unwrap(),
    );
    let before = store.clone();
    let error = dispatch_with_resources_for_process(
        &mut store,
        7,
        "std.data.json.array_push",
        &[receiver.clone(), wrong_kind],
    )
    .unwrap_err();
    assert_eq!(error.code(), "resource.kind");
    assert_eq!(store, before);
    let error = dispatch_with_resources_for_process(
        &mut store,
        7,
        "std.data.json.array_set",
        &[receiver.clone(), V::Int(-1), receiver],
    )
    .unwrap_err();
    assert_eq!(error.code(), "json.index_out_of_bounds");
    assert_eq!(store, before);
}

#[test]
fn package_resource_view_checks_owner_generation_and_kind_without_outer_dispatch() {
    use terlan_runtime_abi::NativeResourceStore;
    let mut store = ResourceStore::new();
    let V::Handle(handle) = allocate(&mut store, 7, "[1]") else {
        panic!("handle required");
    };
    let wrong_kind = store
        .insert_for_owner(7, ResourceValue::NativeVector(vector::new()))
        .unwrap();
    let stale = crate::terlan_native_boundary::handle::NativeBoundaryHandle {
        generation: handle.generation + 1,
        ..handle
    };
    for (owner, handle, code) in [
        (8, handle, "resource.owner"),
        (7, stale, "resource.stale_handle"),
        (7, wrong_kind, "resource.kind"),
    ] {
        assert_eq!(
            NativeResourceStore::<json::Json>::borrow(&store, owner, handle)
                .unwrap_err()
                .code(),
            code
        );
        assert_eq!(
            NativeResourceStore::<json::Json>::borrow_mut(&mut store, owner, handle)
                .unwrap_err()
                .code(),
            code
        );
    }
    assert_eq!(
        render(&mut store, 7, &V::Handle(handle)),
        V::Text("[1]".into())
    );
}

#[test]
fn nested_handles_cannot_enter_the_value_only_part_of_a_resource_call() {
    let mut store = ResourceStore::new();
    let handle = allocate(&mut store, 7, "1");
    let before = store.clone();
    for value in [
        V::List(vec![handle.clone()]),
        V::Tuple(vec![handle.clone()]),
        V::Map(vec![(V::Text("key".into()), handle.clone())]),
        V::Record {
            name: "Wrapper".into(),
            fields: vec![("value".into(), handle)],
        },
    ] {
        assert_eq!(
            dispatch_with_resources_for_process(
                &mut store,
                7,
                "std.data.json.string_object_rows",
                &[V::List(vec![]), value]
            )
            .unwrap_err()
            .code(),
            "dispatch.type"
        );
        assert_eq!(store, before);
    }
    assert_eq!(store.dispose_owner(7), 1);
}
