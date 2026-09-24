//! Validates public storage operations before selecting a bounded worker request.

use super::*;

pub(super) fn arity(operation: &str) -> usize {
    match operation {
        "append"
        | "load_snapshot"
        | "snapshot_isolation_proof"
        | "compact"
        | "transactional_batch_append" => 2,
        "compare_and_swap_append" | "migrate_schema" => 3,
        _ => 1,
    }
}

/// Closed public operation identities; arbitrary package calls cannot acquire storage authority.
pub(super) fn operation_name(operation: &str) -> Option<&'static str> {
    Some(match operation {
        "open" => "open",
        "append" => "append",
        "flush" => "flush",
        "close" => "close",
        "load_snapshot" => "load_snapshot",
        "snapshot_isolation_proof" => "snapshot_isolation",
        "compact" => "compact",
        "migrate_schema" | "schema_migration_proof" | "require_schema_migration" => {
            "schema_migration"
        }
        "compare_and_swap_append" | "compare_and_swap_token" => "compare_and_swap_append",
        "transactional_batch_append" | "require_transactional_batch" => {
            "transactional_batch_append"
        }
        "atomic_append_proof" | "require_atomic_append" => "atomic_append",
        "require_durable_flush" => "durable_flush",
        "require_snapshot_isolation" => "snapshot_isolation",
        _ => return None,
    })
}

impl VmDistributedStorageRuntime {
    /// Checks all source handles before selecting a worker; denied lifecycle calls stay local.
    pub(in super::super::super) fn prepare(
        &self,
        owner: u64,
        request: &PureNativeCapabilityRequest,
    ) -> VmRuntimeResult<Option<Request>> {
        let operation = request
            .operation
            .strip_prefix("std.vm.distributed_storage.")
            .unwrap_or("");
        if resource_validation::is_operation(operation) {
            let args = request.package_arguments.as_deref().ok_or(
                "error[vm.distributed_storage.arguments]: typed resource arguments required",
            )?;
            return self.prepare_resources(owner, operation, args);
        }
        let Some(name) = operation_name(operation) else {
            return Ok(None);
        };
        let args = request
            .package_arguments
            .as_deref()
            .ok_or("error[vm.distributed_storage.arguments]: typed arguments required")?;
        if args.len() != arity(operation) {
            return Err(
                "error[vm.distributed_storage.arguments]: invalid storage operation arity".into(),
            );
        }
        let handle = self.handle(owner, &args[0], "Adapter")?;
        let Resource::Adapter(adapter) = self.resource(owner, &args[0], "Adapter")? else {
            unreachable!("checked kind")
        };
        if adapter.policy.mode == "local_only" {
            return Ok(None);
        }
        if !adapter.policy.available
            || !self.bindings.contains_key(&adapter.policy.name)
            || (operation != "open" && !adapter.open)
        {
            if matches!(
                operation,
                "atomic_append_proof"
                    | "schema_migration_proof"
                    | "compare_and_swap_token"
                    | "snapshot_isolation_proof"
            ) {
                return Err("error[vm.distributed_storage.unavailable]: proof requires an open authorized adapter".into());
            }
            return Ok(None);
        }
        let (wire, arguments, operation) = match (operation, args) {
            ("open", [_]) => (
                "runtime.storage.open",
                vec![],
                Operation::Status(StatusProjection::Open),
            ),
            ("atomic_append_proof", [_]) => (
                "runtime.storage.status",
                vec![],
                Operation::Status(StatusProjection::Atomic),
            ),
            ("schema_migration_proof", [_]) => (
                "runtime.storage.status",
                vec![],
                Operation::Status(StatusProjection::Schema),
            ),
            ("compare_and_swap_token", [_]) => (
                "runtime.storage.status",
                vec![],
                Operation::Status(StatusProjection::Cas),
            ),
            (
                "require_atomic_append"
                | "require_schema_migration"
                | "require_transactional_batch"
                | "require_durable_flush"
                | "require_snapshot_isolation",
                [_],
            ) => (
                "runtime.storage.status",
                vec![],
                Operation::Status(StatusProjection::Require),
            ),
            ("append", [_, snapshot]) => self.append_request(owner, adapter, &[snapshot], None)?,
            ("compare_and_swap_append", [_, snapshot, token]) => {
                let Resource::Proof(Proof::Cas {
                    adapter: token_adapter,
                    sequence,
                }) = self.resource(owner, token, "CompareAndSwapToken")?
                else {
                    unreachable!("checked kind")
                };
                if *token_adapter != handle {
                    return Err(
                        "error[vm.distributed_storage.token]: CAS token belongs to another adapter"
                            .into(),
                    );
                }
                self.append_request(owner, adapter, &[snapshot], Some(*sequence))?
            }
            ("transactional_batch_append", [_, ReplValue::List(snapshots)]) => {
                if snapshots.is_empty() || snapshots.len() > terlan_storage::MAX_BATCH_CHECKPOINTS {
                    return Err("error[vm.distributed_storage.batch]: batch must contain 1..=1024 checkpoints".into());
                }
                self.append_request(owner, adapter, &snapshots.iter().collect::<Vec<_>>(), None)?
            }
            ("flush", [_]) => ("runtime.storage.flush", vec![], Operation::Flush),
            ("close", [_]) => ("runtime.storage.flush", vec![], Operation::Close),
            (operation @ ("load_snapshot" | "snapshot_isolation_proof"), [_, id]) => {
                let id = text(id)?.to_string();
                (
                    "runtime.storage.load",
                    vec![Term::Text(id.clone())],
                    Operation::Load {
                        id,
                        proof: operation == "snapshot_isolation_proof",
                    },
                )
            }
            ("compact", [_, ReplValue::Int(boundary)]) => (
                "runtime.storage.compact",
                vec![Term::Int(*boundary)],
                Operation::Compact,
            ),
            ("migrate_schema", [_, ReplValue::Int(expected), ReplValue::Int(next)]) => (
                "runtime.storage.migrate_schema",
                vec![Term::Int(*expected), Term::Int(*next)],
                Operation::Migrate { next: *next },
            ),
            _ => {
                return Err(
                    "error[vm.distributed_storage.arguments]: invalid storage operation arguments"
                        .into(),
                )
            }
        };
        Ok(Some(Request {
            backend: adapter.policy.name.clone(),
            operation: wire,
            arguments,
            pending: Pending {
                adapter: handle,
                operation,
                name,
            },
        }))
    }

