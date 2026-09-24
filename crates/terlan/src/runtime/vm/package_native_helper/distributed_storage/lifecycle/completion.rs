//! Projects correlated durable replies on the owning VM thread.

use super::*;

impl VmDistributedStorageRuntime {
    /// Only an acknowledged, structurally validated reply updates lifecycle observations.
    pub(in super::super::super) fn complete(
        &mut self,
        owner: u64,
        pending: Pending,
        reply: NativeBoundaryReplyTerm,
    ) -> VmRuntimeResult<ReplValue> {
        let value = match reply {
            NativeBoundaryReplyTerm::Ok(value) => value,
            NativeBoundaryReplyTerm::Error { code, .. } => return Err(format!("error[vm.distributed_storage.worker]: {code}; inspect durable state before retry; failed or lost acknowledgments do not prove rollback").into()),
        };
        let Resource::Adapter(adapter) = self
            .resources
            .get_mut_for_owner(pending.adapter, owner)
            .map_err(resource_error)?
        else {
            return Err(
                "error[vm.distributed_storage.handle]: pending adapter no longer exists".into(),
            );
        };
        if let Some(failure) = StorageFailure::from_term(&value).map_err(|_| invalid_reply())? {
            if matches!(
                failure,
                StorageFailure::Database | StorageFailure::Unavailable
            ) {
                adapter.open = false;
            }
            if matches!(
                pending.operation,
                Operation::Status(
                    StatusProjection::Atomic | StatusProjection::Schema | StatusProjection::Cas
                ) | Operation::Load { proof: true, .. }
            ) {
                return Err("error[vm.distributed_storage.proof]: durable observation failed; no proof was issued".into());
            }
            let mut outcome =
                Outcome::failure(failure, pending.name, adapter.policy.mode, adapter.sequence);
            if let Operation::Append { first, .. } = &pending.operation {
                outcome.project_append_failure(*first)?;
            }
            outcome.checkpoint_id = match &pending.operation {
                Operation::Append { checkpoint_id, .. } => checkpoint_id.clone(),
                Operation::Load { id, .. } => id.clone(),
                _ => String::new(),
            };
            return self.insert(owner, Resource::Outcome(outcome));
        }
        let mut outcome = Outcome::new(
            "opened",
            pending.name,
            adapter.policy.mode,
            adapter.sequence,
        );
        match (pending.operation, value) {
            (Operation::Status(projection), value) => {
                let (sequence, schema, schema_sequence) =
                    if matches!(projection, StatusProjection::Open) {
                        let Term::Tuple(fields) = &value else {
                            return Err(invalid_reply());
                        };
                        let [Term::Bytes(identity), boundary] = fields.as_slice() else {
                            return Err(invalid_reply());
                        };
                        let identity: [u8; 32] = identity
                            .as_slice()
                            .try_into()
                            .map_err(|_| invalid_reply())?;
                        if identity == [0; 32] {
                            return Err(invalid_reply());
                        }
                        let boundary = status(boundary)?;
                        if adapter
                            .identity
                            .is_some_and(|previous| previous != identity)
                        {
                            adapter.open = false;
                            outcome.kind = "storage_identity_mismatch";
                            outcome.recovery = "rebind_storage";
                            return self.insert(owner, Resource::Outcome(outcome));
                        }
                        adapter.identity = Some(identity);
                        boundary
                    } else {
                        status(&value)?
                    };
                adapter.sequence = sequence;
                outcome.sequence = sequence;
                let proof = match projection {
                    StatusProjection::Open => {
                        adapter.open = true;
                        None
                    }
                    StatusProjection::Require => None,
                    StatusProjection::Atomic => Some(Proof::Atomic(sequence)),
                    StatusProjection::Schema => Some(Proof::Schema(schema, schema_sequence)),
                    StatusProjection::Cas => Some(Proof::Cas {
                        adapter: pending.adapter,
                        sequence,
                    }),
                };
                if let Some(proof) = proof {
                    return self.insert(owner, Resource::Proof(proof));
                }
            }
            (operation @ (Operation::Flush | Operation::Close), value) => {
                let (sequence, _, _) = status(&value)?;
                adapter.sequence = sequence;
                adapter.flushed = sequence;
                outcome.sequence = sequence;
                outcome.kind = if matches!(operation, Operation::Close) {
                    adapter.open = false;
                    "closed"
                } else {
                    "flushed"
                };
            }
            (Operation::Migrate { next }, value) => {
                let (sequence, schema, _) = status(&value)?;
                if i64::from(schema) != next {
                    return Err(invalid_reply());
                }
                adapter.sequence = sequence;
                outcome.sequence = sequence;
                outcome.kind = "schema_migrated";
                outcome.schema = schema;
            }
            (
                Operation::Append {
                    first,
                    last,
                    count,
                    checkpoint_id,
                    checksum,
                },
                Term::Bool(_committed),
            ) => {
                adapter.sequence = adapter.sequence.max(last);
                outcome.sequence = last;
                outcome.kind = "appended";
                outcome.checkpoint_id = checkpoint_id;
                outcome.checksum = checksum;
                if pending.name == "transactional_batch_append" {
                    adapter.batch = (first, last, count);
                    outcome.kind = "batch_appended";
                }
            }
            (Operation::Compact, Term::Tuple(fields)) => {
                let [Term::Int(removed), Term::Int(retained), Term::Int(sequence)] =
                    fields.as_slice()
                else {
                    return Err(invalid_reply());
                };
                if *removed < 0
                    || *retained < 0
                    || *sequence < 0
                    || removed
                        .checked_add(*retained)
                        .is_none_or(|count| count > *sequence)
                {
                    return Err(invalid_reply());
                }
                adapter.sequence = *sequence;
                outcome.sequence = *sequence;
                outcome.retained = *retained;
                outcome.kind = "compacted";
            }
            (Operation::Load { id, proof }, Term::List(mut values)) => {
                outcome.checkpoint_id = id.clone();
                if values.is_empty() {
                    if proof {
                        return Err("error[vm.distributed_storage.proof]: checkpoint is missing; no isolation proof was issued".into());
                    }
                    outcome.kind = "snapshot_missing";
                } else {
                    if values.len() != 1 {
                        return Err(invalid_reply());
                    }
                    let Some(Term::Tuple(fields)) = values.pop() else {
                        return Err(invalid_reply());
                    };
                    let [Term::Text(actual), Term::Int(sequence), Term::Int(schema), Term::Bytes(payload)] =
                        fields.as_slice()
                    else {
                        return Err(invalid_reply());
                    };
                    if actual != &id {
                        return Err(invalid_reply());
                    }
                    let checkpoint = Checkpoint {
                        id,
                        sequence: u64::try_from(*sequence).map_err(|_| invalid_reply())?,
                        schema: u32::try_from(*schema).map_err(|_| invalid_reply())?,
                        payload: payload.clone(),
                    };
                    checkpoint.validate().map_err(storage_error)?;
                    let checksum = checkpoint.checksum();
                    if proof {
                        return self.insert(
                            owner,
                            Resource::Proof(Proof::Isolation {
                                id: checkpoint.id,
                                sequence: *sequence,
                                checksum,
                            }),
                        );
                    }
                    outcome.sequence = *sequence;
                    outcome.checksum = checksum;
                    outcome.snapshot = Some(checkpoint);
                    outcome.kind = "snapshot_loaded";
                }
            }
            _ => return Err(invalid_reply()),
        }
        self.insert(owner, Resource::Outcome(outcome))
    }
}

/// A status is one transaction-consistent observation, not three independent reads.
pub(super) fn status(value: &Term) -> VmRuntimeResult<(i64, u32, i64)> {
    let Term::Tuple(fields) = value else {
        return Err(invalid_reply());
    };
    let [Term::Int(sequence), Term::Int(schema), Term::Int(schema_sequence)] = fields.as_slice()
    else {
        return Err(invalid_reply());
    };
    let schema = u32::try_from(*schema).map_err(|_| invalid_reply())?;
    if *sequence < 0 || schema == 0 || *schema_sequence < 0 || schema_sequence > sequence {
        return Err(invalid_reply());
    }
    Ok((*sequence, schema, *schema_sequence))
}
