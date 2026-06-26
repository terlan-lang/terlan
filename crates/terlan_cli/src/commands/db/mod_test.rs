use super::migration::{MigrationStatusEntry, MigrationStatusState};
use super::{parse_db_command, run, DbCommand, MigrationStatusSummary, DEFAULT_MIGRATION_DIR};
use crate::CliCommand;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

const LIVE_POSTGRES_SKIP_MESSAGE: &str =
    "skipping live Postgres migration lifecycle: TERLAN_TEST_POSTGRES_URL is not configured";

/// Resolves the optional live Postgres URL used by integration-style DB tests.
///
/// Inputs:
/// - `url`: result returned by reading `TERLAN_TEST_POSTGRES_URL`.
///
/// Output:
/// - `Ok(url)` when a live database URL is configured.
/// - Stable skip message when the URL is absent or unreadable.
///
/// Transformation:
/// - Converts process-environment variability into a deterministic message
///   that normal unit tests can assert without starting Postgres.
fn live_postgres_url_or_skip_message(
    url: Result<String, std::env::VarError>,
) -> Result<String, &'static str> {
    url.map_err(|_| LIVE_POSTGRES_SKIP_MESSAGE)
}

/// Parses `db init` with the default migration directory.
///
/// Inputs:
/// - Command-local arguments containing only `init`.
///
/// Output:
/// - Test passes when the parser selects the default migration directory.
///
/// Transformation:
/// - Exercises scaffold command parsing without touching the filesystem.
#[test]
fn parse_db_command_accepts_init_default_directory() {
    assert_eq!(
        parse_db_command(&["init".to_string()]),
        Ok(DbCommand::Init {
            directory: PathBuf::from(DEFAULT_MIGRATION_DIR),
        })
    );
}

/// Parses `db new` with the default migration directory.
///
/// Inputs:
/// - Command-local arguments containing `new` and a migration name.
///
/// Output:
/// - Test passes when the parser preserves the name and default directory.
///
/// Transformation:
/// - Exercises migration scaffold parsing without creating a file.
#[test]
fn parse_db_command_accepts_new_default_directory() {
    assert_eq!(
        parse_db_command(&["new".to_string(), "create_users".to_string()]),
        Ok(DbCommand::New {
            name: "create_users".to_string(),
            directory: PathBuf::from(DEFAULT_MIGRATION_DIR),
        })
    );
}

/// Parses `db new` with an explicit migration directory.
///
/// Inputs:
/// - Command-local arguments containing `new`, a migration name, and a
///   directory.
///
/// Output:
/// - Test passes when the parser preserves both values.
///
/// Transformation:
/// - Supports project layouts that place migrations outside `db/migrations`.
#[test]
fn parse_db_command_accepts_new_custom_directory() {
    assert_eq!(
        parse_db_command(&[
            "new".to_string(),
            "create_users".to_string(),
            "schema".to_string(),
        ]),
        Ok(DbCommand::New {
            name: "create_users".to_string(),
            directory: PathBuf::from("schema"),
        })
    );
}

/// Parses `db migrate` with the default migration directory.
///
/// Inputs:
/// - Command-local arguments containing only `migrate`.
///
/// Output:
/// - Test passes when the parser selects the default migration directory.
///
/// Transformation:
/// - Locks the execution command shape before the database adapter exists.
#[test]
fn parse_db_command_accepts_migrate_default_directory() {
    assert_eq!(
        parse_db_command(&["migrate".to_string()]),
        Ok(DbCommand::Migrate {
            directory: PathBuf::from(DEFAULT_MIGRATION_DIR),
            database_url: None,
        })
    );
}

/// Parses `db migrate` with an explicit migration directory.
///
/// Inputs:
/// - Command-local arguments containing `migrate` and one directory.
///
/// Output:
/// - Test passes when the parser preserves the supplied directory.
///
/// Transformation:
/// - Keeps execution command layout aligned with validation and status.
#[test]
fn parse_db_command_accepts_migrate_custom_directory() {
    assert_eq!(
        parse_db_command(&["migrate".to_string(), "schema".to_string()]),
        Ok(DbCommand::Migrate {
            directory: PathBuf::from("schema"),
            database_url: None,
        })
    );
}

