use super::migration::{
    applied_migration_from_history_row, discover_migration_files, load_migration_file,
    load_migration_files, migration_engine_inputs, migration_history_insert_sql,
    migration_history_select_sql, migration_history_table_sql, migration_status,
    parse_migration_file_name, pending_migration_engine_inputs, split_migration_sections,
    AppliedMigration, LoadedMigration, MigrationDiagnostic, MigrationFileName,
    MigrationStatusEntry, MigrationStatusState, MIGRATION_HISTORY_TABLE,
};
use crate::support::is_valid_sha256_hex;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Validates and sorts migration filename fixtures.
///
/// Inputs:
/// - `file_names`: migration basenames used by parser tests.
///
/// Output:
/// - `Ok(Vec<MigrationFileName>)` sorted by timestamp.
/// - `Err(MigrationDiagnostic)` for invalid filenames or duplicate
///   timestamps.
///
/// Transformation:
/// - Reuses production filename parsing, sorts by timestamp, and rejects
///   duplicate versions so tests cover inventory semantics without keeping a
///   test-only wrapper in production code.
fn migration_file_inventory(
    file_names: &[&str],
) -> Result<Vec<MigrationFileName>, MigrationDiagnostic> {
    let mut parsed = file_names
        .iter()
        .map(|file_name| parse_migration_file_name(file_name))
        .collect::<Result<Vec<_>, _>>()?;
    parsed.sort_by(|left, right| left.version.cmp(&right.version));

    for pair in parsed.windows(2) {
        if pair[0].version == pair[1].version {
            return Err(MigrationDiagnostic {
                line: 1,
                message: "duplicate migration timestamp in migration filenames".to_string(),
            });
        }
    }

    Ok(parsed)
}

/// Builds pending status rows from loaded migration fixtures.
///
/// Inputs:
/// - `loaded`: validated migration fixtures.
///
/// Output:
/// - One pending `MigrationStatusEntry` per loaded migration.
///
/// Transformation:
/// - Calls the production status comparator with empty applied history so tests
///   can verify pending-row projection without a production-only helper.
fn pending_migration_status(loaded: &[LoadedMigration]) -> Vec<MigrationStatusEntry> {
    migration_status(loaded, &[])
}

/// Parses a valid timestamped migration filename.
///
/// Inputs:
/// - One filename using `YYYYMMDDHHMMSS_name.sql`.
///
/// Output:
/// - Test passes when timestamp and descriptive name are returned separately.
///
/// Transformation:
/// - Exercises filename validation without touching the filesystem.
#[test]
fn parse_migration_file_name_accepts_timestamped_sql_name() {
    assert_eq!(
        parse_migration_file_name("20260619123000_create_users.sql"),
        Ok(MigrationFileName {
            version: "20260619123000".to_string(),
            name: "create_users".to_string(),
        })
    );
}

/// Rejects migration filenames without the SQL extension.
///
/// Inputs:
/// - One filename ending in a non-SQL extension.
///
/// Output:
/// - Test passes when the parser returns the stable extension diagnostic.
///
/// Transformation:
/// - Keeps migration discovery constrained to SQL files.
#[test]
fn parse_migration_file_name_rejects_non_sql_extension() {
    assert_eq!(
        parse_migration_file_name("20260619123000_create_users.txt"),
        Err(MigrationDiagnostic {
            line: 1,
            message: "migration filename must end with `.sql`".to_string(),
        })
    );
}

/// Rejects migration filenames without the timestamp/name separator.
///
/// Inputs:
/// - One filename with no underscore after the timestamp.
///
/// Output:
/// - Test passes when the parser returns the stable shape diagnostic.
///
/// Transformation:
/// - Prevents ambiguous migration names before sorting.
#[test]
fn parse_migration_file_name_rejects_missing_separator() {
    assert_eq!(
        parse_migration_file_name("20260619123000.sql"),
        Err(MigrationDiagnostic {
            line: 1,
            message: "migration filename must use `YYYYMMDDHHMMSS_name.sql`".to_string(),
        })
    );
}

