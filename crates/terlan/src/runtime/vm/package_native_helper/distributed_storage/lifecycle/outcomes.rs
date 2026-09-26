//! Source-visible outcomes retain typed conflict data, not diagnostic text.

use super::*;

#[cfg(test)]
#[path = "outcomes_test.rs"]
mod tests;

pub(in super::super) struct Outcome {
    pub(super) kind: &'static str,
    pub(super) sequence: i64,
    pub(super) snapshot: Option<Checkpoint>,
    pub(super) operation: &'static str,
    pub(super) mode: &'static str,
    pub(super) failure: Option<StorageFailure>,
    pub(super) recovery: &'static str,
    pub(super) retained: i64,
    pub(super) checkpoint_id: String,
    pub(super) checksum: u32,
    pub(super) schema: u32,
    stale_snapshot: Option<(i64, i64)>,
    pub(super) missing_resource: String,
    pub(super) validated_resources: i64,
}

impl Outcome {
    /// Accounts loaded immutable copies separately from their store and snapshot handles.
    pub(super) fn budget_bytes(&self) -> usize {
        self.checkpoint_id.len()
            + self.missing_resource.len()
            + self.snapshot.as_ref().map_or(0, |checkpoint| {
                checkpoint.id.len() + checkpoint.payload.len()
            })
    }

    pub(super) fn new(
        kind: &'static str,
        operation: &'static str,
        mode: &'static str,
        sequence: i64,
    ) -> Self {
        Self {
            kind,
            operation,
            mode,
            sequence,
            snapshot: None,
            failure: None,
            recovery: "",
            retained: 0,
            checkpoint_id: String::new(),
            checksum: 0,
            schema: 0,
            stale_snapshot: None,
            missing_resource: String::new(),
            validated_resources: 0,
        }
    }

    pub(super) fn failed(&self) -> bool {
        self.failure.is_some()
            || matches!(
                self.kind,
                "snapshot_missing"
                    | "storage_unavailable"
                    | "unsupported"
                    | "storage_identity_mismatch"
                    | "resource_handle_validation_failed"
            )
    }

    /// A rejected old checkpoint is distinct from a stale writer attempting a newer one.
    pub(super) fn project_append_failure(&mut self, incoming: i64) -> VmRuntimeResult<()> {
        let Some(StorageFailure::Sequence { actual, .. }) = self.failure else {
            return Ok(());
        };
        let actual = i64::try_from(actual).map_err(|_| invalid_reply())?;
        if incoming <= 0 {
            return Err(invalid_reply());
        }
        if incoming <= actual && self.operation != "compare_and_swap_append" {
            self.kind = "stale_snapshot";
            self.recovery = "reject_replay";
            self.sequence = incoming;
            self.stale_snapshot = Some((actual, incoming));
        }
        Ok(())
    }

    pub(super) fn failure(
        failure: StorageFailure,
        operation: &'static str,
        mode: &'static str,
        sequence: i64,
    ) -> Self {
        let (kind, recovery) = match failure {
            StorageFailure::Schema { .. } => ("schema_migration_mismatch", "reload_schema"),
            StorageFailure::Sequence { .. } => ("cas_token_mismatch", "reload_snapshot"),
            StorageFailure::Conflict => ("checkpoint_conflict", "inspect_checkpoint"),
            StorageFailure::Corrupt => ("checksum_mismatch", "repair_snapshot"),
            StorageFailure::Checksum { .. } => ("checksum_mismatch", "repair_snapshot"),
            StorageFailure::Busy => ("storage_busy", "retry_flush"),
            StorageFailure::Database => ("commit_indeterminate", "reconcile_commit"),
            StorageFailure::Invalid => ("invalid_request", ""),
            StorageFailure::Unavailable => ("storage_unavailable", "retry_open"),
            StorageFailure::Incompatible => ("storage_incompatible", "inspect_storage_format"),
        };
        let mut outcome = Self::new(kind, operation, mode, sequence);
        outcome.failure = Some(failure);
        if let StorageFailure::Checksum { actual, .. } = failure {
            outcome.checksum = actual;
        }
        outcome.recovery = recovery;
        outcome
    }
}

