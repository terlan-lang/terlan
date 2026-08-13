pub(super) use super::super::{
    distributed_state::{
        VmDistributedStateEntry, VmDistributedStatePolicy, VmDistributedStateScope,
        VmDistributedStateVersion,
    },
    ReplValue,
};
pub(super) use super::{
    VmDistributedStorageAdapter, VmDistributedStorageMode, VmDistributedStorageOperation,
    VmDistributedStorageOutcome, VmDistributedStoragePolicy,
    VmDistributedStorageResourceHandleValidationProof, VmDistributedStorageSchemaMigrationProof,
    VmDistributedStorageSnapshot,
};

#[cfg(test)]
#[path = "distributed_storage_test/migration_and_recovery.rs"]
mod migration_and_recovery;
use migration_and_recovery::*;
#[cfg(test)]
#[path = "distributed_storage_test/snapshots_and_compaction.rs"]
mod snapshots_and_compaction;
