//! Source storage handles preserve owner authority and real encoded contents.

use super::*;
use crate::runtime::native_image::TvmBoundaryType;

fn request(operation: &str, args: Vec<ReplValue>) -> PureNativeCapabilityRequest {
    PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: operation.into(),
        arguments: vec![],
        package_arguments: Some(args),
        result_type: TvmBoundaryType::Unit,
    }
}

fn state_call(
    state: &mut VmDistributedStateRuntime,
    operation: &str,
    args: Vec<ReplValue>,
) -> ReplValue {
    state
        .call(
            1,
            &request(&format!("std.vm.distributed_state.{operation}"), args),
        )
        .unwrap()
}

fn source_snapshot(state: &mut VmDistributedStateRuntime) -> ReplValue {
    let store = state_call(state, "store", vec![]);
    state_call(state, "export_snapshot", vec![store])
}

fn checkpoint(snapshot: ReplValue) -> PureNativeCapabilityRequest {
    request(
        "std.vm.distributed_storage.checkpoint",
        vec![
            ReplValue::String("checkpoint".into()),
            ReplValue::Int(1),
            snapshot,
        ],
    )
}

#[test]
fn checkpoint_handles_enforce_owner_kind_and_lifetime() {
    let mut runtime = VmDistributedStorageRuntime::default();
    let mut state = VmDistributedStateRuntime::default();
    let original = source_snapshot(&mut state);
    let create = checkpoint(original.clone());
    assert!(runtime.call(2, &create, &[], &mut state).is_err());
    let value = runtime.call(1, &create, &[], &mut state).unwrap();
    assert_eq!(
        runtime.snapshot(1, &value).unwrap().payload,
        b"TETF\x01\x03\0\0\0\0"
    );
    let restore = request("std.vm.distributed_storage.restore", vec![value.clone()]);
    assert!(runtime.call(2, &restore, &[], &mut state).is_err());
    assert!(runtime
        .call(
            1,
            &request("std.vm.distributed_storage.restore", vec![original]),
            &[],
            &mut state
        )
        .is_err());
    let restored = runtime.call(1, &restore, &[], &mut state).unwrap();
    let exported = state_call(&mut state, "export_snapshot", vec![restored]);
    assert!(state.snapshot_entries(1, &exported).unwrap().is_empty());
    runtime.close_owner(1);
    assert!(runtime.call(1, &restore, &[], &mut state).is_err());
}

#[test]
fn checkpoint_rejects_bad_identity_and_does_not_route_unknown_calls_to_workers() {
    let mut runtime = VmDistributedStorageRuntime::default();
    let mut state = VmDistributedStateRuntime::default();
    let snapshot = source_snapshot(&mut state);
    for (id, sequence) in [("", 1), ("bad\0id", 1), ("id", 0), ("id", -1)] {
        let create = request(
            "std.vm.distributed_storage.checkpoint",
            vec![
                ReplValue::String(id.into()),
                ReplValue::Int(sequence),
                snapshot.clone(),
            ],
        );
        assert!(runtime.call(1, &create, &[], &mut state).is_err());
    }
    let error = runtime
        .call(
            1,
            &request("std.vm.distributed_storage.flush", vec![]),
            &[],
            &mut state,
        )
        .unwrap_err();
    assert!(error.starts_with("error[vm.distributed_storage.arguments]"));
}

fn call(
    runtime: &mut VmDistributedStorageRuntime,
    state: &mut VmDistributedStateRuntime,
    operation: &str,
    args: Vec<ReplValue>,
) -> ReplValue {
    runtime
        .call(
            1,
            &request(&format!("std.vm.distributed_storage.{operation}"), args),
            &[],
            state,
        )
        .unwrap()
}

fn open_reply(
    identity: u8,
    sequence: i64,
    schema: i64,
    schema_sequence: i64,
) -> crate::terlan_native_boundary::term::NativeBoundaryReplyTerm {
    use crate::terlan_native_boundary::term::{
        NativeBoundaryReplyTerm as Reply, NativeBoundaryTerm as Term,
    };
    Reply::Ok(Term::Tuple(vec![
        Term::Bytes(vec![identity; 32]),
        Term::Tuple(vec![
            Term::Int(sequence),
            Term::Int(schema),
            Term::Int(schema_sequence),
        ]),
    ]))
}

