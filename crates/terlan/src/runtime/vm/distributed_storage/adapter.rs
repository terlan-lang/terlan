use std::collections::BTreeSet;

use super::model::*;
#[cfg(test)]
use crate::runtime::vm::checksum::{crc32_init, crc32_update};

/// VM-owned in-memory storage adapter used to validate distributed contracts.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmDistributedStorageAdapter {
    policy: VmDistributedStoragePolicy,
    opened: bool,
    snapshots: Vec<VmDistributedStorageSnapshot>,
    flushed_sequence: u64,
    highest_sequence: u64,
    fail_next_flush: bool,
    timeout_next_flush: bool,
    partial_write_limit: Option<usize>,
    last_batch_proof: Option<VmDistributedStorageTransactionalBatchProof>,
    last_batch_rollback_proof: Option<VmDistributedStorageTransactionalRollbackProof>,
    schema_version: u32,
    schema_migration_proof: VmDistributedStorageSchemaMigrationProof,
    resource_handles: BTreeSet<String>,
    resource_handle_validation_proof: VmDistributedStorageResourceHandleValidationProof,
}

#[cfg(test)]
impl VmDistributedStorageAdapter {
    /// Creates a closed adapter from a validated policy.
    pub(crate) fn new(policy: VmDistributedStoragePolicy) -> Self {
        Self {
            policy,
            opened: false,
            snapshots: Vec::new(),
            flushed_sequence: 0,
            highest_sequence: 0,
            fail_next_flush: false,
            timeout_next_flush: false,
            partial_write_limit: None,
            last_batch_proof: None,
            last_batch_rollback_proof: None,
            schema_version: 1,
            schema_migration_proof: VmDistributedStorageSchemaMigrationProof::new(1, 0),
            resource_handles: BTreeSet::new(),
            resource_handle_validation_proof:
                VmDistributedStorageResourceHandleValidationProof::new(0, 0),
        }
    }

    /// Opens the adapter and returns a typed outcome.
    pub(crate) fn open(&mut self) -> VmDistributedStorageOutcome {
        if !self.policy.available {
            return self.unavailable(VmDistributedStorageOperation::Open);
        }
        self.opened = true;
        VmDistributedStorageOutcome::Opened {
            mode: self.policy.mode,
        }
    }

    /// Appends a snapshot descriptor to the adapter log.
    pub(crate) fn append(
        &mut self,
        snapshot: VmDistributedStorageSnapshot,
    ) -> VmDistributedStorageOutcome {
        self.append_for_operation(snapshot, VmDistributedStorageOperation::Append)
    }

    /// Replicates a snapshot through a cluster-capable storage adapter.
    pub(crate) fn replicate_snapshot(
        &mut self,
        snapshot: VmDistributedStorageSnapshot,
    ) -> VmDistributedStorageOutcome {
        self.append_for_operation(snapshot, VmDistributedStorageOperation::ClusterReplicate)
    }

    /// Returns a compare-and-swap token for the current committed sequence.
    pub(crate) fn compare_and_swap_token(&self) -> VmDistributedStorageCasToken {
        VmDistributedStorageCasToken::new(self.latest_sequence())
    }

    /// Returns a typed capability check for atomic append support.
    pub(crate) fn require_atomic_append(&self) -> VmDistributedStorageOutcome {
        self.capability(VmDistributedStorageOperation::Append)
    }

    /// Returns a proof object for the current atomic append sequence boundary.
    pub(crate) fn atomic_append_proof(
        &self,
    ) -> Result<VmDistributedStorageAtomicAppendProof, VmDistributedStorageOutcome> {
        match self.require_atomic_append() {
            VmDistributedStorageOutcome::Opened { .. } => Ok(
                VmDistributedStorageAtomicAppendProof::new(self.latest_sequence()),
            ),
            outcome => Err(outcome),
        }
    }

    /// Returns a typed capability check for snapshot isolation support.
    pub(crate) fn require_snapshot_isolation(&self) -> VmDistributedStorageOutcome {
        self.capability(VmDistributedStorageOperation::SnapshotIsolation)
    }

    /// Returns a proof object for one validated loaded snapshot.
    pub(crate) fn snapshot_isolation_proof(
        &self,
        checkpoint_id: &str,
    ) -> Result<VmDistributedStorageSnapshotIsolationProof, VmDistributedStorageOutcome> {
        if let Some(outcome) = self.guard(VmDistributedStorageOperation::SnapshotIsolation) {
            return Err(outcome);
        }
        let outcome = self.load_snapshot(checkpoint_id);
        match outcome {
            VmDistributedStorageOutcome::SnapshotLoaded(snapshot) => {
                Ok(VmDistributedStorageSnapshotIsolationProof::new(
                    snapshot.checkpoint_id,
                    snapshot.sequence,
                    snapshot.checksum,
                ))
            }
            outcome => Err(outcome),
        }
    }

