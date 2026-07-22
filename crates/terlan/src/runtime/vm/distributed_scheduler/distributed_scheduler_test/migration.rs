use super::*;
use crate::runtime::vm::distributed_storage::{
    VmDistributedStorageAdapter, VmDistributedStorageOperation, VmDistributedStorageOutcome,
    VmDistributedStoragePolicy, VmDistributedStorageSnapshot,
};
use crate::runtime::vm::{
    distributed_state::{
        VmDistributedStateEntry, VmDistributedStatePolicy, VmDistributedStateScope,
        VmDistributedStateVersion,
    },
    ReplValue,
};

fn snapshot(
    checkpoint_id: &str,
    sequence: u64,
    entries: Vec<VmDistributedStateEntry>,
) -> VmDistributedStorageSnapshot {
    VmDistributedStorageSnapshot::new(checkpoint_id, sequence, entries)
        .expect("snapshot should build")
}

fn state_entry(namespace: &str, key: &str, value: i64) -> VmDistributedStateEntry {
    VmDistributedStateEntry {
        scope: VmDistributedStateScope::new(namespace, key).expect("scope should be valid"),
        owner_node_id: "node-a".to_string(),
        value: ReplValue::Int(value),
        version: VmDistributedStateVersion::new(value as u64, "node-a")
            .expect("version should be valid"),
        policy: VmDistributedStatePolicy::WinnerTakesAll,
    }
}

#[test]
fn vm_distributed_scheduler_requests_and_commits_ordered_migration_intents() {
    let mut scheduler = two_node_scheduler();

    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("migration should request");

    assert_eq!(
        intent,
        VmMigrationIntent {
            actor_id: "actor-1".to_string(),
            from_node_id: "node-a".to_string(),
            to_node_id: "node-b".to_string(),
            sequence: 1,
            stateful: true,
            phase: VmMigrationPhase::Requested,
            state_snapshot_ready: false,
            in_flight_messages_ready: false,
        }
    );
    assert_eq!(scheduler.migration_intent("actor-1"), Some(&intent));
    let snapshotting = scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Snapshotting)
        .expect("snapshot phase");
    assert_eq!(
        snapshotting,
        VmMigrationIntent {
            phase: VmMigrationPhase::Snapshotting,
            ..intent.clone()
        }
    );
    assert!(!snapshotting.state_snapshot_ready);
    assert!(!snapshotting.in_flight_messages_ready);
    let transferring = scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Transferring)
        .expect("transfer phase");
    assert!(transferring.state_snapshot_ready);
    assert!(!transferring.in_flight_messages_ready);
    let resuming = scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Resuming)
        .expect("resume phase");
    assert!(resuming.state_snapshot_ready);
    assert!(resuming.in_flight_messages_ready);
    assert_eq!(
        scheduler
            .commit_migration("actor-1", intent.sequence)
            .expect("commit should succeed"),
        VmMigrationOutcome::Committed { sequence: 1 }
    );
    assert_eq!(scheduler.migration_intent("actor-1"), None);
}

#[test]
fn vm_distributed_scheduler_and_migration() {
    vm_distributed_scheduler_requests_and_commits_ordered_migration_intents();
}

#[test]
fn vm_distributed_scheduler_rejects_migration_completion_without_ready_state_and_messages() {
    let mut scheduler = two_node_scheduler();
    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("migration");
    for phase in [
        VmMigrationPhase::Snapshotting,
        VmMigrationPhase::Transferring,
        VmMigrationPhase::Resuming,
    ] {
        scheduler
            .advance_migration("actor-1", intent.sequence, phase)
            .expect("ordered phase");
    }
    let mut snapshot = scheduler.source_snapshot();
    snapshot.in_flight_migrations[0].in_flight_messages_ready = false;
    assert_eq!(
        VmDistributedScheduler::from_source_snapshot(snapshot)
            .expect_err("corrupt migration readiness must fail"),
        "error[vm_distributed_scheduler]: source snapshot migration readiness is invalid"
    );
}

