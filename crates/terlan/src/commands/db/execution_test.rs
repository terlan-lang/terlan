use super::{
    development_schema_reset_sql, migration_failed_message, migration_history_insert_sql,
    pending_after_lock, validate_migration_lock_result, MigrationExecutionReport,
    MigrationExecutionRequest, MIGRATION_LOCK_SQL,
};
use crate::commands::db::migration::{AppliedMigration, MigrationEngineInput};
use crate::commands::db::{DatabaseConfigSource, ResolvedDatabaseConfig};
use crate::runtime::vm::postgres::VmPostgresDecodedValue;
use crate::terlan_native::postgres;

/// Builds one migration-engine input fixture.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Deterministic pending migration input.
///
/// Transformation:
/// - Creates the minimum execution payload needed to test adapter request
///   routing without reading a migration file.
fn migration_input() -> MigrationEngineInput {
    MigrationEngineInput {
        version: "20260619123000".to_string(),
        name: "create_users".to_string(),
        up_sql: "CREATE TABLE users (id BIGSERIAL PRIMARY KEY);".to_string(),
        up_start_line: 2,
        down_sql: None,
        down_start_line: None,
        checksum: "a".repeat(64),
    }
}

/// Builds a resolved database config fixture.
///
/// Inputs:
/// - `url`: Postgres URL text.
/// - `source`: source label enum.
///
/// Output:
/// - Resolved database config value.
///
/// Transformation:
/// - Constructs the command-private config shape directly for adapter seam
///   tests, bypassing CLI argument parsing.
fn resolved_config(url: &str, source: DatabaseConfigSource) -> ResolvedDatabaseConfig {
    ResolvedDatabaseConfig {
        config: postgres::Config::new(url),
        source,
    }
}

/// Builds one transaction-wrapped migration application SQL batch.
///
/// Inputs:
/// - `migration`: validated migration execution input.
///
/// Output:
/// - SQL batch that runs the migration body and appends its history row.
///
/// Transformation:
/// - Wraps user-authored `Up` SQL and the canonical history insert in one
///   transaction so SQL-shape tests can assert the intended migration boundary.
fn migration_application_sql(migration: &MigrationEngineInput) -> String {
    format!(
        "BEGIN;\n{}\n{}\nCOMMIT;",
        migration.up_sql,
        migration_history_insert_sql_for(migration)
    )
}

/// Builds one concrete migration-history insert statement for tests.
///
/// Inputs:
/// - `migration`: validated migration execution input.
///
/// Output:
/// - SQL statement that inserts the migration version, name, and checksum.
///
/// Transformation:
/// - Converts the centralized parameterized insert contract into a literal SQL
///   statement only for SQL-shape regression tests. Runtime execution uses
///   parameterized VM Postgres calls instead.
fn migration_history_insert_sql_for(migration: &MigrationEngineInput) -> String {
    migration_history_insert_sql()
        .replace("$1", &sql_string_literal(&migration.version))
        .replace("$2", &sql_string_literal(&migration.name))
        .replace("$3", &sql_string_literal(&migration.checksum))
}

/// Builds one SQL string literal.
///
/// Inputs:
/// - `value`: raw value to represent as SQL text.
///
/// Output:
/// - Single-quoted SQL string literal.
///
/// Transformation:
/// - Doubles embedded single quotes according to SQL string literal escaping.
fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Builds a successful migration execution report.
///
/// Inputs:
/// - Applied count.
///
/// Output:
/// - Report whose public accessor returns the supplied count.
///
/// Transformation:
/// - Locks the report type independently from the unavailable adapter.
#[test]
fn migration_execution_report_exposes_applied_count() {
    let report = MigrationExecutionReport::new(3);

    assert_eq!(report.applied(), 3);
}

/// Preserves request fields for the live migration adapter.
///
/// Inputs:
/// - Resolved config and pending migration fixture.
///
/// Output:
/// - Test passes when request accessors return the borrowed values.
///
/// Transformation:
/// - Verifies command metadata can be carried through the adapter seam without
///   copying SQL payloads into the command router.
#[test]
fn migration_execution_request_preserves_adapter_inputs() {
    let config = resolved_config(
        "postgres://localhost/terlan_dev",
        DatabaseConfigSource::CommandLine,
    );
    let pending = vec![migration_input()];
    let request = MigrationExecutionRequest::new("rebuild", &config, &pending, true);

    assert_eq!(request.command(), "rebuild");
    assert_eq!(request.config().source_label(), "--database-url");
    assert_eq!(request.pending(), pending.as_slice());
    assert!(request.destructive());
}

/// Formats a typed migration failure without leaking source SQL or credentials.
#[test]
fn migration_failed_message_identifies_migration_and_redacts_request_data() {
    let config = resolved_config(
        "postgres://alice:secret@localhost/terlan_dev",
        DatabaseConfigSource::CommandLine,
    );
    let migration = migration_input();
    let pending = [migration.clone()];
    let request = MigrationExecutionRequest::new("migrate", &config, &pending, false);

    let message = migration_failed_message(
        &request,
        &migration,
        "error[postgres.query_failed]: database rejected request".to_string(),
    );

    assert_eq!(
        message,
        "error[db.migration.failed]: Migration `20260619123000` failed: terlc db migrate failed: error[postgres.query_failed]: database rejected request"
    );
    assert!(!message.contains("CREATE TABLE"));
    assert!(!message.contains("alice"));
    assert!(!message.contains("secret"));
}

