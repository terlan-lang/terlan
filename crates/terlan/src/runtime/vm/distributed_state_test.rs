use super::{
    ReplValue, VmDistributedStateEntry, VmDistributedStatePolicy, VmDistributedStateScope,
    VmDistributedStateStore, VmDistributedStateVersion, VmDistributedStateWriteOutcome,
};

#[test]
fn vm_distributed_state_applies_and_replays_local_writes() {
    let mut store = VmDistributedStateStore::new();
    let scope = scope("cart", "alice");
    let version = version(1, "node-a");

    let first = store
        .write(
            scope.clone(),
            "node-a",
            ReplValue::Int(1),
            version.clone(),
            VmDistributedStatePolicy::ExplicitUserResolution,
        )
        .expect("initial write should apply");
    let expected = entry(
        scope.clone(),
        "node-a",
        ReplValue::Int(1),
        version.clone(),
        VmDistributedStatePolicy::ExplicitUserResolution,
    );

    assert_eq!(
        first,
        VmDistributedStateWriteOutcome::Applied(expected.clone())
    );
    assert_eq!(store.get(&scope), Some(&expected));
    let replay = store
        .write(
            scope,
            "node-a",
            ReplValue::Int(1),
            version,
            VmDistributedStatePolicy::ExplicitUserResolution,
        )
        .expect("identical write should replay");
    assert_eq!(replay, VmDistributedStateWriteOutcome::Replayed(expected));
}

#[test]
fn vm_distributed_state_reports_conflicts_with_versions_and_policy() {
    let mut store = VmDistributedStateStore::new();
    let scope = scope("cart", "alice");
    store
        .write(
            scope.clone(),
            "node-a",
            ReplValue::String("local".to_string()),
            version(5, "node-a"),
            VmDistributedStatePolicy::ExplicitUserResolution,
        )
        .expect("local write should apply");

    let conflict = store
        .write(
            scope,
            "node-b",
            ReplValue::String("stale".to_string()),
            version(4, "node-b"),
            VmDistributedStatePolicy::ExplicitUserResolution,
        )
        .expect("stale write should return typed conflict");

    match conflict {
        VmDistributedStateWriteOutcome::Conflict(conflict) => {
            assert_eq!(conflict.local_version, version(5, "node-a"));
            assert_eq!(conflict.incoming_version, version(4, "node-b"));
            assert_eq!(
                conflict.policy,
                VmDistributedStatePolicy::ExplicitUserResolution
            );
        }
        other => panic!("expected conflict, got {other:?}"),
    }
}

#[test]
fn vm_distributed_state_winner_takes_all_rejects_later_writes_without_mutation() {
    let mut store = VmDistributedStateStore::new();
    let scope = scope("settings", "theme");
    let first = entry(
        scope.clone(),
        "node-a",
        ReplValue::String("dark".to_string()),
        version(1, "node-a"),
        VmDistributedStatePolicy::WinnerTakesAll,
    );
    store
        .write(
            first.scope.clone(),
            first.owner_node_id.clone(),
            first.value.clone(),
            first.version.clone(),
            first.policy,
        )
        .expect("first write should apply");

    let conflict = store
        .write(
            scope.clone(),
            "node-b",
            ReplValue::String("light".to_string()),
            version(2, "node-b"),
            VmDistributedStatePolicy::WinnerTakesAll,
        )
        .expect("later winner-takes-all write should be a typed conflict");

    match conflict {
        VmDistributedStateWriteOutcome::Conflict(conflict) => {
            assert_eq!(conflict.scope, scope);
            assert_eq!(conflict.local_version, version(1, "node-a"));
            assert_eq!(conflict.incoming_version, version(2, "node-b"));
            assert_eq!(conflict.policy, VmDistributedStatePolicy::WinnerTakesAll);
        }
        other => panic!("expected winner-takes-all conflict, got {other:?}"),
    }
    assert_eq!(store.get(&first.scope), Some(&first));
}

#[test]
fn vm_distributed_state_applies_last_writer_wins_tie_breaks() {
    let mut store = VmDistributedStateStore::new();
    let scope = scope("presence", "room-1");
    store
        .write(
            scope.clone(),
            "node-a",
            ReplValue::String("a".to_string()),
            version(9, "node-a"),
            VmDistributedStatePolicy::LastWriterWins,
        )
        .expect("first write should apply");

    let winner = store
        .write(
            scope.clone(),
            "node-z",
            ReplValue::String("z".to_string()),
            version(9, "node-z"),
            VmDistributedStatePolicy::LastWriterWins,
        )
        .expect("lexicographic tie winner should apply");

    assert!(matches!(
        winner,
        VmDistributedStateWriteOutcome::Applied(VmDistributedStateEntry { .. })
    ));
    assert_eq!(
        store.get(&scope).map(|entry| &entry.value),
        Some(&ReplValue::String("z".to_string()))
    );
    let loser = store
        .write(
            scope,
            "node-b",
            ReplValue::String("b".to_string()),
            version(9, "node-b"),
            VmDistributedStatePolicy::LastWriterWins,
        )
        .expect("lexicographic tie loser should conflict");
    assert!(matches!(loser, VmDistributedStateWriteOutcome::Conflict(_)));
}