#[test]
fn vm_distributed_scheduler_prevents_duplicate_rollback_loops_from_orphaning_new_migration() {
    let mut scheduler = two_node_scheduler();
    let first = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("first migration");
    let first_outcome = scheduler
        .timeout_migration_at_tick("actor-1", first.sequence, 10, "timeout")
        .expect("first timeout");
    let second = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("replacement migration");

    assert_eq!(
        scheduler
            .timeout_migration_at_tick("actor-1", first.sequence, 10, "timeout")
            .expect("duplicate old rollback replays"),
        first_outcome
    );
    assert_eq!(scheduler.migration_intent("actor-1"), Some(&second));
    assert!(scheduler
        .timeout_migration_at_tick("actor-1", first.sequence, 11, "different timeout")
        .expect_err("conflicting rollback loop must fail")
        .contains("incompatible outcome"));
    assert_eq!(scheduler.migration_intent("actor-1"), Some(&second));
}

#[test]
fn vm_distributed_scheduler_rejects_duplicate_and_out_of_order_migration_outcomes() {
    let mut scheduler = two_node_scheduler();
    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", false)
        .expect("migration should request");

    let duplicate_error = scheduler
        .request_migration("actor-1", "node-b", "node-a", false)
        .expect_err("duplicate migration should fail");
    assert_eq!(
        duplicate_error,
        "error[vm_distributed_scheduler]: actor `actor-1` already has an in-flight migration"
    );
    let sequence_error = scheduler
        .commit_migration("actor-1", intent.sequence + 1)
        .expect_err("wrong sequence should fail");
    assert_eq!(
        sequence_error,
        "error[vm_distributed_scheduler]: migration outcome sequence `2` does not match expected `1` for actor `actor-1`"
    );
    assert_eq!(scheduler.migration_intent("actor-1"), Some(&intent));
}

#[test]
fn vm_distributed_scheduler_enforces_migration_phase_order_before_commit() {
    let mut scheduler = two_node_scheduler();
    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("migration should request");

    let early_commit_error = scheduler
        .commit_migration("actor-1", intent.sequence)
        .expect_err("commit before resume should fail");
    assert_eq!(
        early_commit_error,
        "error[vm_distributed_scheduler]: migration for actor `actor-1` must reach `Resuming` before commit"
    );
    let skipped_phase_error = scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Transferring)
        .expect_err("skipped phase should fail");
    assert_eq!(
        skipped_phase_error,
        "error[vm_distributed_scheduler]: migration for actor `actor-1` expected next phase `Snapshotting` but got `Transferring`"
    );
    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Snapshotting)
        .expect("snapshot phase");
    let repeated_phase_error = scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Snapshotting)
        .expect_err("repeated phase should fail");
    assert_eq!(
        repeated_phase_error,
        "error[vm_distributed_scheduler]: migration for actor `actor-1` expected next phase `Transferring` but got `Snapshotting`"
    );
    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Transferring)
        .expect("transfer phase");
    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Resuming)
        .expect("resume phase");
    let beyond_terminal_error = scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Resuming)
        .expect_err("cannot advance after resuming");
    assert_eq!(
        beyond_terminal_error,
        "error[vm_distributed_scheduler]: migration for actor `actor-1` cannot advance beyond `Resuming`"
    );
}

#[test]
fn vm_distributed_scheduler_rolls_back_and_aborts_migrations_with_reasons() {
    let mut scheduler = two_node_scheduler();
    let rollback_intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("rollback migration should request");

    assert_eq!(
        scheduler
            .rollback_migration("actor-1", rollback_intent.sequence, "snapshot_failed")
            .expect("rollback should succeed"),
        VmMigrationOutcome::RolledBack {
            sequence: 1,
            reason: "snapshot_failed".to_string(),
        }
    );
    let abort_intent = scheduler
        .request_migration("actor-2", "node-b", "node-a", false)
        .expect("abort migration should request");
    assert_eq!(
        scheduler
            .abort_migration("actor-2", abort_intent.sequence, "node_draining")
            .expect("abort should succeed"),
        VmMigrationOutcome::Aborted {
            sequence: 2,
            reason: "node_draining".to_string(),
        }
    );
}

