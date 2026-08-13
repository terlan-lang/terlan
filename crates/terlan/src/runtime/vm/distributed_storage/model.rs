#[cfg(test)]
use crate::runtime::vm::checksum::{crc32_init, crc32_update};
use crate::runtime::vm::distributed_state::VmDistributedStateEntry;
/// Storage mode selected for VM-owned distributed state snapshots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDistributedStorageMode {
    LocalOnly,
    Durable,
    Cluster,
}
#[cfg(test)]
impl VmDistributedStorageMode {
    /// Returns the stable source-facing mode kind.
    pub(crate) fn kind(self) -> &'static str {
        match self {
            Self::LocalOnly => "local_only",
            Self::Durable => "durable",
            Self::Cluster => "cluster",
        }
    }
}

/// One VM distributed storage lifecycle operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDistributedStorageOperation {
    Open,
    Append,
    Flush,
    Compact,
    LoadSnapshot,
    Close,
    ClusterReplicate,
    CompareAndSwapAppend,
    SnapshotIsolation,
    DurableFlush,
    TransactionalBatchAppend,
    SchemaMigration,
    ResourceHandleValidation,
}

#[cfg(test)]
impl VmDistributedStorageOperation {
    /// Returns the stable source-facing operation kind.
    pub(crate) fn kind(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Append => "append",
            Self::Flush => "flush",
            Self::Compact => "compact",
            Self::LoadSnapshot => "load_snapshot",
            Self::Close => "close",
            Self::ClusterReplicate => "cluster_replicate",
            Self::CompareAndSwapAppend => "compare_and_swap_append",
            Self::SnapshotIsolation => "snapshot_isolation",
            Self::DurableFlush => "durable_flush",
            Self::TransactionalBatchAppend => "transactional_batch_append",
            Self::SchemaMigration => "schema_migration",
            Self::ResourceHandleValidation => "resource_handle_validation",
        }
    }
}

/// Declarative storage policy attached to a VM distributed state adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDistributedStoragePolicy {
    pub(crate) name: String,
    pub(crate) mode: VmDistributedStorageMode,
    pub(crate) available: bool,
}

#[cfg(test)]
impl VmDistributedStoragePolicy {
    /// Builds a validated storage policy.
    pub(crate) fn new(
        name: impl Into<String>,
        mode: VmDistributedStorageMode,
        available: bool,
    ) -> Result<Self, String> {
        let name = name.into();
        if name.is_empty() {
            return Err(
                "error[vm_distributed_storage]: storage policy name must be non-empty".to_string(),
            );
        }
        Ok(Self {
            name,
            mode,
            available,
        })
    }

    /// Builds the default in-memory force-local policy used by VM tests.
    pub(crate) fn force_local() -> Self {
        Self {
            name: "force-local".to_string(),
            mode: VmDistributedStorageMode::LocalOnly,
            available: true,
        }
    }

    /// Returns the stable source-facing policy name.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Returns the stable source-facing policy mode kind.
    pub(crate) fn mode_kind(&self) -> &'static str {
        self.mode.kind()
    }

    /// Returns whether the selected backend is available.
    pub(crate) fn is_available(&self) -> bool {
        self.available
    }

    /// Returns whether this policy can perform cluster replication.
    pub(crate) fn can_cluster_replicate(&self) -> bool {
        self.available && self.supports(VmDistributedStorageOperation::ClusterReplicate)
    }

    /// Returns whether this policy supports a lifecycle operation.
    pub(crate) fn supports(&self, operation: VmDistributedStorageOperation) -> bool {
        match operation {
            VmDistributedStorageOperation::ClusterReplicate => {
                self.mode == VmDistributedStorageMode::Cluster
            }
            _ => true,
        }
    }
}

/// Deterministic distributed state snapshot descriptor.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmDistributedStorageSnapshot {
    pub(crate) checkpoint_id: String,
    pub(crate) sequence: u64,
    pub(crate) entries: Vec<VmDistributedStateEntry>,
    pub(crate) checksum: u32,
}

