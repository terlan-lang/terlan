//! Local AOT capability dispatch uses real bounded state, not durable-worker replies.

use super::*;

fn adapter(
    runtime: &mut VmDistributedStorageRuntime,
    state: &mut VmDistributedStateRuntime,
) -> ReplValue {
    let policy = call(runtime, state, "force_local", vec![]);
    let adapter = call(runtime, state, "adapter", vec![policy]);
    call(runtime, state, "open", vec![adapter.clone()]);
    adapter
}

fn entry(
    runtime: &mut VmDistributedStorageRuntime,
    state: &mut VmDistributedStateRuntime,
    id: &str,
    sequence: i64,
) -> ReplValue {
    let snapshot = source_snapshot(state);
    call(
        runtime,
        state,
        "checkpoint",
        vec![
            ReplValue::String(id.into()),
            ReplValue::Int(sequence),
            snapshot,
        ],
    )
}

fn kind(
    runtime: &mut VmDistributedStorageRuntime,
    state: &mut VmDistributedStateRuntime,
    outcome: ReplValue,
) -> ReplValue {
    call(runtime, state, "kind", vec![outcome])
}

#[test]
fn local_storage_is_independent_owner_checked_and_not_durable() {
    let mut runtime = VmDistributedStorageRuntime::default();
    let mut state = VmDistributedStateRuntime::default();
    let adapter = adapter(&mut runtime, &mut state);
    let snapshot = entry(&mut runtime, &mut state, "first", 1);
    let append = request(
        "std.vm.distributed_storage.append",
        vec![adapter.clone(), snapshot.clone()],
    );
    assert!(runtime.prepare(1, &append).unwrap().is_none());
    assert!(runtime.call(2, &append, &[], &mut state).is_err());
    let appended = runtime.call(1, &append, &[], &mut state).unwrap();
    assert_eq!(
        kind(&mut runtime, &mut state, appended),
        ReplValue::String("appended".into())
    );
    let loaded = call(
        &mut runtime,
        &mut state,
        "load_snapshot",
        vec![adapter.clone(), ReplValue::String("first".into())],
    );
    let view = call(&mut runtime, &mut state, "loaded_snapshot", vec![loaded]);
    assert_eq!(
        runtime.snapshot(1, &view).unwrap(),
        runtime.snapshot(1, &snapshot).unwrap()
    );
    let denied = call(
        &mut runtime,
        &mut state,
        "require_durable_flush",
        vec![adapter.clone()],
    );
    assert_eq!(
        kind(&mut runtime, &mut state, denied),
        ReplValue::String("unsupported".into())
    );
    call(&mut runtime, &mut state, "flush", vec![adapter.clone()]);
    assert!(runtime
        .call(
            1,
            &request(
                "std.vm.distributed_storage.durable_flush_proof",
                vec![adapter.clone()]
            ),
            &[],
            &mut state
        )
        .is_err());
    let policy = call(&mut runtime, &mut state, "force_local", vec![]);
    let independent = call(&mut runtime, &mut state, "adapter", vec![policy]);
    call(&mut runtime, &mut state, "open", vec![independent.clone()]);
    let missing = call(
        &mut runtime,
        &mut state,
        "load_snapshot",
        vec![independent, ReplValue::String("first".into())],
    );
    assert_eq!(
        kind(&mut runtime, &mut state, missing),
        ReplValue::String("snapshot_missing".into())
    );
    call(&mut runtime, &mut state, "close", vec![adapter.clone()]);
    assert!(runtime
        .call(
            1,
            &request(
                "std.vm.distributed_storage.atomic_append_proof",
                vec![adapter.clone()]
            ),
            &[],
            &mut state
        )
        .is_err());
    call(&mut runtime, &mut state, "open", vec![adapter]);
    runtime.close_owner(1);
    assert!(runtime.call(1, &append, &[], &mut state).is_err());
    assert!(runtime.snapshot(1, &view).is_err());
}