#[test]
fn vm_distributed_scheduler_replays_duplicate_terminal_migration_outcomes() {
    let mut scheduler = two_node_scheduler();
    let commit_intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("commit migration should request");
    scheduler
        .advance_migration(
            "actor-1",
            commit_intent.sequence,
            VmMigrationPhase::Snapshotting,
        )
        .expect("snapshot phase");
    scheduler
        .advance_migration(
            "actor-1",
            commit_intent.sequence,
            VmMigrationPhase::Transferring,
        )
        .expect("transfer phase");
    scheduler
        .advance_migration(
            "actor-1",
            commit_intent.sequence,
            VmMigrationPhase::Resuming,
        )
        .expect("resume phase");

    let committed = scheduler
        .commit_migration("actor-1", commit_intent.sequence)
        .expect("commit should succeed");
    let event_count = scheduler.events().len();
    assert_eq!(
        scheduler
            .commit_migration("actor-1", commit_intent.sequence)
            .expect("duplicate commit should replay"),
        committed
    );
    assert_eq!(scheduler.events().len(), event_count);

    let rollback_intent = scheduler
        .request_migration("actor-2", "node-b", "node-a", true)
        .expect("rollback migration should request");
    let rolled_back = scheduler
        .rollback_migration("actor-2", rollback_intent.sequence, "snapshot_failed")
        .expect("rollback should succeed");
    let event_count = scheduler.events().len();
    assert_eq!(
        scheduler
            .rollback_migration("actor-2", rollback_intent.sequence, "snapshot_failed")
            .expect("duplicate rollback should replay"),
        rolled_back
    );
    assert_eq!(scheduler.events().len(), event_count);

    let incompatible = scheduler
        .rollback_migration("actor-2", rollback_intent.sequence, "different_reason")
        .expect_err("different retry outcome should fail");
    assert_eq!(
        incompatible,
        "error[vm_distributed_scheduler]: migration for actor `actor-2` sequence `2` already completed with incompatible outcome `RolledBack { sequence: 2, reason: \"snapshot_failed\" }`"
    );
}

#[test]
fn vm_distributed_scheduler_emits_typed_migration_lifecycle_events() {
    let mut scheduler = two_node_scheduler();
    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("migration should request");

    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Snapshotting)
        .expect("snapshot phase");
    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Transferring)
        .expect("transfer phase");
    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Resuming)
        .expect("resume phase");
    scheduler
        .commit_migration("actor-1", intent.sequence)
        .expect("commit should succeed");

    let events = scheduler.events_after(0);
    assert_eq!(events.len(), 5);
    assert_eq!(
        events[0].kind,
        VmSchedulerEventKind::MigrationRequested {
            from_node_id: "node-a".to_string(),
            to_node_id: "node-b".to_string(),
            migration_sequence: 1,
            stateful: true,
        }
    );
    assert_eq!(
        events[1].kind,
        VmSchedulerEventKind::MigrationPhaseAdvanced {
            migration_sequence: 1,
            phase: VmMigrationPhase::Snapshotting,
        }
    );
    assert_eq!(
        events[2].kind,
        VmSchedulerEventKind::MigrationPhaseAdvanced {
            migration_sequence: 1,
            phase: VmMigrationPhase::Transferring,
        }
    );
    assert_eq!(
        events[3].kind,
        VmSchedulerEventKind::MigrationPhaseAdvanced {
            migration_sequence: 1,
            phase: VmMigrationPhase::Resuming,
        }
    );
    assert_eq!(
        events[4].kind,
        VmSchedulerEventKind::MigrationCommitted {
            migration_sequence: 1,
        }
    );
    assert!(events
        .windows(2)
        .all(|pair| pair[0].event_sequence < pair[1].event_sequence));
}

#[test]
fn vm_distributed_scheduler_rejects_invalid_migration_requests_and_reasons() {
    let mut scheduler = two_node_scheduler();

    let same_node_error = scheduler
        .request_migration("actor-1", "node-a", "node-a", false)
        .expect_err("same-node migration should fail");
    assert_eq!(
        same_node_error,
        "error[vm_distributed_scheduler]: migration source and target must differ"
    );
    let inactive_target_error = scheduler
        .request_migration("actor-1", "node-a", "node-c", false)
        .expect_err("inactive target should fail");
    assert_eq!(
        inactive_target_error,
        "error[vm_distributed_scheduler]: migration target node `node-c` is not active"
    );
    let missing_error = scheduler
        .commit_migration("actor-missing", 1)
        .expect_err("missing migration should fail");
    assert_eq!(
        missing_error,
        "error[vm_distributed_scheduler]: actor `actor-missing` has no in-flight migration"
    );
    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", false)
        .expect("migration should request");
    let reason_error = scheduler
        .rollback_migration("actor-1", intent.sequence, "")
        .expect_err("empty reason should fail");
    assert_eq!(
        reason_error,
        "error[vm_distributed_scheduler]: migration outcome reason must be non-empty"
    );
}