impl VmDistributedStorageRuntime {
    /// Projects outcomes or a locally denied operation without starting a worker call.
    pub(super) fn local_outcome(
        &mut self,
        owner: u64,
        operation: &str,
        args: &[ReplValue],
    ) -> VmRuntimeResult<Option<ReplValue>> {
        if let Some(name) = operations::operation_name(operation) {
            let Some(value) = args.first() else {
                return Err("error[vm.distributed_storage.arguments]: adapter required".into());
            };
            let Resource::Adapter(adapter) = self.resource(owner, value, "Adapter")? else {
                unreachable!("checked kind")
            };
            let outcome = if !adapter.policy.available
                || !self.bindings.contains_key(&adapter.policy.name)
                || (operation != "open" && !adapter.open)
            {
                Outcome::new(
                    "storage_unavailable",
                    name,
                    adapter.policy.mode,
                    adapter.sequence,
                )
            } else {
                return Err("error[vm.distributed_storage.dispatch]: durable operation requires the asynchronous owner dispatcher".into());
            };
            return self.insert(owner, Resource::Outcome(outcome)).map(Some);
        }
        if !matches!(
            operation,
            "kind"
                | "sequence"
                | "is_success"
                | "is_failure"
                | "loaded_snapshot"
                | "operation_kind"
                | "mode_kind"
                | "reason"
                | "requires_recovery"
                | "recovery_action"
                | "expected_sequence"
                | "actual_sequence"
                | "expected_schema"
                | "actual_schema"
                | "retained_snapshots"
                | "checkpoint_id"
                | "checksum"
                | "expected_checksum"
                | "local_sequence"
                | "incoming_sequence"
                | "expected_entries"
                | "persisted_entries"
                | "missing_resource_handle"
                | "validated_resource_count"
        ) {
            return Ok(None);
        }
        let [value] = args else {
            return Err("error[vm.distributed_storage.arguments]: expected one outcome".into());
        };
        let Resource::Outcome(outcome) = self.resource(owner, value, "Outcome")? else {
            unreachable!("checked kind")
        };
        let value = match operation {
            "missing_resource_handle" => ReplValue::String(outcome.missing_resource.clone()),
            "validated_resource_count" => ReplValue::Int(outcome.validated_resources),
            "kind" => ReplValue::String(outcome.kind.into()),
            "sequence" => ReplValue::Int(outcome.sequence),
            "is_success" => ReplValue::Bool(!outcome.failed()),
            "is_failure" => ReplValue::Bool(outcome.failed()),
            "operation_kind" => ReplValue::String(outcome.operation.into()),
            "mode_kind" => ReplValue::String(outcome.mode.into()),
            "reason" => ReplValue::String(
                match outcome.kind {
                    "unsupported" => "unsupported_operation",
                    _ if outcome.failed() => outcome.kind,
                    _ => "",
                }
                .into(),
            ),
            "requires_recovery" => ReplValue::Bool(!outcome.recovery.is_empty()),
            "recovery_action" => ReplValue::String(outcome.recovery.into()),
            "expected_sequence" => ReplValue::Int(match outcome.failure {
                Some(StorageFailure::Sequence { expected, .. })
                    if outcome.stale_snapshot.is_none() =>
                {
                    i64::try_from(expected).map_err(|_| invalid_reply())?
                }
                _ => 0,
            }),
            "actual_sequence" => ReplValue::Int(match outcome.failure {
                Some(StorageFailure::Sequence { actual, .. })
                    if outcome.stale_snapshot.is_none() =>
                {
                    i64::try_from(actual).map_err(|_| invalid_reply())?
                }
                _ => 0,
            }),
            "expected_schema" => ReplValue::Int(match outcome.failure {
                Some(StorageFailure::Schema { expected, .. }) => i64::from(expected),
                _ => 0,
            }),
            "actual_schema" => ReplValue::Int(match outcome.failure {
                Some(StorageFailure::Schema { actual, .. }) => i64::from(actual),
                _ => i64::from(outcome.schema),
            }),
            "retained_snapshots" => ReplValue::Int(outcome.retained),
            "local_sequence" => {
                ReplValue::Int(outcome.stale_snapshot.map_or(0, |(local, _)| local))
            }
            "incoming_sequence" => {
                ReplValue::Int(outcome.stale_snapshot.map_or(0, |(_, incoming)| incoming))
            }
            "expected_entries" | "persisted_entries" => {
                // Local/SQLite writes are atomic, and an unknown commit is not a
                // measured partial write. Never invent entry counts from either.
                if outcome.kind == "partial_write" {
                    return Err("error[vm.distributed_storage.receipt]: partial-write counts require acknowledged persistence evidence".into());
                }
                ReplValue::Int(0)
            }
            "checkpoint_id" => ReplValue::String(outcome.checkpoint_id.clone()),
            "checksum" => ReplValue::Int(i64::from(outcome.checksum)),
            "expected_checksum" => ReplValue::Int(match outcome.failure {
                Some(StorageFailure::Checksum { expected, .. }) => i64::from(expected),
                _ => 0,
            }),
            _ => {
                let checkpoint = outcome.snapshot.as_ref().ok_or(
                    "error[vm.distributed_storage.snapshot]: outcome has no loaded checkpoint",
                )?;
                self.budget.check(
                    owner,
                    std::mem::size_of::<Resource>()
                        + 64
                        + checkpoint.id.len()
                        + checkpoint.payload.len(),
                )?;
                let snapshot = checkpoint.clone();
                return self.insert(owner, Resource::Snapshot(snapshot)).map(Some);
            }
        };
        Ok(Some(value))
    }
}