/// Builds a development schema reset batch.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Test passes when the reset batch recreates the public schema.
///
/// Transformation:
/// - Locks the shared SQL used by `rebuild --dev` and `reset --dev`.
#[test]
fn development_schema_reset_sql_rebuilds_public_schema() {
    assert_eq!(
        development_schema_reset_sql(),
        "DROP SCHEMA IF EXISTS public CASCADE;\nCREATE SCHEMA public;"
    );
}

/// Escapes SQL string literal values.
///
/// Inputs:
/// - Text containing a single quote.
///
/// Output:
/// - Test passes when the literal doubles embedded quotes.
///
/// Transformation:
/// - Protects the SQL-shape regression helper even though runtime execution
///   uses parameter binding.
#[test]
fn sql_string_literal_escapes_single_quotes() {
    assert_eq!(sql_string_literal("user's_table"), "'user''s_table'");
}

/// Builds a literal migration-history insert for SQL-shape regression tests.
///
/// Inputs:
/// - Migration execution input fixture.
///
/// Output:
/// - Test passes when the parameterized history contract is materialized with
///   validated literal values.
///
/// Transformation:
/// - Keeps the regression helper aligned with the central history insert
///   statement.
#[test]
fn migration_history_insert_sql_for_materializes_history_values() {
    let migration = migration_input();

    let sql = migration_history_insert_sql_for(&migration);

    assert_eq!(
        sql,
        format!(
            "INSERT INTO terlan_schema_migrations (version, name, checksum) VALUES ('20260619123000', 'create_users', '{}');",
            "a".repeat(64)
        )
    );
}

/// Builds transaction-wrapped migration SQL.
///
/// Inputs:
/// - Migration execution input fixture.
///
/// Output:
/// - Test passes when user SQL and history insert are wrapped in one
///   transaction.
///
/// Transformation:
/// - Verifies failed migration SQL cannot be followed by a committed history
///   row in the generated transaction batch.
#[test]
fn migration_application_sql_wraps_up_sql_and_history_insert() {
    let migration = migration_input();

    let sql = migration_application_sql(&migration);

    assert!(sql.starts_with("BEGIN;\nCREATE TABLE users"));
    assert!(sql.contains("INSERT INTO terlan_schema_migrations"));
    assert!(sql.ends_with("\nCOMMIT;"));
}

#[test]
fn migration_lock_sql_uses_transaction_scoped_nonblocking_lock() {
    assert_eq!(
        MIGRATION_LOCK_SQL,
        "SELECT pg_try_advisory_xact_lock(1413829196, 1296648018) AS acquired;"
    );
}

#[test]
fn migration_lock_result_accepts_only_true_boolean() {
    assert_eq!(
        validate_migration_lock_result(VmPostgresDecodedValue::Bool(true)),
        Ok(())
    );
    assert_eq!(
        validate_migration_lock_result(VmPostgresDecodedValue::Bool(false)),
        Err(
            "error[db.migration.lock_conflict]: Another migration command owns the database lock."
                .to_string()
        )
    );
    assert_eq!(
        validate_migration_lock_result(VmPostgresDecodedValue::Int(1)),
        Err(
            "error[db.migration.lock_protocol]: Postgres returned an invalid migration lock result."
                .to_string()
        )
    );
}

#[test]
fn migration_plan_revalidates_history_after_lock() {
    let first = migration_input();
    let mut second = migration_input();
    second.version = "20260619123100".to_string();
    second.name = "add_user_email".to_string();
    second.checksum = "b".repeat(64);
    let applied = AppliedMigration {
        version: first.version.clone(),
        name: first.name.clone(),
        checksum: first.checksum.clone(),
        applied_at: "2026-06-19T12:30:00.000000Z".to_string(),
    };

    let planned = [first, second.clone()];
    let pending = pending_after_lock(&planned, &[applied]).expect("history should be compatible");

    assert_eq!(pending, vec![&second]);
}

#[test]
fn migration_plan_rejects_history_that_changes_while_waiting_for_lock() {
    let migration = migration_input();
    let applied = AppliedMigration {
        version: migration.version.clone(),
        name: migration.name.clone(),
        checksum: "b".repeat(64),
        applied_at: "2026-06-19T12:30:00.000000Z".to_string(),
    };

    assert_eq!(
        pending_after_lock(&[migration], &[applied]),
        Err(
            "error[db.migration.history_divergent]: Migration `20260619123000` changed while waiting for the database lock."
                .to_string()
        )
    );
}

#[test]
fn migration_plan_rejects_out_of_order_history_introduced_while_waiting_for_lock() {
    let first = migration_input();
    let mut second = migration_input();
    second.version = "20260619123100".to_string();
    second.name = "add_user_email".to_string();
    second.checksum = "b".repeat(64);
    let applied = AppliedMigration {
        version: second.version.clone(),
        name: second.name.clone(),
        checksum: second.checksum.clone(),
        applied_at: "2026-06-19T12:30:00.000000Z".to_string(),
    };

    assert_eq!(
        pending_after_lock(&[first, second], &[applied]),
        Err(
            "error[db.migration.out_of_order]: Database history contains a later applied migration before local migration `20260619123000`."
                .to_string()
        )
    );
}
