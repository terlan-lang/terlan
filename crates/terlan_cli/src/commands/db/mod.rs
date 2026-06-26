mod args;
mod execution;
mod history;
pub(crate) mod migration;
mod status;

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use args::{parse_db_command, DbCommand};
use execution::{MigrationExecutionRequest, MigrationExecutor, SafeNativeMigrationExecutor};
use history::load_applied_migration_history;
use migration::{
    discover_migration_files, load_migration_files, migration_engine_inputs, migration_status,
    parse_migration_file_name, pending_migration_engine_inputs, MigrationDiscoveryDiagnostic,
    MigrationEngineInput, MigrationLoadDiagnostic, MigrationStatusEntry, MigrationStatusState,
};
use terlan_safenative::postgres;
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
        Ok(DbCommand::Migrate {
            directory,
            database_url,
        }) => run_adapter_gated_command("migrate", directory, database_url),
        Ok(DbCommand::Rebuild {
            directory,
            dev,
            database_url,
        }) => run_destructive_adapter_gated_command("rebuild", directory, dev, database_url),
        Ok(DbCommand::Reset {
            directory,
            dev,
            database_url,
        }) => run_destructive_adapter_gated_command("reset", directory, dev, database_url),
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
    println!("terlc db migrate [--database-url URL] [migrations-dir]");
    println!("terlc db rebuild --dev [--database-url URL] [migrations-dir]");
    println!("terlc db reset --dev [--database-url URL] [migrations-dir]");
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
    let applied_history = match resolve_optional_database_config(database_url) {
        Ok(Some(config)) => match load_applied_migration_history(&config) {
            Ok(history) => history,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        },
        Ok(None) => Vec::new(),
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let statuses = migration_status(&loaded, &applied_history);
    let summary = MigrationStatusSummary::from_entries(&statuses);

    println!(
        "migration status for {}: {} pending, {} applied, {} missing, {} divergent",
        directory.display(),
        summary.pending,
        summary.applied,
        summary.missing,
        summary.divergent
    );
    for status in statuses {
        println!(
            "{} {} {} {}",
            status.state.label(),
            status.version,
            status.name,
            status.checksum
        );
    }
    ExitCode::SUCCESS
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
    execute_migration_request(command, &config, &pending, false)
}

/// Validates destructive-command safety before dispatching to the adapter.
///
/// Inputs:
/// - `command`: destructive database subcommand name.
/// - `directory`: migration directory to validate when `--dev` is present.
/// - `dev`: whether the command included the explicit development flag.
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
    database_url: Option<String>,
) -> ExitCode {
    if !dev {
        eprintln!("terlc db {command} is destructive and requires --dev");
        return ExitCode::from(2);
    }
    let pending = match load_pending_migration_inputs(&directory, &[]) {
        Ok(pending) => pending,
        Err(exit) => return exit,
    };
    let config = match resolve_required_database_config(database_url) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    if let Err(message) = validate_development_database_config(command, &config) {
        eprintln!("{message}");
        return ExitCode::from(2);
    }
    execute_migration_request(command, &config, &pending, true)
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
    let executor = SafeNativeMigrationExecutor;
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
    divergent: usize,
    missing: usize,
    pending: usize,
}

/// Resolved database configuration for live `terlc db` commands.
///
/// Inputs:
/// - Produced from `--database-url` or `TERLAN_DATABASE_URL`.
///
/// Output:
/// - Validated Postgres config plus a source label for diagnostics.
///
/// Transformation:
/// - Keeps command parsing separate from configuration validation and avoids
///   exposing the database URL in user-facing adapter-gated messages.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedDatabaseConfig {
    config: postgres::Config,
    source: DatabaseConfigSource,
}