    /// Returns a typed capability check for durable flush proof support.
    pub(crate) fn require_durable_flush(&self) -> VmDistributedStorageOutcome {
        self.capability(VmDistributedStorageOperation::DurableFlush)
    }

    /// Returns a proof object for the last successfully flushed sequence.
    pub(crate) fn durable_flush_proof(
        &self,
    ) -> Result<VmDistributedStorageDurableFlushProof, VmDistributedStorageOutcome> {
        match self.require_durable_flush() {
            VmDistributedStorageOutcome::Opened { .. } => Ok(
                VmDistributedStorageDurableFlushProof::new(self.flushed_sequence),
            ),
            outcome => Err(outcome),
        }
    }

    /// Returns a typed capability check for transactional batch append support.
    pub(crate) fn require_transactional_batch(&self) -> VmDistributedStorageOutcome {
        self.capability(VmDistributedStorageOperation::TransactionalBatchAppend)
    }

    /// Returns a proof object for the last committed transactional batch.
    pub(crate) fn transactional_batch_proof(
        &self,
    ) -> Result<VmDistributedStorageTransactionalBatchProof, VmDistributedStorageOutcome> {
        match self.require_transactional_batch() {
            VmDistributedStorageOutcome::Opened { .. } => Ok(self
                .last_batch_proof
                .unwrap_or(VmDistributedStorageTransactionalBatchProof::new(0, 0, 0))),
            outcome => Err(outcome),
        }
    }

    /// Returns a proof object for the last rolled-back transactional batch.
    pub(crate) fn transactional_rollback_proof(
        &self,
    ) -> Result<VmDistributedStorageTransactionalRollbackProof, VmDistributedStorageOutcome> {
        match self.require_transactional_batch() {
            VmDistributedStorageOutcome::Opened { .. } => Ok(self
                .last_batch_rollback_proof
                .unwrap_or(VmDistributedStorageTransactionalRollbackProof::new(
                    0,
                    0,
                    self.latest_sequence(),
                ))),
            outcome => Err(outcome),
        }
    }

    /// Appends a snapshot only when the caller observed the latest sequence.
    pub(crate) fn compare_and_swap_append(
        &mut self,
        snapshot: VmDistributedStorageSnapshot,
        token: VmDistributedStorageCasToken,
    ) -> VmDistributedStorageOutcome {
        if let Some(outcome) = self.guard(VmDistributedStorageOperation::CompareAndSwapAppend) {
            return outcome;
        }
        let actual_sequence = self.latest_sequence();
        if token.expected_sequence() != actual_sequence {
            return VmDistributedStorageOutcome::CompareAndSwapTokenMismatch {
                operation: VmDistributedStorageOperation::CompareAndSwapAppend,
                expected_sequence: token.expected_sequence(),
                actual_sequence,
            };
        }
        self.append_for_operation(
            snapshot,
            VmDistributedStorageOperation::CompareAndSwapAppend,
        )
    }

    /// Returns a typed capability check for schema migration support.
    pub(crate) fn require_schema_migration(&self) -> VmDistributedStorageOutcome {
        self.capability(VmDistributedStorageOperation::SchemaMigration)
    }

    /// Returns a proof object for the current adapter schema version.
    pub(crate) fn schema_migration_proof(
        &self,
    ) -> Result<VmDistributedStorageSchemaMigrationProof, VmDistributedStorageOutcome> {
        match self.require_schema_migration() {
            VmDistributedStorageOutcome::Opened { .. } => Ok(self.schema_migration_proof),
            outcome => Err(outcome),
        }
    }

    /// Migrates the adapter schema only when the caller observed the current version.
    pub(crate) fn migrate_schema(
        &mut self,
        expected_schema: u32,
        next_schema: u32,
    ) -> VmDistributedStorageOutcome {
        let operation = VmDistributedStorageOperation::SchemaMigration;
        if let Some(outcome) = self.guard(operation) {
            return outcome;
        }
        if expected_schema != self.schema_version || next_schema <= self.schema_version {
            return VmDistributedStorageOutcome::SchemaMigrationMismatch {
                operation,
                expected_schema,
                actual_schema: self.schema_version,
            };
        }
        self.schema_version = next_schema;
        self.schema_migration_proof = VmDistributedStorageSchemaMigrationProof::new(
            self.schema_version,
            self.latest_sequence(),
        );
        VmDistributedStorageOutcome::SchemaMigrated {
            schema_version: self.schema_version,
            sequence: self.latest_sequence(),
        }
    }

