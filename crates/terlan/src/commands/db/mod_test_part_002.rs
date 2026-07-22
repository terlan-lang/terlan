
/// Validates a directory of Terlan migration files through the command runner.
///
/// Inputs:
/// - Temporary directory containing one valid migration file.
///
/// Output:
/// - Test passes when `run` returns success.
///
/// Transformation:
/// - Exercises public command execution without connecting to Postgres.
#[test]
fn run_validate_accepts_valid_migration_directory() {
    let directory = temp_db_dir("run_validate_accepts_valid_migration_directory");
    fs::write(
        directory.join("20260619123000_create_users.sql"),
        "-- +terlan Up\nCREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    )
    .expect("write migration");

    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec!["validate".to_string(), directory.display().to_string()],
    });
    assert_eq!(exit, ExitCode::SUCCESS);

    remove_dir(&directory);
}

/// Attempts `db migrate` through the live migration executor after validation.
///
/// Inputs:
/// - Temporary directory containing one valid migration file.
///
/// Output:
/// - Test passes when `run` validates local files and returns failure for an
///   unreachable local Postgres endpoint.
///
/// Transformation:
/// - Locks the public command shape without requiring a live database in unit
///   tests.
#[test]
fn run_migrate_validates_then_reports_unreachable_executor() {
    let directory = temp_db_dir("run_migrate_validates_then_reports_unreachable_executor");
    fs::write(
        directory.join("20260619123000_create_users.sql"),
        "-- +terlan Up\nCREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    )
    .expect("write migration");

    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec![
            "migrate".to_string(),
            "--database-url".to_string(),
            "postgres://127.0.0.1:1/terlan".to_string(),
            directory.display().to_string(),
        ],
    });
    assert_eq!(exit, ExitCode::from(1));

    remove_dir(&directory);
}

/// Rejects destructive `db rebuild` without `--dev`.
///
/// Inputs:
/// - Command-local `rebuild` without a development flag.
///
/// Output:
/// - Test passes when `run` returns an argument error before filesystem or
///   database work.
///
/// Transformation:
/// - Enforces the 0.0.5 destructive-command safety rule.
#[test]
fn run_rebuild_rejects_missing_dev_flag() {
    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec!["rebuild".to_string()],
    });

    assert_eq!(exit, ExitCode::from(2));
}

/// Rejects destructive work without independent confirmation before file I/O.
#[test]
fn run_rebuild_rejects_missing_confirmation_before_migration_discovery() {
    let missing_directory = temp_db_dir("missing_confirmation").join("does-not-exist");
    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec![
            "rebuild".to_string(),
            "--dev".to_string(),
            "--database-url".to_string(),
            "postgres://127.0.0.1:1/terlan_dev".to_string(),
            missing_directory.display().to_string(),
        ],
    });

    assert_eq!(exit, ExitCode::from(2));
}

/// Attempts destructive `db reset --dev` through the live migration executor.
///
/// Inputs:
/// - Temporary directory containing one valid migration file.
///
/// Output:
/// - Test passes when `run` accepts `--dev`, validates files, and returns
///   failure for an unreachable local Postgres endpoint.
///
/// Transformation:
/// - Separates safety admission from live execution without requiring a
///   database in unit tests.
#[test]
fn run_reset_with_dev_validates_then_reports_unreachable_executor() {
    let directory = temp_db_dir("run_reset_with_dev_validates_then_reports_unreachable_executor");
    fs::write(
        directory.join("20260619123000_create_users.sql"),
        "-- +terlan Up\nCREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    )
    .expect("write migration");

    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec![
            "reset".to_string(),
            "--dev".to_string(),
            "--confirm".to_string(),
            "--database-url".to_string(),
            "postgres://127.0.0.1:1/terlan".to_string(),
            directory.display().to_string(),
        ],
    });
    assert_eq!(exit, ExitCode::from(1));

    remove_dir(&directory);
}

