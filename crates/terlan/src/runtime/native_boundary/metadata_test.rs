use super::*;

#[test]
fn postgres_worker_manifest_is_vm_owned_complete_and_valid() {
    let manifest = postgres_worker_manifest();

    assert_eq!(manifest.adapter, "std.db.Postgres");
    assert_eq!(manifest.runtime, NativeBoundaryRuntime::VmWorker);
    assert_eq!(manifest.transport, NativeBoundaryTransport::VmMailbox);
    assert_eq!(manifest.default_credit_limit, 64);
    assert_eq!(manifest.exports.len(), 9);
    assert_eq!(manifest.validate(), Ok(()));
}

#[test]
fn postgres_worker_manifest_resolves_exports_by_operation_and_source_identity() {
    let manifest = postgres_worker_manifest();
    let query = manifest
        .export_for_operation("std.db.postgres.query")
        .expect("query export");

    assert_eq!(query.module, "std.db.Postgres");
    assert_eq!(query.function, "query");
    assert_eq!(query.arity, 3);
    assert_eq!(query.required_capability, "postgres");
    assert_eq!(query.argument_types, QUERY_ARGS);
    assert_eq!(query.return_type, "Result[List[Row], Error]");
    assert_eq!(
        query.worker_class,
        NativeBoundaryWorkerClass::ResourceOwning
    );
    assert_eq!(
        query.cancellation,
        NativeBoundaryCancellationPolicy::Cooperative
    );
    assert_eq!(query.resource_permissions, READ_QUERY_TARGET_CREATE_ROW);
    assert_eq!(
        query.argument_memory,
        NativeBoundaryMemoryOwnership::BorrowedArguments
    );
    assert_eq!(
        query.result_memory,
        NativeBoundaryMemoryOwnership::ResourceOwnedResult(ROW)
    );
    assert_eq!(query.failure_type, ERROR);
    assert_eq!(manifest.export("std.db.Postgres", "query", 3), Some(query));
    assert!(manifest.export("std.db.Postgres", "query", 2).is_none());
    assert!(manifest
        .export_for_operation("std.http.response.text")
        .is_none());
}

#[test]
fn postgres_worker_manifest_covers_every_operation_and_owned_resource() {
    let manifest = postgres_worker_manifest();
    for (operation, arity) in [
        ("std.db.postgres.connect", 1),
        ("std.db.postgres.query", 3),
        ("std.db.postgres.query_one", 3),
        ("std.db.postgres.execute", 3),
        ("std.db.postgres.transaction", 2),
        ("std.db.postgres.string", 2),
        ("std.db.postgres.int", 2),
        ("std.db.postgres.bool", 2),
        ("std.db.postgres.json", 2),
    ] {
        assert_eq!(
            manifest
                .export_for_operation(operation)
                .expect("manifest operation")
                .arity,
            arity
        );
    }
    for resource_type in [POOL, CONNECTION, ROW] {
        assert!(manifest.owns_resource_type(resource_type));
    }
    assert!(!manifest.owns_resource_type("std.http.Request.Request"));
}

#[test]
fn native_boundary_manifest_validation_rejects_schema_drift_adversarially() {
    const BAD_ARGS: &[&str] = &["Pool"];
    const BAD_PERMISSIONS: &[NativeBoundaryResourcePermission] =
        &[NativeBoundaryResourcePermission::Read("unknown.Resource")];
    const BAD_EXPORT: NativeBoundaryExportManifest = NativeBoundaryExportManifest {
        module: "",
        function: "query",
        arity: 2,
        operation: "duplicate.operation",
        required_capability: "",
        argument_types: BAD_ARGS,
        return_type: "",
        worker_class: NativeBoundaryWorkerClass::LongRunningCancellable,
        cancellation: NativeBoundaryCancellationPolicy::Cooperative,
        resource_permissions: BAD_PERMISSIONS,
        argument_memory: NativeBoundaryMemoryOwnership::BorrowedArguments,
        result_memory: NativeBoundaryMemoryOwnership::ResourceOwnedResult("unknown.Resource"),
        failure_type: "",
    };
    const BAD_EXPORTS: &[NativeBoundaryExportManifest] = &[BAD_EXPORT, BAD_EXPORT];
    let manifest = NativeBoundaryWorkerManifest {
        adapter: "",
        runtime: NativeBoundaryRuntime::VmWorker,
        transport: NativeBoundaryTransport::VmMailbox,
        default_credit_limit: 0,
        resource_types: &[],
        exports: BAD_EXPORTS,
    };

    let diagnostics = manifest.validate().expect_err("invalid manifest");
    for expected in [
        "NativeBoundary worker adapter must not be empty",
        "NativeBoundary worker credit limit must be positive",
        "has empty module",
        "has empty required capability",
        "arity 2 does not match 1 argument types",
        "has empty return type",
        "has empty failure type",
        "references unowned resource `unknown.Resource`",
        "duplicate NativeBoundary operation `duplicate.operation`",
        "duplicate NativeBoundary export `.query/2`",
    ] {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains(expected)),
            "missing diagnostic `{expected}` in {diagnostics:?}"
        );
    }
}

#[test]
fn native_boundary_metadata_contains_no_retired_runtime_transport_terms() {
    let source = include_str!("metadata.rs");
    let retired_worker_term = ["Safe", "NativeWorker"].concat();
    let retired_transport = ["Beam", "Process"].concat();

    assert!(!source.contains(&retired_worker_term));
    assert!(!source.contains(&retired_transport));
}
