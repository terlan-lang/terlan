//! Bounded volatile checkpoints, with no filesystem, worker, or durability claim.

use crate::transaction::{validate_append, validate_batch, validate_migration};
use crate::{
    AppendOutcome, Checkpoint, CompactionOutcome, StorageError, StorageStatus, MAX_BATCH_BYTES,
    MAX_BATCH_CHECKPOINTS,
};
use std::collections::BTreeMap;

/// One owner-local volatile store. Dropping it loses all data.
///
/// Callers must additionally budget multiple stores and retained checkpoint views.
/// It has no durable flush, replication, host-path, or resource-authority capability.
pub struct LocalCheckpointStore {
    checkpoints: BTreeMap<String, Checkpoint>,
    status: StorageStatus,
    bytes: usize,
    maximum_bytes: usize,
    maximum_checkpoints: usize,
}

impl LocalCheckpointStore {
    /// Creates an empty store with explicit nonzero limits no larger than one batch.
    pub fn new(maximum_bytes: usize, maximum_checkpoints: usize) -> Result<Self, StorageError> {
        if maximum_bytes == 0
            || maximum_bytes > MAX_BATCH_BYTES
            || maximum_checkpoints == 0
            || maximum_checkpoints > MAX_BATCH_CHECKPOINTS
        {
            return Err(StorageError::Invalid(
                "local storage limits outside supported range",
            ));
        }
        Ok(Self {
            checkpoints: BTreeMap::new(),
            status: StorageStatus {
                sequence: 0,
                schema: 1,
                schema_sequence: 0,
            },
            bytes: 0,
            maximum_bytes,
            maximum_checkpoints,
        })
    }

    /// Observes the committed in-memory boundary; this does not prove persistence.
    pub fn status(&self) -> StorageStatus {
        self.status
    }

    /// Reports retained identity and payload bytes for the owner's aggregate budget.
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }

    /// Atomically admits a batch after validating every entry, CAS, schema, and quota.
    pub fn append(
        &mut self,
        expected: u64,
        checkpoints: &[Checkpoint],
    ) -> Result<AppendOutcome, StorageError> {
        validate_batch(expected, checkpoints)?;
        let decision = validate_append(self.status, expected, checkpoints, |checkpoint| {
            Ok(self
                .checkpoints
                .get(&checkpoint.id)
                .map(|stored| stored == checkpoint))
        })?;
        if decision == AppendOutcome::Replayed {
            return Ok(decision);
        }
        let bytes = checkpoints
            .iter()
            .try_fold(self.bytes, |bytes, checkpoint| {
                bytes
                    .checked_add(checkpoint.payload.len())
                    .and_then(|bytes| bytes.checked_add(checkpoint.id.len()))
                    .ok_or(StorageError::Invalid("local storage byte count overflow"))
            })?;
        if bytes > self.maximum_bytes
            || self.checkpoints.len().saturating_add(checkpoints.len()) > self.maximum_checkpoints
        {
            return Err(StorageError::Invalid("local storage capacity exceeded"));
        }
        let sequence = checkpoints
            .last()
            .ok_or(StorageError::Invalid("empty local batch"))?
            .sequence;
        // All fallible logical checks precede changes. No untrusted code or I/O runs here.
        for checkpoint in checkpoints {
            self.checkpoints
                .insert(checkpoint.id.clone(), checkpoint.clone());
        }
        self.bytes = bytes;
        self.status.sequence = sequence;
        Ok(decision)
    }

    /// Borrows an immutable retained checkpoint; the owner decides whether to clone a view.
    pub fn load(&self, id: &str) -> Option<&Checkpoint> {
        self.checkpoints.get(id)
    }

    /// Changes the application schema without rewriting previously admitted payloads.
    pub fn migrate_schema(
        &mut self,
        expected: u32,
        next: u32,
    ) -> Result<StorageStatus, StorageError> {
        let status = validate_migration(self.status, expected, next)?;
        self.status = status;
        Ok(status)
    }

    /// Removes old retained data while preserving the high-water sequence and schema.
    pub fn compact(&mut self, boundary: u64) -> Result<CompactionOutcome, StorageError> {
        if boundary > i64::MAX as u64 {
            return Err(StorageError::Invalid(
                "compaction boundary exceeds Terlan Int",
            ));
        }
        let before = self.checkpoints.len();
        self.checkpoints.retain(|_, checkpoint| {
            if checkpoint.sequence < boundary {
                self.bytes -= checkpoint.id.len() + checkpoint.payload.len();
                false
            } else {
                true
            }
        });
        Ok(CompactionOutcome {
            removed: before - self.checkpoints.len(),
            retained: self.checkpoints.len() as u64,
            sequence: self.status.sequence,
        })
    }
}

#[cfg(test)]
#[path = "local_test.rs"]
mod tests;
