//! State descriptors preserve actor authority, mutation, and snapshot isolation.

use super::*;
use crate::runtime::native_image::TvmBoundaryType;

fn request(operation: &str, args: Vec<ReplValue>) -> PureNativeCapabilityRequest {
    PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: format!("std.vm.distributed_state.{operation}"),
        arguments: Vec::new(),
        package_arguments: Some(args),
        result_type: TvmBoundaryType::Int,
    }
}

fn string(value: &str) -> ReplValue {
    ReplValue::String(value.into())
}

fn call(
    runtime: &mut VmDistributedStateRuntime,
    operation: &str,
    args: Vec<ReplValue>,
) -> ReplValue {
    runtime.call(17, &request(operation, args)).unwrap()
}

fn replace(value: &ReplValue, field: &str, replacement: ReplValue) -> ReplValue {
    let mut value = value.clone();
    let ReplValue::Record { fields, .. } = &mut value else {
        panic!("opaque descriptor")
    };
    fields.iter_mut().find(|(name, _)| name == field).unwrap().1 = replacement;
    value
}

#[test]
fn handles_reject_forgery_wrong_kinds_and_replays_after_cleanup() {
    let mut runtime = VmDistributedStateRuntime::default();
    let store = call(&mut runtime, "store", vec![]);
    let second = runtime.call(18, &request("store", vec![])).unwrap();
    for (owner, forged) in [
        (18, store.clone()),
        (18, replace(&store, "$native_owner", string("18"))),
        (17, replace(&store, "$native_generation", ReplValue::Int(2))),
        (
            17,
            replace(&store, "$native_type", string("std.vm.Cluster.Store")),
        ),
    ] {
        assert!(runtime
            .call(owner, &request("export_snapshot", vec![forged]))
            .is_err());
    }
    let scope = call(&mut runtime, "scope", vec![string("ns"), string("key")]);
    let ReplValue::Record { fields, .. } = &scope else {
        panic!("scope")
    };
    let id = fields
        .iter()
        .find(|(key, _)| key == "$native_id")
        .unwrap()
        .1
        .clone();
    let forged = replace(&store, "$native_id", id);
    assert!(runtime
        .call(17, &request("export_snapshot", vec![forged]))
        .is_err());
    runtime.close_owner(17);
    assert!(runtime
        .call(17, &request("export_snapshot", vec![store]))
        .is_err());
    assert!(runtime
        .call(18, &request("export_snapshot", vec![second]))
        .is_ok());
}

#[test]
fn snapshots_entries_and_conflicts_are_immutable_views_of_a_mutable_store() {
    let mut runtime = VmDistributedStateRuntime::default();
    let store = call(&mut runtime, "store", vec![]);
    let scope = call(&mut runtime, "scope", vec![string("ns"), string("key")]);
    let policy = call(&mut runtime, "policy", vec![string("last_writer_wins")]);
    let first = call(
        &mut runtime,
        "version",
        vec![ReplValue::Int(1), string("node")],
    );
    let second = call(
        &mut runtime,
        "version",
        vec![ReplValue::Int(2), string("node")],
    );
    let write = |store: ReplValue, version: ReplValue, value: ReplValue| {
        vec![
            store,
            scope.clone(),
            string("node"),
            value,
            version,
            policy.clone(),
        ]
    };
    let payload = ReplValue::Tuple(vec![ReplValue::Int(42), string("original")]);
    let applied = call(
        &mut runtime,
        "write",
        write(store.clone(), first.clone(), payload.clone()),
    );
    assert_eq!(call(&mut runtime, "kind", vec![applied]), string("applied"));
    let snapshot = call(&mut runtime, "export_snapshot", vec![store.clone()]);
    let entry = call(&mut runtime, "get", vec![store.clone(), scope.clone()]);
    call(
        &mut runtime,
        "write",
        write(store.clone(), second, string("changed")),
    );
    assert_eq!(
        call(&mut runtime, "entry_sequence", vec![entry]),
        ReplValue::Int(1)
    );
    let restored = call(&mut runtime, "restore", vec![snapshot]);
    let replay = call(
        &mut runtime,
        "write",
        write(restored.clone(), first.clone(), payload),
    );
    assert_eq!(call(&mut runtime, "kind", vec![replay]), string("replayed"));
    let stale = call(&mut runtime, "write", write(store, first, string("stale")));
    let conflict = call(&mut runtime, "conflict", vec![stale]);
    assert_eq!(
        call(
            &mut runtime,
            "conflict_local_sequence",
            vec![conflict.clone()]
        ),
        ReplValue::Int(2)
    );
    assert_eq!(
        call(&mut runtime, "conflict_incoming_sequence", vec![conflict]),
        ReplValue::Int(1)
    );
    let Resource::Store(restored) = runtime.resource(17, &restored, "Store").unwrap() else {
        panic!("restored store")
    };
    assert_eq!(
        restored.export_snapshot()[0].value,
        ReplValue::Tuple(vec![ReplValue::Int(42), string("original")])
    );
}

#[test]
fn invalid_inputs_fail_without_mutation_and_operations_are_closed() {
    let mut runtime = VmDistributedStateRuntime::default();
    for (operation, args) in [
        ("scope", vec![string(""), string("key")]),
        ("version", vec![ReplValue::Int(0), string("node")]),
        ("version", vec![ReplValue::Int(-1), string("node")]),
        ("version", vec![ReplValue::Int(1), string("")]),
        ("policy", vec![string("typo")]),
        ("store", vec![ReplValue::Int(0)]),
        ("unknown", vec![]),
    ] {
        assert!(runtime.call(17, &request(operation, args)).is_err());
    }
    let store = call(&mut runtime, "store", vec![]);
    let scope = call(&mut runtime, "scope", vec![string("ns"), string("key")]);
    let version = call(
        &mut runtime,
        "version",
        vec![ReplValue::Int(1), string("node")],
    );
    let policy = call(
        &mut runtime,
        "policy",
        vec![string("explicit_user_resolution")],
    );
    assert!(runtime
        .call(
            17,
            &request(
                "write",
                vec![
                    store.clone(),
                    scope.clone(),
                    string(""),
                    string("value"),
                    version,
                    policy
                ]
            )
        )
        .is_err());
    assert!(runtime
        .call(17, &request("get", vec![store.clone(), scope]))
        .is_err());
    let snapshot = call(&mut runtime, "export_snapshot", vec![store]);
    let Resource::Snapshot(entries) = runtime.resource(17, &snapshot, "Snapshot").unwrap() else {
        panic!("snapshot")
    };
    assert!(entries.is_empty());
}
