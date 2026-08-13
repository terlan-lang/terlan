mod args;
mod execution;
mod history;
pub(crate) mod migration;
mod snapshot;
mod status;

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::commands::dev_dependencies;
use crate::terlan_native::postgres;
use args::{parse_db_command, DbCommand};
use execution::{MigrationExecutionRequest, MigrationExecutor, VmMigrationExecutor};
use history::load_applied_migration_history;
use migration::{
    discover_migration_files, load_migration_files, migration_engine_inputs, migration_status,
    parse_migration_file_name, pending_migration_engine_inputs, MigrationDiscoveryDiagnostic,
    MigrationEngineInput, MigrationLoadDiagnostic, MigrationStatusEntry, MigrationStatusState,
};
use snapshot::{
    capture_schema_snapshot, check_schema_snapshot, default_snapshot_path, write_schema_snapshot,
};
use time::{format_description, OffsetDateTime};

use crate::CliCommand;

const DEFAULT_MIGRATION_DIR: &str = "db/migrations";
const DATABASE_URL_ENV: &str = "TERLAN_DATABASE_URL";

/// Executes the `db` CLI command group.
///
/// Inputs:
/// - `cmd`: parsed CLI command whose first argument is the database
///   subcommand.
///
/// Output:
/// - `ExitCode::SUCCESS` when the selected database command succeeds.
/// - `ExitCode::from(2)` for malformed command-local arguments.
/// - `ExitCode::from(1)` for validation failures.
///
/// Transformation:
/// - Dispatches supported `db` subcommands while keeping database execution out
///   of the top-level CLI router.
pub(crate) fn run(cmd: CliCommand) -> ExitCode {
    match parse_db_command(&cmd.args) {
        Ok(DbCommand::Init { directory }) => run_init(directory),
        Ok(DbCommand::New { name, directory }) => run_new(&name, directory),
        Ok(DbCommand::Validate { directory }) => run_validate(directory),
        Ok(DbCommand::Status {
            directory,
            database_url,
        }) => run_status(directory, database_url),
        Ok(DbCommand::Snapshot {
            directory,
            database_url,
            output,
            check,
        }) => run_snapshot(directory, database_url, output, check),
        Ok(DbCommand::Migrate {
            directory,
            database_url,
        }) => run_adapter_gated_command("migrate", directory, database_url),
        Ok(DbCommand::Rebuild {
            directory,
            dev,
            confirm,
            database_url,
        }) => {
            run_destructive_adapter_gated_command("rebuild", directory, dev, confirm, database_url)
        }
        Ok(DbCommand::Reset {
            directory,
            dev,
            confirm,
            database_url,
        }) => run_destructive_adapter_gated_command("reset", directory, dev, confirm, database_url),
        Ok(DbCommand::Help) => {
            print_usage();
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            print_usage();
            ExitCode::from(2)
        }
    }
}

/// Prints usage for the `db` command group.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Writes concise usage lines to stdout.
///
/// Transformation:
/// - Keeps command-local help text near the command parser.
fn print_usage() {
    println!("terlc db init [migrations-dir]");
    println!("terlc db new <name> [migrations-dir]");
    println!("terlc db validate [migrations-dir]");
    println!("terlc db status [--database-url URL] [migrations-dir]");
    println!("terlc db snapshot [--check] [--output PATH] [--database-url URL] [migrations-dir]");
    println!("terlc db migrate [--database-url URL] [migrations-dir]");
    println!("terlc db rebuild --dev --confirm [--database-url URL] [migrations-dir]");
    println!("terlc db reset --dev --confirm [--database-url URL] [migrations-dir]");
}