/// Rejects destructive development commands for every non-local target.
///
/// Inputs:
/// - Temporary directory containing one valid migration file.
/// - Remote Postgres URL whose database name includes a development marker.
///
/// Output:
/// - Test passes when `run` returns an argument error before adapter gating.
///
/// Transformation:
/// - Proves database naming cannot bypass the loopback-only guard.
#[test]
fn run_rebuild_with_dev_rejects_remote_development_database_url() {
    let directory = temp_db_dir("run_rebuild_with_dev_rejects_remote_development_database_url");
    fs::write(
        directory.join("20260619123000_create_users.sql"),
        "-- +terlan Up\nCREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    )
    .expect("write migration");

    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec![
            "rebuild".to_string(),
            "--dev".to_string(),
            "--confirm".to_string(),
            "--database-url".to_string(),
            "postgres://db.example.com/terlan_dev".to_string(),
            directory.display().to_string(),
        ],
    });
    assert_eq!(exit, ExitCode::from(2));

    remove_dir(&directory);
}

/// Allows confirmed destructive commands for loopback database targets.
///
/// Inputs:
/// - Temporary directory containing one valid migration file.
/// - Loopback Postgres URL pointing at an unreachable port.
///
/// Output:
/// - Test passes when `run` validates safety and then returns live executor
///   failure.
///
/// Transformation:
/// - Separates safety admission from socket execution without a live database.
#[test]
fn run_rebuild_with_confirmation_accepts_loopback_database() {
    let directory = temp_db_dir("run_rebuild_with_confirmation_accepts_loopback_database");
    fs::write(
        directory.join("20260619123000_create_users.sql"),
        "-- +terlan Up\nCREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    )
    .expect("write migration");

    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec![
            "rebuild".to_string(),
            "--dev".to_string(),
            "--confirm".to_string(),
            "--database-url".to_string(),
            "postgres://127.0.0.1:1/terlan_dev".to_string(),
            directory.display().to_string(),
        ],
    });
    assert_eq!(exit, ExitCode::from(1));

    remove_dir(&directory);
}

/// Rejects certificate and strict TLS options before migration discovery.
#[test]
fn run_rebuild_rejects_protected_transport_options_before_file_or_socket_work() {
    let protected_options = [
        "sslmode=require",
        "sslmode=verify-ca",
        "sslmode=verify-full",
        "sslcert=client.crt",
        "sslkey=client.key",
        "sslrootcert=root.crt",
        "sslcrl=client.crl",
        "sslcrldir=crls",
        "ssl=true",
        "tls=require",
    ];
    let missing_directory = temp_db_dir("protected_transport").join("does-not-exist");

    for option in protected_options {
        let exit = run(CliCommand {
            verb: Some("db".to_string()),
            args: vec![
                "rebuild".to_string(),
                "--dev".to_string(),
                "--confirm".to_string(),
                "--database-url".to_string(),
                format!("postgres://127.0.0.1:1/terlan_dev?{option}"),
                missing_directory.display().to_string(),
            ],
        });
        assert_eq!(exit, ExitCode::from(2), "option {option}");
    }
}

/// Fails validation for malformed migration files through the command runner.
///
/// Inputs:
/// - Temporary directory containing one migration file without an `Up` marker.
///
/// Output:
/// - Test passes when `run` returns failure.
///
/// Transformation:
/// - Confirms command execution surfaces parser failures before database work
///   exists.
#[test]
fn run_validate_rejects_invalid_migration_source() {
    let directory = temp_db_dir("run_validate_rejects_invalid_migration_source");
    fs::write(
        directory.join("20260619123000_create_users.sql"),
        "CREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    )
    .expect("write migration");

    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec!["validate".to_string(), directory.display().to_string()],
    });
    assert_eq!(exit, ExitCode::from(1));

    remove_dir(&directory);
}