/// Rejects migration filenames with invalid timestamps.
///
/// Inputs:
/// - One filename whose timestamp contains a non-digit character.
///
/// Output:
/// - Test passes when the parser returns the stable timestamp diagnostic.
///
/// Transformation:
/// - Protects deterministic migration ordering before execution exists.
#[test]
fn parse_migration_file_name_rejects_invalid_timestamp() {
    assert_eq!(
        parse_migration_file_name("2026061912300x_create_users.sql"),
        Err(MigrationDiagnostic {
            line: 1,
            message: "migration filename timestamp must use fourteen digits".to_string(),
        })
    );
}

/// Rejects migration filenames with invalid descriptive names.
///
/// Inputs:
/// - One filename whose descriptive name is not snake_case.
///
/// Output:
/// - Test passes when the parser returns the stable name diagnostic.
///
/// Transformation:
/// - Keeps generated migration filenames predictable and shell-friendly.
#[test]
fn parse_migration_file_name_rejects_invalid_name() {
    assert_eq!(
        parse_migration_file_name("20260619123000_CreateUsers.sql"),
        Err(MigrationDiagnostic {
            line: 1,
            message: "migration filename name must be snake_case letters, digits, and underscores"
                .to_string(),
        })
    );
}

/// Sorts valid migration filenames by timestamp.
///
/// Inputs:
/// - Unordered migration basenames.
///
/// Output:
/// - Test passes when parsed inventory is ordered by timestamp.
///
/// Transformation:
/// - Exercises deterministic ordering before filesystem discovery or database
///   execution exists.
#[test]
fn migration_file_inventory_sorts_by_timestamp() {
    let inventory = migration_file_inventory(&[
        "20260619130000_add_email.sql",
        "20260619123000_create_users.sql",
        "20260619124500_create_accounts.sql",
    ])
    .expect("inventory should parse");

    let versions = inventory
        .iter()
        .map(|migration| migration.version.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        versions,
        vec!["20260619123000", "20260619124500", "20260619130000"]
    );
}

/// Rejects duplicate migration timestamps.
///
/// Inputs:
/// - Two migration basenames with the same timestamp.
///
/// Output:
/// - Test passes when duplicate versions produce a stable diagnostic.
///
/// Transformation:
/// - Prevents ambiguous migration order before execution.
#[test]
fn migration_file_inventory_rejects_duplicate_timestamps() {
    assert_eq!(
        migration_file_inventory(&[
            "20260619123000_create_users.sql",
            "20260619123000_create_accounts.sql",
        ]),
        Err(MigrationDiagnostic {
            line: 1,
            message: "duplicate migration timestamp in migration filenames".to_string(),
        })
    );
}

/// Discovers and sorts migration files from a directory.
///
/// Inputs:
/// - Temporary directory containing valid migration filenames out of order.
///
/// Output:
/// - Test passes when discovered files are sorted by timestamp.
///
/// Transformation:
/// - Exercises immediate filesystem discovery without executing SQL.
#[test]
fn discover_migration_files_sorts_regular_files() {
    let directory = temp_migration_dir("sorts_regular_files");
    write_file(
        &directory,
        "20260619130000_add_email.sql",
        "-- +terlan Up\nSELECT 3;\n",
    );
    write_file(
        &directory,
        "20260619123000_create_users.sql",
        "-- +terlan Up\nSELECT 1;\n",
    );
    write_file(
        &directory,
        "20260619124500_create_accounts.sql",
        "-- +terlan Up\nSELECT 2;\n",
    );

    let discovered = discover_migration_files(&directory).expect("directory should discover");
    let names = discovered
        .iter()
        .map(|migration| migration.file_name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "20260619123000_create_users.sql",
            "20260619124500_create_accounts.sql",
            "20260619130000_add_email.sql",
        ]
    );

    remove_dir(&directory);
}