/// Parses `db migrate` with an explicit database URL.
///
/// Inputs:
/// - Command-local arguments containing `migrate`, `--database-url`, a URL, and
///   an explicit migration directory.
///
/// Output:
/// - Test passes when the parser preserves the URL and directory.
///
/// Transformation:
/// - Locks the live database command shape before the Postgres migration
///   adapter is wired.
#[test]
fn parse_db_command_accepts_migrate_database_url_and_directory() {
    assert_eq!(
        parse_db_command(&[
            "migrate".to_string(),
            "--database-url".to_string(),
            "postgres://localhost/terlan".to_string(),
            "schema".to_string(),
        ]),
        Ok(DbCommand::Migrate {
            directory: PathBuf::from("schema"),
            database_url: Some("postgres://localhost/terlan".to_string()),
        })
    );
}

/// Parses `db rebuild` without the development flag.
///
/// Inputs:
/// - Command-local arguments containing only `rebuild`.
///
/// Output:
/// - Test passes when the parser preserves the missing `--dev` state.
///
/// Transformation:
/// - Lets execution own the destructive-command safety diagnostic.
#[test]
fn parse_db_command_accepts_rebuild_without_dev_for_later_rejection() {
    assert_eq!(
        parse_db_command(&["rebuild".to_string()]),
        Ok(DbCommand::Rebuild {
            directory: PathBuf::from(DEFAULT_MIGRATION_DIR),
            dev: false,
            database_url: None,
        })
    );
}

/// Parses `db rebuild --dev` with an explicit migration directory.
///
/// Inputs:
/// - Command-local arguments containing `rebuild`, `--dev`, and one directory.
///
/// Output:
/// - Test passes when the parser preserves the development flag and directory.
///
/// Transformation:
/// - Accepts the preferred destructive development command spelling.
#[test]
fn parse_db_command_accepts_rebuild_with_dev_and_directory() {
    assert_eq!(
        parse_db_command(&[
            "rebuild".to_string(),
            "--dev".to_string(),
            "schema".to_string(),
        ]),
        Ok(DbCommand::Rebuild {
            directory: PathBuf::from("schema"),
            dev: true,
            database_url: None,
        })
    );
}

/// Parses `db rebuild --dev` with database URL and migration directory.
///
/// Inputs:
/// - Command-local arguments containing `rebuild`, `--dev`,
///   `--database-url`, and one directory.
///
/// Output:
/// - Test passes when URL, development flag, and directory are preserved.
///
/// Transformation:
/// - Covers the destructive live-command parser surface before execution is
///   wired to the Postgres migration adapter.
#[test]
fn parse_db_command_accepts_rebuild_database_url_and_directory() {
    assert_eq!(
        parse_db_command(&[
            "rebuild".to_string(),
            "--dev".to_string(),
            "--database-url".to_string(),
            "postgres://localhost/terlan_dev".to_string(),
            "schema".to_string(),
        ]),
        Ok(DbCommand::Rebuild {
            directory: PathBuf::from("schema"),
            dev: true,
            database_url: Some("postgres://localhost/terlan_dev".to_string()),
        })
    );
}

/// Parses `db reset --dev` with an explicit migration directory.
///
/// Inputs:
/// - Command-local arguments containing `reset`, one directory, and `--dev`.
///
/// Output:
/// - Test passes when the parser accepts either safe argument order.
///
/// Transformation:
/// - Keeps destructive development commands ergonomic without weakening the
///   explicit `--dev` requirement.
#[test]
fn parse_db_command_accepts_reset_with_dev_and_directory() {
    assert_eq!(
        parse_db_command(&[
            "reset".to_string(),
            "schema".to_string(),
            "--dev".to_string(),
        ]),
        Ok(DbCommand::Reset {
            directory: PathBuf::from("schema"),
            dev: true,
            database_url: None,
        })
    );
}

/// Parses `db reset --dev` with database URL and migration directory.
///
/// Inputs:
/// - Command-local arguments containing `reset`, `--database-url`, `--dev`,
///   and one directory.
///
/// Output:
/// - Test passes when URL, development flag, and directory are preserved.
///
/// Transformation:
/// - Locks the reset parser shape for the live adapter while preserving
///   flexible argument ordering.
#[test]
fn parse_db_command_accepts_reset_database_url_and_directory() {
    assert_eq!(
        parse_db_command(&[
            "reset".to_string(),
            "--database-url".to_string(),
            "postgres://localhost/terlan_test".to_string(),
            "schema".to_string(),
            "--dev".to_string(),
        ]),
        Ok(DbCommand::Reset {
            directory: PathBuf::from("schema"),
            dev: true,
            database_url: Some("postgres://localhost/terlan_test".to_string()),
        })
    );
}