/// Computes a deterministic CRC-32 for a VM distributed snapshot.
#[cfg(test)]
fn snapshot_checksum(
    checkpoint_id: &str,
    sequence: u64,
    entries: &[VmDistributedStateEntry],
) -> u32 {
    let mut checksum = crc32_init();
    checksum = crc32_update(checksum, checkpoint_id.as_bytes());
    checksum = crc32_update(checksum, &sequence.to_be_bytes());
    for entry in entries {
        checksum = crc32_update(checksum, entry.scope.namespace.as_bytes());
        checksum = crc32_update(checksum, entry.scope.key.as_bytes());
        checksum = crc32_update(checksum, entry.owner_node_id.as_bytes());
        checksum = crc32_update(checksum, &entry.version.sequence.to_be_bytes());
        checksum = crc32_update(checksum, entry.version.node_id.as_bytes());
        checksum = crc32_update(checksum, format!("{:?}", entry.policy).as_bytes());
        checksum = crc32_update(checksum, entry.value.render().as_bytes());
    }
    checksum
}

#[cfg(test)]
impl VmDistributedStorageSnapshot {
    /// Builds a snapshot and calculates its deterministic checksum.
    pub(crate) fn new(
        checkpoint_id: impl Into<String>,
        sequence: u64,
        entries: Vec<VmDistributedStateEntry>,
    ) -> Result<Self, String> {
        let checkpoint_id = checkpoint_id.into();
        if checkpoint_id.is_empty() {
            return Err(
                "error[vm_distributed_storage]: checkpoint id must be non-empty".to_string(),
            );
        }
        if sequence == 0 {
            return Err(
                "error[vm_distributed_storage]: checkpoint sequence must be non-zero".to_string(),
            );
        }
        let checksum = snapshot_checksum(&checkpoint_id, sequence, &entries);
        Ok(Self {
            checkpoint_id,
            sequence,
            entries,
            checksum,
        })
    }

    /// Builds a snapshot with an explicit checksum for corruption tests.
    pub(crate) fn with_checksum(
        checkpoint_id: impl Into<String>,
        sequence: u64,
        entries: Vec<VmDistributedStateEntry>,
        checksum: u32,
    ) -> Result<Self, String> {
        let mut snapshot = Self::new(checkpoint_id, sequence, entries)?;
        snapshot.checksum = checksum;
        Ok(snapshot)
    }

    /// Returns the checksum expected from the current snapshot fields.
    pub(crate) fn expected_checksum(&self) -> u32 {
        snapshot_checksum(&self.checkpoint_id, self.sequence, &self.entries)
    }
}

/// Typed outcome returned by VM distributed storage operations.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmDistributedStorageOutcome {
    Opened {
        mode: VmDistributedStorageMode,
    },
    Appended {
        checkpoint_id: String,
        sequence: u64,
        checksum: u32,
    },
    Flushed {
        sequence: u64,
    },
    Compacted {
        retained: usize,
    },
    SnapshotLoaded(VmDistributedStorageSnapshot),
    SnapshotMissing {
        checkpoint_id: String,
    },
    Closed,
    Unsupported {
        operation: VmDistributedStorageOperation,
        mode: VmDistributedStorageMode,
    },
    StorageUnavailable {
        operation: VmDistributedStorageOperation,
        mode: VmDistributedStorageMode,
    },
    StaleSnapshot {
        local_sequence: u64,
        incoming_sequence: u64,
    },
    ChecksumMismatch {
        operation: VmDistributedStorageOperation,
        checkpoint_id: String,
        sequence: u64,
        expected: u32,
        actual: u32,
    },
    FinalizeFailed {
        operation: VmDistributedStorageOperation,
        sequence: u64,
    },
    FlushTimedOut {
        operation: VmDistributedStorageOperation,
        sequence: u64,
    },
    PartialWrite {
        operation: VmDistributedStorageOperation,
        checkpoint_id: String,
        sequence: u64,
        expected_entries: usize,
        persisted_entries: usize,
    },
    CompareAndSwapTokenMismatch {
        operation: VmDistributedStorageOperation,
        expected_sequence: u64,
        actual_sequence: u64,
    },
    SchemaMigrationMismatch {
        operation: VmDistributedStorageOperation,
        expected_schema: u32,
        actual_schema: u32,
    },
    ResourceHandleValidationFailed {
        operation: VmDistributedStorageOperation,
        missing_handle: String,
    },
    SchemaMigrated {
        schema_version: u32,
        sequence: u64,
    },
    ResourceHandlesValidated {
        validated_count: usize,
        sequence: u64,
    },
    TransactionalBatchCommitted {
        first_sequence: u64,
        last_sequence: u64,
        count: usize,
        checksum: u32,
    },
}