#[test]
fn vm_distributed_scheduler_rejects_invalid_scheduling_limits() {
    let zero_in_flight = VmDistributedScheduler::from_membership_with_limits(
        [node("node-a", VmClusterNodeState::Active)],
        VmSchedulingLimits {
            max_in_flight_migrations: 0,
            max_migrations_per_tick: 1,
            min_migration_interval_ticks: 0,
        },
    )
    .expect_err("zero in-flight limit should fail");
    assert_eq!(
        zero_in_flight,
        "error[vm_distributed_scheduler]: max in-flight migrations must be non-zero"
    );

    let zero_per_tick = VmDistributedScheduler::from_membership_with_limits(
        [node("node-a", VmClusterNodeState::Active)],
        VmSchedulingLimits {
            max_in_flight_migrations: 1,
            max_migrations_per_tick: 0,
            min_migration_interval_ticks: 0,
        },
    )
    .expect_err("zero per-tick limit should fail");
    assert_eq!(
        zero_per_tick,
        "error[vm_distributed_scheduler]: max migrations per tick must be non-zero"
    );
}

#[test]
fn vm_distributed_scheduler_enforces_max_in_flight_migration_limit() {
    let mut scheduler = limited_scheduler(VmSchedulingLimits {
        max_in_flight_migrations: 1,
        max_migrations_per_tick: 10,
        min_migration_interval_ticks: 0,
    });

    let first = scheduler
        .request_migration_at_tick("actor-1", "node-a", "node-b", true, 1)
        .expect("first migration should request");
    assert_eq!(scheduler.in_flight_migration_count(), 1);
    let limit_error = scheduler
        .request_migration_at_tick("actor-2", "node-a", "node-b", false, 2)
        .expect_err("in-flight limit should fail");
    assert_eq!(
        limit_error,
        "error[vm_distributed_scheduler]: in-flight migration limit `1` reached"
    );

    scheduler
        .rollback_migration("actor-1", first.sequence, "rebalance_cancelled")
        .expect("rollback clears in-flight slot");
    scheduler
        .request_migration_at_tick("actor-2", "node-a", "node-b", false, 2)
        .expect("new migration should fit after rollback");
}

#[test]
fn vm_distributed_scheduler_enforces_tick_caps_intervals_and_monotonic_ticks() {
    let mut per_tick = limited_scheduler(VmSchedulingLimits {
        max_in_flight_migrations: 10,
        max_migrations_per_tick: 1,
        min_migration_interval_ticks: 0,
    });
    per_tick
        .request_migration_at_tick("actor-1", "node-a", "node-b", true, 5)
        .expect("first migration should request");
    let per_tick_error = per_tick
        .request_migration_at_tick("actor-2", "node-a", "node-b", false, 5)
        .expect_err("per-tick cap should fail");
    assert_eq!(
        per_tick_error,
        "error[vm_distributed_scheduler]: migration tick `5` reached per-tick limit `1`"
    );

    let mut interval = limited_scheduler(VmSchedulingLimits {
        max_in_flight_migrations: 10,
        max_migrations_per_tick: 10,
        min_migration_interval_ticks: 2,
    });
    interval
        .request_migration_at_tick("actor-1", "node-a", "node-b", true, 5)
        .expect("first migration should request");
    let interval_error = interval
        .request_migration_at_tick("actor-2", "node-a", "node-b", false, 6)
        .expect_err("minimum interval should fail");
    assert_eq!(
        interval_error,
        "error[vm_distributed_scheduler]: migration tick `6` violates minimum interval `2` after tick `5`"
    );
    interval
        .request_migration_at_tick("actor-2", "node-a", "node-b", false, 7)
        .expect("minimum interval should pass");
    let stale_tick_error = interval
        .request_migration_at_tick("actor-3", "node-a", "node-b", false, 6)
        .expect_err("stale tick should fail");
    assert_eq!(
        stale_tick_error,
        "error[vm_distributed_scheduler]: migration tick `6` is older than last migration tick `7`"
    );
}