/// Parses `db validate` with the default migration directory.
///
/// Inputs:
/// - Command-local arguments containing only `validate`.
///
/// Output:
/// - Test passes when the parser selects the default migration directory.
///
/// Transformation:
/// - Exercises command parsing without touching the filesystem.
#[test]
fn parse_db_command_accepts_validate_default_directory() {
    assert_eq!(
        parse_db_command(&["validate".to_string()]),
        Ok(DbCommand::Validate {
            directory: PathBuf::from(DEFAULT_MIGRATION_DIR),
        })
    );
}

/// Parses `db validate` with an explicit migration directory.
///
/// Inputs:
/// - Command-local arguments containing `validate` and one directory.
///
/// Output:
/// - Test passes when the parser preserves the supplied directory.
///
/// Transformation:
/// - Keeps migration directory selection explicit for project layouts that do
///   not use the default path.
#[test]
fn parse_db_command_accepts_validate_custom_directory() {
    assert_eq!(
        parse_db_command(&["validate".to_string(), "schema".to_string()]),
        Ok(DbCommand::Validate {
            directory: PathBuf::from("schema"),
        })
    );
}

/// Parses `db status` with the default migration directory.
///
/// Inputs:
/// - Command-local arguments containing only `status`.
///
/// Output:
/// - Test passes when the parser selects the default migration directory.
///
/// Transformation:
/// - Exercises status command parsing without touching the filesystem.
#[test]
fn parse_db_command_accepts_status_default_directory() {
    assert_eq!(
        parse_db_command(&["status".to_string()]),
        Ok(DbCommand::Status {
            directory: PathBuf::from(DEFAULT_MIGRATION_DIR),
            database_url: None,
        })
    );
}

/// Parses `db status` with an explicit migration directory.
///
/// Inputs:
/// - Command-local arguments containing `status` and one directory.
///
/// Output:
/// - Test passes when the parser preserves the supplied directory.
///
/// Transformation:
/// - Keeps status directory selection consistent with `validate`.
#[test]
fn parse_db_command_accepts_status_custom_directory() {
    assert_eq!(
        parse_db_command(&["status".to_string(), "schema".to_string()]),
        Ok(DbCommand::Status {
            directory: PathBuf::from("schema"),
            database_url: None,
        })
    );
}

/// Parses `db status` with database URL and migration directory.
///
/// Inputs:
/// - Command-local arguments containing `status`, `--database-url`, a URL, and
///   one directory.
///
/// Output:
/// - Test passes when both URL and directory are preserved.
///
/// Transformation:
/// - Covers the status command's future database-history loading surface.
#[test]
fn parse_db_command_accepts_status_database_url_and_directory() {
    assert_eq!(
        parse_db_command(&[
            "status".to_string(),
            "--database-url".to_string(),
            "postgres://localhost/terlan".to_string(),
            "schema".to_string(),
        ]),
        Ok(DbCommand::Status {
            directory: PathBuf::from("schema"),
            database_url: Some("postgres://localhost/terlan".to_string()),
        })
    );
}

/// Parses help flags for every documented DB subcommand.
///
/// Inputs:
/// - Command-local database arguments ending in `--help`.
///
/// Output:
/// - Test passes when each documented subcommand routes to DB help.
///
/// Transformation:
/// - Keeps the parser aligned with the public `terlc help db` surface while
///   avoiding per-subcommand usage text duplication.
#[test]
fn parse_db_command_accepts_help_for_documented_subcommands() {
    for subcommand in [
        "init", "new", "validate", "status", "migrate", "rebuild", "reset",
    ] {
        assert_eq!(
            parse_db_command(&[subcommand.to_string(), "--help".to_string()]),
            Ok(DbCommand::Help),
            "db {subcommand} --help should route to DB help"
        );
    }
}

/// Resolves database config from command-line input before environment input.
///
/// Inputs:
/// - Explicit and environment database URLs.
///
/// Output:
/// - Test passes when the explicit URL wins and remains validated.
///
/// Transformation:
/// - Exercises source precedence without mutating process environment
///   variables.
#[test]
fn resolve_optional_database_config_prefers_explicit_url() {
    let resolved = super::resolve_optional_database_config_from_sources(
        Some("postgres://explicit/terlan".to_string()),
        Some("postgres://env/terlan".to_string()),
    )
    .expect("config should validate")
    .expect("config should exist");

    assert_eq!(resolved.source_label(), "--database-url");
    assert_eq!(resolved.config().url(), "postgres://explicit/terlan");
}

