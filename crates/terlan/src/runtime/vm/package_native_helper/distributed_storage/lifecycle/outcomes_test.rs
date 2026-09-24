//! Metadata comes from the backend observation, never the adapter's stale cache.

use super::*;

fn field(runtime: &mut VmDistributedStorageRuntime, value: &ReplValue, name: &str) -> ReplValue {
    runtime
        .local_outcome(1, name, std::slice::from_ref(value))
        .unwrap()
        .unwrap()
}

#[test]
fn stale_snapshots_and_stale_cas_tokens_keep_distinct_metadata() {
    for (operation, incoming, stale) in [
        ("append", 5, true),
        ("append", 7, true),
        ("append", 9, false),
        ("transactional_batch_append", 5, true),
        ("compare_and_swap_append", 5, false),
        ("compare_and_swap_append", 9, false),
    ] {
        let mut runtime = VmDistributedStorageRuntime::default();
        let mut outcome = Outcome::failure(
            StorageFailure::Sequence {
                expected: 3,
                actual: 7,
            },
            operation,
            "durable",
            3,
        );
        outcome.project_append_failure(incoming).unwrap();
        let value = runtime.insert(1, Resource::Outcome(outcome)).unwrap();
        assert_eq!(
            field(&mut runtime, &value, "local_sequence"),
            ReplValue::Int(if stale { 7 } else { 0 })
        );
        assert_eq!(
            field(&mut runtime, &value, "incoming_sequence"),
            ReplValue::Int(if stale { incoming } else { 0 })
        );
        assert_eq!(
            field(&mut runtime, &value, "expected_sequence"),
            ReplValue::Int(if stale { 0 } else { 3 })
        );
        assert_eq!(
            field(&mut runtime, &value, "actual_sequence"),
            ReplValue::Int(if stale { 0 } else { 7 })
        );
        assert_eq!(
            field(&mut runtime, &value, "kind"),
            ReplValue::String(
                if stale {
                    "stale_snapshot"
                } else {
                    "cas_token_mismatch"
                }
                .into()
            )
        );
        assert_eq!(
            field(&mut runtime, &value, "recovery_action"),
            ReplValue::String(
                if stale {
                    "reject_replay"
                } else {
                    "reload_snapshot"
                }
                .into()
            )
        );
        assert_eq!(
            field(&mut runtime, &value, "is_failure"),
            ReplValue::Bool(true)
        );
        assert_eq!(
            field(&mut runtime, &value, "sequence"),
            ReplValue::Int(if stale { incoming } else { 3 })
        );
    }
}

#[test]
fn atomic_or_indeterminate_outcomes_never_invent_partial_progress() {
    let mut runtime = VmDistributedStorageRuntime::default();
    for failure in [
        StorageFailure::Invalid,
        StorageFailure::Conflict,
        StorageFailure::Database,
    ] {
        let mut outcome = Outcome::failure(failure, "append", "durable", 7);
        outcome.project_append_failure(5).unwrap();
        let value = runtime.insert(1, Resource::Outcome(outcome)).unwrap();
        for name in [
            "expected_entries",
            "persisted_entries",
            "local_sequence",
            "incoming_sequence",
        ] {
            assert_eq!(field(&mut runtime, &value, name), ReplValue::Int(0));
            assert!(runtime
                .local_outcome(2, name, std::slice::from_ref(&value))
                .is_err());
        }
        if failure == StorageFailure::Database {
            assert_eq!(
                field(&mut runtime, &value, "kind"),
                ReplValue::String("commit_indeterminate".into())
            );
            assert_eq!(
                field(&mut runtime, &value, "requires_recovery"),
                ReplValue::Bool(true)
            );
        }
    }
    let value = runtime
        .insert(
            1,
            Resource::Outcome(Outcome::new("partial_write", "append", "durable", 7)),
        )
        .unwrap();
    for name in ["expected_entries", "persisted_entries"] {
        assert!(runtime
            .local_outcome(1, name, std::slice::from_ref(&value))
            .unwrap_err()
            .contains("acknowledged persistence evidence"));
    }
}

#[test]
fn rejected_sequence_projection_rejects_invalid_metadata() {
    for (actual, incoming) in [(u64::MAX, 1), (7, 0), (7, -1)] {
        let mut outcome = Outcome::failure(
            StorageFailure::Sequence {
                expected: 3,
                actual,
            },
            "append",
            "durable",
            3,
        );
        assert!(outcome.project_append_failure(incoming).is_err());
    }
}
