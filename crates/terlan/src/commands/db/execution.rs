//! Migration execution adapter boundary for `terlc db`.

use std::collections::BTreeMap;

use super::history::load_applied_migration_history_transaction;
use super::migration::{
    migration_history_insert_sql, migration_history_table_sql, migration_matches_applied,
    migration_out_of_order_message, AppliedMigration, MigrationEngineInput,
};
use super::ResolvedDatabaseConfig;
use crate::runtime::vm::{
    postgres::{VmPostgresDecodedValue, VmPostgresTransaction},
    postgres_command::VmPostgresCommandClient,
};
use crate::terlan_native::json;

pub(super) const MIGRATION_LOCK_SQL: &str =
    "SELECT pg_try_advisory_xact_lock(1413829196, 1296648018) AS acquired;";

/// Database migration execution request.
///
/// Inputs:
/// - Built by `terlc db migrate`, `terlc db rebuild --dev`, or
///   `terlc db reset --dev` after local migration validation and database URL
///   validation.
///
/// Output:
/// - Borrowed request data passed to a concrete migration executor.
///
/// Transformation:
/// - Groups command name, redacted database configuration, pending migration
///   inputs, and destructive-command mode without opening database sockets.
#[derive(Debug)]
pub(super) struct MigrationExecutionRequest<'a> {
    command: &'a str,
    config: &'a ResolvedDatabaseConfig,
    pending: &'a [MigrationEngineInput],
    destructive: bool,
}

impl<'a> MigrationExecutionRequest<'a> {
    /// Builds a migration execution request.
    ///
    /// Inputs:
    /// - `command`: command name such as `migrate`, `rebuild`, or `reset`.
    /// - `config`: resolved database configuration.
    /// - `pending`: migration inputs selected for execution.
    /// - `destructive`: whether the command may drop/reset database state.
    ///
    /// Output:
    /// - Request value borrowed from command-local data.
    ///
    /// Transformation:
    /// - Preserves the data a live adapter needs while keeping the command
    ///   runner independent from the concrete database client.
    pub(super) fn new(
        command: &'a str,
        config: &'a ResolvedDatabaseConfig,
        pending: &'a [MigrationEngineInput],
        destructive: bool,
    ) -> Self {
        Self {
            command,
            config,
            pending,
            destructive,
        }
    }

    /// Returns the command name.
    ///
    /// Inputs:
    /// - `self`: migration execution request.
    ///
    /// Output:
    /// - Borrowed command name.
    ///
    /// Transformation:
    /// - Exposes the command label for diagnostics and adapter dispatch.
    pub(super) fn command(&self) -> &str {
        self.command
    }

    /// Returns the resolved database configuration.
    ///
    /// Inputs:
    /// - `self`: migration execution request.
    ///
    /// Output:
    /// - Borrowed database configuration.
    ///
    /// Transformation:
    /// - Exposes configuration to the adapter without moving ownership out of
    ///   the command layer.
    pub(super) fn config(&self) -> &ResolvedDatabaseConfig {
        self.config
    }

    /// Returns the pending migration inputs.
    ///
    /// Inputs:
    /// - `self`: migration execution request.
    ///
    /// Output:
    /// - Borrowed pending migration slice in deterministic execution order.
    ///
    /// Transformation:
    /// - Exposes local migration SQL and checksums selected by the pure
    ///   planning layer.
    pub(super) fn pending(&self) -> &[MigrationEngineInput] {
        self.pending
    }

    /// Returns whether the request represents destructive development work.
    ///
    /// Inputs:
    /// - `self`: migration execution request.
    ///
    /// Output:
    /// - Boolean destructive-mode flag.
    ///
    /// Transformation:
    /// - Preserves the safety decision already made by the command runner.
    pub(super) fn destructive(&self) -> bool {
        self.destructive
    }
}

/// Successful migration execution report.
///
/// Inputs:
/// - Produced by a concrete migration executor.
///
/// Output:
/// - User-facing count of applied migration inputs.
///
/// Transformation:
/// - Keeps command output independent from driver-specific execution results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MigrationExecutionReport {
    applied: usize,
}

impl MigrationExecutionReport {
    /// Builds a migration execution report.
    ///
    /// Inputs:
    /// - `applied`: number of migrations applied by the adapter.
    ///
    /// Output:
    /// - Report value for command output.
    ///
    /// Transformation:
    /// - Normalizes concrete adapter results to the stable CLI surface.
    pub(super) fn new(applied: usize) -> Self {
        Self { applied }
    }

    /// Returns the applied migration count.
    ///
    /// Inputs:
    /// - `self`: execution report.
    ///
    /// Output:
    /// - Applied migration count.
    ///
    /// Transformation:
    /// - Exposes the stable result field without leaking adapter details.
    pub(super) fn applied(&self) -> usize {
        self.applied
    }
}

