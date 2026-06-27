use std::collections::{BTreeMap, BTreeSet};

use super::migration::{
    is_valid_migration_name, migration_engine_inputs, LoadedMigration, MigrationDiagnostic,
    MigrationEngineInput,
};
use crate::support::is_valid_sha256_hex;

/// Canonical Postgres migration-history table name.
pub(crate) const MIGRATION_HISTORY_TABLE: &str = "terlan_schema_migrations";

/// Applied migration history row.
///
/// Inputs:
/// - Produced by a migration-history database reader.
///
/// Output:
/// - `version`: migration timestamp recorded as applied.
/// - `name`: descriptive migration name recorded as applied.
/// - `checksum`: full-file SHA-256 checksum recorded when applied.
///
/// Transformation:
/// - Gives the pure status comparator a database-independent shape so tests
///   can lock applied/pending/missing/divergent semantics independently from
///   live Postgres access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppliedMigration {
    pub(crate) version: String,
    pub(crate) name: String,
    pub(crate) checksum: String,
}

/// Migration status entry.
///
/// Inputs:
/// - Produced by `migration_status` from loaded local files and applied
///   migration history.
///
/// Output:
/// - `version`: migration timestamp.
/// - `name`: descriptive migration name.
/// - `checksum`: full-file SHA-256 checksum.
/// - `state`: current status classification.
///
/// Transformation:
/// - Captures the stable status row shape used by both filesystem-only and
///   database-backed status reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MigrationStatusEntry {
    pub(crate) version: String,
    pub(crate) name: String,
    pub(crate) checksum: String,
    pub(crate) state: MigrationStatusState,
}

/// Migration status state.
///
/// Inputs:
/// - Produced by status comparison code.
///
/// Output:
/// - Stable status variants for local and history comparison.
///
/// Transformation:
/// - Defines the status vocabulary independently from command formatting and
///   database execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MigrationStatusState {
    Applied,
    Divergent,
    Missing,
    Pending,
}

impl MigrationStatusState {
    /// Returns the stable CLI label for this status state.
    ///
    /// Inputs:
    /// - `self`: migration status state.
    ///
    /// Output:
    /// - Lowercase status label for human and script-readable command output.
    ///
    /// Transformation:
    /// - Keeps status text centralized so later database-backed states can be
    ///   added without duplicating string literals across command rendering.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Divergent => "divergent",
            Self::Missing => "missing",
            Self::Pending => "pending",
        }
    }
}

/// Returns the canonical Postgres migration-history table DDL.
///
/// Inputs:
/// - None.
///
/// Output:
/// - SQL statement that creates the Terlan migration-history table if absent.
///
/// Transformation:
/// - Centralizes the table contract for SafeNative Postgres migration
///   execution and status history loading.
pub(crate) fn migration_history_table_sql() -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {MIGRATION_HISTORY_TABLE} (
  version TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  checksum TEXT NOT NULL,
  applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
);"
    )
}

/// Returns the canonical migration-history read query.
///
/// Inputs:
/// - None.
///
/// Output:
/// - SQL query that reads applied migration history in deterministic order.
///
/// Transformation:
/// - Centralizes the status adapter query text so Postgres wiring reads the
///   same columns that `applied_migration_from_history_row` validates.
pub(crate) fn migration_history_select_sql() -> String {
    format!("SELECT version, name, checksum FROM {MIGRATION_HISTORY_TABLE} ORDER BY version ASC;")
}

/// Returns the canonical migration-history insert statement.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Parameterized SQL statement for recording one applied migration.
///
/// Transformation:
/// - Keeps the execution adapter aligned with the history row contract while
///   leaving parameter binding/materialization to the selected Postgres client.
pub(crate) fn migration_history_insert_sql() -> String {
    format!("INSERT INTO {MIGRATION_HISTORY_TABLE} (version, name, checksum) VALUES ($1, $2, $3);")
}

/// Converts one database history row into an applied migration.
///
/// Inputs:
/// - `version`: timestamp string read from the migration-history table.
/// - `name`: descriptive migration name read from the table.
/// - `checksum`: full-file SHA-256 checksum read from the table.
///
/// Output:
/// - `Ok(AppliedMigration)` when row values satisfy Terlan's migration
///   metadata contract.
/// - `Err(MigrationDiagnostic)` when database history contains invalid data.
///
/// Transformation:
/// - Reuses the same timestamp/name/checksum invariants as local migration
///   discovery so database-backed status cannot trust malformed history rows.
pub(crate) fn applied_migration_from_history_row(
    version: &str,
    name: &str,
    checksum: &str,
) -> Result<AppliedMigration, MigrationDiagnostic> {
    if version.len() != 14 || !version.chars().all(|character| character.is_ascii_digit()) {
        return Err(diagnostic(
            1,
            "migration history version must use fourteen digits",
        ));
    }
    if !is_valid_migration_name(name) {
        return Err(diagnostic(
            1,
            "migration history name must be snake_case letters, digits, and underscores",
        ));
    }
    if !is_valid_sha256_hex(checksum) {
        return Err(diagnostic(
            1,
            "migration history checksum must be SHA-256 lowercase hex",
        ));
    }

    Ok(AppliedMigration {
        version: version.to_string(),
        name: name.to_string(),
        checksum: checksum.to_string(),
    })
}

