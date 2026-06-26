//! Migration-history adapter boundary for `terlc db`.
//!
//! This module owns the command-facing seam between validated local migration
//! files and database-applied migration history. It uses Terlan's maintained
//! Rust/Tokio Postgres adapter so status reporting does not depend on external
//! shell tools.

use super::migration::{
    applied_migration_from_history_row, migration_history_select_sql, migration_history_table_sql,
    AppliedMigration, MIGRATION_HISTORY_TABLE,
};
use super::ResolvedDatabaseConfig;
use terlan_safenative::postgres;

/// Loads applied migration history through the SafeNative Postgres seam.
///
/// Inputs:
/// - `config`: resolved database configuration for the status command.
///
/// Output:
/// - Applied migration history rows read from the canonical history table.
/// - User-facing diagnostic when the maintained Postgres adapter cannot read
///   the history table.
///
/// Transformation:
/// - Ensures the canonical history table exists, reads rows through
///   SafeNative Postgres, and normalizes row values through the same validator
///   used by pure status tests.
pub(super) fn load_applied_migration_history(
    config: &ResolvedDatabaseConfig,
) -> Result<Vec<AppliedMigration>, String> {
    let pool = postgres::connect(&config.config).map_err(postgres_history_error)?;
    postgres::batch_execute(&pool, &migration_history_table_sql())
        .map_err(postgres_history_error)?;
    let rows = postgres::query(&pool, &migration_history_select_sql(), &[])
        .map_err(postgres_history_error)?;
    rows.iter()
        .enumerate()
        .map(|(index, row)| applied_migration_from_postgres_row(index + 1, row))
        .collect()
}

/// Converts one SafeNative Postgres row into an applied migration row.
///
/// Inputs:
/// - `line_number`: one-based row number for diagnostics.
/// - `row`: SafeNative Postgres row returned by the maintained adapter.
///
/// Output:
/// - Applied migration row when version, name, and checksum are present and
///   valid.
/// - User-facing diagnostic for missing, mistyped, or invalid row content.
///
/// Transformation:
/// - Reads typed columns through SafeNative row accessors, then delegates
///   invariant validation to `applied_migration_from_history_row`.
fn applied_migration_from_postgres_row(
    line_number: usize,
    row: &postgres::Row,
) -> Result<AppliedMigration, String> {
    let version = history_string_column(line_number, row, "version")?;
    let name = history_string_column(line_number, row, "name")?;
    let checksum = history_string_column(line_number, row, "checksum")?;
    applied_migration_from_history_row(&version, &name, &checksum).map_err(|diagnostic| {
        format!(
            "terlc db status found invalid `{MIGRATION_HISTORY_TABLE}` row {line_number}: {}",
            diagnostic.message
        )
    })
}

/// Reads one string column from a migration-history row.
///
/// Inputs:
/// - `line_number`: one-based row number for diagnostics.
/// - `row`: SafeNative Postgres row.
/// - `name`: required column name.
///
/// Output:
/// - String column value.
/// - User-facing diagnostic when the column is missing or not text.
///
/// Transformation:
/// - Wraps adapter diagnostics with migration-history row context.
fn history_string_column(
    line_number: usize,
    row: &postgres::Row,
    name: &str,
) -> Result<String, String> {
    postgres::string(row, name).map_err(|error| {
        format!(
            "terlc db status could not read `{MIGRATION_HISTORY_TABLE}` row {line_number} column `{name}`: error[{}]: {}",
            error.code(),
            error.message()
        )
    })
}

/// Formats a SafeNative Postgres history error.
///
/// Inputs:
/// - `error`: stable Postgres adapter error.
///
/// Output:
/// - User-facing status diagnostic with stable error code.
///
/// Transformation:
/// - Keeps database URL and driver details out of command formatting while
///   preserving the SafeNative error code.
fn postgres_history_error(error: postgres::PostgresError) -> String {
    format!(
        "terlc db status failed to read `{MIGRATION_HISTORY_TABLE}`: error[{}]: {}",
        error.code(),
        error.message()
    )
}

#[cfg(test)]
#[path = "history_test.rs"]
mod history_test;
