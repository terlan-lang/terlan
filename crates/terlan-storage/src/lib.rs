//! Bounded logical checkpoints with explicitly distinct durable and volatile stores.
//!
//! This crate contains no actor scheduler. The SQLite backend runs only on a
//! blocking storage capability worker, never a VM shard owner. The pure Rust
//! local store performs bounded, owner-local work and makes no durability claim.
//! SQLite owns durable transactions, locking, and crash recovery. Checkpoint payloads
//! are bounded encoded logical state, never live heaps or native pointers.

mod database;
mod error;
mod local;
mod snapshot;
mod transaction;
pub use local::LocalCheckpointStore;

pub use database::CheckpointStore;
pub use error::StorageError;
pub use snapshot::Checkpoint;

/// One transaction-consistent observation of a logical store and its committed boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageObservation {
    /// Persistent random instance identity; public metadata, never authentication authority.
    /// A byte-for-byte database backup retains this logical identity.
    pub identity: [u8; 32],
    /// Schema and sequence read in the same database snapshot as the identity.
    pub status: StorageStatus,
}

/// A transaction-consistent view of committed data and application schema metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageStatus {
    /// Highest committed checkpoint sequence, never lowered by compaction.
    pub sequence: u64,
    /// Application payload schema admitted for new checkpoints.
    pub schema: u32,
    /// Committed checkpoint boundary observed by the last schema transition.
    pub schema_sequence: u64,
}

/// Committed compaction facts observed in the deletion transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompactionOutcome {
    /// Number of rows removed by this operation.
    pub removed: usize,
    /// Number of checkpoint rows retained at commit.
    pub retained: u64,
    /// Committed sequence high-water mark, which compaction never lowers.
    pub sequence: u64,
}

/// Maximum encoded checkpoint payload admitted before storage allocation.
pub const MAX_CHECKPOINT_BYTES: usize = 16 * 1024 * 1024;

/// Maximum total payload bytes in one atomic append batch.
pub const MAX_BATCH_BYTES: usize = 64 * 1024 * 1024;

/// Maximum number of checkpoints in one atomic append batch.
pub const MAX_BATCH_CHECKPOINTS: usize = 1024;

/// Result of one backend append, including retry-safe identical replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppendOutcome {
    /// All new checkpoints and the sequence boundary committed atomically.
    Committed,
    /// Every requested checkpoint already exists with identical metadata/bytes.
    Replayed,
}

#[cfg(test)]
#[path = "storage_test.rs"]
mod tests;

#[cfg(test)]
mod schema_test;

#[cfg(test)]
mod flush_test;

#[cfg(test)]
mod identity_test;
