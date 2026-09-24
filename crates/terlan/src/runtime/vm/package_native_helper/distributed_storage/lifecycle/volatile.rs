//! Direct owner-thread local storage: no worker, wire encoding, filesystem, or durability.

use super::*;

impl VmDistributedStorageRuntime {
    /// Executes only local-mode adapters; durable requests retain their async dispatcher.
    pub(super) fn volatile_call(
        &mut self,
        owner: u64,
        operation: &str,
        args: &[ReplValue],
    ) -> VmRuntimeResult<Option<ReplValue>> {
        let Some(name) = operations::operation_name(operation) else {
            return Ok(None);
        };
        if args.len() != operations::arity(operation) {
            return Err(
                "error[vm.distributed_storage.arguments]: invalid storage operation arity".into(),
            );
        }
        let handle = self.handle(owner, &args[0], "Adapter")?;
        let Resource::Adapter(adapter) = self.resource(owner, &args[0], "Adapter")? else {
            unreachable!("checked kind")
        };
        if adapter.policy.mode != "local_only" {
            return Ok(None);
        }
        let proof = matches!(
            operation,
            "atomic_append_proof"
                | "schema_migration_proof"
                | "compare_and_swap_token"
                | "snapshot_isolation_proof"
        );
        if !adapter.policy.available || (operation != "open" && !adapter.open) {
            if proof {
                return Err("error[vm.distributed_storage.unavailable]: proof requires an open local adapter".into());
            }
            let outcome = Outcome::new("storage_unavailable", name, "local_only", adapter.sequence);
            return self.insert(owner, Resource::Outcome(outcome)).map(Some);
        }
        // Reserve room for the largest mutating operation's receipt before mutation.
        // Checkpoint IDs are bounded to 1024 bytes by the shared backend contract.
        self.budget
            .check(owner, std::mem::size_of::<Resource>() + 64 + 1024)?;
        let resource = match (operation, args) {
            ("append", [_, snapshot]) => {
                self.volatile_append(owner, handle, &[snapshot], None, name)?
            }
            ("compare_and_swap_append", [_, snapshot, token]) => {
                let Resource::Proof(Proof::Cas { adapter, sequence }) =
                    self.resource(owner, token, "CompareAndSwapToken")?
                else {
                    unreachable!("checked kind")
                };
                if *adapter != handle {
                    return Err(
                        "error[vm.distributed_storage.token]: CAS token belongs to another adapter"
                            .into(),
                    );
                }
                self.volatile_append(owner, handle, &[snapshot], Some(*sequence), name)?
            }
            ("transactional_batch_append", [_, ReplValue::List(snapshots)]) => {
                if snapshots.is_empty() || snapshots.len() > terlan_storage::MAX_BATCH_CHECKPOINTS {
                    Resource::Outcome(Outcome::failure(
                        StorageFailure::Invalid,
                        name,
                        "local_only",
                        adapter.sequence,
                    ))
                } else {
                    self.volatile_append(
                        owner,
                        handle,
                        &snapshots.iter().collect::<Vec<_>>(),
                        None,
                        name,
                    )?
                }
            }
            ("load_snapshot" | "snapshot_isolation_proof", [_, id]) => {
                self.volatile_load(owner, adapter, text(id)?, proof, name)?
            }
            _ => {
                let Resource::Adapter(adapter) = self
                    .resources
                    .get_mut_for_owner(handle, owner)
                    .map_err(resource_error)?
                else {
                    unreachable!("checked kind")
                };
                observe_or_mutate(adapter, handle, operation, name, args)?
            }
        };
        self.insert(owner, resource).map(Some)
    }

    /// Validates owner handles and transient batch bounds before copying any payload.
    fn volatile_append(
        &mut self,
        owner: u64,
        handle: NativeBoundaryHandle,
        snapshots: &[&ReplValue],
        expected: Option<i64>,
        name: &'static str,
    ) -> VmRuntimeResult<Resource> {
        let checkpoints = snapshots
            .iter()
            .map(|value| self.snapshot(owner, value))
            .collect::<VmRuntimeResult<Vec<_>>>()?;
        let bytes = checkpoints
            .iter()
            .try_fold(0usize, |bytes, checkpoint| {
                bytes.checked_add(checkpoint.payload.len() + checkpoint.id.len())
            })
            .ok_or("error[vm.distributed_storage.batch]: batch size overflow")?;
        if bytes > terlan_storage::MAX_BATCH_BYTES {
            return Err("error[vm.distributed_storage.batch]: batch payload limit exceeded".into());
        }
        let checkpoints = checkpoints.into_iter().cloned().collect::<Vec<_>>();
        let first = checkpoints
            .first()
            .ok_or("error[vm.distributed_storage.batch]: empty batch")?
            .sequence as i64;
        let last = checkpoints
            .last()
            .ok_or("error[vm.distributed_storage.batch]: empty batch")?;
        let Resource::Adapter(adapter) = self
            .resources
            .get_mut_for_owner(handle, owner)
            .map_err(resource_error)?
        else {
            unreachable!("checked kind")
        };
        let mut outcome = Outcome::new("appended", name, "local_only", adapter.sequence);
        outcome.checkpoint_id = last.id.clone();
        let expected = expected.unwrap_or_else(|| adapter.sequence.min(first - 1)) as u64;
        let store = adapter
            .local
            .as_mut()
            .ok_or("error[vm.distributed_storage.local]: missing local store")?;
        match store.append(expected, &checkpoints) {
            Ok(_) => {
                adapter.sequence = store.status().sequence as i64;
                outcome.sequence = last.sequence as i64;
                outcome.checksum = last.checksum();
                if name == "transactional_batch_append" {
                    adapter.batch = (first, last.sequence as i64, checkpoints.len() as i64);
                    outcome.kind = "batch_appended";
                }
            }
            Err(error) => {
                outcome = Outcome::failure(error.into(), name, "local_only", adapter.sequence);
                outcome.project_append_failure(first)?;
                outcome.checkpoint_id = last.id.clone();
            }
        }
        Ok(Resource::Outcome(outcome))
    }