/// Migration execution adapter interface.
///
/// Inputs:
/// - Implemented by the live VM-owned Postgres migration adapter.
///
/// Output:
/// - Stable execution result or user-facing diagnostic.
///
/// Transformation:
/// - Decouples command validation/planning from database mutation.
pub(super) trait MigrationExecutor {
    /// Executes one validated migration request.
    ///
    /// Inputs:
    /// - `request`: validated migration execution request.
    ///
    /// Output:
    /// - `Ok(report)` when execution succeeds.
    /// - `Err(message)` when the adapter refuses or fails execution.
    ///
    /// Transformation:
    /// - Concrete adapters apply command semantics to a database target. The trait
    ///   keeps those side effects out of the command router.
    fn execute(
        &self,
        request: MigrationExecutionRequest<'_>,
    ) -> Result<MigrationExecutionReport, String>;
}

/// VM-owned migration executor.
///
/// Inputs:
/// - Receives fully validated migration execution requests from the command
///   layer.
///
/// Output:
/// - Applies pending migrations and returns the applied count.
/// - Returns a user-facing diagnostic when the maintained Postgres adapter
///   cannot execute the generated SQL.
///
/// Transformation:
/// - Opens one transaction for the command, acquires the database-scoped
///   migration lock, revalidates applied history, optionally resets the
///   development schema, and atomically applies migrations with their
///   parameterized history rows.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct VmMigrationExecutor;

impl MigrationExecutor for VmMigrationExecutor {
    /// Executes validated migrations through the VM-owned Postgres worker.
    ///
    /// Inputs:
    /// - `request`: validated migration execution request.
    ///
    /// Output:
    /// - Applied migration report on success.
    /// - Stable connection/database diagnostic on failure.
    ///
    /// Transformation:
    /// - Connects once through the maintained pool, owns one transaction and
    ///   advisory lock for the command, revalidates history after locking, and
    ///   records migration bodies with history through parameter binding.
    fn execute(
        &self,
        request: MigrationExecutionRequest<'_>,
    ) -> Result<MigrationExecutionReport, String> {
        let mut client = VmPostgresCommandClient::connect(&request.config().config)
            .map_err(|error| request_postgres_error_message(&request, error))?;
        let transaction = client
            .begin()
            .map_err(|error| request_postgres_error_message(&request, error))?;
        let applied = match run_locked_migration_request(&request, &mut client, transaction) {
            Ok(applied) => applied,
            Err(error) => {
                let _rollback_result = client.finish_transaction(transaction, false);
                return Err(error);
            }
        };
        client
            .finish_transaction(transaction, true)
            .map_err(|error| request_postgres_error_message(&request, error))?;
        Ok(MigrationExecutionReport::new(applied))
    }
}

fn run_locked_migration_request(
    request: &MigrationExecutionRequest<'_>,
    client: &mut VmPostgresCommandClient,
    transaction: VmPostgresTransaction,
) -> Result<usize, String> {
    acquire_migration_lock(client, transaction)
        .map_err(|error| request_postgres_error_message(request, error))?;
    if request.destructive() {
        run_transaction_batch_for_request(
            request,
            client,
            transaction,
            &development_schema_reset_sql(),
        )?;
    }
    run_transaction_batch_for_request(
        request,
        client,
        transaction,
        &migration_history_table_sql(),
    )?;
    let applied_history = load_applied_migration_history_transaction(client, transaction)
        .map_err(|error| request_postgres_error_message(request, error))?;
    let pending = pending_after_lock(request.pending(), &applied_history)
        .map_err(|error| request_postgres_error_message(request, error))?;
    for migration in &pending {
        run_migration_body_and_history(request, client, transaction, migration)?;
    }
    Ok(pending.len())
}

fn acquire_migration_lock(
    client: &mut VmPostgresCommandClient,
    transaction: VmPostgresTransaction,
) -> Result<(), String> {
    let row = client.query_one_transaction(transaction, MIGRATION_LOCK_SQL, Vec::new())?;
    let acquired =
        match row {
            Some(row) => client.decode_dynamic(row, "acquired")?,
            None => return Err(
                "error[db.migration.lock_protocol]: Postgres returned no migration lock result."
                    .to_string(),
            ),
        };
    validate_migration_lock_result(acquired)
}

fn validate_migration_lock_result(value: VmPostgresDecodedValue) -> Result<(), String> {
    match value {
        VmPostgresDecodedValue::Bool(true) => Ok(()),
        VmPostgresDecodedValue::Bool(false) => Err(
            "error[db.migration.lock_conflict]: Another migration command owns the database lock."
                .to_string(),
        ),
        _ => Err(
            "error[db.migration.lock_protocol]: Postgres returned an invalid migration lock result."
                .to_string(),
        ),
    }
}