#[test]
fn vm_distributed_state_reports_policy_mismatch_without_mutating_state() {
    let mut store = VmDistributedStateStore::new();
    let scope = scope("settings", "theme");
    store
        .write(
            scope.clone(),
            "node-a",
            ReplValue::String("dark".to_string()),
            version(1, "node-a"),
            VmDistributedStatePolicy::WinnerTakesAll,
        )
        .expect("first write should apply");

    let mismatch = store
        .write(
            scope.clone(),
            "node-b",
            ReplValue::String("light".to_string()),
            version(2, "node-b"),
            VmDistributedStatePolicy::Merge,
        )
        .expect("policy mismatch should be typed");

    assert_eq!(
        mismatch,
        VmDistributedStateWriteOutcome::PolicyMismatch {
            scope: scope.clone(),
            existing_policy: VmDistributedStatePolicy::WinnerTakesAll,
            incoming_policy: VmDistributedStatePolicy::Merge,
        }
    );
    assert_eq!(
        store.get(&scope).map(|entry| &entry.value),
        Some(&ReplValue::String("dark".to_string()))
    );
}

#[test]
fn vm_distributed_state_exports_and_imports_deterministic_snapshots() {
    let mut store = VmDistributedStateStore::new();
    store
        .write(
            scope("settings", "b"),
            "node-a",
            ReplValue::Int(2),
            version(2, "node-a"),
            VmDistributedStatePolicy::WinnerTakesAll,
        )
        .expect("b write should apply");
    store
        .write(
            scope("settings", "a"),
            "node-a",
            ReplValue::Int(1),
            version(1, "node-a"),
            VmDistributedStatePolicy::WinnerTakesAll,
        )
        .expect("a write should apply");

    let snapshot = store.export_snapshot();

    assert_eq!(snapshot[0].scope, scope("settings", "a"));
    assert_eq!(snapshot[1].scope, scope("settings", "b"));
    let restored =
        VmDistributedStateStore::import_snapshot(snapshot).expect("snapshot should restore");
    assert_eq!(restored.export_snapshot(), store.export_snapshot());
}

#[test]
fn vm_distributed_state_rejects_invalid_scopes_versions_and_snapshots() {
    assert_eq!(
        VmDistributedStateScope::new("", "key").expect_err("empty namespace should fail"),
        "error[vm_distributed_state]: state namespace must be non-empty"
    );
    assert_eq!(
        VmDistributedStateScope::new("ns", "").expect_err("empty key should fail"),
        "error[vm_distributed_state]: state key must be non-empty"
    );
    assert_eq!(
        VmDistributedStateVersion::new(0, "node-a").expect_err("zero sequence should fail"),
        "error[vm_distributed_state]: state version sequence must be non-zero"
    );
    assert_eq!(
        VmDistributedStateVersion::new(1, "").expect_err("empty node should fail"),
        "error[vm_distributed_state]: state version node id must be non-empty"
    );

    let mut store = VmDistributedStateStore::new();
    let owner_error = store
        .write(
            scope("ns", "key"),
            "",
            ReplValue::Int(1),
            version(1, "node-a"),
            VmDistributedStatePolicy::WinnerTakesAll,
        )
        .expect_err("empty owner should fail");
    assert_eq!(
        owner_error,
        "error[vm_distributed_state]: state owner node id must be non-empty"
    );

    let duplicate = entry(
        scope("ns", "key"),
        "node-a",
        ReplValue::Int(1),
        version(1, "node-a"),
        VmDistributedStatePolicy::WinnerTakesAll,
    );
    let snapshot_error = VmDistributedStateStore::import_snapshot([duplicate.clone(), duplicate])
        .expect_err("duplicate snapshot scope should fail");
    assert_eq!(
        snapshot_error,
        "error[vm_distributed_state]: snapshot contains duplicate state scope"
    );

    let invalid_scope_snapshot = entry(
        VmDistributedStateScope {
            namespace: String::new(),
            key: "key".to_string(),
        },
        "node-a",
        ReplValue::Int(1),
        version(1, "node-a"),
        VmDistributedStatePolicy::WinnerTakesAll,
    );
    assert_eq!(
        VmDistributedStateStore::import_snapshot([invalid_scope_snapshot])
            .expect_err("invalid snapshot scope should fail"),
        "error[vm_distributed_state]: snapshot scope must be valid"
    );
}

fn scope(namespace: &str, key: &str) -> VmDistributedStateScope {
    VmDistributedStateScope::new(namespace, key).expect("scope should be valid")
}

fn version(sequence: u64, node_id: &str) -> VmDistributedStateVersion {
    VmDistributedStateVersion::new(sequence, node_id).expect("version should be valid")
}

fn entry(
    scope: VmDistributedStateScope,
    owner_node_id: &str,
    value: ReplValue,
    version: VmDistributedStateVersion,
    policy: VmDistributedStatePolicy,
) -> VmDistributedStateEntry {
    VmDistributedStateEntry {
        scope,
        owner_node_id: owner_node_id.to_string(),
        value,
        version,
        policy,
    }
}