/// Resolves database config from environment input when CLI input is absent.
///
/// Inputs:
/// - No explicit database URL and one environment URL.
///
/// Output:
/// - Test passes when the environment source is preserved.
///
/// Transformation:
/// - Covers the `TERLAN_DATABASE_URL` fallback without touching global
///   environment state.
#[test]
fn resolve_optional_database_config_uses_env_url() {
    let resolved = super::resolve_optional_database_config_from_sources(
        None,
        Some("postgresql://env/terlan".to_string()),
    )
    .expect("config should validate")
    .expect("config should exist");

    assert_eq!(resolved.source_label(), "TERLAN_DATABASE_URL");
    assert_eq!(resolved.config().url(), "postgresql://env/terlan");
}

/// Rejects invalid database URL schemes during config resolution.
///
/// Inputs:
/// - Explicit non-Postgres database URL.
///
/// Output:
/// - Test passes when resolution reports a stable invalid URL diagnostic.
///
/// Transformation:
/// - Reuses the SafeNative Postgres config validator before any adapter path
///   can run.
#[test]
fn resolve_optional_database_config_rejects_invalid_scheme() {
    let error = super::resolve_optional_database_config_from_sources(
        Some("sqlite://local.db".to_string()),
        None,
    )
    .expect_err("invalid database URL should fail");

    assert!(error.contains("invalid Postgres database URL"));
}

/// Rejects unknown database subcommands.
///
/// Inputs:
/// - Command-local arguments with an unsupported subcommand.
///
/// Output:
/// - Test passes when parsing returns a stable unknown-subcommand error.
///
/// Transformation:
/// - Prevents future migration commands from appearing as accepted before they
///   are implemented.
#[test]
fn parse_db_command_rejects_unknown_subcommand() {
    assert_eq!(
        parse_db_command(&["apply".to_string()]),
        Err("unknown terlc db subcommand: apply".to_string())
    );
}

/// Rejects `db new` without a migration name.
///
/// Inputs:
/// - Command-local arguments containing only `new`.
///
/// Output:
/// - Test passes when parsing reports the missing-name error.
///
/// Transformation:
/// - Prevents creation of timestamp-only migration files.
#[test]
fn parse_db_command_rejects_new_without_name() {
    assert_eq!(
        parse_db_command(&["new".to_string()]),
        Err("terlc db new requires a migration name".to_string())
    );
}

/// Rejects extra `db validate` operands.
///
/// Inputs:
/// - Command-local arguments with two migration directories.
///
/// Output:
/// - Test passes when parsing reports the arity error.
///
/// Transformation:
/// - Keeps `validate` command shape deterministic for scripts.
#[test]
fn parse_db_command_rejects_validate_extra_operands() {
    assert_eq!(
        parse_db_command(&["validate".to_string(), "one".to_string(), "two".to_string(),]),
        Err("terlc db validate accepts at most one migration directory".to_string())
    );
}

/// Rejects extra `db status` operands.
///
/// Inputs:
/// - Command-local arguments with two migration directories.
///
/// Output:
/// - Test passes when parsing reports the arity error.
///
/// Transformation:
/// - Keeps `status` command shape deterministic for scripts.
#[test]
fn parse_db_command_rejects_status_extra_operands() {
    assert_eq!(
        parse_db_command(&["status".to_string(), "one".to_string(), "two".to_string(),]),
        Err("terlc db status accepts at most one migration directory".to_string())
    );
}

/// Rejects duplicate database URL flags for live DB commands.
///
/// Inputs:
/// - Command-local migrate arguments with two `--database-url` flags.
///
/// Output:
/// - Test passes when parsing returns the duplicate-URL diagnostic.
///
/// Transformation:
/// - Keeps live DB command configuration single-sourced and predictable.
#[test]
fn parse_db_command_rejects_duplicate_database_url() {
    assert_eq!(
        parse_db_command(&[
            "migrate".to_string(),
            "--database-url".to_string(),
            "postgres://localhost/one".to_string(),
            "--database-url".to_string(),
            "postgres://localhost/two".to_string(),
        ]),
        Err("terlc db migrate accepts one --database-url".to_string())
    );
}