#[cfg(test)]
impl VmDistributedStorageOutcome {
    /// Returns the stable source-facing lifecycle outcome kind.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Opened { .. } => "opened",
            Self::Appended { .. } => "appended",
            Self::Flushed { .. } => "flushed",
            Self::Compacted { .. } => "compacted",
            Self::SnapshotLoaded(_) => "snapshot_loaded",
            Self::SnapshotMissing { .. } => "snapshot_missing",
            Self::Closed => "closed",
            Self::Unsupported { .. } => "unsupported",
            Self::StorageUnavailable { .. } => "storage_unavailable",
            Self::StaleSnapshot { .. } => "stale_snapshot",
            Self::ChecksumMismatch { .. } => "checksum_mismatch",
            Self::FinalizeFailed { .. } => "finalize_failed",
            Self::FlushTimedOut { .. } => "flush_timed_out",
            Self::PartialWrite { .. } => "partial_write",
            Self::CompareAndSwapTokenMismatch { .. } => "cas_token_mismatch",
            Self::SchemaMigrationMismatch { .. } => "schema_migration_mismatch",
            Self::ResourceHandleValidationFailed { .. } => "resource_handle_validation_failed",
            Self::SchemaMigrated { .. } => "schema_migrated",
            Self::ResourceHandlesValidated { .. } => "resource_handles_validated",
            Self::TransactionalBatchCommitted { .. } => "batch_appended",
        }
    }

    /// Returns whether this outcome represents a failed storage operation.
    pub(crate) fn is_failure(&self) -> bool {
        matches!(
            self,
            Self::SnapshotMissing { .. }
                | Self::Unsupported { .. }
                | Self::StorageUnavailable { .. }
                | Self::StaleSnapshot { .. }
                | Self::ChecksumMismatch { .. }
                | Self::FinalizeFailed { .. }
                | Self::FlushTimedOut { .. }
                | Self::PartialWrite { .. }
                | Self::CompareAndSwapTokenMismatch { .. }
                | Self::SchemaMigrationMismatch { .. }
                | Self::ResourceHandleValidationFailed { .. }
        )
    }

    /// Returns whether this outcome represents a successful storage operation.
    pub(crate) fn is_success(&self) -> bool {
        !self.is_failure()
    }

    /// Returns whether this failure requires storage recovery work.
    pub(crate) fn requires_recovery(&self) -> bool {
        matches!(
            self,
            Self::StaleSnapshot { .. }
                | Self::ChecksumMismatch { .. }
                | Self::FinalizeFailed { .. }
                | Self::FlushTimedOut { .. }
                | Self::PartialWrite { .. }
                | Self::CompareAndSwapTokenMismatch { .. }
                | Self::SchemaMigrationMismatch { .. }
                | Self::ResourceHandleValidationFailed { .. }
        )
    }

    /// Returns the stable recovery action for failure outcomes.
    pub(crate) fn recovery_action(&self) -> &'static str {
        match self {
            Self::StaleSnapshot { .. } => "reject_replay",
            Self::ChecksumMismatch { .. } => "repair_snapshot",
            Self::FinalizeFailed { .. } => "retry_finalize",
            Self::FlushTimedOut { .. } => "retry_flush",
            Self::PartialWrite { .. } => "rewrite_checkpoint",
            Self::CompareAndSwapTokenMismatch { .. } => "reload_snapshot",
            Self::SchemaMigrationMismatch { .. } => "reload_schema",
            Self::ResourceHandleValidationFailed { .. } => "recover_resource_handle",
            _ => "",
        }
    }

    /// Returns the source-facing sequence metadata for outcomes that carry it.
    pub(crate) fn sequence(&self) -> u64 {
        match self {
            Self::Appended { sequence, .. }
            | Self::Flushed { sequence }
            | Self::SnapshotLoaded(VmDistributedStorageSnapshot { sequence, .. })
            | Self::ChecksumMismatch { sequence, .. }
            | Self::StaleSnapshot {
                incoming_sequence: sequence,
                ..
            }
            | Self::FinalizeFailed { sequence, .. }
            | Self::FlushTimedOut { sequence, .. }
            | Self::PartialWrite { sequence, .. } => *sequence,
            Self::CompareAndSwapTokenMismatch {
                actual_sequence, ..
            } => *actual_sequence,
            Self::SchemaMigrated { sequence, .. } => *sequence,
            Self::ResourceHandlesValidated { sequence, .. } => *sequence,
            Self::TransactionalBatchCommitted { last_sequence, .. } => *last_sequence,
            _ => 0,
        }
    }

    /// Returns the first sequence committed by a transactional batch.
    pub(crate) fn first_sequence(&self) -> u64 {
        match self {
            Self::TransactionalBatchCommitted { first_sequence, .. } => *first_sequence,
            _ => 0,
        }
    }

    /// Returns the last sequence committed by a transactional batch.
    pub(crate) fn last_sequence(&self) -> u64 {
        match self {
            Self::TransactionalBatchCommitted { last_sequence, .. } => *last_sequence,
            _ => 0,
        }
    }

    /// Returns the number of snapshots committed by a transactional batch.
    pub(crate) fn committed_count(&self) -> usize {
        match self {
            Self::TransactionalBatchCommitted { count, .. } => *count,
            _ => 0,
        }
    }

    /// Returns the expected sequence for compare-and-swap token failures.
    pub(crate) fn expected_sequence(&self) -> u64 {
        match self {
            Self::CompareAndSwapTokenMismatch {
                expected_sequence, ..
            } => *expected_sequence,
            _ => 0,
        }
    }

    /// Returns the actual sequence for compare-and-swap token failures.
    pub(crate) fn actual_sequence(&self) -> u64 {
        match self {
            Self::CompareAndSwapTokenMismatch {
                actual_sequence, ..
            } => *actual_sequence,
            _ => 0,
        }
    }

    /// Returns the expected schema version for schema migration failures.
    pub(crate) fn expected_schema(&self) -> u32 {
        match self {
            Self::SchemaMigrationMismatch {
                expected_schema, ..
            } => *expected_schema,
            _ => 0,
        }
    }

    /// Returns the actual schema version for schema migration outcomes.
    pub(crate) fn actual_schema(&self) -> u32 {
        match self {
            Self::SchemaMigrationMismatch { actual_schema, .. } => *actual_schema,
            Self::SchemaMigrated { schema_version, .. } => *schema_version,
            _ => 0,
        }
    }

    /// Returns the missing resource handle for resource validation failures.
    pub(crate) fn missing_resource_handle(&self) -> &str {
        match self {
            Self::ResourceHandleValidationFailed { missing_handle, .. } => missing_handle,
            _ => "",
        }
    }

    /// Returns the count validated by a successful resource handle check.
    pub(crate) fn validated_resource_count(&self) -> usize {
        match self {
            Self::ResourceHandlesValidated {
                validated_count, ..
            } => *validated_count,
            _ => 0,
        }
    }

    /// Returns the checkpoint id for outcomes that carry one.
    pub(crate) fn checkpoint_id(&self) -> &str {
        match self {
            Self::Appended { checkpoint_id, .. }
            | Self::SnapshotMissing { checkpoint_id }
            | Self::ChecksumMismatch { checkpoint_id, .. }
            | Self::PartialWrite { checkpoint_id, .. } => checkpoint_id,
            Self::SnapshotLoaded(VmDistributedStorageSnapshot { checkpoint_id, .. }) => {
                checkpoint_id
            }
            _ => "",
        }
    }

    /// Returns the local sequence for stale-snapshot outcomes.
    pub(crate) fn local_sequence(&self) -> u64 {
        match self {
            Self::StaleSnapshot { local_sequence, .. } => *local_sequence,
            _ => 0,
        }
    }

    /// Returns the incoming sequence for stale-snapshot outcomes.
    pub(crate) fn incoming_sequence(&self) -> u64 {
        match self {
            Self::StaleSnapshot {
                incoming_sequence, ..
            } => *incoming_sequence,
            _ => 0,
        }
    }

    /// Returns the source-facing checksum metadata for outcomes that carry it.
    pub(crate) fn checksum(&self) -> u32 {
        match self {
            Self::Appended { checksum, .. }
            | Self::SnapshotLoaded(VmDistributedStorageSnapshot { checksum, .. })
            | Self::ChecksumMismatch {
                actual: checksum, ..
            }
            | Self::TransactionalBatchCommitted { checksum, .. } => *checksum,
            _ => 0,
        }
    }

    /// Returns the expected checksum for checksum-mismatch outcomes.
    pub(crate) fn expected_checksum(&self) -> u32 {
        match self {
            Self::ChecksumMismatch { expected, .. } => *expected,
            _ => 0,
        }
    }

    /// Returns the expected entry count for partial-write outcomes.
    pub(crate) fn expected_entries(&self) -> usize {
        match self {
            Self::PartialWrite {
                expected_entries, ..
            } => *expected_entries,
            _ => 0,
        }
    }

    /// Returns the persisted entry count for partial-write outcomes.
    pub(crate) fn persisted_entries(&self) -> usize {
        match self {
            Self::PartialWrite {
                persisted_entries, ..
            } => *persisted_entries,
            _ => 0,
        }
    }

    /// Returns the retained snapshot count for compaction outcomes.
    pub(crate) fn retained_snapshots(&self) -> usize {
        match self {
            Self::Compacted { retained } => *retained,
            _ => 0,
        }
    }

    /// Returns the operation kind for outcomes that carry operation metadata.
    pub(crate) fn operation_kind(&self) -> &'static str {
        match self {
            Self::Unsupported { operation, .. }
            | Self::StorageUnavailable { operation, .. }
            | Self::FinalizeFailed { operation, .. }
            | Self::FlushTimedOut { operation, .. }
            | Self::PartialWrite { operation, .. }
            | Self::ChecksumMismatch { operation, .. }
            | Self::CompareAndSwapTokenMismatch { operation, .. }
            | Self::SchemaMigrationMismatch { operation, .. }
            | Self::ResourceHandleValidationFailed { operation, .. } => operation.kind(),
            _ => "",
        }
    }

    /// Returns the storage mode kind for outcomes that carry backend metadata.
    pub(crate) fn mode_kind(&self) -> &'static str {
        match self {
            Self::Opened { mode }
            | Self::Unsupported { mode, .. }
            | Self::StorageUnavailable { mode, .. } => mode.kind(),
            _ => "",
        }
    }

    /// Returns a stable source-facing failure reason for failure outcomes.
    pub(crate) fn reason(&self) -> &'static str {
        match self {
            Self::SnapshotMissing { .. } => "snapshot_missing",
            Self::Unsupported { .. } => "unsupported_operation",
            Self::StorageUnavailable { .. } => "storage_unavailable",
            Self::StaleSnapshot { .. } => "stale_snapshot",
            Self::ChecksumMismatch { .. } => "checksum_mismatch",
            Self::FinalizeFailed { .. } => "finalize_failed",
            Self::FlushTimedOut { .. } => "flush_timed_out",
            Self::PartialWrite { .. } => "partial_write",
            Self::CompareAndSwapTokenMismatch { .. } => "cas_token_mismatch",
            Self::SchemaMigrationMismatch { .. } => "schema_migration_mismatch",
            Self::ResourceHandleValidationFailed { .. } => "resource_handle_validation_failed",
            _ => "",
        }
    }
}