    /// Returns a typed capability check for resource handle validation support.
    pub(crate) fn require_resource_handle_validation(&self) -> VmDistributedStorageOutcome {
        self.capability(VmDistributedStorageOperation::ResourceHandleValidation)
    }

    /// Returns a proof object for the last successful resource handle validation.
    pub(crate) fn resource_handle_validation_proof(
        &self,
    ) -> Result<VmDistributedStorageResourceHandleValidationProof, VmDistributedStorageOutcome>
    {
        match self.require_resource_handle_validation() {
            VmDistributedStorageOutcome::Opened { .. } => Ok(self.resource_handle_validation_proof),
            outcome => Err(outcome),
        }
    }

    /// Registers a durable resource handle owned by this adapter.
    pub(crate) fn register_resource_handle(&mut self, handle: &str) -> VmDistributedStorageOutcome {
        if let Some(outcome) = self.guard(VmDistributedStorageOperation::ResourceHandleValidation) {
            return outcome;
        }
        if handle.is_empty() {
            return VmDistributedStorageOutcome::ResourceHandleValidationFailed {
                operation: VmDistributedStorageOperation::ResourceHandleValidation,
                missing_handle: handle.to_string(),
            };
        }
        self.resource_handles.insert(handle.to_string());
        self.resource_handle_validation_proof =
            VmDistributedStorageResourceHandleValidationProof::new(
                self.resource_handles.len(),
                self.latest_sequence(),
            );
        VmDistributedStorageOutcome::ResourceHandlesValidated {
            validated_count: self.resource_handles.len(),
            sequence: self.latest_sequence(),
        }
    }

    /// Validates that every requested durable resource handle is available.
    pub(crate) fn validate_resource_handles(
        &mut self,
        handles: &[String],
    ) -> VmDistributedStorageOutcome {
        if let Some(outcome) = self.guard(VmDistributedStorageOperation::ResourceHandleValidation) {
            return outcome;
        }
        for handle in handles {
            if handle.is_empty() || !self.resource_handles.contains(handle) {
                return VmDistributedStorageOutcome::ResourceHandleValidationFailed {
                    operation: VmDistributedStorageOperation::ResourceHandleValidation,
                    missing_handle: handle.clone(),
                };
            }
        }
        self.resource_handle_validation_proof =
            VmDistributedStorageResourceHandleValidationProof::new(
                handles.len(),
                self.latest_sequence(),
            );
        VmDistributedStorageOutcome::ResourceHandlesValidated {
            validated_count: handles.len(),
            sequence: self.latest_sequence(),
        }
    }

    /// Commits every snapshot in a batch or rejects the batch without mutation.
    pub(crate) fn transactional_batch_append(
        &mut self,
        snapshots: Vec<VmDistributedStorageSnapshot>,
    ) -> VmDistributedStorageOutcome {
        let operation = VmDistributedStorageOperation::TransactionalBatchAppend;
        if let Some(outcome) = self.guard(operation) {
            return outcome;
        }
        let mut previous_sequence = self.latest_sequence();
        for snapshot in &snapshots {
            let expected = snapshot.expected_checksum();
            if expected != snapshot.checksum {
                return VmDistributedStorageOutcome::ChecksumMismatch {
                    operation,
                    checkpoint_id: snapshot.checkpoint_id.clone(),
                    sequence: snapshot.sequence,
                    expected,
                    actual: snapshot.checksum,
                };
            }
            if snapshot.sequence <= previous_sequence {
                return VmDistributedStorageOutcome::StaleSnapshot {
                    local_sequence: previous_sequence,
                    incoming_sequence: snapshot.sequence,
                };
            }
            previous_sequence = snapshot.sequence;
        }
        if let Some(limit) = self.partial_write_limit.take() {
            if limit < snapshots.len() {
                self.last_batch_rollback_proof =
                    Some(VmDistributedStorageTransactionalRollbackProof::new(
                        snapshots.len(),
                        limit,
                        self.latest_sequence(),
                    ));
                return VmDistributedStorageOutcome::PartialWrite {
                    operation,
                    checkpoint_id: snapshots
                        .first()
                        .map(|snapshot| snapshot.checkpoint_id.clone())
                        .unwrap_or_default(),
                    sequence: snapshots
                        .last()
                        .map_or(self.latest_sequence(), |s| s.sequence),
                    expected_entries: snapshots.len(),
                    persisted_entries: limit,
                };
            }
        }
        let first_sequence = snapshots
            .first()
            .map_or(self.latest_sequence(), |snapshot| snapshot.sequence);
        let last_sequence = snapshots
            .last()
            .map_or(self.latest_sequence(), |snapshot| snapshot.sequence);
        let count = snapshots.len();
        let checksum = batch_checksum(&snapshots);
        self.highest_sequence = last_sequence;
        self.snapshots.extend(snapshots);
        self.last_batch_proof = Some(VmDistributedStorageTransactionalBatchProof::new(
            first_sequence,
            last_sequence,
            count,
        ));
        VmDistributedStorageOutcome::TransactionalBatchCommitted {
            first_sequence,
            last_sequence,
            count,
            checksum,
        }
    }

