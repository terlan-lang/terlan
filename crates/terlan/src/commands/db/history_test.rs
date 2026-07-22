use super::{applied_migration_from_decoded_columns, history_column_error};

const APPLIED_AT: &str = "2026-06-19T12:30:00.000000Z";

/// Converts valid VM-decoded columns into applied migration history.
///
/// Inputs:
/// - Canonical version, name, and checksum columns.
///
/// Output:
/// - Test passes when the row becomes an applied migration record.
///
/// Transformation:
/// - Verifies the live status adapter reuses the migration-history row
///   contract instead of accepting driver-specific row details directly.
#[test]
fn applied_migration_from_decoded_columns_accepts_valid_row() {
    let applied = applied_migration_from_decoded_columns(
        1,
        "20260619123000",
        "create_users",
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        APPLIED_AT,
    )
    .expect("row should parse");

    assert_eq!(applied.version, "20260619123000");
    assert_eq!(applied.name, "create_users");
    assert_eq!(applied.applied_at, APPLIED_AT);
}

#[test]
fn history_column_error_preserves_context_and_vm_error() {
    assert_eq!(
        history_column_error(
            2,
            "checksum",
            "error[postgres.decode.column]: Postgres column was not found.",
        ),
        "terlc db status could not read `terlan_schema_migrations` row 2 column `checksum`: error[postgres.decode.column]: Postgres column was not found."
    );
}

/// Rejects invalid migration-history row values.
///
/// Inputs:
/// - Decoded columns whose checksum is not a SHA-256 lowercase hex digest.
///
/// Output:
/// - Test passes when row validation comes from the central migration
///   invariant checker.
///
/// Transformation:
/// - Keeps live database history from bypassing local migration metadata
///   validation rules.
#[test]
fn applied_migration_from_decoded_columns_rejects_invalid_row_content() {
    let error = applied_migration_from_decoded_columns(
        1,
        "20260619123000",
        "create_users",
        "not-a-checksum",
        APPLIED_AT,
    )
    .expect_err("row should fail");

    assert_eq!(
        error,
        "terlc db status found invalid `terlan_schema_migrations` row 1: migration history checksum must be SHA-256 lowercase hex"
    );
}

#[test]
fn applied_migration_from_decoded_columns_rejects_non_utc_timestamp() {
    let error = applied_migration_from_decoded_columns(
        1,
        "20260619123000",
        "create_users",
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "2026-06-19T15:30:00+03:00",
    )
    .expect_err("non-UTC timestamp must fail");

    assert!(error.contains("applied_at must use UTC offset `Z`"));
}

#[test]
fn applied_migration_from_decoded_columns_rejects_noncanonical_zero_offset() {
    let error = applied_migration_from_decoded_columns(
        1,
        "20260619123000",
        "create_users",
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "2026-06-19T12:30:00+00:00",
    )
    .expect_err("noncanonical zero offset must fail");

    assert!(error.contains("applied_at must use UTC offset `Z`"));
}