/// Monotonic compare-and-swap token for adapter append operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct VmDistributedStorageCasToken {
    expected_sequence: u64,
}

#[cfg(test)]
impl VmDistributedStorageCasToken {
    /// Builds a token for the sequence a writer observed before append.
    pub(crate) fn new(expected_sequence: u64) -> Self {
        Self { expected_sequence }
    }

    /// Returns the sequence the token expects to still be current.
    pub(crate) fn expected_sequence(self) -> u64 {
        self.expected_sequence
    }
}

/// Capability proof that ordinary appends commit atomically at one sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct VmDistributedStorageAtomicAppendProof {
    observed_sequence: u64,
}

#[cfg(test)]
impl VmDistributedStorageAtomicAppendProof {
    /// Builds a proof for the sequence observed before a potential append.
    pub(crate) fn new(observed_sequence: u64) -> Self {
        Self { observed_sequence }
    }

    /// Returns the adapter sequence covered by this proof.
    pub(crate) fn observed_sequence(self) -> u64 {
        self.observed_sequence
    }
}

/// Capability proof that one loaded snapshot is isolated from later mutations.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct VmDistributedStorageSnapshotIsolationProof {
    checkpoint_id: String,
    sequence: u64,
    checksum: u32,
}

#[cfg(test)]
impl VmDistributedStorageSnapshotIsolationProof {
    /// Builds a proof from a validated loaded snapshot descriptor.
    pub(crate) fn new(checkpoint_id: impl Into<String>, sequence: u64, checksum: u32) -> Self {
        Self {
            checkpoint_id: checkpoint_id.into(),
            sequence,
            checksum,
        }
    }