#[test]
fn vm_distributed_scheduler_rolls_back_timed_out_migration_with_failure_envelope() {
    let mut scheduler = two_node_scheduler();
    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("migration should request");
    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Snapshotting)
        .expect("snapshot phase should advance");

    assert_eq!(
        scheduler
            .timeout_migration_at_tick("actor-1", intent.sequence, 20, "snapshot_timeout")
            .expect("timeout should roll back"),
        VmMigrationOutcome::RolledBack {
            sequence: intent.sequence,
            reason: "snapshot_timeout".to_string(),
        }
    );
    assert_eq!(scheduler.migration_intent("actor-1"), None);
    assert_eq!(
        scheduler.failure_envelopes_after(19),
        vec![failure_envelope(
            "node-b",
            20,
            VmDistributedFailureKind::MigrationTimeout {
                actor_id: "actor-1".to_string(),
                migration_sequence: intent.sequence,
                phase: VmMigrationPhase::Snapshotting,
            },
            "snapshot_timeout",
        )]
    );
    assert_eq!(scheduler.failure_envelopes_after(20), Vec::new());
    assert_eq!(
        scheduler.events().last().map(|event| &event.kind),
        Some(&VmSchedulerEventKind::MigrationRolledBack {
            migration_sequence: intent.sequence,
            reason: "snapshot_timeout".to_string(),
        })
    );
}

#[test]
fn vm_distributed_scheduler_replays_duplicate_timeout_rollbacks_without_duplicate_envelopes() {
    let mut scheduler = two_node_scheduler();
    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("migration should request");
    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Snapshotting)
        .expect("snapshot phase should advance");

    let rolled_back = scheduler
        .timeout_migration_at_tick("actor-1", intent.sequence, 20, "snapshot_timeout")
        .expect("timeout should roll back");
    let event_count = scheduler.events().len();
    let envelope_count = scheduler.failure_envelopes_after(0).len();

    assert_eq!(
        scheduler
            .timeout_migration_at_tick("actor-1", intent.sequence, 20, "snapshot_timeout")
            .expect("duplicate timeout should replay"),
        rolled_back
    );
    assert_eq!(scheduler.events().len(), event_count);
    assert_eq!(scheduler.failure_envelopes_after(0).len(), envelope_count);

    let incompatible = scheduler
        .timeout_migration_at_tick("actor-1", intent.sequence, 21, "late_timeout")
        .expect_err("different timeout retry should fail");
    assert_eq!(
        incompatible,
        "error[vm_distributed_scheduler]: migration for actor `actor-1` sequence `1` already completed with incompatible outcome `RolledBack { sequence: 1, reason: \"snapshot_timeout\" }`"
    );
}

#[test]
fn vm_distributed_scheduler_rolls_back_storage_timeout_idempotently() {
    let mut scheduler = two_node_scheduler();
    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("migration should request");
    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Snapshotting)
        .expect("snapshot phase should advance");

    let mut storage = VmDistributedStorageAdapter::new(VmDistributedStoragePolicy::force_local());
    assert!(matches!(
        storage.open(),
        VmDistributedStorageOutcome::Opened { .. }
    ));
    assert!(matches!(
        storage.append(snapshot(
            "actor-1.snapshot",
            intent.sequence,
            vec![state_entry("actor-1", "phase", 1)]
        )),
        VmDistributedStorageOutcome::Appended { .. }
    ));
    storage.timeout_next_flush_for_test();
    let storage_failure = storage.flush();

    assert_eq!(
        storage_failure,
        VmDistributedStorageOutcome::FlushTimedOut {
            operation: VmDistributedStorageOperation::Flush,
            sequence: intent.sequence,
        }
    );
    assert!(storage_failure.requires_recovery());
    assert_eq!(storage_failure.recovery_action(), "retry_flush");

    let reason = storage_failure.kind();
    let rolled_back = scheduler
        .timeout_migration_at_tick("actor-1", intent.sequence, 40, reason)
        .expect("storage timeout should roll back migration");
    let event_count = scheduler.events().len();
    let envelope_count = scheduler.failure_envelopes_after(0).len();

    assert_eq!(
        rolled_back,
        VmMigrationOutcome::RolledBack {
            sequence: intent.sequence,
            reason: "flush_timed_out".to_string(),
        }
    );
    assert_eq!(
        scheduler.failure_envelopes_after(39),
        vec![failure_envelope(
            "node-b",
            40,
            VmDistributedFailureKind::MigrationTimeout {
                actor_id: "actor-1".to_string(),
                migration_sequence: intent.sequence,
                phase: VmMigrationPhase::Snapshotting,
            },
            "flush_timed_out",
        )]
    );

    assert_eq!(
        scheduler
            .timeout_migration_at_tick("actor-1", intent.sequence, 40, reason)
            .expect("duplicate storage timeout should replay rollback"),
        rolled_back
    );
    assert_eq!(scheduler.events().len(), event_count);
    assert_eq!(scheduler.failure_envelopes_after(0).len(), envelope_count);
}