fn pending_after_lock<'a>(
    planned: &'a [MigrationEngineInput],
    applied: &[AppliedMigration],
) -> Result<Vec<&'a MigrationEngineInput>, String> {
    let applied_by_version = applied
        .iter()
        .map(|migration| (migration.version.as_str(), migration))
        .collect::<BTreeMap<_, _>>();
    let latest_compatible_applied_version = planned
        .iter()
        .filter_map(|migration| {
            applied_by_version
                .get(migration.version.as_str())
                .filter(|applied| {
                    migration_matches_applied(&migration.name, &migration.checksum, applied)
                })
                .map(|_| migration.version.as_str())
        })
        .max();
    planned
        .iter()
        .filter_map(|migration| match applied_by_version.get(migration.version.as_str()) {
            None
                if latest_compatible_applied_version
                    .is_some_and(|applied_version| migration.version.as_str() < applied_version) =>
            {
                Some(Err(migration_out_of_order_message(&migration.version)))
            }
            None => Some(Ok(migration)),
            Some(applied)
                if migration_matches_applied(&migration.name, &migration.checksum, applied) =>
            {
                None
            }
            Some(_) => Some(Err(format!(
                "error[db.migration.history_divergent]: Migration `{}` changed while waiting for the database lock.",
                migration.version
            ))),
        })
        .collect()
}

/// Builds the development schema reset SQL.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - SQL batch that drops and recreates the public schema.
///
/// Transformation:
/// - Models 0.0.5 `reset --dev` and `rebuild --dev` as a schema-level clean
///   rebuild without dropping the database itself.
fn development_schema_reset_sql() -> String {
    "DROP SCHEMA IF EXISTS public CASCADE;\nCREATE SCHEMA public;".to_string()
}

/// Runs one migration body and history insert inside an open transaction.
///
/// Inputs:
/// - `request`: migration execution request used for diagnostics.
/// - `client`: connected VM Postgres command client.
/// - `transaction`: live typed VM transaction.
/// - `migration`: migration input to apply.
///
/// Output:
/// - Success after user SQL and history insert both succeed.
/// - User-facing diagnostic for either migration body or history failure.
///
/// Transformation:
/// - Uses `batch_execute` for user SQL and `execute` with JSON-backed
///   parameters for the canonical history insert.
fn run_migration_body_and_history(
    request: &MigrationExecutionRequest<'_>,
    client: &mut VmPostgresCommandClient,
    transaction: VmPostgresTransaction,
    migration: &MigrationEngineInput,
) -> Result<(), String> {
    client
        .batch_execute_transaction(transaction, &migration.up_sql)
        .map_err(|error| migration_failed_message(request, migration, error))?;
    let params = vec![
        json::string(&migration.version),
        json::string(&migration.name),
        json::string(&migration.checksum),
    ];
    client
        .execute_transaction(transaction, &migration_history_insert_sql(), params)
        .map(|_| ())
        .map_err(|error| migration_failed_message(request, migration, error))
}

/// Formats a migration-scoped execution failure without exposing migration SQL.
fn migration_failed_message(
    request: &MigrationExecutionRequest<'_>,
    migration: &MigrationEngineInput,
    error: String,
) -> String {
    format!(
        "error[db.migration.failed]: Migration `{}` failed: {}",
        migration.version,
        request_postgres_error_message(request, error)
    )
}

/// Runs one SQL batch for a migration request.
///
/// Inputs:
/// - `request`: migration execution request.
/// - `client`: connected VM Postgres command client.
/// - `sql`: SQL batch to run.
///
/// Output:
/// - Success when the maintained Postgres adapter accepts the batch.
/// - User-facing diagnostic on database failure.
///
/// Transformation:
/// - Adds command-specific failure context and delegates execution to the
///   VM-owned Postgres worker instead of spawning external database tools.
fn run_transaction_batch_for_request(
    request: &MigrationExecutionRequest<'_>,
    client: &mut VmPostgresCommandClient,
    transaction: VmPostgresTransaction,
    sql: &str,
) -> Result<(), String> {
    client
        .batch_execute_transaction(transaction, sql)
        .map_err(|error| request_postgres_error_message(request, error))
}

/// Formats a Postgres adapter error for one DB command request.
///
/// Inputs:
/// - `request`: migration execution request that failed.
/// - `error`: stable VM Postgres error.
///
/// Output:
/// - User-facing diagnostic string.
///
/// Transformation:
/// - Prefixes adapter diagnostics with command context while avoiding database
///   URL leakage.
fn request_postgres_error_message(
    request: &MigrationExecutionRequest<'_>,
    error: String,
) -> String {
    let failure_context = if request.destructive() {
        format!(
            "terlc db {} failed for development target {}",
            request.command(),
            request.config().target_summary()
        )
    } else {
        format!("terlc db {} failed", request.command())
    };
    format!("{failure_context}: {error}")
}

#[cfg(test)]
#[path = "execution_test.rs"]
mod execution_test;
