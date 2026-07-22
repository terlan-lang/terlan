//! Migration-history adapter boundary for `terlc db`.

use super::migration::{
    applied_migration_from_history_row, migration_history_select_sql, migration_history_table_sql,
    AppliedMigration, MIGRATION_HISTORY_TABLE,
};
use super::ResolvedDatabaseConfig;
use crate::runtime::vm::{
    postgres::{VmPostgresRow, VmPostgresTransaction},
    postgres_command::VmPostgresCommandClient,
};

/// Loads applied migration history through the VM-owned Postgres worker.
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
///   the VM Postgres worker, and normalizes row values through the same validator
///   used by pure status tests.
pub(super) fn load_applied_migration_history(
    config: &ResolvedDatabaseConfig,
) -> Result<Vec<AppliedMigration>, String> {
    let mut client =
        VmPostgresCommandClient::connect(&config.config).map_err(postgres_history_error)?;
    client
        .batch_execute(&migration_history_table_sql())
        .map_err(postgres_history_error)?;
    let rows = client
        .query(&migration_history_select_sql(), Vec::new())
        .map_err(postgres_history_error)?;
    decode_applied_migration_rows(&mut client, rows)
}

/// Loads applied migration history inside an existing locked transaction.
pub(super) fn load_applied_migration_history_transaction(
    client: &mut VmPostgresCommandClient,
    transaction: VmPostgresTransaction,
) -> Result<Vec<AppliedMigration>, String> {
    let rows = client
        .query_transaction(transaction, &migration_history_select_sql(), Vec::new())
        .map_err(postgres_history_error)?;
    decode_applied_migration_rows(client, rows)
}

fn decode_applied_migration_rows(
    client: &mut VmPostgresCommandClient,
    rows: Vec<VmPostgresRow>,
) -> Result<Vec<AppliedMigration>, String> {
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| applied_migration_from_vm_row(client, index + 1, row))
        .collect()
}

/// Converts one VM Postgres row into an applied migration row.
///
/// Inputs:
/// - `line_number`: one-based row number for diagnostics.
/// - `row`: opaque VM Postgres row returned by the maintained worker.
///
/// Output:
/// - Applied migration row when version, name, and checksum are present and
///   valid.
/// - User-facing diagnostic for missing, mistyped, or invalid row content.
///
/// Transformation:
/// - Reads typed columns through VM row handles, then delegates
///   invariant validation to `applied_migration_from_history_row`.
fn applied_migration_from_vm_row(
    client: &mut VmPostgresCommandClient,
    line_number: usize,
    row: VmPostgresRow,
) -> Result<AppliedMigration, String> {
    let version = history_string_column(client, line_number, row, "version")?;
    let name = history_string_column(client, line_number, row, "name")?;
    let checksum = history_string_column(client, line_number, row, "checksum")?;
    let applied_at = history_string_column(client, line_number, row, "applied_at")?;
    applied_migration_from_decoded_columns(line_number, &version, &name, &checksum, &applied_at)
}

fn applied_migration_from_decoded_columns(
    line_number: usize,
    version: &str,
    name: &str,
    checksum: &str,
    applied_at: &str,
) -> Result<AppliedMigration, String> {
    applied_migration_from_history_row(version, name, checksum, applied_at).map_err(|diagnostic| {
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
/// - `row`: opaque VM Postgres row.
/// - `name`: required column name.
///
/// Output:
/// - String column value.
/// - User-facing diagnostic when the column is missing or not text.
///
/// Transformation:
/// - Wraps adapter diagnostics with migration-history row context.
fn history_string_column(
    client: &mut VmPostgresCommandClient,
    line_number: usize,
    row: VmPostgresRow,
    name: &str,
) -> Result<String, String> {
    client
        .decode_string(row, name)
        .map_err(|error| history_column_error(line_number, name, &error))
}

fn history_column_error(line_number: usize, name: &str, error: &str) -> String {
    format!(
        "terlc db status could not read `{MIGRATION_HISTORY_TABLE}` row {line_number} column `{name}`: {error}"
    )
}

/// Formats a VM Postgres history error.
///
/// Inputs:
/// - `error`: stable Postgres adapter error.
///
/// Output:
/// - User-facing status diagnostic with stable error code.
///
/// Transformation:
/// - Keeps database URL and driver details out of command formatting while
///   preserving the VM worker error code.
fn postgres_history_error(error: String) -> String {
    format!("terlc db status failed to read `{MIGRATION_HISTORY_TABLE}`: {error}")
}

#[cfg(test)]
#[path = "history_test.rs"]
mod history_test;