/// Rejects duplicate development flags for destructive DB commands.
///
/// Inputs:
/// - Command-local rebuild arguments with two `--dev` flags.
///
/// Output:
/// - Test passes when parsing returns the duplicate-development-flag
///   diagnostic.
///
/// Transformation:
/// - Keeps destructive command opt-in explicit and single-sourced before any
///   database configuration or migration execution can run.
#[test]
fn parse_db_command_rejects_duplicate_dev_flag() {
    assert_eq!(
        parse_db_command(&[
            "rebuild".to_string(),
            "--dev".to_string(),
            "--dev".to_string(),
        ]),
        Err("terlc db rebuild accepts one --dev flag".to_string())
    );
}

/// Initializes a migration directory through the command runner.
///
/// Inputs:
/// - Temporary parent directory and one child migration directory path.
///
/// Output:
/// - Test passes when `run` creates the directory and returns success.
///
/// Transformation:
/// - Exercises `terlc db init` without touching a database.
#[test]
fn run_init_creates_migration_directory() {
    let directory = temp_db_dir("run_init_creates_migration_directory").join("db/migrations");

    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec!["init".to_string(), directory.display().to_string()],
    });

    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(directory.is_dir());

    remove_dir(
        directory
            .parent()
            .and_then(Path::parent)
            .expect("temp root"),
    );
}

/// Creates a timestamped migration template through the command runner.
///
/// Inputs:
/// - Temporary migration directory and one snake-case migration name.
///
/// Output:
/// - Test passes when exactly one valid migration file is created and
///   validation accepts it.
///
/// Transformation:
/// - Exercises `terlc db new` and immediately verifies that the generated file
///   conforms to the migration parser contract.
#[test]
fn run_new_creates_valid_migration_template() {
    let directory = temp_db_dir("run_new_creates_valid_migration_template");

    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec![
            "new".to_string(),
            "create_users".to_string(),
            directory.display().to_string(),
        ],
    });

    assert_eq!(exit, ExitCode::SUCCESS);
    let files = fs::read_dir(&directory)
        .expect("read migration dir")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect files");
    assert_eq!(files.len(), 1);
    let file_name = files[0].file_name().to_string_lossy().to_string();
    assert!(file_name.ends_with("_create_users.sql"));

    let validate_exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec!["validate".to_string(), directory.display().to_string()],
    });
    assert_eq!(validate_exit, ExitCode::SUCCESS);

    remove_dir(&directory);
}

/// Rejects invalid migration names through the command runner.
///
/// Inputs:
/// - Temporary migration directory and one non-snake-case migration name.
///
/// Output:
/// - Test passes when `run` returns an argument error and creates no file.
///
/// Transformation:
/// - Reuses filename parser validation for generated migration names.
#[test]
fn run_new_rejects_invalid_migration_name() {
    let directory = temp_db_dir("run_new_rejects_invalid_migration_name");

    let exit = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec![
            "new".to_string(),
            "CreateUsers".to_string(),
            directory.display().to_string(),
        ],
    });

    assert_eq!(exit, ExitCode::from(2));
    assert!(fs::read_dir(&directory)
        .expect("read migration dir")
        .next()
        .is_none());

    remove_dir(&directory);
}

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
            "--database-url".to_string(),
            "postgres://127.0.0.1:1/terlan".to_string(),
            directory.display().to_string(),
        ],
    });
    assert_eq!(exit, ExitCode::from(1));

    remove_dir(&directory);
}

/// Rejects destructive development commands for production-looking targets.
///
/// Inputs:
/// - Temporary directory containing one valid migration file.
/// - Remote Postgres URL whose database name does not include a development
///   marker and points at an unreachable port.
///
/// Output:
/// - Test passes when `run` returns an argument error before adapter gating.
///
/// Transformation:
/// - Exercises the static development-target guard before any live database
///   execution exists.
#[test]
fn run_rebuild_with_dev_rejects_non_development_database_url() {
    let directory = temp_db_dir("run_rebuild_with_dev_rejects_non_development_database_url");
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
            "--database-url".to_string(),
            "postgres://db.example.com/terlan_prod".to_string(),
            directory.display().to_string(),
        ],
    });
    assert_eq!(exit, ExitCode::from(2));

    remove_dir(&directory);
}

/// Allows destructive development commands for explicitly development-named DBs.
///
/// Inputs:
/// - Temporary directory containing one valid migration file.
/// - Remote Postgres URL whose database name includes a development marker.
///
/// Output:
/// - Test passes when `run` validates safety and then returns live executor
///   failure.
///
/// Transformation:
/// - Keeps CI and remote development databases usable without weakening the
///   production-looking target rejection without depending on a live database.
#[test]
fn run_rebuild_with_dev_accepts_development_database_name() {
    let directory = temp_db_dir("run_rebuild_with_dev_accepts_development_database_name");
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
            "--database-url".to_string(),
            "postgres://127.0.0.1:1/terlan_dev".to_string(),
            directory.display().to_string(),
        ],
    });
    assert_eq!(exit, ExitCode::from(1));

    remove_dir(&directory);
}

