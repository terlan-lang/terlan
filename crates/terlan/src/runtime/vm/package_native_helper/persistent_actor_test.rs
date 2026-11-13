use super::*;
use crate::runtime::native_image::TvmBoundaryType;

fn request(operation: &str, arguments: Vec<ReplValue>) -> PureNativeCapabilityRequest {
    PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: format!("std.vm.persistent_actor.{operation}"),
        arguments: Vec::new(),
        package_arguments: Some(arguments),
        result_type: TvmBoundaryType::Int,
    }
}

fn string(value: &str) -> ReplValue {
    ReplValue::String(value.into())
}

fn call(
    runtime: &mut VmPersistentActorRuntime,
    operation: &str,
    arguments: Vec<ReplValue>,
) -> ReplValue {
    runtime.call(11, &request(operation, arguments)).unwrap()
}

#[test]
fn constructors_are_structurally_interned_and_schema_compatibility_is_typed() {
    let mut runtime = VmPersistentActorRuntime::default();
    let actor = call(
        &mut runtime,
        "actor_id",
        vec![string("Order"), string("one")],
    );
    let actor_again = call(
        &mut runtime,
        "actor_id",
        vec![string("Order"), string("one")],
    );
    assert_eq!(actor, actor_again);
    let first = call(
        &mut runtime,
        "schema_id",
        vec![string("Order"), ReplValue::Int(1)],
    );
    let same = call(
        &mut runtime,
        "schema_id",
        vec![string("Order"), ReplValue::Int(1)],
    );
    let second = call(
        &mut runtime,
        "schema_id",
        vec![string("Order"), ReplValue::Int(2)],
    );
    assert_eq!(
        call(&mut runtime, "compatible_schema", vec![first.clone(), same]),
        ReplValue::Bool(true)
    );
    assert_eq!(
        call(&mut runtime, "compatible_schema", vec![first, second]),
        ReplValue::Bool(false)
    );
}

#[test]
fn constructor_catalog_accepts_every_declared_shape() {
    let mut runtime = VmPersistentActorRuntime::default();
    for (operation, kind_arity) in [
        ("actor_id", 2),
        ("schema_id", 2),
        ("schema", 4),
        ("event_variant_schema", 5),
        ("durable_adapter_schema", 5),
        ("persistent_actor", 3),
        ("retention_policy", 3),
        ("family_retention_defaults", 3),
        ("audit_retention", 3),
        ("package_retention_policy", 3),
        ("model_sync_retention_continuity", 4),
        ("redaction_policy", 3),
        ("snapshot", 4),
        ("replay", 2),
        ("resource_checkpoint", 2),
        ("restore_resource", 2),
        ("mailbox_checkpoint", 2),
        ("restore_mailbox", 2),
        ("migration_rollback", 5),
        ("register_package_migration", 5),
        ("timer_checkpoint", 3),
        ("restore_timer", 2),
        ("package_store", 3),
    ] {
        let arguments = (0..kind_arity)
            .map(|index| {
                if integer_indexes(&format!("std.vm.persistent_actor.{operation}")).contains(&index)
                {
                    ReplValue::Int(1)
                } else {
                    string("value")
                }
            })
            .collect();
        assert!(runtime.call(11, &request(operation, arguments)).is_ok());
    }
}

#[test]
fn invalid_calls_owner_mismatches_and_cleanup_are_rejected() {
    let mut runtime = VmPersistentActorRuntime::default();
    assert!(runtime
        .call(11, &request("actor_id", vec![string(""), string("one")]))
        .is_err());
    assert!(runtime
        .call(
            11,
            &request("schema_id", vec![string("Schema"), ReplValue::Int(-1)])
        )
        .is_err());
    assert!(runtime.call(11, &request("unknown", vec![])).is_err());
    assert!(runtime.call(11, &request("schema_id", vec![])).is_err());
    let schema = call(
        &mut runtime,
        "schema_id",
        vec![string("Schema"), ReplValue::Int(1)],
    );
    assert!(runtime
        .call(
            12,
            &request("compatible_schema", vec![schema.clone(), schema.clone()])
        )
        .is_err());
    runtime.close_owner(11);
    assert!(runtime
        .call(
            11,
            &request("compatible_schema", vec![schema.clone(), schema])
        )
        .is_err());
}