/// Ignores nested directories during migration discovery.
///
/// Inputs:
/// - Temporary directory containing a subdirectory and one valid migration.
///
/// Output:
/// - Test passes when only the regular file is returned.
///
/// Transformation:
/// - Keeps migration discovery shallow and avoids accidental recursive
///   execution.
#[test]
fn discover_migration_files_ignores_subdirectories() {
    let directory = temp_migration_dir("ignores_subdirectories");
    fs::create_dir(directory.join("nested")).expect("create nested directory");
    write_file(
        &directory,
        "20260619123000_create_users.sql",
        "-- +terlan Up\nSELECT 1;\n",
    );

    let discovered = discover_migration_files(&directory).expect("directory should discover");
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].file_name, "20260619123000_create_users.sql");

    remove_dir(&directory);
}

/// Rejects invalid migration filenames discovered on disk.
///
/// Inputs:
/// - Temporary directory containing a non-SQL migration-like file.
///
/// Output:
/// - Test passes when discovery reports the invalid filename.
///
/// Transformation:
/// - Prevents accidental files in the migration directory from being silently
///   ignored.
#[test]
fn discover_migration_files_rejects_invalid_filename() {
    let directory = temp_migration_dir("rejects_invalid_filename");
    write_file(&directory, "not_a_migration.txt", "SELECT 1;\n");

    let diagnostic = discover_migration_files(&directory).expect_err("invalid file should fail");
    assert_eq!(diagnostic.path, directory.join("not_a_migration.txt"));
    assert!(diagnostic
        .message
        .contains("migration filename must end with `.sql`"));

    remove_dir(&directory);
}

/// Rejects duplicate timestamps discovered on disk.
///
/// Inputs:
/// - Temporary directory containing two valid files with the same timestamp.
///
/// Output:
/// - Test passes when discovery reports duplicate timestamp ambiguity.
///
/// Transformation:
/// - Extends pure inventory duplicate protection to filesystem discovery.
#[test]
fn discover_migration_files_rejects_duplicate_timestamps() {
    let directory = temp_migration_dir("rejects_duplicate_timestamps");
    write_file(
        &directory,
        "20260619123000_create_users.sql",
        "-- +terlan Up\nSELECT 1;\n",
    );
    write_file(
        &directory,
        "20260619123000_create_accounts.sql",
        "-- +terlan Up\nSELECT 2;\n",
    );

    let diagnostic = discover_migration_files(&directory).expect_err("duplicates should fail");
    assert_eq!(
        diagnostic.message,
        "duplicate migration timestamp in migration filenames"
    );

    remove_dir(&directory);
}

/// Loads and parses discovered migration files in timestamp order.
///
/// Inputs:
/// - Temporary directory containing two valid migrations.
///
/// Output:
/// - Test passes when loaded migrations preserve discovered ordering and SQL
///   sections.
///
/// Transformation:
/// - Exercises the discovery-to-parser bridge without checksums or database
///   execution.
#[test]
fn load_migration_files_parses_discovered_sources() {
    let directory = temp_migration_dir("loads_discovered_sources");
    write_file(
        &directory,
        "20260619124500_add_email.sql",
        "-- +terlan Up\nALTER TABLE users ADD COLUMN email TEXT;\n",
    );
    write_file(
        &directory,
        "20260619123000_create_users.sql",
        "-- +terlan Up\nCREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    );

    let discovered = discover_migration_files(&directory).expect("directory should discover");
    let loaded = load_migration_files(&discovered).expect("files should load");

    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].file.file_name, "20260619123000_create_users.sql");
    assert!(is_valid_sha256_hex(&loaded[0].checksum));
    assert_eq!(
        loaded[0].sections.up.sql,
        "CREATE TABLE users (id BIGSERIAL PRIMARY KEY);"
    );
    assert_eq!(loaded[1].file.file_name, "20260619124500_add_email.sql");
    assert!(is_valid_sha256_hex(&loaded[1].checksum));
    assert_eq!(
        loaded[1].sections.up.sql,
        "ALTER TABLE users ADD COLUMN email TEXT;"
    );

    remove_dir(&directory);
}