#[test]
fn storage_policy_cannot_grant_itself_host_authority_or_replication() {
    let mut runtime = VmDistributedStorageRuntime::default();
    let mut state = VmDistributedStateRuntime::default();
    for (mode, bound, requested, expected) in [
        ("durable", false, true, false),
        ("durable", true, false, false),
        ("durable", true, true, true),
        ("cluster", true, true, false),
        ("local_only", true, true, true),
        ("local_only", false, true, true),
        ("local_only", false, false, false),
    ] {
        runtime.bindings.clear();
        if bound {
            runtime.bind([("primary".to_string(), None)].into_iter());
        }
        let worker = mode == "durable" && expected;
        let mode = call(&mut runtime, &mut state, mode, vec![]);
        let policy = call(
            &mut runtime,
            &mut state,
            "policy",
            vec![
                ReplValue::String("primary".into()),
                mode,
                ReplValue::Bool(requested),
            ],
        );
        assert_eq!(
            call(
                &mut runtime,
                &mut state,
                "policy_available",
                vec![policy.clone()]
            ),
            ReplValue::Bool(expected)
        );
        assert_eq!(
            call(
                &mut runtime,
                &mut state,
                "policy_can_cluster_replicate",
                vec![policy.clone()]
            ),
            ReplValue::Bool(false)
        );
        let adapter = call(&mut runtime, &mut state, "adapter", vec![policy]);
        let open = request("std.vm.distributed_storage.open", vec![adapter.clone()]);
        assert_eq!(runtime.prepare(1, &open).unwrap().is_some(), worker);
        assert!(runtime.prepare(2, &open).is_err());
        assert!(runtime
            .prepare(
                1,
                &request("std.vm.distributed_storage.flush", vec![adapter])
            )
            .unwrap()
            .is_none());
    }
}

#[path = "distributed_storage_local_test.rs"]
mod local;

#[test]
fn durable_lifecycle_updates_only_after_checked_successful_completions() {
    use crate::terlan_native_boundary::term::{
        NativeBoundaryReplyTerm as Reply, NativeBoundaryTerm as Term,
    };
    let mut runtime = VmDistributedStorageRuntime::default();
    let mut state = VmDistributedStateRuntime::default();
    runtime.bind([("primary".to_string(), None)].into_iter());
    let mode = call(&mut runtime, &mut state, "durable", vec![]);
    let policy = call(
        &mut runtime,
        &mut state,
        "policy",
        vec![
            ReplValue::String("primary".into()),
            mode,
            ReplValue::Bool(true),
        ],
    );
    let adapter = call(&mut runtime, &mut state, "adapter", vec![policy]);
    let open = request("std.vm.distributed_storage.open", vec![adapter.clone()]);
    let flush = request("std.vm.distributed_storage.flush", vec![adapter.clone()]);
    let pending = runtime.prepare(1, &open).unwrap().unwrap();
    assert_eq!(pending.backend, "primary");
    assert_eq!(pending.operation, "runtime.storage.open");
    assert!(pending.arguments.is_empty());
    assert!(runtime
        .complete(1, pending.pending, Reply::Ok(Term::Int(0)))
        .is_err());
    assert!(runtime.prepare(1, &flush).unwrap().is_none());
    let pending = runtime.prepare(1, &open).unwrap().unwrap();
    let status = Reply::Ok(Term::Tuple(vec![Term::Int(0), Term::Int(1), Term::Int(0)]));
    let opened = runtime
        .complete(1, pending.pending, open_reply(1, 0, 1, 0))
        .unwrap();
    assert_eq!(
        call(&mut runtime, &mut state, "kind", vec![opened]),
        ReplValue::String("opened".into())
    );
    let pending = runtime.prepare(1, &flush).unwrap().unwrap();
    let failed = Reply::Error {
        code: "storage.busy".into(),
        message: "must not echo backend contents".into(),
        offset: 0,
    };
    let error = runtime.complete(1, pending.pending, failed).unwrap_err();
    assert!(error.contains("do not prove rollback"));
    assert!(!error.contains("must not echo"));
    let close = request("std.vm.distributed_storage.close", vec![adapter]);
    let pending = runtime.prepare(1, &close).unwrap().unwrap();
    runtime.complete(1, pending.pending, status).unwrap();
    assert!(runtime.prepare(1, &flush).unwrap().is_none());
    runtime.close_owner(1);
    assert!(runtime.prepare(1, &open).is_err());
}

