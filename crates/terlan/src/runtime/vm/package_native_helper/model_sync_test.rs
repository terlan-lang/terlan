use super::*;
use crate::runtime::native_image::TvmBoundaryType;

fn request(operation: &str, arguments: Vec<ReplValue>) -> PureNativeCapabilityRequest {
    PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: format!("std.vm.model_sync.{operation}"),
        arguments: Vec::new(),
        package_arguments: Some(arguments),
        result_type: TvmBoundaryType::Int,
    }
}

fn string(value: &str) -> ReplValue {
    ReplValue::String(value.into())
}

fn call(runtime: &mut VmModelSyncRuntime, operation: &str, arguments: Vec<ReplValue>) -> ReplValue {
    runtime.call(7, &request(operation, arguments)).unwrap()
}

#[test]
fn versions_are_checked_advanced_and_compared() {
    let mut runtime = VmModelSyncRuntime::default();
    let first = call(&mut runtime, "initial_version", vec![string("node-a")]);
    let duplicate = call(
        &mut runtime,
        "version",
        vec![ReplValue::Int(1), string("node-a")],
    );
    assert_eq!(first, duplicate);
    let second = call(
        &mut runtime,
        "next_version",
        vec![first.clone(), string("node-a")],
    );
    assert_eq!(
        call(&mut runtime, "stale", vec![first.clone(), second.clone()]),
        ReplValue::Bool(true)
    );
    assert_eq!(
        call(&mut runtime, "can_apply", vec![first.clone(), duplicate]),
        ReplValue::Bool(true)
    );
    assert_eq!(
        call(&mut runtime, "can_apply", vec![first, second]),
        ReplValue::Bool(false)
    );
}

#[test]
fn every_plan_constructor_is_vm_owned() {
    let mut runtime = VmModelSyncRuntime::default();
    let key = call(&mut runtime, "key", vec![string("User"), string("one")]);
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
    let capability = call(&mut runtime, "capability", vec![string("put")]);
    let contract = call(
        &mut runtime,
        "adapter_contract",
        vec![
            string("memory"),
            string("vm"),
            ReplValue::List(vec![capability]),
        ],
    );
    for (operation, arguments) in [
        (
            "write",
            vec![key.clone(), first.clone(), second.clone(), string("value")],
        ),
        ("delete", vec![key.clone(), first.clone(), second.clone()]),
        ("conflict", vec![key.clone(), second.clone(), first.clone()]),
        (
            "persistent_actor_adapter",
            vec![key.clone(), contract.clone()],
        ),
        (
            "package_store_adapter",
            vec![string("pkg@1"), contract.clone()],
        ),
        (
            "syncable_model",
            vec![string("User"), key.clone(), contract.clone()],
        ),
    ] {
        assert!(runtime.call(7, &request(operation, arguments)).is_ok());
    }
}

#[test]
fn invalid_calls_and_foreign_handles_are_rejected() {
    let mut runtime = VmModelSyncRuntime::default();
    assert!(runtime
        .call(7, &request("key", vec![string(""), string("one")]))
        .is_err());
    assert!(runtime
        .call(
            7,
            &request("version", vec![ReplValue::Int(0), string("node")])
        )
        .is_err());
    assert!(runtime.call(7, &request("unknown", vec![])).is_err());
    assert!(runtime.call(7, &request("write", vec![])).is_err());
    let version = call(
        &mut runtime,
        "version",
        vec![ReplValue::Int(1), string("node")],
    );
    assert!(runtime
        .call(
            8,
            &request("next_version", vec![version.clone(), string("node")])
        )
        .is_err());
    runtime.close_owner(7);
    assert!(runtime
        .call(7, &request("next_version", vec![version, string("node")]))
        .is_err());
}
