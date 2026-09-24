//! Immutable observations derived from acknowledged operations, not simulated counters.

use super::*;

pub(in super::super) enum Proof {
    Resources(i64, i64),
    Atomic(i64),
    Flush(i64),
    Batch(i64, i64, i64),
    Schema(u32, i64),
    Isolation {
        id: String,
        sequence: i64,
        checksum: u32,
    },
    Cas {
        adapter: NativeBoundaryHandle,
        sequence: i64,
    },
}

impl Proof {
    pub(super) fn kind(&self) -> &'static str {
        match self {
            Self::Resources(..) => "ResourceHandleValidationProof",
            Self::Atomic(_) => "AtomicAppendProof",
            Self::Flush(_) => "DurableFlushProof",
            Self::Batch(..) => "TransactionalBatchProof",
            Self::Schema(..) => "SchemaMigrationProof",
            Self::Isolation { .. } => "SnapshotIsolationProof",
            Self::Cas { .. } => "CompareAndSwapToken",
        }
    }
}

impl VmDistributedStorageRuntime {
    pub(super) fn read_observation(
        &mut self,
        owner: u64,
        operation: &str,
        args: &[ReplValue],
    ) -> VmRuntimeResult<Option<ReplValue>> {
        let kind = match operation {
            "resource_handle_count" | "resource_handle_sequence" => "ResourceHandleValidationProof",
            "proof_sequence" => "AtomicAppendProof",
            "durable_flush_sequence" => "DurableFlushProof",
            "batch_first_sequence" | "batch_last_sequence" | "batch_committed_count" => {
                "TransactionalBatchProof"
            }
            "schema_version" | "schema_sequence" => "SchemaMigrationProof",
            "isolation_checkpoint_id" | "isolation_sequence" | "isolation_checksum" => {
                "SnapshotIsolationProof"
            }
            "durable_flush_proof" | "transactional_batch_proof" => "Adapter",
            _ => return Ok(None),
        };
        let [value] = args else {
            return Err("error[vm.distributed_storage.arguments]: expected one observation".into());
        };
        let resource = self.resource(owner, value, kind)?;
        if let Resource::Adapter(adapter) = resource {
            if !adapter.open {
                return Err("error[vm.distributed_storage.closed]: open the adapter before requesting proofs".into());
            }
            let proof = if operation == "durable_flush_proof" {
                if adapter.policy.mode != "durable" {
                    return Err("error[vm.distributed_storage.proof]: volatile storage cannot issue a durable flush proof".into());
                }
                Proof::Flush(adapter.flushed)
            } else {
                Proof::Batch(adapter.batch.0, adapter.batch.1, adapter.batch.2)
            };
            return self.insert(owner, Resource::Proof(proof)).map(Some);
        }
        let Resource::Proof(proof) = resource else {
            unreachable!("checked kind")
        };
        if let Proof::Isolation { id, .. } = proof {
            if operation == "isolation_checkpoint_id" {
                return Ok(Some(ReplValue::String(id.clone())));
            }
        }
        let value = match (operation, proof) {
            ("resource_handle_count", Proof::Resources(count, _)) => *count,
            ("resource_handle_sequence", Proof::Resources(_, sequence)) => *sequence,
            ("proof_sequence", Proof::Atomic(sequence))
            | ("durable_flush_sequence", Proof::Flush(sequence))
            | ("schema_sequence", Proof::Schema(_, sequence)) => *sequence,
            ("schema_version", Proof::Schema(version, _)) => i64::from(*version),
            ("isolation_sequence", Proof::Isolation { sequence, .. }) => *sequence,
            ("isolation_checksum", Proof::Isolation { checksum, .. }) => i64::from(*checksum),
            ("batch_first_sequence", Proof::Batch(first, _, _)) => *first,
            ("batch_last_sequence", Proof::Batch(_, last, _)) => *last,
            ("batch_committed_count", Proof::Batch(_, _, count)) => *count,
            _ => {
                return Err("error[vm.distributed_storage.proof]: invalid observation kind".into())
            }
        };
        Ok(Some(ReplValue::Int(value)))
    }
}
