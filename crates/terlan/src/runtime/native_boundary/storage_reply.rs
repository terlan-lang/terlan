//! Typed storage-domain failures carried by the existing capability term protocol.
//!
//! A successful transport reply is not necessarily a successful storage operation.
//! This closed record retains conflict metadata without parsing diagnostic strings.

use super::term::NativeBoundaryTerm as Term;

/// Redacted storage failure categories, independent of SQLite diagnostic text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StorageFailure {
    /// Invalid metadata, argument shape, or resource limit.
    Invalid,
    /// Backend initialization could not obtain required operating-system randomness.
    Unavailable,
    /// An unsupported database format or application owns the database.
    Incompatible,
    /// Stored checkpoint integrity verification failed.
    Corrupt,
    /// Full digest mismatch; values are compact diagnostics, never integrity decisions.
    Checksum {
        /// Prefix of the stored digest.
        expected: u32,
        /// Prefix of the calculated digest.
        actual: u32,
    },
    /// A checkpoint identity was reused with different content.
    Conflict,
    /// WAL checkpoint completion was blocked by a reader or writer.
    Busy,
    /// Database I/O failed; the commit outcome may be indeterminate.
    Database,
    /// The schema read inside the transaction differs from the caller's expectation.
    Schema {
        /// Requested schema.
        expected: u32,
        /// Transaction-observed schema.
        actual: u32,
    },
    /// The sequence read inside the transaction differs from the caller's CAS token.
    Sequence {
        /// Requested sequence.
        expected: u64,
        /// Transaction-observed sequence.
        actual: u64,
    },
}

impl StorageFailure {
    /// Encodes only bounded typed metadata; database paths and diagnostics never cross.
    pub(crate) fn into_term(self) -> Result<Term, &'static str> {
        if matches!(
            self,
            Self::Schema { expected: 0, .. } | Self::Schema { actual: 0, .. }
        ) {
            return Err("zero storage schema");
        }
        let (kind, expected, actual) = match self {
            Self::Invalid => ("invalid", 0, 0),
            Self::Unavailable => ("unavailable", 0, 0),
            Self::Incompatible => ("incompatible", 0, 0),
            Self::Corrupt => ("corrupt", 0, 0),
            Self::Conflict => ("conflict", 0, 0),
            Self::Busy => ("busy", 0, 0),
            Self::Database => ("database", 0, 0),
            Self::Checksum { expected, actual } => {
                ("checksum", u64::from(expected), u64::from(actual))
            }
            Self::Schema { expected, actual } => ("schema", u64::from(expected), u64::from(actual)),
            Self::Sequence { expected, actual } => ("sequence", expected, actual),
        };
        Ok(Term::Record {
            name: "StorageFailureV1".into(),
            fields: vec![
                ("kind".into(), Term::Text(kind.into())),
                (
                    "expected".into(),
                    Term::Int(
                        i64::try_from(expected).map_err(|_| "storage failure sequence overflow")?,
                    ),
                ),
                (
                    "actual".into(),
                    Term::Int(
                        i64::try_from(actual).map_err(|_| "storage failure sequence overflow")?,
                    ),
                ),
            ],
        })
    }

    /// Decodes the exact closed record; malformed or unknown failure records fail closed.
    pub(crate) fn from_term(term: &Term) -> Result<Option<Self>, &'static str> {
        let Term::Record { name, fields } = term else {
            return Ok(None);
        };
        if name != "StorageFailureV1" {
            return Err("unknown storage reply record");
        }
        let [(kind_key, Term::Text(kind)), (expected_key, Term::Int(expected)), (actual_key, Term::Int(actual))] =
            fields.as_slice()
        else {
            return Err("invalid storage failure fields");
        };
        if kind_key != "kind"
            || expected_key != "expected"
            || actual_key != "actual"
            || *expected < 0
            || *actual < 0
        {
            return Err("invalid storage failure metadata");
        }
        let failure = match kind.as_str() {
            "checksum" => Self::Checksum {
                expected: u32::try_from(*expected).map_err(|_| "storage checksum overflow")?,
                actual: u32::try_from(*actual).map_err(|_| "storage checksum overflow")?,
            },
            "schema" => {
                let expected = u32::try_from(*expected).map_err(|_| "storage schema overflow")?;
                let actual = u32::try_from(*actual).map_err(|_| "storage schema overflow")?;
                if expected == 0 || actual == 0 {
                    return Err("zero storage schema");
                }
                Self::Schema { expected, actual }
            }
            "sequence" => Self::Sequence {
                expected: *expected as u64,
                actual: *actual as u64,
            },
            _ if *expected != 0 || *actual != 0 => {
                return Err("unexpected storage failure counters")
            }
            "invalid" => Self::Invalid,
            "unavailable" => Self::Unavailable,
            "incompatible" => Self::Incompatible,
            "corrupt" => Self::Corrupt,
            "conflict" => Self::Conflict,
            "busy" => Self::Busy,
            "database" => Self::Database,
            _ => return Err("unknown storage failure category"),
        };
        Ok(Some(failure))
    }
}

impl From<terlan_storage::StorageError> for StorageFailure {
    fn from(error: terlan_storage::StorageError) -> Self {
        use terlan_storage::StorageError;
        match error {
            StorageError::Invalid(_) => Self::Invalid,
            StorageError::Unavailable => Self::Unavailable,
            StorageError::Incompatible => Self::Incompatible,
            StorageError::Corrupt => Self::Corrupt,
            StorageError::ChecksumMismatch { expected, actual } => {
                Self::Checksum { expected, actual }
            }
            StorageError::Conflict => Self::Conflict,
            StorageError::Busy => Self::Busy,
            StorageError::SchemaMismatch { expected, actual } => Self::Schema { expected, actual },
            StorageError::Stale { expected, actual } => Self::Sequence { expected, actual },
            StorageError::Database(_) => Self::Database,
        }
    }
}

#[cfg(test)]
#[path = "storage_reply_test.rs"]
mod tests;