/// Creates the migration directory.
///
/// Inputs:
/// - `directory`: migration directory to create.
///
/// Output:
/// - Success when the directory exists or was created.
/// - Failure when the directory cannot be created.
///
/// Transformation:
/// - Uses `create_dir_all` so repeated `db init` is idempotent.
fn run_init(directory: PathBuf) -> ExitCode {
    match fs::create_dir_all(&directory) {
        Ok(()) => {
            println!("initialized migration directory {}", directory.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!(
                "{}: cannot initialize migration directory: {error}",
                directory.display()
            );
            ExitCode::from(1)
        }
    }
}

/// Creates one new timestamped migration file.
///
/// Inputs:
/// - `name`: snake-case migration name.
/// - `directory`: migration directory where the file should be created.
///
/// Output:
/// - Success when a new migration template is written.
/// - Failure when the name is invalid or the file cannot be created.
///
/// Transformation:
/// - Generates a UTC timestamped filename, validates it through the same parser
///   used by discovery, creates the directory if needed, and writes a Terlan
///   marker template without touching a database.
fn run_new(name: &str, directory: PathBuf) -> ExitCode {
    let timestamp = match current_migration_timestamp() {
        Ok(timestamp) => timestamp,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let file_name = format!("{timestamp}_{name}.sql");
    if let Err(diagnostic) = parse_migration_file_name(&file_name) {
        eprintln!("invalid migration name `{name}`: {}", diagnostic.message);
        return ExitCode::from(2);
    }
    if let Err(error) = fs::create_dir_all(&directory) {
        eprintln!(
            "{}: cannot create migration directory: {error}",
            directory.display()
        );
        return ExitCode::from(1);
    }

    let path = directory.join(file_name);
    let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("{}: cannot create migration file: {error}", path.display());
            return ExitCode::from(1);
        }
    };
    if let Err(error) = file.write_all(migration_template(name).as_bytes()) {
        eprintln!("{}: cannot write migration file: {error}", path.display());
        return ExitCode::from(1);
    }

    println!("created migration {}", path.display());
    ExitCode::SUCCESS
}

/// Executes migration validation without touching a database.
///
/// Inputs:
/// - `directory`: migration directory to scan.
///
/// Output:
/// - Success when all migration files can be discovered, parsed, and
///   checksummed.
/// - Failure when discovery or source validation reports a diagnostic.
///
/// Transformation:
/// - Runs filesystem discovery, migration loading, and engine-input
///   conversion, then prints a compact validation summary for users and
///   scripts.
fn run_validate(directory: PathBuf) -> ExitCode {
    let files = match discover_migration_files(&directory) {
        Ok(files) => files,
        Err(diagnostic) => {
            eprintln!("{}", format_discovery_diagnostic(diagnostic));
            return ExitCode::from(1);
        }
    };
    let loaded = match load_migration_files(&files) {
        Ok(loaded) => loaded,
        Err(diagnostic) => {
            eprintln!("{}", format_load_diagnostic(diagnostic));
            return ExitCode::from(1);
        }
    };

    let engine_inputs = migration_engine_inputs(&loaded);

    println!(
        "validated {} migration file(s) in {}",
        engine_inputs.len(),
        directory.display()
    );
    ExitCode::SUCCESS
}

