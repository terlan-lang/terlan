//! Shared transaction decisions; backend ownership controls when the plan is committed.

use crate::{
    AppendOutcome, Checkpoint, StorageError, StorageStatus, MAX_BATCH_BYTES, MAX_BATCH_CHECKPOINTS,
};
use std::collections::BTreeSet;

pub(crate) fn validate_batch(
    expected: u64,
    checkpoints: &[Checkpoint],
) -> Result<(), StorageError> {
    if checkpoints.is_empty() || checkpoints.len() > MAX_BATCH_CHECKPOINTS {
        return Err(StorageError::Invalid(
            "batch must contain 1..=1024 checkpoints",
        ));
    }
    let mut previous = expected;
    let mut ids = BTreeSet::new();
    let mut bytes = 0usize;
    for checkpoint in checkpoints {
        checkpoint.validate()?;
        if checkpoint.sequence <= previous || !ids.insert(&checkpoint.id) {
            return Err(StorageError::Invalid(
                "batch sequences must increase and ids must be unique",
            ));
        }
        previous = checkpoint.sequence;
        bytes = bytes
            .checked_add(checkpoint.payload.len())
            .ok_or(StorageError::Invalid("batch size overflow"))?;
        if bytes > MAX_BATCH_BYTES {
            return Err(StorageError::Invalid("batch payload exceeds limit"));
        }
    }
    Ok(())
}

/// Checks replay, conflicts, CAS, and schema without modifying either backend.
pub(crate) fn validate_append(
    status: StorageStatus,
    expected: u64,
    checkpoints: &[Checkpoint],
    mut existing: impl FnMut(&Checkpoint) -> Result<Option<bool>, StorageError>,
) -> Result<AppendOutcome, StorageError> {
    let mut retained = 0;
    for checkpoint in checkpoints {
        match existing(checkpoint)? {
            Some(false) => return Err(StorageError::Conflict),
            Some(true) => retained += 1,
            None => {}
        }
    }
    if retained == checkpoints.len() {
        return Ok(AppendOutcome::Replayed);
    }
    if retained != 0 {
        return Err(StorageError::Conflict);
    }
    if status.sequence != expected {
        return Err(StorageError::Stale {
            expected,
            actual: status.sequence,
        });
    }
    for checkpoint in checkpoints {
        if checkpoint.schema != status.schema {
            return Err(StorageError::SchemaMismatch {
                expected: checkpoint.schema,
                actual: status.schema,
            });
        }
    }
    Ok(AppendOutcome::Committed)
}

/// Computes a schema transition; old checkpoints keep their original tags.
pub(crate) fn validate_migration(
    status: StorageStatus,
    expected: u32,
    next: u32,
) -> Result<StorageStatus, StorageError> {
    if expected == 0 || next <= expected {
        return Err(StorageError::Invalid(
            "schema versions must be nonzero and increase",
        ));
    }
    if status.schema != expected {
        return Err(StorageError::SchemaMismatch {
            expected,
            actual: status.schema,
        });
    }
    Ok(StorageStatus {
        schema: next,
        schema_sequence: status.sequence,
        ..status
    })
}
