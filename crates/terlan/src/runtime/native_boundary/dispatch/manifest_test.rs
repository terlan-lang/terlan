use super::*;
use crate::terlan_native::{json, postgres};
use crate::terlan_native_boundary::resource::{ResourceStore, ResourceValue};

fn bridge_error(
    store: &mut ResourceStore,
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> DispatchError {
    super::super::dispatch_with_resources(store, operation, args)
        .err()
        .unwrap_or_else(|| DispatchError::new("missing", "expected dispatch error", 0))
}

fn process_bridge_error(
    store: &mut ResourceStore,
    granted_capabilities: &[&str],
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> DispatchError {
    super::super::dispatch_with_resources_for_process_with_capabilities(
        store,
        7,
        granted_capabilities,
        operation,
        args,
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "expected dispatch error", 0))
}

fn process_policy_bridge_error(
    store: &mut ResourceStore,
    admitted_worker_classes: &[NativeBoundaryWorkerClass],
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> DispatchError {
    super::super::dispatch_with_resources_for_process_with_policy(
        store,
        7,
        &["postgres"],
        admitted_worker_classes,
        operation,
        args,
    )
    .err()
    .unwrap_or_else(|| DispatchError::new("missing", "expected dispatch error", 0))
}

#[test]
fn postgres_dispatch_rejects_missing_manifest_before_global_dispatch() {
    let error = bridge_error(
        &mut ResourceStore::new(),
        "std.db.postgres.unregistered",
        &[],
    );

    assert_eq!(error.code(), "native_boundary.missing_manifest");
}

#[test]
fn postgres_dispatch_rejects_missing_capability_before_argument_shape() {
    let malformed_args = [NativeBoundaryBridgeValue::Text(String::from(
        "not a config",
    ))];
    for granted_capabilities in [&[][..], &["filesystem.path"][..]] {
        let error = process_bridge_error(
            &mut ResourceStore::new(),
            granted_capabilities,
            "std.db.postgres.connect",
            &malformed_args,
        );

        assert_eq!(error.code(), "native_boundary.capability_denied");
        assert_eq!(
            error.message(),
            "NativeBoundary operation `std.db.postgres.connect` requires capability `postgres`."
        );
    }
}

#[test]
fn postgres_dispatch_granted_capability_still_requires_scheduler_admission() {
    let error = process_bridge_error(
        &mut ResourceStore::new(),
        &["postgres"],
        "std.db.postgres.connect",
        &[NativeBoundaryBridgeValue::Text(String::from(
            "not a config",
        ))],
    );

    assert_eq!(error.code(), "native_boundary.scheduler_denied");
    assert_eq!(
        error.message(),
        "NativeBoundary operation `std.db.postgres.connect` requires `resource-owning` scheduler admission."
    );
}

#[test]
fn postgres_dispatch_rejects_wrong_scheduler_class_before_argument_shape() {
    let error = process_policy_bridge_error(
        &mut ResourceStore::new(),
        &[NativeBoundaryWorkerClass::Fast],
        "std.db.postgres.connect",
        &[NativeBoundaryBridgeValue::Text(String::from(
            "not a config",
        ))],
    );

    assert_eq!(error.code(), "native_boundary.scheduler_denied");
}

#[test]
fn postgres_dispatch_admitted_scheduler_class_reaches_shape_validation() {
    let error = process_policy_bridge_error(
        &mut ResourceStore::new(),
        &[NativeBoundaryWorkerClass::ResourceOwning],
        "std.db.postgres.connect",
        &[NativeBoundaryBridgeValue::Text(String::from(
            "not a config",
        ))],
    );

    assert_eq!(error.code(), "native_boundary.argument_shape");
}

#[test]
fn postgres_dispatch_rejects_manifest_arity_before_argument_decoding() {
    let error = bridge_error(
        &mut ResourceStore::new(),
        "std.db.postgres.query",
        &[NativeBoundaryBridgeValue::Text(String::from("not a pool"))],
    );

    assert_eq!(error.code(), "native_boundary.arity");
}

#[test]
fn postgres_dispatch_rejects_argument_shape_before_resource_lookup() {
    let error = bridge_error(
        &mut ResourceStore::new(),
        "std.db.postgres.query",
        &[
            NativeBoundaryBridgeValue::Text(String::from("not a pool handle")),
            NativeBoundaryBridgeValue::Text(String::from("SELECT 1")),
            NativeBoundaryBridgeValue::List(Vec::new()),
        ],
    );

    assert_eq!(error.code(), "native_boundary.argument_shape");
    assert!(error.message().contains("argument 0"));
}

#[test]
fn postgres_manifest_query_target_union_accepts_resource_handles() {
    let mut store = ResourceStore::new();
    let pool = store
        .insert(ResourceValue::PostgresPool(
            postgres::test_support::disconnected_pool("postgres://localhost/terlan"),
        ))
        .expect("resource fixture must allocate");
    let handle = NativeBoundaryBridgeValue::Handle(pool);

    assert!(bridge_value_matches_type(&handle, "Pool | Connection"));
    assert!(bridge_value_matches_type(&handle, "Pool|Connection"));
    assert!(bridge_value_matches_type(
        &handle,
        "std.db.Postgres.Pool | std.db.Postgres.Connection"
    ));
}

#[test]
fn postgres_manifest_query_target_union_rejects_non_handles_and_empty_members() {
    let text = NativeBoundaryBridgeValue::Text(String::from("not a query target"));
    let mut store = ResourceStore::new();
    let pool = store
        .insert(ResourceValue::PostgresPool(
            postgres::test_support::disconnected_pool("postgres://localhost/terlan"),
        ))
        .expect("resource fixture must allocate");
    let handle = NativeBoundaryBridgeValue::Handle(pool);

    assert!(!bridge_value_matches_type(&text, "Pool | Connection"));
    assert!(!bridge_value_matches_type(&handle, "Pool | "));
    assert!(!bridge_value_matches_type(&handle, " | Connection"));
    assert!(!bridge_value_matches_type(
        &handle,
        "List[Pool | Connection]"
    ));
    assert!(!bridge_value_matches_type(&handle, "Pool] | Connection"));
}

#[test]
fn postgres_dispatch_rejects_non_handle_json_parameter_shape() {
    let mut store = ResourceStore::new();
    let Ok(pool) = store.insert(ResourceValue::PostgresPool(
        postgres::test_support::disconnected_pool("postgres://localhost/terlan"),
    )) else {
        assert!(false, "resource fixture must allocate");
        return;
    };
    let error = bridge_error(
        &mut store,
        "std.db.postgres.query",
        &[
            NativeBoundaryBridgeValue::Handle(pool),
            NativeBoundaryBridgeValue::Text(String::from("SELECT $1")),
            NativeBoundaryBridgeValue::List(vec![NativeBoundaryBridgeValue::Int(1)]),
        ],
    );

    assert_eq!(error.code(), "native_boundary.argument_shape");
    assert!(error.message().contains("argument 2"));
}

#[test]
fn postgres_dispatch_checks_resource_liveness_after_manifest_shape() {
    let mut store = ResourceStore::new();
    let Ok(pool) = store.insert(ResourceValue::PostgresPool(
        postgres::test_support::disconnected_pool("postgres://localhost/terlan"),
    )) else {
        assert!(false, "resource fixture must allocate");
        return;
    };
    let Ok(json_handle) = store.insert(ResourceValue::Json(json::int(1))) else {
        assert!(false, "resource fixture must allocate");
        return;
    };
    assert_eq!(store.dispose(json_handle), Ok(()));

    let error = bridge_error(
        &mut store,
        "std.db.postgres.query",
        &[
            NativeBoundaryBridgeValue::Handle(pool),
            NativeBoundaryBridgeValue::Text(String::from("SELECT $1")),
            NativeBoundaryBridgeValue::List(vec![NativeBoundaryBridgeValue::Handle(json_handle)]),
        ],
    );

    assert_eq!(error.code(), "resource.stale_handle");
}