impl ResolvedDatabaseConfig {
    /// Returns a user-facing source label.
    ///
    /// Inputs:
    /// - `self`: resolved database configuration.
    ///
    /// Output:
    /// - Stable diagnostic label for the source of the database URL.
    ///
    /// Transformation:
    /// - Converts the enum source into text without exposing secret URL data.
    fn source_label(&self) -> &'static str {
        match self.source {
            DatabaseConfigSource::CommandLine => "--database-url",
            DatabaseConfigSource::Environment => DATABASE_URL_ENV,
        }
    }

    /// Returns a redacted target summary for database diagnostics.
    ///
    /// Inputs:
    /// - `self`: resolved database configuration.
    ///
    /// Output:
    /// - Host/database text without user info, password, query, or fragment
    ///   data.
    ///
    /// Transformation:
    /// - Parses the already validated URL and extracts only routing identity
    ///   for destructive-command confirmation messages.
    fn target_summary(&self) -> String {
        match parse_database_target(self.config.url()) {
            Ok(target) => target.summary(),
            Err(_) => "host=<invalid> database=<invalid>".to_string(),
        }
    }

    /// Returns the validated Postgres config.
    ///
    /// Inputs:
    /// - `self`: resolved database configuration.
    ///
    /// Output:
    /// - Borrowed validated config.
    ///
    /// Transformation:
    /// - Exposes config to future database adapters while keeping ownership in
    ///   the command layer.
    #[allow(dead_code)]
    fn config(&self) -> &postgres::Config {
        &self.config
    }
}

/// Source of a database URL used by a live DB command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DatabaseConfigSource {
    CommandLine,
    Environment,
}

/// Parsed, redacted Postgres target identity.
///
/// Inputs:
/// - Produced from a validated Postgres URL.
///
/// Output:
/// - Host and database name used for diagnostics and development safeguards.
///
/// Transformation:
/// - Drops credentials and connection options so command output can identify a
///   target without leaking secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DatabaseTarget {
    host: String,
    database: String,
}

impl DatabaseTarget {
    /// Formats this target for user-facing diagnostics.
    ///
    /// Inputs:
    /// - `self`: parsed target identity.
    ///
    /// Output:
    /// - Compact host/database summary.
    ///
    /// Transformation:
    /// - Uses stable labels so tests and scripts can match the message shape.
    fn summary(&self) -> String {
        format!("host={} database={}", self.host, self.database)
    }
}

/// Validates a destructive command's database target as development-scoped.
///
/// Inputs:
/// - `command`: destructive command name.
/// - `config`: resolved and scheme-validated Postgres configuration.
///
/// Output:
/// - `Ok(())` when the host or database name looks development-scoped.
/// - User-facing error when the target looks unsafe for destructive work.
///
/// Transformation:
/// - Applies a conservative static guard before live migration execution exists.
fn validate_development_database_config(
    command: &str,
    config: &ResolvedDatabaseConfig,
) -> Result<(), String> {
    let target = parse_database_target(config.config.url())?;
    if is_development_database_target(&target) {
        return Ok(());
    }
    Err(format!(
        "terlc db {command} refuses destructive database target {} from {}; use localhost/127.0.0.1/::1 or a database name containing dev, test, or local",
        target.summary(),
        config.source_label()
    ))
}

/// Parses redacted target identity from a Postgres URL.
///
/// Inputs:
/// - `url`: validated Postgres URL text.
///
/// Output:
/// - Host and database name, or a stable invalid-target message.
///
/// Transformation:
/// - Delegates URL parsing to the `url` crate and extracts only non-secret
///   fields needed by diagnostics.
fn parse_database_target(url: &str) -> Result<DatabaseTarget, String> {
    let parsed =
        url::Url::parse(url).map_err(|error| format!("invalid Postgres database URL: {error}"))?;
    let host = parsed
        .host_str()
        .filter(|host| !host.trim().is_empty())
        .ok_or_else(|| "Postgres database URL must include a host".to_string())?;
    let database = parsed
        .path_segments()
        .and_then(|mut segments| segments.find(|segment| !segment.trim().is_empty()))
        .ok_or_else(|| "Postgres database URL must include a database name".to_string())?;
    Ok(DatabaseTarget {
        host: host.to_string(),
        database: database.to_string(),
    })
}