#[test]
fn vm_distributed_scheduler_rolls_back_storage_partial_write_idempotently() {
    let mut scheduler = two_node_scheduler();
    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("migration should request");
    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Snapshotting)
        .expect("snapshot phase should advance");

    let mut storage = VmDistributedStorageAdapter::new(VmDistributedStoragePolicy::force_local());
    assert!(matches!(
        storage.open(),
        VmDistributedStorageOutcome::Opened { .. }
    ));
    storage.set_partial_write_limit_for_test(0);
    let storage_failure = storage.append(snapshot(
        "actor-1.snapshot",
        intent.sequence,
        vec![state_entry("actor-1", "phase", 1)],
    ));

    assert_eq!(
        storage_failure,
        VmDistributedStorageOutcome::PartialWrite {
            operation: VmDistributedStorageOperation::Append,
            checkpoint_id: "actor-1.snapshot".to_string(),
            sequence: intent.sequence,
            expected_entries: 1,
            persisted_entries: 0,
        }
    );
    assert!(storage_failure.requires_recovery());
    assert_eq!(storage_failure.recovery_action(), "rewrite_checkpoint");

    let reason = storage_failure.kind();
    let rolled_back = scheduler
        .partial_commit_migration_at_tick("actor-1", intent.sequence, 41, reason)
        .expect("partial storage write should roll back migration");
    let event_count = scheduler.events().len();
    let envelope_count = scheduler.failure_envelopes_after(0).len();

    assert_eq!(
        rolled_back,
        VmMigrationOutcome::RolledBack {
            sequence: intent.sequence,
            reason: "partial_write".to_string(),
        }
    );
    assert_eq!(
        scheduler.failure_envelopes_after(40),
        vec![failure_envelope(
            "node-b",
            41,
            VmDistributedFailureKind::MigrationPartialCommit {
                actor_id: "actor-1".to_string(),
                migration_sequence: intent.sequence,
                phase: VmMigrationPhase::Snapshotting,
            },
            "partial_write",
        )]
    );

    assert_eq!(
        scheduler
            .partial_commit_migration_at_tick("actor-1", intent.sequence, 41, reason)
            .expect("duplicate partial write rollback should replay"),
        rolled_back
    );
    assert_eq!(scheduler.events().len(), event_count);
    assert_eq!(scheduler.failure_envelopes_after(0).len(), envelope_count);
}

#[test]
fn vm_distributed_scheduler_rolls_back_partial_commit_with_failure_envelope() {
    let mut scheduler = two_node_scheduler();
    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("migration should request");
    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Snapshotting)
        .expect("snapshot phase should advance");

    assert_eq!(
        scheduler
            .partial_commit_migration_at_tick("actor-1", intent.sequence, 30, "partial_commit")
            .expect("partial commit should roll back"),
        VmMigrationOutcome::RolledBack {
            sequence: intent.sequence,
            reason: "partial_commit".to_string(),
        }
    );
    assert_eq!(scheduler.migration_intent("actor-1"), None);
    assert_eq!(
        scheduler.failure_envelopes_after(29),
        vec![failure_envelope(
            "node-b",
            30,
            VmDistributedFailureKind::MigrationPartialCommit {
                actor_id: "actor-1".to_string(),
                migration_sequence: intent.sequence,
                phase: VmMigrationPhase::Snapshotting,
            },
            "partial_commit",
        )]
    );
    assert_eq!(
        scheduler.events().last().map(|event| &event.kind),
        Some(&VmSchedulerEventKind::MigrationRolledBack {
            migration_sequence: intent.sequence,
            reason: "partial_commit".to_string(),
        })
    );
}