    /// Appends or replicates a snapshot after applying operation-specific guards.
    fn append_for_operation(
        &mut self,
        snapshot: VmDistributedStorageSnapshot,
        operation: VmDistributedStorageOperation,
    ) -> VmDistributedStorageOutcome {
        if let Some(outcome) = self.guard(operation) {
            return outcome;
        }
        let expected = snapshot.expected_checksum();
        if expected != snapshot.checksum {
            return VmDistributedStorageOutcome::ChecksumMismatch {
                operation,
                checkpoint_id: snapshot.checkpoint_id,
                sequence: snapshot.sequence,
                expected,
                actual: snapshot.checksum,
            };
        }
        let local_sequence = self.latest_sequence();
        if snapshot.sequence <= local_sequence {
            return VmDistributedStorageOutcome::StaleSnapshot {
                local_sequence,
                incoming_sequence: snapshot.sequence,
            };
        }
        if let Some(limit) = self.partial_write_limit.take() {
            if limit < snapshot.entries.len() {
                return VmDistributedStorageOutcome::PartialWrite {
                    operation,
                    checkpoint_id: snapshot.checkpoint_id,
                    sequence: snapshot.sequence,
                    expected_entries: snapshot.entries.len(),
                    persisted_entries: limit,
                };
            }
        }
        let outcome = VmDistributedStorageOutcome::Appended {
            checkpoint_id: snapshot.checkpoint_id.clone(),
            sequence: snapshot.sequence,
            checksum: snapshot.checksum,
        };
        self.highest_sequence = snapshot.sequence;
        self.snapshots.push(snapshot);
        outcome
    }

    /// Flushes appended snapshots through the adapter boundary.
    pub(crate) fn flush(&mut self) -> VmDistributedStorageOutcome {
        if let Some(outcome) = self.guard(VmDistributedStorageOperation::Flush) {
            return outcome;
        }
        if self.fail_next_flush {
            self.fail_next_flush = false;
            return VmDistributedStorageOutcome::FinalizeFailed {
                operation: VmDistributedStorageOperation::Flush,
                sequence: self.latest_sequence(),
            };
        }
        if self.timeout_next_flush {
            self.timeout_next_flush = false;
            return VmDistributedStorageOutcome::FlushTimedOut {
                operation: VmDistributedStorageOperation::Flush,
                sequence: self.latest_sequence(),
            };
        }
        self.flushed_sequence = self.latest_sequence();
        VmDistributedStorageOutcome::Flushed {
            sequence: self.flushed_sequence,
        }
    }

    /// Compacts snapshots below a sequence threshold.
    pub(crate) fn compact(&mut self, retain_from_sequence: u64) -> VmDistributedStorageOutcome {
        if let Some(outcome) = self.guard(VmDistributedStorageOperation::Compact) {
            return outcome;
        }
        self.snapshots
            .retain(|snapshot| snapshot.sequence >= retain_from_sequence);
        VmDistributedStorageOutcome::Compacted {
            retained: self.snapshots.len(),
        }
    }