/// Reports pending status for valid migration files through the command runner.
///
/// Inputs:
/// - Temporary directory containing one valid migration file.
///
/// Output:
/// - Test passes when `run` returns success.
///
/// Transformation:
/// - Exercises `terlc db status` without database history or Postgres
///   connectivity.
#[test]
fn run_status_accepts_valid_migration_directory() {
    let directory = temp_db_dir("run_status_accepts_valid_migration_directory");
    fs::write(
        directory.join("20260619123000_create_users.sql"),
        "-- +terlan Up\nCREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    )
    .expect("write migration");

    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec!["status".to_string(), directory.display().to_string()],
    });
    assert_eq!(exit, ExitCode::SUCCESS);

    remove_dir(&directory);
}

/// Attempts live `db status` through the migration-history loader.
///
/// Inputs:
/// - Temporary directory containing one valid migration file.
/// - Explicit Postgres database URL.
///
/// Output:
/// - Test passes when the command validates local migrations and returns
///   failure for an unreachable local Postgres endpoint.
///
/// Transformation:
/// - Covers the command path that loads `terlan_schema_migrations` through the
///   live-history boundary without requiring a running test database.
#[test]
fn run_status_with_database_url_reports_unreachable_history_loader() {
    let directory = temp_db_dir("run_status_with_database_url_reports_unreachable_history_loader");
    fs::write(
        directory.join("20260619123000_create_users.sql"),
        "-- +terlan Up\nCREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    )
    .expect("write migration");

    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec![
            "status".to_string(),
            "--database-url".to_string(),
            "postgres://127.0.0.1:1/terlan".to_string(),
            directory.display().to_string(),
        ],
    });
    assert_eq!(exit, ExitCode::from(1));

    remove_dir(&directory);
}

/// Counts every migration status state for command summaries.
///
/// Inputs:
/// - Synthetic status entries covering every status state.
///
/// Output:
/// - Test passes when each status bucket is counted exactly once.
///
/// Transformation:
/// - Exercises the command summary formatter independently from filesystem
///   discovery and future database history loading.
#[test]
fn migration_status_summary_counts_all_status_states() {
    let entries = vec![
        status_entry("20260619120000", MigrationStatusState::Applied),
        status_entry("20260619121000", MigrationStatusState::Pending),
        status_entry("20260619122000", MigrationStatusState::Missing),
        status_entry("20260619123000", MigrationStatusState::OutOfOrder),
        status_entry("20260619124000", MigrationStatusState::ChecksumMismatch),
        status_entry("20260619125000", MigrationStatusState::NameMismatch),
    ];

    let summary = MigrationStatusSummary::from_entries(&entries);

    assert_eq!(
        summary,
        MigrationStatusSummary {
            applied: 1,
            checksum_mismatch: 1,
            missing: 1,
            name_mismatch: 1,
            out_of_order: 1,
            pending: 1,
        }
    );
}

/// Fails status for malformed migration files through the command runner.
///
/// Inputs:
/// - Temporary directory containing one migration file without an `Up` marker.
///
/// Output:
/// - Test passes when `run` returns failure.
///
/// Transformation:
/// - Confirms status is backed by the same validation path as `validate`.
#[test]
fn run_status_rejects_invalid_migration_source() {
    let directory = temp_db_dir("run_status_rejects_invalid_migration_source");
    fs::write(
        directory.join("20260619123000_create_users.sql"),
        "CREATE TABLE users (id BIGSERIAL PRIMARY KEY);\n",
    )
    .expect("write migration");

    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec!["status".to_string(), directory.display().to_string()],
    });
    assert_eq!(exit, ExitCode::from(1));

    remove_dir(&directory);
}

/// Builds one synthetic migration status entry for command tests.
///
/// Inputs:
/// - `version`: migration timestamp.
/// - `state`: status state to attach to the row.
///
/// Output:
/// - Migration status entry with stable dummy name and checksum.
///
/// Transformation:
/// - Keeps status-summary tests independent from migration files and checksums.
fn status_entry(version: &str, state: MigrationStatusState) -> MigrationStatusEntry {
    MigrationStatusEntry {
        version: version.to_string(),
        name: "example".to_string(),
        checksum: "0".repeat(64),
        applied_at: None,
        state,
    }
}
