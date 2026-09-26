//! SQLite WAL persistence; callers schedule these blocking operations off-shard.

use std::{path::Path, time::Duration};

use rusqlite::limits::Limit;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};

use crate::transaction::{validate_append, validate_batch, validate_migration};
use crate::{AppendOutcome, Checkpoint, StorageError, StorageStatus, MAX_CHECKPOINT_BYTES};

mod format;

/// One thread-owned connection to a configured local-filesystem database.
pub struct CheckpointStore {
    connection: Connection,
}

impl CheckpointStore {
    /// Opens a database at an explicit absolute path with bounded lock waiting.
    ///
    /// Parent-directory authorization and permissions belong to the worker's
    /// capability policy. Callers must not place WAL databases on network filesystems.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        if !path.is_absolute() {
            return Err(StorageError::Invalid(
                "durable storage requires an absolute database path",
            ));
        }
        let mut connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        connection.busy_timeout(Duration::from_millis(100))?;
        // Bound SQLite's own row allocation, not only the subsequent Rust copy.
        connection.set_limit(
            Limit::SQLITE_LIMIT_LENGTH,
            (MAX_CHECKPOINT_BYTES + 4096) as i32,
        )?;
        connection.set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)?;
        connection.pragma_update(None, "trusted_schema", false)?;
        // Initialization and format upgrades must be synchronized too, not only appends.
        connection.pragma_update(None, "synchronous", "FULL")?;
        format::initialize(&mut connection)?;
        let mode: String = connection.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
        if mode != "wal" {
            return Err(StorageError::Invalid("database does not support WAL mode"));
        }
        connection.pragma_update(None, "synchronous", "FULL")?;
        let synchronous: i64 =
            connection.pragma_query_value(None, "synchronous", |row| row.get(0))?;
        if synchronous != 2 {
            return Err(StorageError::Invalid(
                "database rejected full synchronization",
            ));
        }
        Ok(Self { connection })
    }

    /// Returns the committed high-water mark, which compaction never regresses.
    pub fn sequence(&self) -> Result<u64, StorageError> {
        Ok(self.status()?.sequence)
    }

    /// Reads schema and sequence together, without mixing observations from concurrent writers.
    pub fn status(&self) -> Result<StorageStatus, StorageError> {
        read_status(&self.connection)
    }

    /// Reads the persistent identity and committed schema/sequence in one SQL statement.
    /// Identity is not a credential or proof of independent replication.
    pub fn observation(&self) -> Result<crate::StorageObservation, StorageError> {
        format::observation(&self.connection)
    }

    /// Completes a SQLite FULL WAL checkpoint after observing a committed boundary.
    ///
    /// Appends are already synchronized at commit. This explicitly requests WAL
    /// checkpoint completion; a competing reader/writer is an error, not a proof.
    /// Concurrent commits may exceed the returned boundary, and this operation
    /// does not make copying only the main database file a safe backup protocol.
    pub fn flush(&mut self) -> Result<StorageStatus, StorageError> {
        let status = self.status()?;
        let (busy, frames, checkpointed): (i64, i64, i64) =
            self.connection
                .query_row("PRAGMA wal_checkpoint(FULL)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
        if busy != 0 || frames < 0 || checkpointed != frames {
            return Err(StorageError::Busy);
        }
        Ok(status)
    }

    /// Commits a compare-and-swap application schema transition without rewriting payloads.
    ///
    /// Existing checkpoints retain their original schema tags and remain readable.
    /// This operation is not a user-data conversion or a database-format migration.
    pub fn migrate_schema(
        &mut self,
        expected: u32,
        next: u32,
    ) -> Result<StorageStatus, StorageError> {
        if expected == 0 || next <= expected {
            return Err(StorageError::Invalid(
                "schema versions must be nonzero and increase",
            ));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let status = read_status(&transaction)?;
        let next_status = validate_migration(status, expected, next)?;
        transaction.execute(
            "UPDATE storage_meta SET payload_schema=?1, schema_sequence=highest_sequence WHERE singleton=1",
            [next],
        )?;
        transaction.commit()?;
        Ok(next_status)
    }

    /// Atomically appends an ordered batch and checks a durable CAS boundary.
    ///
    /// An exact retry is replayed even after the CAS token becomes stale.
    /// Mixed previously committed/new batches are rejected rather than partly replayed.
    pub fn append(
        &mut self,
        expected_sequence: u64,
        checkpoints: &[Checkpoint],
    ) -> Result<AppendOutcome, StorageError> {
        validate_batch(expected_sequence, checkpoints)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let status = read_status(&transaction)?;
        let decision = validate_append(status, expected_sequence, checkpoints, |checkpoint| {
            Ok(load_checkpoint(&transaction, &checkpoint.id)?.map(|stored| stored == *checkpoint))
        })?;
        if decision == AppendOutcome::Replayed {
            return Ok(AppendOutcome::Replayed);
        }
        for checkpoint in checkpoints {
            let digest = checkpoint.digest();
            transaction.execute("INSERT INTO checkpoints (id,sequence,schema_version,payload,digest) VALUES (?1,?2,?3,?4,?5)",
                params![checkpoint.id, checkpoint.sequence as i64, checkpoint.schema, checkpoint.payload, digest.as_slice()])?;
        }
        transaction.execute(
            "UPDATE storage_meta SET highest_sequence=?1 WHERE singleton=1",
            [checkpoints
                .last()
                .expect("validated nonempty batch")
                .sequence as i64],
        )?;
        transaction.commit()?;
        Ok(AppendOutcome::Committed)
    }

    /// Restores and integrity-checks one immutable checkpoint view.
    pub fn load(&self, id: &str) -> Result<Option<Checkpoint>, StorageError> {
        load_checkpoint(&self.connection, id)
    }

    /// Durably removes retained payloads below a boundary without resetting CAS.
    pub fn compact(
        &mut self,
        retain_from_sequence: u64,
    ) -> Result<crate::CompactionOutcome, StorageError> {
        let boundary = i64::try_from(retain_from_sequence)
            .map_err(|_| StorageError::Invalid("compaction boundary exceeds Terlan Int"))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let deleted =
            transaction.execute("DELETE FROM checkpoints WHERE sequence < ?1", [boundary])?;
        let retained: i64 =
            transaction.query_row("SELECT COUNT(*) FROM checkpoints", [], |row| row.get(0))?;
        let outcome = crate::CompactionOutcome {
            removed: deleted,
            retained: u64::try_from(retained).map_err(|_| StorageError::Corrupt)?,
            sequence: read_status(&transaction)?.sequence,
        };
        transaction.commit()?;
        Ok(outcome)
    }
}

fn read_status(connection: &Connection) -> Result<StorageStatus, StorageError> {
    let (sequence, schema, schema_sequence): (i64, i64, i64) = connection.query_row(
        "SELECT highest_sequence, payload_schema, schema_sequence FROM storage_meta WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    checked_status(sequence, schema, schema_sequence)
}

fn checked_status(
    sequence: i64,
    schema: i64,
    schema_sequence: i64,
) -> Result<StorageStatus, StorageError> {
    let status = StorageStatus {
        sequence: u64::try_from(sequence).map_err(|_| StorageError::Corrupt)?,
        schema: u32::try_from(schema).map_err(|_| StorageError::Corrupt)?,
        schema_sequence: u64::try_from(schema_sequence).map_err(|_| StorageError::Corrupt)?,
    };
    if status.schema == 0 || status.schema_sequence > status.sequence {
        return Err(StorageError::Corrupt);
    }
    Ok(status)
}

fn load_checkpoint(connection: &Connection, id: &str) -> Result<Option<Checkpoint>, StorageError> {
    let row = connection
        .query_row(
            "SELECT sequence,schema_version,payload,digest FROM checkpoints WHERE id=?1",
            [id],
            |row| {
                let payload = row.get_ref(2)?.as_blob()?;
                if payload.len() > MAX_CHECKPOINT_BYTES {
                    return Err(rusqlite::Error::InvalidQuery);
                }
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, u32>(1)?,
                    payload.to_vec(),
                    row.get::<_, Vec<u8>>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((sequence, schema, payload, digest)) = row else {
        return Ok(None);
    };
    let checkpoint = Checkpoint {
        id: id.to_string(),
        sequence: u64::try_from(sequence).map_err(|_| StorageError::Corrupt)?,
        schema,
        payload,
    };
    checkpoint.validate().map_err(|_| StorageError::Corrupt)?;
    let stored: [u8; 32] = digest.try_into().map_err(|_| StorageError::Corrupt)?;
    let actual = checkpoint.digest();
    if stored != actual {
        return Err(StorageError::ChecksumMismatch {
            expected: crate::snapshot::digest_checksum(&stored),
            actual: crate::snapshot::digest_checksum(&actual),
        });
    }
    Ok(Some(checkpoint))
}
