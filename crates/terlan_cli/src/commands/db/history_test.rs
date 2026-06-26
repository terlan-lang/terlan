use super::applied_migration_from_postgres_row;
use crate::commands::db::migration::MIGRATION_HISTORY_TABLE;
use terlan_safenative::postgres;

/// Builds one migration-history row fixture.
///
/// Inputs:
/// - `version`: migration timestamp value.
/// - `name`: migration name value.
/// - `checksum`: migration checksum value.
///
/// Output:
/// - SafeNative Postgres row containing the supplied history columns.
///
/// Transformation:
/// - Uses the same row shape returned by the maintained Postgres adapter so
///   status parsing tests do not depend on external command output.
fn history_row(version: &str, name: &str, checksum: &str) -> postgres::Row {
    let mut row = postgres::Row::new();
    row.put_string("version", version);
    row.put_string("name", name);
    row.put_string("checksum", checksum);
    row
}

/// Converts a valid SafeNative Postgres row into applied migration history.
///
/// Inputs:
/// - SafeNative row containing canonical version, name, and checksum columns.
///
/// Output:
/// - Test passes when the row becomes an applied migration record.
///
/// Transformation:
/// - Verifies the live status adapter reuses the migration-history row
///   contract instead of accepting driver-specific row details directly.
#[test]
fn applied_migration_from_postgres_row_accepts_valid_row() {
    let row = history_row(
        "20260619123000",
        "create_users",
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    );

    let applied = applied_migration_from_postgres_row(1, &row).expect("row should parse");

    assert_eq!(applied.version, "20260619123000");
    assert_eq!(applied.name, "create_users");
}

/// Rejects missing migration-history columns.
///
/// Inputs:
/// - SafeNative row without the required `checksum` column.
///
/// Output:
/// - Test passes when the diagnostic names the history table, row, and column.
///
/// Transformation:
/// - Locks the maintained-adapter row failure shape used by `terlc db status`.
#[test]
fn applied_migration_from_postgres_row_rejects_missing_column() {
    let mut row = postgres::Row::new();
    row.put_string("version", "20260619123000");
    row.put_string("name", "create_users");

    let error = applied_migration_from_postgres_row(1, &row).expect_err("row should fail");

    assert_eq!(
        error,
        format!(
            "terlc db status could not read `{MIGRATION_HISTORY_TABLE}` row 1 column `checksum`: error[postgres.row.missing_column]: Postgres row does not contain column `checksum`."
        )
    );
}

/// Rejects invalid migration-history row values.
///
/// Inputs:
/// - SafeNative row whose checksum is not a SHA-256 lowercase hex digest.
///
/// Output:
/// - Test passes when row validation comes from the central migration
///   invariant checker.
///
/// Transformation:
/// - Keeps live database history from bypassing local migration metadata
///   validation rules.
#[test]
fn applied_migration_from_postgres_row_rejects_invalid_row_content() {
    let row = history_row("20260619123000", "create_users", "not-a-checksum");

    let error = applied_migration_from_postgres_row(1, &row).expect_err("row should fail");

    assert_eq!(
        error,
        "terlc db status found invalid `terlan_schema_migrations` row 1: migration history checksum must be SHA-256 lowercase hex"
    );
}
