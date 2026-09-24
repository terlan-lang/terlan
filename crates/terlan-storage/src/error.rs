//! Typed storage failures distinguish invalid requests, conflicts, and I/O.

use std::fmt;

/// A failure never implies that an unacknowledged transaction cannot have committed.
#[derive(Debug)]
pub enum StorageError {
    /// Invalid caller metadata or a configured resource limit violation.
    Invalid(&'static str),
    /// Required operating-system randomness is unavailable during identity creation.
    Unavailable,
    /// Another application or unsupported schema owns the database.
    Incompatible,
    /// A durable row has invalid structure or metadata.
    Corrupt,
    /// Full SHA-256 validation failed; compact values are diagnostic prefixes only.
    ChecksumMismatch {
        /// Diagnostic prefix of the digest stored with the checkpoint.
        expected: u32,
        /// Diagnostic prefix calculated from the observed checkpoint bytes.
        actual: u32,
    },
    /// A checkpoint id was already committed with different metadata or bytes.
    Conflict,
    /// A reader or writer prevented completion of the requested WAL checkpoint.
    Busy,
    /// The requested write or migration does not match the durable application schema.
    SchemaMismatch {
        /// Schema supplied by the caller.
        expected: u32,
        /// Schema committed in the database.
        actual: u32,
    },
    /// The caller's compare-and-swap boundary differs from durable state.
    Stale {
        /// Sequence supplied by the caller.
        expected: u64,
        /// Sequence found in the transaction's consistent view.
        actual: u64,
    },
    /// SQLite reported a locking, transaction, filesystem, or decoding failure.
    Database(rusqlite::Error),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(reason) => write!(f, "storage.invalid: {reason}"),
            Self::Unavailable => {
                f.write_str("storage.unavailable: secure identity initialization unavailable")
            }
            Self::Incompatible => f.write_str("storage.incompatible: database identity or format"),
            Self::Corrupt => f.write_str("storage.corrupt: checkpoint integrity check failed"),
            Self::ChecksumMismatch { expected, actual } => write!(
                f,
                "storage.checksum_mismatch: expected {expected}, found {actual}"
            ),
            Self::Conflict => f.write_str("storage.conflict: checkpoint identity was reused"),
            Self::Busy => f.write_str("storage.busy: WAL checkpoint could not complete"),
            Self::SchemaMismatch { expected, actual } => {
                write!(
                    f,
                    "storage.schema_mismatch: expected {expected}, found {actual}"
                )
            }
            Self::Stale { expected, actual } => {
                write!(f, "storage.stale: expected {expected}, found {actual}")
            }
            Self::Database(error) => write!(f, "storage.database: {error}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}
