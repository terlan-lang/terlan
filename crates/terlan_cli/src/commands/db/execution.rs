//! Migration execution adapter boundary for `terlc db`.
//!
//! This module owns the final command-facing seam before live Postgres
//! migration execution. The implementation uses Terlan's maintained
//! Rust/Tokio Postgres adapter so database commands do not depend on external
//! shell tools or hand-rolled wire protocol code.

use super::migration::{
    migration_history_insert_sql, migration_history_table_sql, MigrationEngineInput,
};
use super::ResolvedDatabaseConfig;
use terlan_safenative::{json, postgres};

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
/// - Implemented by the live SafeNative Postgres migration adapter.
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

/// SafeNative-backed migration executor.
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
/// - Optionally resets the development schema, ensures the migration-history
///   table exists, executes each migration through `tokio-postgres`, and
///   appends one parameterized history row per applied migration.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct SafeNativeMigrationExecutor;

impl MigrationExecutor for SafeNativeMigrationExecutor {
    /// Executes validated migrations through SafeNative Postgres.
    ///
    /// Inputs:
    /// - `request`: validated migration execution request.
    ///
    /// Output:
    /// - Applied migration report on success.
    /// - Stable connection/database diagnostic on failure.
    ///
    /// Transformation:
    /// - Connects once through the maintained pool, runs schema/history setup,
    ///   applies each migration in a transaction, and records history through
    ///   parameter binding rather than literal SQL interpolation.
    fn execute(
        &self,
        request: MigrationExecutionRequest<'_>,
    ) -> Result<MigrationExecutionReport, String> {
        let pool = postgres::connect(&request.config().config).map_err(postgres_error_message)?;
        if request.destructive() {
            run_batch_for_request(&request, &pool, &development_schema_reset_sql())?;
        }
        run_batch_for_request(&request, &pool, &migration_history_table_sql())?;
        for migration in request.pending() {
            apply_migration_for_request(&request, &pool, migration)?;
        }
        Ok(MigrationExecutionReport::new(request.pending().len()))
    }
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

/// Applies one migration through the maintained Postgres adapter.
///
/// Inputs:
/// - `request`: migration execution request.
/// - `pool`: connected SafeNative Postgres pool.
/// - `migration`: validated migration input.
///
/// Output:
/// - Success when the migration and history insert commit together.
/// - User-facing diagnostic on database failure.
///
/// Transformation:
/// - Starts a transaction, executes user-authored `Up` SQL as a batch, records
///   the migration through parameter binding, commits on success, and attempts
///   rollback on failure.
fn apply_migration_for_request(
    request: &MigrationExecutionRequest<'_>,
    pool: &postgres::Pool,
    migration: &MigrationEngineInput,
) -> Result<(), String> {
    run_batch_for_request(request, pool, "BEGIN")?;
    if let Err(error) = run_migration_body_and_history(request, pool, migration) {
        let _rollback_result = postgres::batch_execute(pool, "ROLLBACK");
        return Err(error);
    }
    run_batch_for_request(request, pool, "COMMIT")
}

/// Runs one migration body and history insert inside an open transaction.
///
/// Inputs:
/// - `request`: migration execution request used for diagnostics.
/// - `pool`: connected SafeNative Postgres pool.
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
    pool: &postgres::Pool,
    migration: &MigrationEngineInput,
) -> Result<(), String> {
    run_batch_for_request(request, pool, &migration.up_sql)?;
    let params = [
        json::string(&migration.version),
        json::string(&migration.name),
        json::string(&migration.checksum),
    ];
    postgres::execute(pool, &migration_history_insert_sql(), &params)
        .map(|_| ())
        .map_err(|error| request_postgres_error_message(request, error))
}

/// Runs one SQL batch for a migration request.
///
/// Inputs:
/// - `request`: migration execution request.
/// - `pool`: connected SafeNative Postgres pool.
/// - `sql`: SQL batch to run.
///
/// Output:
/// - Success when the maintained Postgres adapter accepts the batch.
/// - User-facing diagnostic on database failure.
///
/// Transformation:
/// - Adds command-specific failure context and delegates execution to
///   SafeNative Postgres instead of spawning external database tools.
fn run_batch_for_request(
    request: &MigrationExecutionRequest<'_>,
    pool: &postgres::Pool,
    sql: &str,
) -> Result<(), String> {
    postgres::batch_execute(pool, sql)
        .map_err(|error| request_postgres_error_message(request, error))
}

/// Formats a Postgres adapter error for one DB command request.
///
/// Inputs:
/// - `request`: migration execution request that failed.
/// - `error`: stable SafeNative Postgres error.
///
/// Output:
/// - User-facing diagnostic string.
///
/// Transformation:
/// - Prefixes adapter diagnostics with command context while avoiding database
///   URL leakage.
fn request_postgres_error_message(
    request: &MigrationExecutionRequest<'_>,
    error: postgres::PostgresError,
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
    format!("{failure_context}: {}", postgres_error_message(error))
}

/// Formats a SafeNative Postgres error for command output.
///
/// Inputs:
/// - `error`: stable SafeNative Postgres error.
///
/// Output:
/// - Human-readable error with the stable error code included.
///
/// Transformation:
/// - Keeps command diagnostics compact while preserving the adapter-owned
///   machine-readable code.
fn postgres_error_message(error: postgres::PostgresError) -> String {
    format!("error[{}]: {}", error.code(), error.message())
}

#[cfg(test)]
#[path = "execution_test.rs"]
mod execution_test;