    /// Returns the checkpoint id covered by this proof.
    pub(crate) fn checkpoint_id(&self) -> &str {
        &self.checkpoint_id
    }

    /// Returns the isolated snapshot sequence covered by this proof.
    pub(crate) fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the isolated snapshot checksum covered by this proof.
    pub(crate) fn checksum(&self) -> u32 {
        self.checksum
    }
}

/// Capability proof that only successful flushes advance the durable boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct VmDistributedStorageDurableFlushProof {
    flushed_sequence: u64,
}

#[cfg(test)]
impl VmDistributedStorageDurableFlushProof {
    /// Builds a proof for the adapter's last successfully flushed sequence.
    pub(crate) fn new(flushed_sequence: u64) -> Self {
        Self { flushed_sequence }
    }

    /// Returns the sequence covered by this durable flush proof.
    pub(crate) fn flushed_sequence(self) -> u64 {
        self.flushed_sequence
    }
}

/// Capability proof that a batch committed as one all-or-nothing unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VmDistributedStorageTransactionalBatchProof {
    first_sequence: u64,
    last_sequence: u64,
    committed_count: usize,
}

/// Capability proof that a failed transactional batch restored the pre-commit boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VmDistributedStorageTransactionalRollbackProof {
    attempted_count: usize,
    persisted_entries: usize,
    restored_sequence: u64,
}