/// Converts loaded migrations into engine-ready inputs.
///
/// Inputs:
/// - Temporary directory containing migrations with `Up` and optional `Down`
///   sections.
///
/// Output:
/// - Test passes when conversion preserves ordering, SQL bodies, line offsets,
///   and checksums.
///
/// Transformation:
/// - Exercises the pure bridge between Terlan marker parsing and maintained
///   migration execution.
#[test]
fn migration_engine_inputs_preserve_sections_and_metadata() {
    let directory = temp_migration_dir("engine_inputs_preserve_sections");
    write_file(
        &directory,
        "20260619123000_create_users.sql",
        "-- +terlan Up\nCREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n\n-- +terlan Down\nDROP TABLE users;\n",
    );
    write_file(
        &directory,
        "20260619124500_add_email.sql",
        "-- +terlan Up\nALTER TABLE users ADD COLUMN email TEXT;\n",
    );

    let discovered = discover_migration_files(&directory).expect("directory should discover");
    let loaded = load_migration_files(&discovered).expect("files should load");
    let inputs = migration_engine_inputs(&loaded);

    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[0].version, "20260619123000");
    assert_eq!(inputs[0].name, "create_users");
    assert_eq!(
        inputs[0].up_sql,
        "CREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n"
    );
    assert_eq!(inputs[0].up_start_line, 2);
    assert_eq!(inputs[0].down_sql.as_deref(), Some("DROP TABLE users;"));
    assert_eq!(inputs[0].down_start_line, Some(5));
    assert_eq!(inputs[0].checksum, loaded[0].checksum);
    assert_eq!(inputs[1].version, "20260619124500");
    assert_eq!(inputs[1].down_sql, None);
    assert_eq!(inputs[1].down_start_line, None);
    assert_eq!(inputs[1].checksum, loaded[1].checksum);

    remove_dir(&directory);
}

