use super::migration::{MigrationStatusEntry, MigrationStatusState};
use super::test_support::{remove_dir, temp_db_dir};
use super::{parse_db_command, run, DbCommand, MigrationStatusSummary, DEFAULT_MIGRATION_DIR};
use crate::CliCommand;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
            confirm: false,
            database_url: None,
        })
    );
}

/// Parses confirmed `db rebuild --dev` with an explicit migration directory.
///
/// Inputs:
/// - Command-local arguments containing `rebuild`, both safety flags, and one
///   directory.
///
/// Output:
/// - Test passes when the parser preserves both safety flags and directory.
///
/// Transformation:
/// - Accepts the preferred destructive development command spelling.
#[test]
fn parse_db_command_accepts_rebuild_with_dev_and_directory() {
    assert_eq!(
        parse_db_command(&[
            "rebuild".to_string(),
            "--dev".to_string(),
            "--confirm".to_string(),
            "schema".to_string(),
        ]),
        Ok(DbCommand::Rebuild {
            directory: PathBuf::from("schema"),
            dev: true,
            confirm: true,
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
            "--confirm".to_string(),
            "--database-url".to_string(),
            "postgres://localhost/terlan_dev".to_string(),
            "schema".to_string(),
        ]),
        Ok(DbCommand::Rebuild {
            directory: PathBuf::from("schema"),
            dev: true,
            confirm: true,
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
            "--confirm".to_string(),
        ]),
        Ok(DbCommand::Reset {
            directory: PathBuf::from("schema"),
            dev: true,
            confirm: true,
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
            "--confirm".to_string(),
        ]),
        Ok(DbCommand::Reset {
            directory: PathBuf::from("schema"),
            dev: true,
            confirm: true,
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

#[test]
fn parse_db_command_accepts_snapshot_contract_options() {
    assert_eq!(
        parse_db_command(&[
            "snapshot".to_string(),
            "--check".to_string(),
            "--output".to_string(),
            "artifacts/schema.json".to_string(),
            "--database-url".to_string(),
            "postgres://localhost/terlan".to_string(),
            "schema/migrations".to_string(),
        ]),
        Ok(DbCommand::Snapshot {
            directory: PathBuf::from("schema/migrations"),
            database_url: Some("postgres://localhost/terlan".to_string()),
            output: Some(PathBuf::from("artifacts/schema.json")),
            check: true,
        })
    );
}

#[test]
fn parse_db_command_defaults_snapshot_paths_and_rejects_duplicate_options() {
    assert_eq!(
        parse_db_command(&["snapshot".to_string()]),
        Ok(DbCommand::Snapshot {
            directory: PathBuf::from(DEFAULT_MIGRATION_DIR),
            database_url: None,
            output: None,
            check: false,
        })
    );
    assert_eq!(
        parse_db_command(&[
            "snapshot".to_string(),
            "--output".to_string(),
            "one.json".to_string(),
            "--output".to_string(),
            "two.json".to_string(),
        ]),
        Err("terlc db snapshot accepts one --output".to_string())
    );
    assert_eq!(
        parse_db_command(&[
            "snapshot".to_string(),
            "--check".to_string(),
            "--check".to_string(),
        ]),
        Err("terlc db snapshot accepts one --check flag".to_string())
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
        "init", "new", "validate", "status", "snapshot", "migrate", "rebuild", "reset",
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
    assert_eq!(resolved.config.url(), "postgres://explicit/terlan");
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
    assert_eq!(resolved.config.url(), "postgresql://env/terlan");
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
/// - Reuses the shared Postgres config validator before any adapter path
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

#[test]
fn local_database_dependencies_validate_project_compose_before_socket_work() {
    let root = temp_db_dir("local_dependency_compose_validation");
    let migrations = root.join("db/migrations");
    fs::create_dir_all(&migrations).expect("create migrations");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n",
    )
    .expect("write manifest");
    fs::write(root.join("compose.yml"), "services:\n  postgres: [")
        .expect("write malformed compose");
    let config = super::resolve_optional_database_config_from_sources(
        Some("postgres://127.0.0.1:5432/terlan_dev".to_string()),
        None,
    )
    .expect("valid config")
    .expect("resolved config");

    let error = super::prepare_local_database_dependencies(&migrations, &config)
        .expect_err("malformed declared dependency must fail");

    assert!(error.contains("malformed Docker Compose file"));
    remove_dir(&root);
}

#[test]
fn remote_database_commands_do_not_start_local_project_dependencies() {
    let root = temp_db_dir("remote_dependency_bypass");
    let migrations = root.join("db/migrations");
    fs::create_dir_all(&migrations).expect("create migrations");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n",
    )
    .expect("write manifest");
    fs::write(root.join("compose.yml"), "services:\n  postgres: [")
        .expect("write malformed compose");
    let config = super::resolve_optional_database_config_from_sources(
        Some("postgres://db.example.test/terlan".to_string()),
        None,
    )
    .expect("valid config")
    .expect("resolved config");

    let session = super::prepare_local_database_dependencies(&migrations, &config)
        .expect("remote target must not touch local dependencies");

    assert!(session.is_none());

    remove_dir(&root);
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

/// Rejects duplicate destructive-action confirmation flags.
#[test]
fn parse_db_command_rejects_duplicate_confirm_flag() {
    assert_eq!(
        parse_db_command(&[
            "reset".to_string(),
            "--dev".to_string(),
            "--confirm".to_string(),
            "--confirm".to_string(),
        ]),
        Err("terlc db reset accepts one --confirm flag".to_string())
    );
}

/// Rejects destructive-action confirmation on non-destructive commands.
#[test]
fn parse_db_command_rejects_confirm_for_migrate() {
    assert_eq!(
        parse_db_command(&["migrate".to_string(), "--confirm".to_string()]),
        Err("terlc db migrate does not accept --confirm".to_string())
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