/// Selects migration-engine inputs that still need execution.
///
/// Inputs:
/// - `loaded`: validated local migration files in deterministic order.
/// - `applied`: validated migration-history rows loaded from the database.
///
/// Output:
/// - Pending migration-engine inputs when history is compatible with local
///   files.
/// - `Err(MigrationDiagnostic)` when history contains missing or divergent
///   migrations that must be resolved before executing more SQL.
///
/// Transformation:
/// - Reuses the status comparator to skip already-applied migrations, preserve
///   deterministic pending order, and convert unsafe history states into
///   adapter-blocking diagnostics before a live database transaction starts.
pub(crate) fn pending_migration_engine_inputs(
    loaded: &[LoadedMigration],
    applied: &[AppliedMigration],
) -> Result<Vec<MigrationEngineInput>, MigrationDiagnostic> {
    let inputs_by_version = migration_engine_inputs(loaded)
        .into_iter()
        .map(|input| (input.version.clone(), input))
        .collect::<BTreeMap<_, _>>();
    let mut pending = Vec::new();

    for status in migration_status(loaded, applied) {
        match status.state {
            MigrationStatusState::Applied => {}
            MigrationStatusState::Pending => {
                let Some(input) = inputs_by_version.get(&status.version) else {
                    return Err(diagnostic(
                        1,
                        "pending migration has no local execution input",
                    ));
                };
                pending.push(input.clone());
            }
            MigrationStatusState::Missing => {
                return Err(diagnostic(
                    1,
                    "database history contains a migration missing from local files",
                ));
            }
            MigrationStatusState::Divergent => {
                return Err(diagnostic(
                    1,
                    "database history diverges from local migration files",
                ));
            }
        }
    }

    Ok(pending)
}

/// Builds migration status entries from local files and applied history.
///
/// Inputs:
/// - `loaded`: validated, checksummed migration files in deterministic order.
/// - `applied`: migration-history rows loaded from a database or fixture.
///
/// Output:
/// - `MigrationStatusEntry` rows sorted by version.
///
/// Transformation:
/// - Compares local migration files with applied history by version and
///   checksum. Matching rows are `Applied`, local-only rows are `Pending`,
///   history-only rows are `Missing`, and checksum/name mismatches are
///   `Divergent`.
pub(crate) fn migration_status(
    loaded: &[LoadedMigration],
    applied: &[AppliedMigration],
) -> Vec<MigrationStatusEntry> {
    let local_by_version = loaded
        .iter()
        .map(|migration| (migration.file.parsed.version.as_str(), migration))
        .collect::<BTreeMap<_, _>>();
    let applied_by_version = applied
        .iter()
        .map(|migration| (migration.version.as_str(), migration))
        .collect::<BTreeMap<_, _>>();
    let versions = local_by_version
        .keys()
        .chain(applied_by_version.keys())
        .copied()
        .collect::<BTreeSet<_>>();

    versions
        .into_iter()
        .map(|version| {
            match (
                local_by_version.get(version),
                applied_by_version.get(version),
            ) {
                (Some(local), Some(history)) => {
                    let state = if local.checksum == history.checksum
                        && local.file.parsed.name == history.name
                    {
                        MigrationStatusState::Applied
                    } else {
                        MigrationStatusState::Divergent
                    };
                    MigrationStatusEntry {
                        version: local.file.parsed.version.clone(),
                        name: local.file.parsed.name.clone(),
                        checksum: local.checksum.clone(),
                        state,
                    }
                }
                (Some(local), None) => MigrationStatusEntry {
                    version: local.file.parsed.version.clone(),
                    name: local.file.parsed.name.clone(),
                    checksum: local.checksum.clone(),
                    state: MigrationStatusState::Pending,
                },
                (None, Some(history)) => MigrationStatusEntry {
                    version: history.version.clone(),
                    name: history.name.clone(),
                    checksum: history.checksum.clone(),
                    state: MigrationStatusState::Missing,
                },
                (None, None) => unreachable!("version set is built from local and applied maps"),
            }
        })
        .collect()
}

/// Builds a migration status/history diagnostic.
///
/// Inputs:
/// - `line`: one-based source line number for compatibility with migration
///   diagnostics.
/// - `message`: stable diagnostic message.
///
/// Output:
/// - `MigrationDiagnostic` containing both fields.
///
/// Transformation:
/// - Allocates the message once at the status/history boundary.
fn diagnostic(line: usize, message: &str) -> MigrationDiagnostic {
    MigrationDiagnostic {
        line,
        message: message.to_string(),
    }
}