/// Returns whether a parsed target is development-scoped.
///
/// Inputs:
/// - `target`: parsed database target.
///
/// Output:
/// - `true` when the host or database name follows the development guard.
///
/// Transformation:
/// - Allows local hosts and database names containing `dev`, `test`, or
///   `local`, rejecting other remote-looking targets for destructive commands.
fn is_development_database_target(target: &DatabaseTarget) -> bool {
    matches!(
        target.host.as_str(),
        "localhost" | "127.0.0.1" | "::1" | "[::1]"
    ) || is_development_database_name(&target.database)
}

/// Returns whether a database name is development-scoped.
///
/// Inputs:
/// - `name`: database name from the Postgres URL path.
///
/// Output:
/// - `true` when the lowercase name contains a development marker.
///
/// Transformation:
/// - Keeps the destructive-command guard simple and explainable.
fn is_development_database_name(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    lowered.contains("dev") || lowered.contains("test") || lowered.contains("local")
}

/// Resolves an optional database config from CLI or environment input.
///
/// Inputs:
/// - `database_url`: optional URL supplied through `--database-url`.
///
/// Output:
/// - `Ok(Some(config))` when CLI or environment provided a valid URL.
/// - `Ok(None)` when neither source provided a URL.
/// - `Err(message)` for invalid Postgres configuration.
///
/// Transformation:
/// - Reads `TERLAN_DATABASE_URL`, prefers explicit CLI input, and validates the
///   resulting URL through the shared SafeNative Postgres validator.
fn resolve_optional_database_config(
    database_url: Option<String>,
) -> Result<Option<ResolvedDatabaseConfig>, String> {
    let env_url = env::var(DATABASE_URL_ENV).ok();
    resolve_optional_database_config_from_sources(database_url, env_url)
}

/// Resolves a required database config from CLI or environment input.
///
/// Inputs:
/// - `database_url`: optional URL supplied through `--database-url`.
///
/// Output:
/// - Validated database config or a user-facing missing/invalid configuration
///   message.
///
/// Transformation:
/// - Reuses optional resolution and upgrades missing config into the required
///   live-command diagnostic.
fn resolve_required_database_config(
    database_url: Option<String>,
) -> Result<ResolvedDatabaseConfig, String> {
    resolve_optional_database_config(database_url)?.ok_or_else(|| {
        format!("terlc db requires --database-url or {DATABASE_URL_ENV} for live database commands")
    })
}

/// Resolves database config from explicit testable sources.
///
/// Inputs:
/// - `database_url`: command-line URL.
/// - `env_url`: environment URL.
///
/// Output:
/// - Optional validated config or an invalid-config message.
///
/// Transformation:
/// - Gives tests deterministic control over source precedence without mutating
///   process environment variables.
fn resolve_optional_database_config_from_sources(
    database_url: Option<String>,
    env_url: Option<String>,
) -> Result<Option<ResolvedDatabaseConfig>, String> {
    let (url, source) = match database_url {
        Some(url) => (url, DatabaseConfigSource::CommandLine),
        None => match env_url {
            Some(url) => (url, DatabaseConfigSource::Environment),
            None => return Ok(None),
        },
    };
    if url.trim().is_empty() {
        return Err(format!(
            "{} must not be empty",
            match source {
                DatabaseConfigSource::CommandLine => "--database-url",
                DatabaseConfigSource::Environment => DATABASE_URL_ENV,
            }
        ));
    }
    let config = postgres::Config::new(url);
    postgres::validate_config(&config)
        .map_err(|error| format!("invalid Postgres database URL: {}", error.message()))?;
    Ok(Some(ResolvedDatabaseConfig { config, source }))
}

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
            divergent: 0,
            missing: 0,
            pending: 0,
        };

        for entry in entries {
            match entry.state {
                MigrationStatusState::Applied => summary.applied += 1,
                MigrationStatusState::Divergent => summary.divergent += 1,
                MigrationStatusState::Missing => summary.missing += 1,
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
    let format = format_description::parse("[year][month][day][hour][minute][second]")
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

#[cfg(test)]
#[path = "migration_test.rs"]
mod migration_test;
#[cfg(test)]
#[path = "mod_test.rs"]
mod mod_test;