    /// Loads and validates a snapshot by checkpoint id.
    pub(crate) fn load_snapshot(&self, checkpoint_id: &str) -> VmDistributedStorageOutcome {
        if let Some(outcome) = self.guard(VmDistributedStorageOperation::LoadSnapshot) {
            return outcome;
        }
        let Some(snapshot) = self
            .snapshots
            .iter()
            .find(|snapshot| snapshot.checkpoint_id == checkpoint_id)
        else {
            return VmDistributedStorageOutcome::SnapshotMissing {
                checkpoint_id: checkpoint_id.to_string(),
            };
        };
        let expected = snapshot.expected_checksum();
        if expected != snapshot.checksum {
            return VmDistributedStorageOutcome::ChecksumMismatch {
                operation: VmDistributedStorageOperation::LoadSnapshot,
                checkpoint_id: snapshot.checkpoint_id.clone(),
                sequence: snapshot.sequence,
                expected,
                actual: snapshot.checksum,
            };
        }
        VmDistributedStorageOutcome::SnapshotLoaded(snapshot.clone())
    }

    /// Closes the adapter.
    pub(crate) fn close(&mut self) -> VmDistributedStorageOutcome {
        if let Some(outcome) = self.guard(VmDistributedStorageOperation::Close) {
            return outcome;
        }
        self.opened = false;
        VmDistributedStorageOutcome::Closed
    }

    /// Returns a typed capability check for cluster replication.
    pub(crate) fn require_cluster_replication(&self) -> VmDistributedStorageOutcome {
        self.capability(VmDistributedStorageOperation::ClusterReplicate)
    }

    /// Returns the stable source-facing policy name for this adapter.
    pub(crate) fn policy_name(&self) -> &str {
        self.policy.name()
    }

    /// Returns the stable source-facing policy mode kind for this adapter.
    pub(crate) fn policy_mode_kind(&self) -> &'static str {
        self.policy.mode_kind()
    }

    /// Returns whether this adapter's selected backend is available.
    pub(crate) fn policy_available(&self) -> bool {
        self.policy.is_available()
    }

    /// Returns whether this adapter can perform cluster replication.
    pub(crate) fn can_cluster_replicate(&self) -> bool {
        self.policy.can_cluster_replicate()
    }

    /// Returns the highest appended sequence number.
    pub(crate) fn latest_sequence(&self) -> u64 {
        self.highest_sequence
    }

    /// Inserts a raw snapshot for adversarial validation.
    pub(crate) fn inject_snapshot_for_test(&mut self, snapshot: VmDistributedStorageSnapshot) {
        self.highest_sequence = self.highest_sequence.max(snapshot.sequence);
        self.snapshots.push(snapshot);
    }

    /// Forces the next flush to return a finalize failure for adversarial validation.
    pub(crate) fn fail_next_flush_for_test(&mut self) {
        self.fail_next_flush = true;
    }

    /// Forces the next flush to return a timeout failure for adversarial validation.
    pub(crate) fn timeout_next_flush_for_test(&mut self) {
        self.timeout_next_flush = true;
    }

    /// Limits the next append to a partial write for adversarial validation.
    pub(crate) fn set_partial_write_limit_for_test(&mut self, persisted_entries: usize) {
        self.partial_write_limit = Some(persisted_entries);
    }

    /// Returns a typed guard for operation readiness.
    pub(super) fn guard(
        &self,
        operation: VmDistributedStorageOperation,
    ) -> Option<VmDistributedStorageOutcome> {
        let capability = self.capability(operation);
        if !matches!(capability, VmDistributedStorageOutcome::Opened { .. }) {
            return Some(capability);
        }
        if !self.opened {
            return Some(self.unavailable(operation));
        }
        None
    }

    /// Returns a typed capability decision for one operation.
    fn capability(&self, operation: VmDistributedStorageOperation) -> VmDistributedStorageOutcome {
        if !self.policy.supports(operation) {
            return VmDistributedStorageOutcome::Unsupported {
                operation,
                mode: self.policy.mode,
            };
        }
        if !self.policy.available {
            return self.unavailable(operation);
        }
        VmDistributedStorageOutcome::Opened {
            mode: self.policy.mode,
        }
    }

    /// Returns a typed unavailable outcome.
    fn unavailable(&self, operation: VmDistributedStorageOperation) -> VmDistributedStorageOutcome {
        VmDistributedStorageOutcome::StorageUnavailable {
            operation,
            mode: self.policy.mode,
        }
    }
}

/// Computes a deterministic CRC-32 for a transactional batch descriptor.
#[cfg(test)]
fn batch_checksum(snapshots: &[VmDistributedStorageSnapshot]) -> u32 {
    let mut checksum = crc32_init();
    for snapshot in snapshots {
        checksum = crc32_update(checksum, snapshot.checkpoint_id.as_bytes());
        checksum = crc32_update(checksum, &snapshot.sequence.to_be_bytes());
        checksum = crc32_update(checksum, &snapshot.checksum.to_be_bytes());
    }
    checksum
}