/// Validates the stable skip message for absent live Postgres configuration.
///
/// Inputs:
/// - Missing environment variable marker.
///
/// Output:
/// - Test passes when the helper returns the documented skip message.
///
/// Transformation:
/// - Keeps no-database local test behavior visible without invoking Docker or
///   requiring a live Postgres service.
#[test]
fn live_postgres_url_reports_stable_skip_message_when_unconfigured() {
    let error = live_postgres_url_or_skip_message(Err(std::env::VarError::NotPresent))
        .expect_err("missing live database URL should skip");

    assert_eq!(error, LIVE_POSTGRES_SKIP_MESSAGE);
}

/// Runs migration commands against a live Docker-provided Postgres database.
///
/// Inputs:
/// - `TERLAN_TEST_POSTGRES_URL`: optional live Postgres URL supplied by the
///   Docker-backed Postgres make target.
///
/// Output:
/// - Test skips when no URL is configured.
/// - Test passes when rebuild, status, incremental migrate, and second status
///   all succeed against the live database.
///
/// Transformation:
/// - Exercises the SafeNative migration executor through the public CLI
///   command router while keeping normal unit tests database-free.
#[test]
fn run_db_migration_lifecycle_against_live_postgres_when_configured() {
    let database_url =
        match live_postgres_url_or_skip_message(std::env::var("TERLAN_TEST_POSTGRES_URL")) {
            Ok(url) => url,
            Err(message) => {
                println!("{message}");
                return;
            }
        };
    let directory = temp_db_dir("run_db_migration_lifecycle_against_live_postgres_when_configured");
    fs::write(
        directory.join("20260619123000_create_live_users.sql"),
        "-- +terlan Up\nCREATE TABLE live_users (id BIGSERIAL PRIMARY KEY);\n",
    )
    .expect("write first migration");

    let rebuild = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec![
            "rebuild".to_string(),
            "--dev".to_string(),
            "--database-url".to_string(),
            database_url.clone(),
            directory.display().to_string(),
        ],
    });
    assert_eq!(rebuild, ExitCode::SUCCESS);

    let first_status = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec![
            "status".to_string(),
            "--database-url".to_string(),
            database_url.clone(),
            directory.display().to_string(),
        ],
    });
    assert_eq!(first_status, ExitCode::SUCCESS);

    fs::write(
        directory.join("20260619123100_add_live_user_email.sql"),
        "-- +terlan Up\nALTER TABLE live_users ADD COLUMN email TEXT;\n",
    )
    .expect("write second migration");

    let migrate = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec![
            "migrate".to_string(),
            "--database-url".to_string(),
            database_url.clone(),
            directory.display().to_string(),
        ],
    });
    assert_eq!(migrate, ExitCode::SUCCESS);

    let second_status = run(CliCommand {
        verb: Some("db".to_string()),
        args: vec![
            "status".to_string(),
            "--database-url".to_string(),
            database_url,
            directory.display().to_string(),
        ],
    });
    assert_eq!(second_status, ExitCode::SUCCESS);

    remove_dir(&directory);
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
/// - Synthetic status entries covering applied, pending, missing, and
///   divergent states.
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
        status_entry("20260619123000", MigrationStatusState::Divergent),
    ];

    let summary = MigrationStatusSummary::from_entries(&entries);

    assert_eq!(
        summary,
        MigrationStatusSummary {
            applied: 1,
            divergent: 1,
            missing: 1,
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

/// Creates a unique temporary DB-command test directory.
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
fn temp_db_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "terlan_db_command_test_{}_{}_{}",
        std::process::id(),
        nanos,
        label
    ));
    fs::create_dir_all(&directory).expect("create temp db command directory");
    directory
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
        state,
    }
}

/// Removes a temporary DB-command test directory.
///
/// Inputs:
/// - `directory`: path created by `temp_db_dir`.
///
/// Output:
/// - Directory is removed or the test fails.
///
/// Transformation:
/// - Cleans up files created by command tests.
fn remove_dir(directory: &Path) {
    fs::remove_dir_all(directory).expect("remove temp db command directory");
}