#[test]
fn structured_storage_failures_keep_exact_metadata_and_fence_uncertain_adapters() {
    use crate::terlan_native_boundary::storage_reply::StorageFailure;
    use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm as Reply;
    let mut runtime = VmDistributedStorageRuntime::default();
    let mut state = VmDistributedStateRuntime::default();
    runtime.bind([("primary".to_string(), None)].into_iter());
    let mode = call(&mut runtime, &mut state, "durable", vec![]);
    let policy = call(
        &mut runtime,
        &mut state,
        "policy",
        vec![
            ReplValue::String("primary".into()),
            mode,
            ReplValue::Bool(true),
        ],
    );
    let adapter = call(&mut runtime, &mut state, "adapter", vec![policy]);
    let open = request("std.vm.distributed_storage.open", vec![adapter.clone()]);
    let pending = runtime.prepare(1, &open).unwrap().unwrap();
    runtime
        .complete(1, pending.pending, open_reply(1, 7, 2, 5))
        .unwrap();
    let migrate = request(
        "std.vm.distributed_storage.migrate_schema",
        vec![adapter.clone(), ReplValue::Int(1), ReplValue::Int(3)],
    );
    let pending = runtime.prepare(1, &migrate).unwrap().unwrap();
    let outcome = runtime
        .complete(
            1,
            pending.pending,
            Reply::Ok(
                StorageFailure::Schema {
                    expected: 1,
                    actual: 2,
                }
                .into_term()
                .unwrap(),
            ),
        )
        .unwrap();
    assert_eq!(
        call(
            &mut runtime,
            &mut state,
            "expected_schema",
            vec![outcome.clone()]
        ),
        ReplValue::Int(1)
    );
    assert_eq!(
        call(
            &mut runtime,
            &mut state,
            "actual_schema",
            vec![outcome.clone()]
        ),
        ReplValue::Int(2)
    );
    assert_eq!(
        call(&mut runtime, &mut state, "is_failure", vec![outcome]),
        ReplValue::Bool(true)
    );
    for operation in [
        "atomic_append_proof",
        "schema_migration_proof",
        "compare_and_swap_token",
    ] {
        let proof = request(
            &format!("std.vm.distributed_storage.{operation}"),
            vec![adapter.clone()],
        );
        let pending = runtime.prepare(1, &proof).unwrap().unwrap();
        let error = runtime
            .complete(
                1,
                pending.pending,
                Reply::Ok(StorageFailure::Busy.into_term().unwrap()),
            )
            .unwrap_err();
        assert!(error.contains("no proof was issued"));
    }
    let flush = request("std.vm.distributed_storage.flush", vec![adapter.clone()]);
    let pending = runtime.prepare(1, &flush).unwrap().unwrap();
    let outcome = runtime
        .complete(
            1,
            pending.pending,
            Reply::Ok(StorageFailure::Database.into_term().unwrap()),
        )
        .unwrap();
    assert_eq!(
        call(
            &mut runtime,
            &mut state,
            "requires_recovery",
            vec![outcome.clone()]
        ),
        ReplValue::Bool(true)
    );
    assert_eq!(
        call(&mut runtime, &mut state, "recovery_action", vec![outcome]),
        ReplValue::String("reconcile_commit".into())
    );
    assert!(runtime.prepare(1, &flush).unwrap().is_none());
    assert!(runtime.prepare(1, &open).unwrap().is_some());
}

#[path = "distributed_storage_identity_test.rs"]
mod identity;

#[path = "distributed_storage_resource_test.rs"]
mod resource_validation;
