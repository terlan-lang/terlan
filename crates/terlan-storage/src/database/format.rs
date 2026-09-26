//! Atomic format admission and a persistent, non-authorizing logical instance identity.

use super::*;
use crate::StorageObservation;

const APPLICATION_ID: i64 = 0x544c5354;
const FORMAT_VERSION: i64 = 3;

pub(super) fn initialize(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let application: i64 =
        transaction.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let version: i64 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if application == 0 && version == 0 {
        let objects: i64 = transaction.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        if objects != 0 {
            return Err(StorageError::Incompatible);
        }
        transaction.execute_batch("CREATE TABLE storage_meta (singleton INTEGER PRIMARY KEY CHECK(singleton=1), highest_sequence INTEGER NOT NULL CHECK(highest_sequence>=0), payload_schema INTEGER NOT NULL CHECK(payload_schema>0 AND payload_schema<=4294967295), schema_sequence INTEGER NOT NULL CHECK(schema_sequence>=0 AND schema_sequence<=highest_sequence)) STRICT;
            INSERT INTO storage_meta VALUES(1, 0, 1, 0);
            CREATE TABLE checkpoints (id TEXT PRIMARY KEY, sequence INTEGER NOT NULL UNIQUE CHECK(sequence>0), schema_version INTEGER NOT NULL CHECK(schema_version>0), payload BLOB NOT NULL, digest BLOB NOT NULL CHECK(length(digest)=32)) STRICT;")?;
        create_identity(&transaction)?;
        transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
        transaction.pragma_update(None, "user_version", FORMAT_VERSION)?;
    } else if application == APPLICATION_ID && version == 2 {
        // Format 2 has the same checkpoint/schema tables but no instance identity.
        // Validate before adding metadata; the transaction preserves all old data on failure.
        read_status(&transaction)?;
        create_identity(&transaction)?;
        transaction.pragma_update(None, "user_version", FORMAT_VERSION)?;
    } else if application != APPLICATION_ID || version != FORMAT_VERSION {
        return Err(StorageError::Incompatible);
    }
    observation(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn create_identity(connection: &Connection) -> Result<(), StorageError> {
    let mut identity = [0u8; 32];
    getrandom::fill(&mut identity).map_err(|_| StorageError::Unavailable)?;
    if identity == [0; 32] {
        return Err(StorageError::Unavailable);
    }
    connection.execute_batch("CREATE TABLE storage_identity (singleton INTEGER PRIMARY KEY CHECK(singleton=1), identity BLOB NOT NULL CHECK(length(identity)=32 AND identity!=zeroblob(32))) STRICT;")?;
    connection.execute("INSERT INTO storage_identity VALUES(1, ?1)", [identity])?;
    Ok(())
}

pub(super) fn observation(connection: &Connection) -> Result<StorageObservation, StorageError> {
    let row = connection.query_row(
        "SELECT i.identity, m.highest_sequence, m.payload_schema, m.schema_sequence FROM storage_identity i JOIN storage_meta m ON i.singleton=m.singleton WHERE i.singleton=1",
        [],
        |row| {
            // Check the borrowed blob's shape before copying or allocating any identity bytes.
            let identity = row.get_ref(0)?.as_blob().ok().and_then(|bytes| <[u8; 32]>::try_from(bytes).ok()).filter(|identity| *identity != [0; 32]);
            Ok((identity, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?))
        },
    ).optional()?.ok_or(StorageError::Corrupt)?;
    Ok(StorageObservation {
        identity: row.0.ok_or(StorageError::Corrupt)?,
        status: checked_status(row.1, row.2, row.3)?,
    })
}