    /// Validates ownership and total allocation before copying any checkpoint payload.
    fn append_request(
        &self,
        owner: u64,
        adapter: &Adapter,
        snapshots: &[&ReplValue],
        expected: Option<i64>,
    ) -> VmRuntimeResult<(&'static str, Vec<Term>, Operation)> {
        let checkpoints = snapshots
            .iter()
            .map(|value| self.snapshot(owner, value))
            .collect::<VmRuntimeResult<Vec<_>>>()?;
        let mut bytes = 0usize;
        for checkpoint in &checkpoints {
            checkpoint.validate().map_err(storage_error)?;
            bytes = bytes
                .checked_add(checkpoint.payload.len())
                .ok_or("error[vm.distributed_storage.batch]: batch size overflow")?;
            if bytes > terlan_storage::MAX_BATCH_BYTES {
                return Err(
                    "error[vm.distributed_storage.batch]: batch payload limit exceeded".into(),
                );
            }
        }
        let first = checkpoints
            .first()
            .ok_or("error[vm.distributed_storage.batch]: empty batch")?
            .sequence as i64;
        let last = checkpoints
            .last()
            .ok_or("error[vm.distributed_storage.batch]: empty batch")?
            .sequence as i64;
        let count = checkpoints.len() as i64;
        let checkpoint = checkpoints
            .last()
            .ok_or("error[vm.distributed_storage.batch]: empty batch")?;
        let checkpoint_id = checkpoint.id.clone();
        let checksum = checkpoint.checksum();
        let values = checkpoints
            .into_iter()
            .map(|checkpoint| {
                Term::Tuple(vec![
                    Term::Text(checkpoint.id.clone()),
                    Term::Int(checkpoint.sequence as i64),
                    Term::Int(i64::from(checkpoint.schema)),
                    Term::Bytes(checkpoint.payload.clone()),
                ])
            })
            .collect();
        Ok((
            "runtime.storage.append",
            vec![
                Term::Int(expected.unwrap_or_else(|| adapter.sequence.min(first - 1))),
                Term::List(values),
            ],
            Operation::Append {
                first,
                last,
                count,
                checkpoint_id,
                checksum,
            },
        ))
    }
}
