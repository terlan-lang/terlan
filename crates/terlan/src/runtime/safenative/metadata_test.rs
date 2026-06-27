use super::*;

/// Verifies the Postgres worker metadata names the selected runtime bridge.
///
/// Inputs:
/// - Static Postgres worker spec.
///
/// Output:
/// - Test passes when the spec locks the BEAM-supervised Tokio worker shape.
///
/// Transformation:
/// - Reads immutable metadata and compares it with the 0.0.5 SafeNative
///   bridge contract.
#[test]
fn postgres_worker_spec_names_runtime_and_transport_contract() {
    let spec = postgres_worker_spec();

    assert_eq!(spec.adapter, "std.db.Postgres");
    assert_eq!(spec.runtime, SafeNativeWorkerRuntime::Tokio);
    assert_eq!(spec.transport, SafeNativeWorkerTransport::BeamProcess);
    assert_eq!(
        spec.resource_policy,
        SafeNativeResourcePolicy::SupervisedExplicitDispose
    );
    assert_eq!(spec.default_credit_limit, 64);
}

/// Verifies Postgres worker metadata owns the std.db operation surface.
///
/// Inputs:
/// - Static Postgres worker spec and representative operation ids.
///
/// Output:
/// - Test passes when core query, transaction, and row access operations are
///   accepted while unrelated operations are rejected.
///
/// Transformation:
/// - Exercises exact operation-id matching without invoking the runtime
///   dispatcher or a live database adapter.
#[test]
fn postgres_worker_spec_accepts_only_postgres_operations() {
    let spec = postgres_worker_spec();

    assert!(spec.accepts_operation("std.db.postgres.connect"));
    assert!(spec.accepts_operation("std.db.postgres.query"));
    assert!(spec.accepts_operation("std.db.postgres.query_one"));
    assert!(spec.accepts_operation("std.db.postgres.execute"));
    assert!(spec.accepts_operation("std.db.postgres.transaction"));
    assert!(spec.accepts_operation("std.db.postgres.string"));
    assert!(spec.accepts_operation("std.db.postgres.int"));
    assert!(spec.accepts_operation("std.db.postgres.bool"));
    assert!(spec.accepts_operation("std.db.postgres.json"));
    assert!(!spec.accepts_operation("std.http.response.text"));
}

/// Verifies Postgres worker metadata owns opaque Postgres resource types.
///
/// Inputs:
/// - Static Postgres worker spec and representative resource type names.
///
/// Output:
/// - Test passes when Postgres pool, connection, and row resources are owned
///   while unrelated resource types are rejected.
///
/// Transformation:
/// - Exercises exact resource-type matching without allocating resources.
#[test]
fn postgres_worker_spec_owns_only_postgres_resources() {
    let spec = postgres_worker_spec();

    assert!(spec.owns_resource_type("std.db.Postgres.Pool"));
    assert!(spec.owns_resource_type("std.db.Postgres.Connection"));
    assert!(spec.owns_resource_type("std.db.Postgres.Row"));
    assert!(!spec.owns_resource_type("std.http.Request.Request"));
}
