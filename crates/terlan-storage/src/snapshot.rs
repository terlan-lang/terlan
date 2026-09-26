//! Canonical, bounded checkpoint metadata and collision-resistant integrity.

use sha2::{Digest, Sha256};

use crate::{StorageError, MAX_CHECKPOINT_BYTES};

/// Immutable logical checkpoint supplied to or restored from a durable store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    /// Application checkpoint identity, not a filesystem path.
    pub id: String,
    /// Nonzero monotonic storage sequence, representable by Terlan `Int`.
    pub sequence: u64,
    /// Nonzero application payload schema version.
    pub schema: u32,
    /// Encoded logical state; the VM owns type/resource/atom validation.
    pub payload: Vec<u8>,
}

impl Checkpoint {
    /// Validates limits before opening a transaction or encoding a digest.
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.id.is_empty() || self.id.len() > 1024 || self.id.contains('\0') {
            return Err(StorageError::Invalid(
                "checkpoint id must be 1..=1024 UTF-8 bytes without NUL",
            ));
        }
        if self.sequence == 0 || self.sequence > i64::MAX as u64 || self.schema == 0 {
            return Err(StorageError::Invalid(
                "checkpoint sequence/schema outside supported range",
            ));
        }
        if self.payload.len() > MAX_CHECKPOINT_BYTES {
            return Err(StorageError::Invalid("checkpoint payload exceeds limit"));
        }
        Ok(())
    }

    /// Returns the leading 32 SHA-256 bits for source-level diagnostic metadata.
    ///
    /// This compact checksum is not an integrity or authentication decision.
    /// Persistent reads always compare all 256 digest bits.
    pub fn checksum(&self) -> u32 {
        digest_checksum(&self.digest())
    }

    /// Returns the domain-separated SHA-256 binding of identity, schema, sequence, and bytes.
    ///
    /// A digest detects accidental corruption, not an authorized writer's tampering.
    pub fn digest(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"terlan.storage.checkpoint.v1\0");
        hash.update((self.id.len() as u64).to_be_bytes());
        hash.update(self.id.as_bytes());
        hash.update(self.sequence.to_be_bytes());
        hash.update(self.schema.to_be_bytes());
        hash.update((self.payload.len() as u64).to_be_bytes());
        hash.update(&self.payload);
        hash.finalize().into()
    }
}

pub(crate) fn digest_checksum(digest: &[u8; 32]) -> u32 {
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]])
}