#[test]
fn local_transactions_cas_replay_schema_and_retained_views_use_real_state() {
    let mut runtime = VmDistributedStorageRuntime::default();
    let mut state = VmDistributedStateRuntime::default();
    let adapter = adapter(&mut runtime, &mut state);
    let token = call(
        &mut runtime,
        &mut state,
        "compare_and_swap_token",
        vec![adapter.clone()],
    );
    let first = entry(&mut runtime, &mut state, "first", 1);
    let second = entry(&mut runtime, &mut state, "second", 2);
    call(
        &mut runtime,
        &mut state,
        "append",
        vec![adapter.clone(), first.clone()],
    );
    let stale = call(
        &mut runtime,
        &mut state,
        "compare_and_swap_append",
        vec![adapter.clone(), second.clone(), token],
    );
    assert_eq!(
        kind(&mut runtime, &mut state, stale),
        ReplValue::String("cas_token_mismatch".into())
    );
    let failed = call(
        &mut runtime,
        &mut state,
        "transactional_batch_append",
        vec![
            adapter.clone(),
            ReplValue::List(vec![second.clone(), first.clone()]),
        ],
    );
    assert_eq!(
        kind(&mut runtime, &mut state, failed),
        ReplValue::String("invalid_request".into())
    );
    let missing = call(
        &mut runtime,
        &mut state,
        "load_snapshot",
        vec![adapter.clone(), ReplValue::String("second".into())],
    );
    assert_eq!(
        kind(&mut runtime, &mut state, missing),
        ReplValue::String("snapshot_missing".into())
    );
    call(
        &mut runtime,
        &mut state,
        "transactional_batch_append",
        vec![adapter.clone(), ReplValue::List(vec![second])],
    );
    let replay = call(
        &mut runtime,
        &mut state,
        "append",
        vec![adapter.clone(), first],
    );
    assert_eq!(
        kind(&mut runtime, &mut state, replay),
        ReplValue::String("appended".into())
    );
    let loaded = call(
        &mut runtime,
        &mut state,
        "load_snapshot",
        vec![adapter.clone(), ReplValue::String("first".into())],
    );
    let migrated = call(
        &mut runtime,
        &mut state,
        "migrate_schema",
        vec![adapter.clone(), ReplValue::Int(1), ReplValue::Int(2)],
    );
    assert_eq!(
        call(&mut runtime, &mut state, "actual_schema", vec![migrated]),
        ReplValue::Int(2)
    );
    call(
        &mut runtime,
        &mut state,
        "compact",
        vec![adapter.clone(), ReplValue::Int(2)],
    );
    let view = call(&mut runtime, &mut state, "loaded_snapshot", vec![loaded]);
    assert_eq!(runtime.snapshot(1, &view).unwrap().id, "first");
    let sequence = call(
        &mut runtime,
        &mut state,
        "atomic_append_proof",
        vec![adapter],
    );
    assert_eq!(
        call(&mut runtime, &mut state, "proof_sequence", vec![sequence]),
        ReplValue::Int(2)
    );
}

#[test]
fn local_adapter_reservations_are_bounded_and_reclaimed_on_owner_exit() {
    let mut runtime = VmDistributedStorageRuntime::default();
    let mut state = VmDistributedStateRuntime::default();
    let policy = call(&mut runtime, &mut state, "force_local", vec![]);
    let create = request("std.vm.distributed_storage.adapter", vec![policy]);
    for _ in 0..3 {
        runtime.call(1, &create, &[], &mut state).unwrap();
    }
    let error = runtime.call(1, &create, &[], &mut state).unwrap_err();
    assert!(error.starts_with("error[vm.distributed_storage.capacity]"));
    runtime.close_owner(1);
    let fresh = adapter(&mut runtime, &mut state);
    assert!(runtime
        .prepare(1, &request("std.vm.distributed_storage.open", vec![fresh]))
        .unwrap()
        .is_none());
}

#[test]
fn replication_is_explicitly_unsupported_without_dispatch_or_local_fallback() {
    for mode in ["local_only", "durable", "cluster"] {
        let mut runtime = VmDistributedStorageRuntime::default();
        let mut state = VmDistributedStateRuntime::default();
        runtime.bind([("named".into(), None)].into_iter());
        let mode_value = call(&mut runtime, &mut state, mode, vec![]);
        let policy = call(
            &mut runtime,
            &mut state,
            "policy",
            vec![
                ReplValue::String("named".into()),
                mode_value,
                ReplValue::Bool(true),
            ],
        );
        let adapter = call(&mut runtime, &mut state, "adapter", vec![policy]);
        let snapshot = entry(&mut runtime, &mut state, "rejected", 1);
        for (operation, arguments) in [
            ("require_cluster_replication", vec![adapter.clone()]),
            (
                "replicate_snapshot",
                vec![adapter.clone(), snapshot.clone()],
            ),
        ] {
            let request = request(
                &format!("std.vm.distributed_storage.{operation}"),
                arguments,
            );
            assert!(runtime.prepare(1, &request).unwrap().is_none());
            assert!(runtime.call(2, &request, &[], &mut state).is_err());
            let outcome = runtime.call(1, &request, &[], &mut state).unwrap();
            assert_eq!(
                kind(&mut runtime, &mut state, outcome.clone()),
                ReplValue::String("unsupported".into())
            );
            assert_eq!(
                call(
                    &mut runtime,
                    &mut state,
                    "is_success",
                    vec![outcome.clone()]
                ),
                ReplValue::Bool(false)
            );
            assert_eq!(
                call(
                    &mut runtime,
                    &mut state,
                    "is_failure",
                    vec![outcome.clone()]
                ),
                ReplValue::Bool(true)
            );
            assert_eq!(
                call(&mut runtime, &mut state, "sequence", vec![outcome]),
                ReplValue::Int(0)
            );
        }
        let malformed = request(
            "std.vm.distributed_storage.replicate_snapshot",
            vec![adapter.clone()],
        );
        assert!(runtime.call(1, &malformed, &[], &mut state).is_err());
        let wrong_kind = request(
            "std.vm.distributed_storage.replicate_snapshot",
            vec![adapter.clone(), adapter.clone()],
        );
        assert!(runtime.call(1, &wrong_kind, &[], &mut state).is_err());
        if mode == "local_only" {
            call(&mut runtime, &mut state, "open", vec![adapter.clone()]);
            let missing = call(
                &mut runtime,
                &mut state,
                "load_snapshot",
                vec![adapter.clone(), ReplValue::String("rejected".into())],
            );
            assert_eq!(
                kind(&mut runtime, &mut state, missing),
                ReplValue::String("snapshot_missing".into())
            );
        }
        runtime.close_owner(1);
        let stale = request(
            "std.vm.distributed_storage.replicate_snapshot",
            vec![adapter, snapshot],
        );
        assert!(runtime.call(1, &stale, &[], &mut state).is_err());
    }
}