    /// Accounts a loaded immutable copy before allocation; compaction cannot reclaim views.
    fn volatile_load(
        &self,
        owner: u64,
        adapter: &Adapter,
        id: &str,
        proof: bool,
        name: &'static str,
    ) -> VmRuntimeResult<Resource> {
        if id.is_empty() || id.len() > 1024 || id.contains('\0') {
            return Err(
                "error[vm.distributed_storage.arguments]: invalid checkpoint identity".into(),
            );
        }
        let store = adapter
            .local
            .as_ref()
            .ok_or("error[vm.distributed_storage.local]: missing local store")?;
        let mut outcome = Outcome::new("snapshot_missing", name, "local_only", adapter.sequence);
        outcome.checkpoint_id = id.into();
        if let Some(checkpoint) = store.load(id) {
            if proof {
                return Ok(Resource::Proof(Proof::Isolation {
                    id: id.into(),
                    sequence: checkpoint.sequence as i64,
                    checksum: checkpoint.checksum(),
                }));
            }
            self.budget.check(
                owner,
                std::mem::size_of::<Resource>() + 64 + id.len() * 2 + checkpoint.payload.len(),
            )?;
            outcome.kind = "snapshot_loaded";
            outcome.sequence = checkpoint.sequence as i64;
            outcome.checksum = checkpoint.checksum();
            outcome.snapshot = Some(checkpoint.clone());
        } else if proof {
            return Err("error[vm.distributed_storage.proof]: checkpoint is missing; no isolation proof was issued".into());
        }
        Ok(Resource::Outcome(outcome))
    }
}

/// Local observations describe committed memory state, never disk or peer persistence.
fn observe_or_mutate(
    adapter: &mut Adapter,
    handle: NativeBoundaryHandle,
    operation: &str,
    name: &'static str,
    args: &[ReplValue],
) -> VmRuntimeResult<Resource> {
    let store = adapter
        .local
        .as_mut()
        .ok_or("error[vm.distributed_storage.local]: missing local store")?;
    let status = store.status();
    let mut outcome = Outcome::new("opened", name, "local_only", status.sequence as i64);
    match (operation, args) {
        ("open", [_]) => adapter.open = true,
        ("close", [_]) => {
            adapter.open = false;
            outcome.kind = "closed";
        }
        ("flush", [_]) => outcome.kind = "flushed", // Volatile ordering barrier only.
        ("require_durable_flush", [_]) => outcome.kind = "unsupported",
        ("atomic_append_proof", [_]) => {
            return Ok(Resource::Proof(Proof::Atomic(status.sequence as i64)))
        }
        ("schema_migration_proof", [_]) => {
            return Ok(Resource::Proof(Proof::Schema(
                status.schema,
                status.schema_sequence as i64,
            )))
        }
        ("compare_and_swap_token", [_]) => {
            return Ok(Resource::Proof(Proof::Cas {
                adapter: handle,
                sequence: status.sequence as i64,
            }))
        }
        (
            "require_atomic_append"
            | "require_schema_migration"
            | "require_transactional_batch"
            | "require_snapshot_isolation",
            [_],
        ) => {}
        ("compact", [_, ReplValue::Int(boundary)]) => {
            let result = u64::try_from(*boundary)
                .map_err(|_| terlan_storage::StorageError::Invalid("negative boundary"))
                .and_then(|boundary| store.compact(boundary));
            match result {
                Ok(result) => {
                    outcome.kind = "compacted";
                    outcome.retained = result.retained as i64;
                }
                Err(error) => {
                    outcome = Outcome::failure(error.into(), name, "local_only", adapter.sequence)
                }
            }
        }
        ("migrate_schema", [_, ReplValue::Int(expected), ReplValue::Int(next)]) => {
            let result = u32::try_from(*expected)
                .and_then(|expected| u32::try_from(*next).map(|next| (expected, next)))
                .map_err(|_| terlan_storage::StorageError::Invalid("schema range"))
                .and_then(|(expected, next)| store.migrate_schema(expected, next));
            match result {
                Ok(status) => {
                    outcome.kind = "schema_migrated";
                    outcome.schema = status.schema;
                }
                Err(error) => {
                    outcome = Outcome::failure(error.into(), name, "local_only", adapter.sequence)
                }
            }
        }
        _ => {
            return Err(
                "error[vm.distributed_storage.arguments]: invalid local operation arguments".into(),
            )
        }
    }
    adapter.sequence = store.status().sequence as i64;
    Ok(Resource::Outcome(outcome))
}
