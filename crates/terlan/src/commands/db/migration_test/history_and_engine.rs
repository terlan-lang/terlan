use super::*;

/// Rejects execution planning when an applied name differs locally.
#[test]
pub(super) fn pending_migration_engine_inputs_rejects_name_mismatch() {
    let directory = temp_migration_dir("rejects_name_mismatch");
    write_file(
        &directory,
        "20260619123000_create_users.sql",
        "-- +terlan Up\nCREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    );

    let discovered = discover_migration_files(&directory).expect("directory should discover");
    let loaded = load_migration_files(&discovered).expect("files should load");
    let result = pending_migration_engine_inputs(
        &loaded,
        &[AppliedMigration {
            version: loaded[0].file.parsed.version.clone(),
            name: "renamed_users".to_string(),
            checksum: loaded[0].checksum.clone(),
            applied_at: "2026-06-19T12:30:00.000000Z".to_string(),
        }],
    );

    assert_eq!(
        result,
        Err(MigrationDiagnostic {
            line: 1,
            message: "error[db.migration.name_mismatch]: Local migration `20260619123000` does not match its applied name.".to_string(),
        })
    );
    remove_dir(&directory);
}

/// Defines the canonical migration-history table SQL.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Test passes when the table name and required columns are present.
///
/// Transformation:
/// - Locks the database-history contract before the live Postgres adapter
///   creates or queries the table.
#[test]
pub(super) fn migration_history_table_sql_defines_required_columns() {
    let sql = migration_history_table_sql();

    assert!(sql.contains(MIGRATION_HISTORY_TABLE));
    assert!(sql.contains("version TEXT PRIMARY KEY"));
    assert!(sql.contains("name TEXT NOT NULL"));
    assert!(sql.contains("checksum TEXT NOT NULL"));
    assert!(sql.contains("applied_at TIMESTAMPTZ NOT NULL DEFAULT now()"));
}

/// Defines the canonical migration-history read query.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Test passes when the query reads the validated history columns in
///   deterministic version order.
///
/// Transformation:
/// - Locks the SQL text the future Postgres adapter will use before live
///   database history loading is implemented.
#[test]
pub(super) fn migration_history_select_sql_reads_ordered_history_rows() {
    assert_eq!(
        migration_history_select_sql(),
        "SELECT version, name, checksum, to_char(applied_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS applied_at FROM terlan_schema_migrations ORDER BY version ASC;"
    );
}

/// Defines the canonical migration-history insert statement.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Test passes when the statement records one applied migration through
///   Postgres placeholders.
///
/// Transformation:
/// - Keeps future adapter parameter binding aligned with the validated history
///   row shape.
#[test]
pub(super) fn migration_history_insert_sql_records_one_applied_migration() {
    assert_eq!(
        migration_history_insert_sql(),
        "INSERT INTO terlan_schema_migrations (version, name, checksum) VALUES ($1, $2, $3);"
    );
}

/// Converts a valid database history row into applied migration metadata.
///
/// Inputs:
/// - Version, name, and checksum values shaped like database row columns.
///
/// Output:
/// - Test passes when values are preserved in `AppliedMigration`.
///
/// Transformation:
/// - Exercises the pure row-normalization boundary before database loading is
///   wired to Postgres.
#[test]
pub(super) fn applied_migration_from_history_row_accepts_valid_row() {
    let row = applied_migration_from_history_row(
        "20260619123000",
        "create_users",
        &"a".repeat(64),
        "2026-06-19T12:30:00.000000Z",
    )
    .expect("valid history row");

    assert_eq!(
        row,
        AppliedMigration {
            version: "20260619123000".to_string(),
            name: "create_users".to_string(),
            checksum: "a".repeat(64),
            applied_at: "2026-06-19T12:30:00.000000Z".to_string(),
        }
    );
}

/// Rejects malformed database history rows.
///
/// Inputs:
/// - Invalid version, name, and checksum values.
///
/// Output:
/// - Test passes when each invalid value returns the stable history diagnostic.
///
/// Transformation:
/// - Prevents future database-backed status from silently accepting invalid
///   migration history data.
#[test]
pub(super) fn applied_migration_from_history_row_rejects_invalid_row_values() {
    assert_eq!(
        applied_migration_from_history_row(
            "2026061912300x",
            "create_users",
            &"a".repeat(64),
            "2026-06-19T12:30:00.000000Z",
        ),
        Err(MigrationDiagnostic {
            line: 1,
            message: "migration history version must use fourteen digits".to_string(),
        })
    );
    assert_eq!(
        applied_migration_from_history_row(
            "20260619123000",
            "CreateUsers",
            &"a".repeat(64),
            "2026-06-19T12:30:00.000000Z",
        ),
        Err(MigrationDiagnostic {
            line: 1,
            message: "migration history name must be snake_case letters, digits, and underscores"
                .to_string(),
        })
    );
    assert_eq!(
        applied_migration_from_history_row(
            "20260619123000",
            "create_users",
            "not-a-checksum",
            "2026-06-19T12:30:00.000000Z",
        ),
        Err(MigrationDiagnostic {
            line: 1,
            message: "migration history checksum must be SHA-256 lowercase hex".to_string(),
        })
    );
    assert_eq!(
        applied_migration_from_history_row(
            "20260619123000",
            "create_users",
            &"a".repeat(64),
            "not-a-timestamp",
        ),
        Err(MigrationDiagnostic {
            line: 1,
            message: "migration history applied_at must be an RFC 3339 timestamp".to_string(),
        })
    );
}

/// Parses a migration with both required and optional sections.
///
/// Inputs:
/// - Migration text containing `Up` and `Down` markers.
///
/// Output:
/// - Test passes when both sections preserve SQL text and start lines.
///
/// Transformation:
/// - Exercises normal marker splitting without invoking a database or
///   migration engine.
#[test]
pub(super) fn split_migration_sections_accepts_up_and_down() {
    let parsed = split_migration_sections(
        "\
-- +terlan Up
CREATE TABLE users (
  id BIGSERIAL PRIMARY KEY
);

-- +terlan Down
DROP TABLE users;
",
    )
    .expect("migration should parse");

    assert_eq!(parsed.up.start_line, 2);
    assert_eq!(
        parsed.up.sql,
        "CREATE TABLE users (\n  id BIGSERIAL PRIMARY KEY\n);\n"
    );
    let down = parsed.down.expect("down section");
    assert_eq!(down.start_line, 7);
    assert_eq!(down.sql, "DROP TABLE users;");
}

/// Parses a migration without a down section.
///
/// Inputs:
/// - Migration text containing only the required `Up` marker.
///
/// Output:
/// - Test passes when `down` is absent.
///
/// Transformation:
/// - Verifies the production rule that local rollback SQL is optional.
#[test]
pub(super) fn split_migration_sections_accepts_up_only() {
    let parsed = split_migration_sections(
        "\
-- +terlan Up
CREATE TABLE users (id BIGSERIAL PRIMARY KEY);
",
    )
    .expect("migration should parse");

    assert_eq!(
        parsed.up.sql,
        "CREATE TABLE users (id BIGSERIAL PRIMARY KEY);"
    );
    assert!(parsed.down.is_none());
}

/// Rejects a migration with no up marker.
///
/// Inputs:
/// - SQL text without Terlan markers.
///
/// Output:
/// - Test passes when the parser returns a stable missing-up diagnostic.
///
/// Transformation:
/// - Protects the required section rule before execution exists.
#[test]
pub(super) fn split_migration_sections_rejects_missing_up() {
    assert_eq!(
        split_migration_sections("CREATE TABLE users (id BIGSERIAL PRIMARY KEY);"),
        Err(MigrationDiagnostic {
            line: 1,
            message: "missing required `-- +terlan Up` marker".to_string(),
        })
    );
}

/// Rejects a duplicate up marker.
///
/// Inputs:
/// - Migration text containing two `Up` markers.
///
/// Output:
/// - Test passes when the parser reports the second marker line.
///
/// Transformation:
/// - Prevents ambiguous migration bodies before execution.
#[test]
pub(super) fn split_migration_sections_rejects_duplicate_up() {
    assert_eq!(
        split_migration_sections(
            "\
-- +terlan Up
CREATE TABLE users (id BIGSERIAL PRIMARY KEY);
-- +terlan Up
CREATE TABLE accounts (id BIGSERIAL PRIMARY KEY);
",
        ),
        Err(MigrationDiagnostic {
            line: 3,
            message: "duplicate `-- +terlan Up` marker".to_string(),
        })
    );
}

/// Rejects a duplicate down marker.
///
/// Inputs:
/// - Migration text containing two `Down` markers.
///
/// Output:
/// - Test passes when the parser reports the second down marker line.
///
/// Transformation:
/// - Prevents ambiguous local rollback sections.
#[test]
pub(super) fn split_migration_sections_rejects_duplicate_down() {
    assert_eq!(
        split_migration_sections(
            "\
-- +terlan Up
CREATE TABLE users (id BIGSERIAL PRIMARY KEY);
-- +terlan Down
DROP TABLE users;
-- +terlan Down
DROP TABLE accounts;
",
        ),
        Err(MigrationDiagnostic {
            line: 5,
            message: "duplicate `-- +terlan Down` marker".to_string(),
        })
    );
}

/// Rejects a down marker before the up marker.
///
/// Inputs:
/// - Migration text where `Down` appears first.
///
/// Output:
/// - Test passes when the parser points at the out-of-order marker.
///
/// Transformation:
/// - Preserves deterministic section ordering before execution.
#[test]
pub(super) fn split_migration_sections_rejects_down_before_up() {
    assert_eq!(
        split_migration_sections(
            "\
-- +terlan Down
DROP TABLE users;
",
        ),
        Err(MigrationDiagnostic {
            line: 1,
            message: "`-- +terlan Down` marker must follow `-- +terlan Up`".to_string(),
        })
    );
}

/// Rejects unknown Terlan migration markers.
///
/// Inputs:
/// - Migration text with a misspelled Terlan marker.
///
/// Output:
/// - Test passes when the parser rejects the unknown marker instead of
///   treating it as SQL.
///
/// Transformation:
/// - Gives users fast feedback for marker typos.
#[test]
pub(super) fn split_migration_sections_rejects_unknown_marker() {
    assert_eq!(
        split_migration_sections(
            "\
-- +terlan Up
CREATE TABLE users (id BIGSERIAL PRIMARY KEY);
-- +terlan Undo
DROP TABLE users;
",
        ),
        Err(MigrationDiagnostic {
            line: 3,
            message: "unknown Terlan migration marker".to_string(),
        })
    );
}

/// Rejects an empty up section.
///
/// Inputs:
/// - Migration text where `Up` contains only whitespace before `Down`.
///
/// Output:
/// - Test passes when the parser reports the first up body line.
///
/// Transformation:
/// - Prevents migrations that would record an applied migration without doing
///   any forward schema work.
#[test]
pub(super) fn split_migration_sections_rejects_empty_up() {
    assert_eq!(
        split_migration_sections(
            "\
-- +terlan Up

-- +terlan Down
DROP TABLE users;
",
        ),
        Err(MigrationDiagnostic {
            line: 2,
            message: "`-- +terlan Up` section must not be empty".to_string(),
        })
    );
}

/// Accepts whitespace around marker lines.
///
/// Inputs:
/// - Migration text with leading and trailing whitespace around markers.
///
/// Output:
/// - Test passes when trimmed markers are accepted.
///
/// Transformation:
/// - Keeps marker parsing practical without accepting alternate marker names.
#[test]
pub(super) fn split_migration_sections_accepts_marker_line_whitespace() {
    let parsed = split_migration_sections(
        "\
   -- +terlan Up
SELECT 1;
",
    )
    .expect("migration should parse");

    assert_eq!(parsed.up.start_line, 2);
    assert_eq!(parsed.up.sql, "SELECT 1;");
}

/// Creates a unique temporary migration directory.
///
/// Inputs:
/// - `label`: human-readable test label.
///
/// Output:
/// - Path to a newly-created temporary directory.
///
/// Transformation:
/// - Combines process id, timestamp, and label under the OS temp directory so
///   tests do not need an external tempfile crate.
pub(super) fn temp_migration_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "terlan_migration_test_{}_{}_{}",
        std::process::id(),
        nanos,
        label
    ));
    fs::create_dir_all(&directory).expect("create temp migration directory");
    directory
}

/// Writes one file in a migration test directory.
///
/// Inputs:
/// - `directory`: existing temp directory.
/// - `file_name`: filename to write inside the directory.
/// - `contents`: file text.
///
/// Output:
/// - File is written or the test fails.
///
/// Transformation:
/// - Keeps filesystem setup compact in migration discovery tests.
pub(super) fn write_file(directory: &Path, file_name: &str, contents: &str) {
    fs::write(directory.join(file_name), contents).expect("write migration test file");
}

/// Removes a temporary migration test directory.
///
/// Inputs:
/// - `directory`: path created by `temp_migration_dir`.
///
/// Output:
/// - Directory is removed or the test fails.
///
/// Transformation:
/// - Cleans up files created by discovery tests.
pub(super) fn remove_dir(directory: &Path) {
    fs::remove_dir_all(directory).expect("remove temp migration directory");
}