/// Captures or verifies a deterministic database schema snapshot.
fn run_snapshot(
    directory: PathBuf,
    database_url: Option<String>,
    output: Option<PathBuf>,
    check: bool,
) -> ExitCode {
    let migrations = match load_all_migration_inputs(&directory) {
        Ok(migrations) => migrations,
        Err(exit) => return exit,
    };
    let config = match resolve_required_database_config(database_url) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let dependency_session = match prepare_local_database_dependencies(&directory, &config) {
        Ok(session) => session,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let mut client = match crate::runtime::vm::postgres_command::VmPostgresCommandClient::connect(
        &config.config,
    ) {
        Ok(client) => client,
        Err(message) => {
            eprintln!("terlc db snapshot failed through VM Postgres: {message}");
            return ExitCode::from(1);
        }
    };
    let snapshot = match capture_schema_snapshot(&mut client, &migrations) {
        Ok(snapshot) => snapshot,
        Err(message) => {
            eprintln!("terlc db snapshot failed through VM Postgres: {message}");
            return ExitCode::from(1);
        }
    };
    let path = output.unwrap_or_else(|| default_snapshot_path(&directory));
    let result = if check {
        check_schema_snapshot(&path, &snapshot)
    } else {
        write_schema_snapshot(&path, &snapshot)
    };
    let outcome = match result {
        Ok(()) => {
            println!(
                "{} schema snapshot {}",
                if check { "validated" } else { "wrote" },
                path.display()
            );
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    };
    dev_dependencies::finish_dependency_session(dependency_session, outcome)
}

/// Reports migration status.
///
/// Inputs:
/// - `directory`: migration directory to scan.
///
/// Output:
/// - Success when all migrations are valid and status rows are printed.
/// - Failure when discovery or source validation reports a diagnostic.
///
/// Transformation:
/// - Reuses validation loading and renders status rows through the general
///   status summary path. Applied database history is still empty in this
///   slice, so valid local migrations remain pending until database history
///   loading is wired in.
fn run_status(directory: PathBuf, database_url: Option<String>) -> ExitCode {
    let files = match discover_migration_files(&directory) {
        Ok(files) => files,
        Err(diagnostic) => {
            eprintln!("{}", format_discovery_diagnostic(diagnostic));
            return ExitCode::from(1);
        }
    };
    let loaded = match load_migration_files(&files) {
        Ok(loaded) => loaded,
        Err(diagnostic) => {
            eprintln!("{}", format_load_diagnostic(diagnostic));
            return ExitCode::from(1);
        }
    };
    let (applied_history, dependency_session) = match resolve_optional_database_config(database_url)
    {
        Ok(Some(config)) => {
            let dependency_session = match prepare_local_database_dependencies(&directory, &config)
            {
                Ok(session) => session,
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            };
            let history = match load_applied_migration_history(&config) {
                Ok(history) => history,
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            };
            (history, dependency_session)
        }
        Ok(None) => (Vec::new(), None),
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let statuses = migration_status(&loaded, &applied_history);
    let summary = MigrationStatusSummary::from_entries(&statuses);

    println!(
        "migration status for {}: {} pending, {} applied, {} missing, {} out-of-order, {} checksum-mismatch, {} name-mismatch",
        directory.display(),
        summary.pending,
        summary.applied,
        summary.missing,
        summary.out_of_order,
        summary.checksum_mismatch,
        summary.name_mismatch
    );
    for status in statuses {
        println!(
            "{} {} {} {} {}",
            status.state.label(),
            status.version,
            status.name,
            status.checksum,
            status.applied_at.as_deref().unwrap_or("-")
        );
    }
    dev_dependencies::finish_dependency_session(dependency_session, ExitCode::SUCCESS)
}

/// Validates migrations and dispatches to the current database execution adapter.
///
/// Inputs:
/// - `command`: database subcommand name being executed.
/// - `directory`: migration directory to validate before execution.
/// - `database_url`: optional URL supplied through `--database-url`.
///
/// Output:
/// - Failure until the Postgres migration adapter is wired.
/// - Validation failure when local migration files are invalid.
///
/// Transformation:
/// - Reuses local migration planning, resolves the database target, then
///   delegates to the adapter boundary instead of embedding execution behavior
///   in the command router.
fn run_adapter_gated_command(
    command: &str,
    directory: PathBuf,
    database_url: Option<String>,
) -> ExitCode {
    let config = match resolve_required_database_config(database_url) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let dependency_session = match prepare_local_database_dependencies(&directory, &config) {
        Ok(session) => session,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let applied_history = match load_applied_migration_history(&config) {
        Ok(history) => history,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let pending = match load_pending_migration_inputs(&directory, &applied_history) {
        Ok(pending) => pending,
        Err(exit) => return exit,
    };
    let outcome = execute_migration_request(command, &config, &pending, false);
    dev_dependencies::finish_dependency_session(dependency_session, outcome)
}

/// Validates destructive-command safety before dispatching to the adapter.
///
/// Inputs:
/// - `command`: destructive database subcommand name.
/// - `directory`: migration directory to validate when `--dev` is present.
/// - `dev`: whether the command included the explicit development flag.
/// - `confirm`: whether the command included the independent confirmation flag.
/// - `database_url`: optional URL supplied through `--database-url`.
///
/// Output:
/// - Argument error when `--dev` is missing.
/// - Failure until the Postgres migration adapter is wired when `--dev` is
///   present and migrations validate.
///
/// Transformation:
/// - Enforces the 0.0.5 safety rule before local migration planning and before
///   any future database execution can happen.
fn run_destructive_adapter_gated_command(
    command: &str,
    directory: PathBuf,
    dev: bool,
    confirm: bool,
    database_url: Option<String>,
) -> ExitCode {
    if !dev {
        eprintln!("terlc db {command} is destructive and requires --dev");
        return ExitCode::from(2);
    }
    let config = match resolve_required_database_config(database_url) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    if let Err(message) = validate_development_database_config(command, &config, confirm) {
        eprintln!("{message}");
        return ExitCode::from(2);
    }
    let dependency_session = match prepare_local_database_dependencies(&directory, &config) {
        Ok(session) => session,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let pending = match load_pending_migration_inputs(&directory, &[]) {
        Ok(pending) => pending,
        Err(exit) => return exit,
    };
    let outcome = execute_migration_request(command, &config, &pending, true);
    dev_dependencies::finish_dependency_session(dependency_session, outcome)
}

/// Loads pending migration execution inputs from one directory.
///
/// Inputs:
/// - `directory`: migration directory to scan and parse.
///
/// Output:
/// - Engine-ready pending migration inputs when validation succeeds.
/// - Exit code for filesystem or source validation failures.
///
/// Transformation:
/// - Shares command validation and planning for `migrate`, `rebuild`, and
///   `reset` without coupling those commands to database mutation.
fn load_pending_migration_inputs(
    directory: &Path,
    applied_history: &[migration::AppliedMigration],
) -> Result<Vec<MigrationEngineInput>, ExitCode> {
    let files = match discover_migration_files(directory) {
        Ok(files) => files,
        Err(diagnostic) => {
            eprintln!("{}", format_discovery_diagnostic(diagnostic));
            return Err(ExitCode::from(1));
        }
    };
    let loaded = match load_migration_files(&files) {
        Ok(loaded) => loaded,
        Err(diagnostic) => {
            eprintln!("{}", format_load_diagnostic(diagnostic));
            return Err(ExitCode::from(1));
        }
    };
    match pending_migration_engine_inputs(&loaded, applied_history) {
        Ok(inputs) => Ok(inputs),
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.message);
            Err(ExitCode::from(1))
        }
    }
}

/// Loads every validated migration for schema snapshot identity.
fn load_all_migration_inputs(directory: &Path) -> Result<Vec<MigrationEngineInput>, ExitCode> {
    let files = discover_migration_files(directory).map_err(|diagnostic| {
        eprintln!("{}", format_discovery_diagnostic(diagnostic));
        ExitCode::from(1)
    })?;
    let loaded = load_migration_files(&files).map_err(|diagnostic| {
        eprintln!("{}", format_load_diagnostic(diagnostic));
        ExitCode::from(1)
    })?;
    Ok(migration_engine_inputs(&loaded))
}

/// Executes a validated migration request through the configured adapter.
///
/// Inputs:
/// - `command`: database command name.
/// - `config`: resolved database configuration.
/// - `pending`: validated pending migration inputs.
/// - `destructive`: whether the command passed destructive development guards.
///
/// Output:
/// - Success when the adapter reports applied migrations.
/// - Failure when the adapter reports a user-facing diagnostic.
///
/// Transformation:
/// - Builds the command-independent execution request and lets the adapter own
///   database mutation behavior.
fn execute_migration_request(
    command: &str,
    config: &ResolvedDatabaseConfig,
    pending: &[MigrationEngineInput],
    destructive: bool,
) -> ExitCode {
    let executor = VmMigrationExecutor;
    let request = MigrationExecutionRequest::new(command, config, pending, destructive);
    match executor.execute(request) {
        Ok(report) => {
            println!(
                "terlc db {command} applied {} migration file(s)",
                report.applied()
            );
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Starts declared local dependencies before a loopback database command.
fn prepare_local_database_dependencies(
    directory: &Path,
    config: &ResolvedDatabaseConfig,
) -> Result<Option<dev_dependencies::DevDependencySession>, String> {
    let target = parse_database_target(config.config.url()).map_err(|error| error.to_string())?;
    if !is_local_database_host(&target.host) {
        return Ok(None);
    }
    dev_dependencies::start_project_dependencies_for_path(directory).map(Some)
}

/// Counted summary of migration status rows.
///
/// Inputs:
/// - Produced from rendered migration status entries.
///
/// Output:
/// - Status counts used by `terlc db status` command output.
///
/// Transformation:
/// - Keeps command summary formatting independent from status comparison so
///   future database history loading can reuse the same rendering path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MigrationStatusSummary {
    applied: usize,
    checksum_mismatch: usize,
    missing: usize,
    name_mismatch: usize,
    out_of_order: usize,
    pending: usize,
}

mod database_config;
use database_config::{
    is_local_database_host, parse_database_target, resolve_optional_database_config,
    resolve_required_database_config, validate_development_database_config, ResolvedDatabaseConfig,
};
#[cfg(test)]
pub(crate) use database_config::{
    resolve_optional_database_config_from_sources, DatabaseConfigSource,
};

impl MigrationStatusSummary {
    /// Builds a status summary from migration status entries.
    ///
    /// Inputs:
    /// - `entries`: status rows produced by `migration_status`.
    ///
    /// Output:
    /// - Counted summary grouped by status state.
    ///
    /// Transformation:
    /// - Iterates rows once and increments the matching stable status bucket.
    fn from_entries(entries: &[MigrationStatusEntry]) -> Self {
        let mut summary = Self {
            applied: 0,
            checksum_mismatch: 0,
            missing: 0,
            name_mismatch: 0,
            out_of_order: 0,
            pending: 0,
        };

        for entry in entries {
            match entry.state {
                MigrationStatusState::Applied => summary.applied += 1,
                MigrationStatusState::ChecksumMismatch => summary.checksum_mismatch += 1,
                MigrationStatusState::Missing => summary.missing += 1,
                MigrationStatusState::NameMismatch => summary.name_mismatch += 1,
                MigrationStatusState::OutOfOrder => summary.out_of_order += 1,
                MigrationStatusState::Pending => summary.pending += 1,
            }
        }

        summary
    }
}

/// Formats a migration discovery diagnostic.
///
/// Inputs:
/// - `diagnostic`: filesystem or filename diagnostic.
///
/// Output:
/// - Human-readable single-line error.
///
/// Transformation:
/// - Adds path context without changing the stable diagnostic message.
fn format_discovery_diagnostic(diagnostic: MigrationDiscoveryDiagnostic) -> String {
    format!("{}: {}", diagnostic.path.display(), diagnostic.message)
}

/// Formats a migration loading diagnostic.
///
/// Inputs:
/// - `diagnostic`: file read, checksum, or marker parsing diagnostic.
///
/// Output:
/// - Human-readable single-line error.
///
/// Transformation:
/// - Adds path and line context without changing the stable diagnostic message.
fn format_load_diagnostic(diagnostic: MigrationLoadDiagnostic) -> String {
    format!(
        "{}:{}: {}",
        diagnostic.path.display(),
        diagnostic.line,
        diagnostic.message
    )
}

/// Builds the current UTC migration timestamp.
///
/// Inputs:
/// - Current system clock.
///
/// Output:
/// - Fourteen-digit UTC timestamp in `YYYYMMDDHHMMSS` form.
/// - Error message when formatting fails.
///
/// Transformation:
/// - Uses the `time` crate instead of hand-rolled calendar arithmetic so
///   generated migration filenames match the parser contract.
fn current_migration_timestamp() -> Result<String, String> {
    let format =
        format_description::parse_borrowed::<2>("[year][month][day][hour][minute][second]")
            .map_err(|error| format!("cannot create migration timestamp formatter: {error}"))?;
    OffsetDateTime::now_utc()
        .format(&format)
        .map_err(|error| format!("cannot format migration timestamp: {error}"))
}

/// Builds the initial SQL migration template.
///
/// Inputs:
/// - `name`: migration name used in a comment for reader context.
///
/// Output:
/// - SQL template with Terlan `Up` and `Down` markers.
///
/// Transformation:
/// - Keeps generated migration files immediately compatible with
///   `terlc db validate`.
fn migration_template(name: &str) -> String {
    format!("-- {name}\n-- +terlan Up\n-- Write forward migration SQL here.\n\n-- +terlan Down\n-- Write optional local rollback SQL here.\n")
}

#[cfg(all(test, not(feature = "serve-runtime-bin"), feature = "postgres-libpq"))]
#[cfg(test)]
#[path = "live_test.rs"]
#[cfg(test)]
mod live_test;
#[cfg(test)]
#[path = "migration_test.rs"]
#[cfg(test)]
mod migration_test;
#[cfg(test)]
#[path = "mod_test.rs"]
#[cfg(test)]
mod mod_test;
#[cfg(test)]
mod test_support;