/// Reports migration parser diagnostics with file path context.
///
/// Inputs:
/// - One valid migration filename whose contents omit the required `Up`
///   marker.
///
/// Output:
/// - Test passes when loading points at that file and parser line.
///
/// Transformation:
/// - Verifies that command-facing diagnostics can identify the invalid file.
#[test]
fn load_migration_file_reports_parser_error_with_path() {
    let directory = temp_migration_dir("reports_parser_error_with_path");
    write_file(
        &directory,
        "20260619123000_create_users.sql",
        "CREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    );

    let discovered = discover_migration_files(&directory).expect("directory should discover");
    let diagnostic = load_migration_file(&discovered[0]).expect_err("invalid source should fail");

    assert_eq!(
        diagnostic.path,
        directory.join("20260619123000_create_users.sql")
    );
    assert_eq!(diagnostic.line, 1);
    assert_eq!(
        diagnostic.message,
        "missing required `-- +terlan Up` marker"
    );

    remove_dir(&directory);
}

/// Builds pending status rows from loaded migrations.
///
/// Inputs:
/// - Temporary directory containing two valid migration files.
///
/// Output:
/// - Test passes when status rows preserve order, metadata, checksums, and
///   pending state.
///
/// Transformation:
/// - Exercises the filesystem-only status projection before database history
///   comparison exists.
#[test]
fn pending_migration_status_marks_loaded_migrations_pending() {
    let directory = temp_migration_dir("marks_loaded_migrations_pending");
    write_file(
        &directory,
        "20260619124500_add_email.sql",
        "-- +terlan Up\nALTER TABLE users ADD COLUMN email TEXT;\n",
    );
    write_file(
        &directory,
        "20260619123000_create_users.sql",
        "-- +terlan Up\nCREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    );

    let discovered = discover_migration_files(&directory).expect("directory should discover");
    let loaded = load_migration_files(&discovered).expect("files should load");
    let statuses = pending_migration_status(&loaded);

    assert_eq!(statuses.len(), 2);
    assert_eq!(statuses[0].version, "20260619123000");
    assert_eq!(statuses[0].name, "create_users");
    assert_eq!(statuses[0].state, MigrationStatusState::Pending);
    assert!(is_valid_sha256_hex(&statuses[0].checksum));
    assert_eq!(statuses[1].version, "20260619124500");
    assert_eq!(statuses[1].name, "add_email");
    assert_eq!(statuses[1].state.label(), "pending");
    assert!(is_valid_sha256_hex(&statuses[1].checksum));

    remove_dir(&directory);
}

/// Compares local migrations with applied history.
///
/// Inputs:
/// - Temporary directory containing applied, pending, and divergent local
///   migrations.
/// - In-memory applied history containing applied, missing, and divergent
///   rows.
///
/// Output:
/// - Test passes when status rows are sorted by version and classified as
///   applied, pending, missing, or divergent.
///
/// Transformation:
/// - Locks migration history comparison semantics before database history
///   loading is implemented.
#[test]
fn migration_status_classifies_applied_pending_missing_and_divergent() {
    let directory = temp_migration_dir("classifies_status");
    write_file(
        &directory,
        "20260619123000_create_users.sql",
        "-- +terlan Up\nCREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    );
    write_file(
        &directory,
        "20260619124500_add_email.sql",
        "-- +terlan Up\nALTER TABLE users ADD COLUMN email TEXT;\n",
    );
    write_file(
        &directory,
        "20260619130000_add_name.sql",
        "-- +terlan Up\nALTER TABLE users ADD COLUMN name TEXT;\n",
    );

    let discovered = discover_migration_files(&directory).expect("directory should discover");
    let loaded = load_migration_files(&discovered).expect("files should load");
    let applied_checksum = loaded[0].checksum.clone();
    let statuses = migration_status(
        &loaded,
        &[
            AppliedMigration {
                version: "20260619123000".to_string(),
                name: "create_users".to_string(),
                checksum: applied_checksum,
            },
            AppliedMigration {
                version: "20260619125000".to_string(),
                name: "drop_unused".to_string(),
                checksum: "0".repeat(64),
            },
            AppliedMigration {
                version: "20260619130000".to_string(),
                name: "add_name".to_string(),
                checksum: "1".repeat(64),
            },
        ],
    );

    let states = statuses
        .iter()
        .map(|status| (status.version.as_str(), status.state))
        .collect::<Vec<_>>();
    assert_eq!(
        states,
        vec![
            ("20260619123000", MigrationStatusState::Applied),
            ("20260619124500", MigrationStatusState::Pending),
            ("20260619125000", MigrationStatusState::Missing),
            ("20260619130000", MigrationStatusState::Divergent),
        ]
    );
    assert_eq!(statuses[0].state.label(), "applied");
    assert_eq!(statuses[2].name, "drop_unused");
    assert_eq!(statuses[3].checksum, loaded[2].checksum);

    remove_dir(&directory);
}

/// Selects only pending migrations for execution.
///
/// Inputs:
/// - Local migrations where the first migration is already applied.
/// - Applied history row matching the first local migration.
///
/// Output:
/// - Test passes when only unapplied migrations become engine inputs.
///
/// Transformation:
/// - Exercises the pure execution-planning boundary before the live Postgres
///   adapter starts applying migrations.
#[test]
fn pending_migration_engine_inputs_selects_unapplied_migrations() {
    let directory = temp_migration_dir("selects_unapplied_migrations");
    write_file(
        &directory,
        "20260619123000_create_users.sql",
        "-- +terlan Up\nCREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    );
    write_file(
        &directory,
        "20260619124500_add_email.sql",
        "-- +terlan Up\nALTER TABLE users ADD COLUMN email TEXT;\n",
    );

    let discovered = discover_migration_files(&directory).expect("directory should discover");
    let loaded = load_migration_files(&discovered).expect("files should load");
    let pending = pending_migration_engine_inputs(
        &loaded,
        &[AppliedMigration {
            version: loaded[0].file.parsed.version.clone(),
            name: loaded[0].file.parsed.name.clone(),
            checksum: loaded[0].checksum.clone(),
        }],
    )
    .expect("compatible history should plan pending inputs");

    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].version, "20260619124500");
    assert_eq!(pending[0].name, "add_email");
    assert_eq!(
        pending[0].up_sql,
        "ALTER TABLE users ADD COLUMN email TEXT;"
    );

    remove_dir(&directory);
}

/// Rejects execution planning when applied history is missing locally.
///
/// Inputs:
/// - Empty local migration set.
/// - Applied history row that has no matching local file.
///
/// Output:
/// - Test passes when execution planning returns a missing-history diagnostic.
///
/// Transformation:
/// - Prevents the future adapter from applying new SQL after local migration
///   files no longer match the database history.
#[test]
fn pending_migration_engine_inputs_rejects_missing_history() {
    assert_eq!(
        pending_migration_engine_inputs(
            &[],
            &[AppliedMigration {
                version: "20260619123000".to_string(),
                name: "create_users".to_string(),
                checksum: "0".repeat(64),
            }],
        ),
        Err(MigrationDiagnostic {
            line: 1,
            message: "database history contains a migration missing from local files".to_string(),
        })
    );
}

/// Rejects execution planning when history diverges from local files.
///
/// Inputs:
/// - One local migration.
/// - Applied history row with the same version and different checksum.
///
/// Output:
/// - Test passes when execution planning returns a divergent-history
///   diagnostic.
///
/// Transformation:
/// - Blocks future migration execution when an applied migration was edited
///   after being recorded in the database.
#[test]
fn pending_migration_engine_inputs_rejects_divergent_history() {
    let directory = temp_migration_dir("rejects_divergent_history");
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
            version: "20260619123000".to_string(),
            name: "create_users".to_string(),
            checksum: "1".repeat(64),
        }],
    );

    assert_eq!(
        result,
        Err(MigrationDiagnostic {
            line: 1,
            message: "database history diverges from local migration files".to_string(),
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
fn migration_history_table_sql_defines_required_columns() {
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
fn migration_history_select_sql_reads_ordered_history_rows() {
    assert_eq!(
        migration_history_select_sql(),
        "SELECT version, name, checksum FROM terlan_schema_migrations ORDER BY version ASC;"
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
fn migration_history_insert_sql_records_one_applied_migration() {
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
fn applied_migration_from_history_row_accepts_valid_row() {
    let row = applied_migration_from_history_row("20260619123000", "create_users", &"a".repeat(64))
        .expect("valid history row");

    assert_eq!(
        row,
        AppliedMigration {
            version: "20260619123000".to_string(),
            name: "create_users".to_string(),
            checksum: "a".repeat(64),
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
fn applied_migration_from_history_row_rejects_invalid_row_values() {
    assert_eq!(
        applied_migration_from_history_row("2026061912300x", "create_users", &"a".repeat(64)),
        Err(MigrationDiagnostic {
            line: 1,
            message: "migration history version must use fourteen digits".to_string(),
        })
    );
    assert_eq!(
        applied_migration_from_history_row("20260619123000", "CreateUsers", &"a".repeat(64)),
        Err(MigrationDiagnostic {
            line: 1,
            message: "migration history name must be snake_case letters, digits, and underscores"
                .to_string(),
        })
    );
    assert_eq!(
        applied_migration_from_history_row("20260619123000", "create_users", "not-a-checksum"),
        Err(MigrationDiagnostic {
            line: 1,
            message: "migration history checksum must be SHA-256 lowercase hex".to_string(),
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
fn split_migration_sections_accepts_up_and_down() {
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
fn split_migration_sections_accepts_up_only() {
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
fn split_migration_sections_rejects_missing_up() {
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
fn split_migration_sections_rejects_duplicate_up() {
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
fn split_migration_sections_rejects_duplicate_down() {
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
fn split_migration_sections_rejects_down_before_up() {
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
fn split_migration_sections_rejects_unknown_marker() {
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
fn split_migration_sections_rejects_empty_up() {
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
fn split_migration_sections_accepts_marker_line_whitespace() {
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
fn temp_migration_dir(label: &str) -> PathBuf {
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
fn write_file(directory: &Path, file_name: &str, contents: &str) {
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
fn remove_dir(directory: &Path) {
    fs::remove_dir_all(directory).expect("remove temp migration directory");
}