#[test]
fn vm_distributed_scheduler_replays_duplicate_partial_commit_rollbacks_without_duplicate_envelopes()
{
    let mut scheduler = two_node_scheduler();
    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("migration should request");
    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Snapshotting)
        .expect("snapshot phase should advance");

    let rolled_back = scheduler
        .partial_commit_migration_at_tick("actor-1", intent.sequence, 30, "partial_commit")
        .expect("partial commit should roll back");
    let event_count = scheduler.events().len();
    let envelope_count = scheduler.failure_envelopes_after(0).len();

    assert_eq!(
        scheduler
            .partial_commit_migration_at_tick("actor-1", intent.sequence, 30, "partial_commit")
            .expect("duplicate partial commit should replay"),
        rolled_back
    );
    assert_eq!(scheduler.events().len(), event_count);
    assert_eq!(scheduler.failure_envelopes_after(0).len(), envelope_count);
}

#[test]
fn vm_distributed_scheduler_rejects_invalid_partial_commit_inputs() {
    let mut scheduler = two_node_scheduler();
    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("migration should request");

    let wrong_sequence = scheduler
        .partial_commit_migration_at_tick("actor-1", intent.sequence + 1, 30, "partial_commit")
        .expect_err("wrong sequence should fail");
    assert_eq!(
        wrong_sequence,
        "error[vm_distributed_scheduler]: migration outcome sequence `2` does not match expected `1` for actor `actor-1`"
    );
    let empty_reason = scheduler
        .partial_commit_migration_at_tick("actor-1", intent.sequence, 30, "")
        .expect_err("empty reason should fail");
    assert_eq!(
        empty_reason,
        "error[vm_distributed_scheduler]: failure envelope reason must be non-empty"
    );

    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Snapshotting)
        .expect("snapshot phase");
    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Transferring)
        .expect("transfer phase");
    scheduler
        .advance_migration("actor-1", intent.sequence, VmMigrationPhase::Resuming)
        .expect("resume phase");
    let resumed = scheduler
        .partial_commit_migration_at_tick("actor-1", intent.sequence, 30, "late_partial_commit")
        .expect_err("resumed migration should not partial-commit fail");
    assert_eq!(
        resumed,
        "error[vm_distributed_scheduler]: migration for actor `actor-1` sequence `1` already reached `Resuming` and cannot be marked as a partial commit"
    );
}

#[test]
fn vm_distributed_scheduler_rejects_invalid_migration_timeout_inputs() {
    let mut scheduler = two_node_scheduler();
    let intent = scheduler
        .request_migration("actor-1", "node-a", "node-b", true)
        .expect("migration should request");

    let wrong_sequence = scheduler
        .timeout_migration_at_tick("actor-1", intent.sequence + 1, 20, "snapshot_timeout")
        .expect_err("wrong sequence should fail");
    assert_eq!(
        wrong_sequence,
        "error[vm_distributed_scheduler]: migration outcome sequence `2` does not match expected `1` for actor `actor-1`"
    );

    let empty_reason = scheduler
        .timeout_migration_at_tick("actor-1", intent.sequence, 20, "")
        .expect_err("empty timeout reason should fail");
    assert_eq!(
        empty_reason,
        "error[vm_distributed_scheduler]: failure envelope reason must be non-empty"
    );

    scheduler
        .abort_migration("actor-1", intent.sequence, "operator_abort")
        .expect("abort clears migration");
    let missing_migration = scheduler
        .timeout_migration_at_tick("actor-1", intent.sequence, 21, "late_timeout")
        .expect_err("timeout after terminal outcome should fail");
    assert_eq!(
        missing_migration,
        "error[vm_distributed_scheduler]: migration for actor `actor-1` sequence `1` already completed with incompatible outcome `Aborted { sequence: 1, reason: \"operator_abort\" }`"
    );
}
