//! Structured failure metadata round trips without diagnostic parsing or unchecked narrowing.

use super::*;

#[test]
fn storage_failure_round_trips_every_category_and_exact_conflict_metadata(
) -> Result<(), &'static str> {
    for failure in [
        StorageFailure::Invalid,
        StorageFailure::Unavailable,
        StorageFailure::Incompatible,
        StorageFailure::Corrupt,
        StorageFailure::Conflict,
        StorageFailure::Busy,
        StorageFailure::Database,
        StorageFailure::Checksum {
            expected: 0,
            actual: u32::MAX,
        },
        StorageFailure::Schema {
            expected: 1,
            actual: u32::MAX,
        },
        StorageFailure::Sequence {
            expected: 0,
            actual: i64::MAX as u64,
        },
    ] {
        assert_eq!(
            StorageFailure::from_term(&failure.into_term()?)?,
            Some(failure)
        );
    }
    assert_eq!(StorageFailure::from_term(&Term::Int(42))?, None);
    assert!(StorageFailure::Sequence {
        expected: u64::MAX,
        actual: 0
    }
    .into_term()
    .is_err());
    assert!(StorageFailure::Schema {
        expected: 0,
        actual: 1
    }
    .into_term()
    .is_err());
    Ok(())
}

#[test]
fn storage_failure_rejects_unknown_versions_fields_categories_and_counters(
) -> Result<(), &'static str> {
    let valid = StorageFailure::Busy.into_term()?;
    for variant in 0..6 {
        let mut malformed = valid.clone();
        let Term::Record { name, fields } = &mut malformed else {
            return Err("failure must be a record");
        };
        match variant {
            0 => *name = "StorageFailureV2".into(),
            1 => {
                fields.remove(0);
            }
            2 => fields.swap(0, 1),
            3 => fields[0].1 = Term::Text("unknown".into()),
            4 => fields[1].1 = Term::Int(-1),
            _ => fields[2].1 = Term::Int(1),
        }
        assert!(StorageFailure::from_term(&malformed).is_err());
    }
    Ok(())
}