/// Capability proof that the adapter schema changed through the migration API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VmDistributedStorageSchemaMigrationProof {
    schema_version: u32,
    sequence: u64,
}

/// Capability proof that resource handles were validated through the adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VmDistributedStorageResourceHandleValidationProof {
    validated_count: usize,
    sequence: u64,
}

#[cfg(test)]
impl VmDistributedStorageSchemaMigrationProof {
    /// Builds a proof for the adapter's current schema version and sequence.
    pub(crate) fn new(schema_version: u32, sequence: u64) -> Self {
        Self {
            schema_version,
            sequence,
        }
    }

    /// Returns the schema version covered by this proof.
    pub(crate) fn schema_version(self) -> u32 {
        self.schema_version
    }

    /// Returns the adapter sequence covered by this proof.
    pub(crate) fn sequence(self) -> u64 {
        self.sequence
    }
}

#[cfg(test)]
impl VmDistributedStorageResourceHandleValidationProof {
    /// Builds a proof for the last successful resource handle validation.
    pub(crate) fn new(validated_count: usize, sequence: u64) -> Self {
        Self {
            validated_count,
            sequence,
        }
    }

    /// Returns the number of handles covered by this proof.
    pub(crate) fn validated_count(self) -> usize {
        self.validated_count
    }

    /// Returns the adapter sequence covered by this proof.
    pub(crate) fn sequence(self) -> u64 {
        self.sequence
    }
}

#[cfg(test)]
impl VmDistributedStorageTransactionalBatchProof {
    /// Builds a proof for the last committed transactional batch.
    pub(crate) fn new(first_sequence: u64, last_sequence: u64, committed_count: usize) -> Self {
        Self {
            first_sequence,
            last_sequence,
            committed_count,
        }
    }

    /// Returns the first sequence committed by the batch.
    pub(crate) fn first_sequence(self) -> u64 {
        self.first_sequence
    }

    /// Returns the last sequence committed by the batch.
    pub(crate) fn last_sequence(self) -> u64 {
        self.last_sequence
    }

    /// Returns the number of snapshots committed by the batch.
    pub(crate) fn committed_count(self) -> usize {
        self.committed_count
    }
}

#[cfg(test)]
impl VmDistributedStorageTransactionalRollbackProof {
    /// Builds a proof for one rolled-back transactional batch.
    pub(crate) fn new(
        attempted_count: usize,
        persisted_entries: usize,
        restored_sequence: u64,
    ) -> Self {
        Self {
            attempted_count,
            persisted_entries,
            restored_sequence,
        }
    }

    /// Returns the number of snapshots attempted in the failed batch.
    pub(crate) fn attempted_count(self) -> usize {
        self.attempted_count
    }

    /// Returns the simulated persisted entries observed before rollback.
    pub(crate) fn persisted_entries(self) -> usize {
        self.persisted_entries
    }

    /// Returns the sequence restored by the rollback.
    pub(crate) fn restored_sequence(self) -> u64 {
        self.restored_sequence
    }
}
