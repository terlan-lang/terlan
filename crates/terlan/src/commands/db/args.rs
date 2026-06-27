use std::path::PathBuf;

use super::DEFAULT_MIGRATION_DIR;

/// Parsed `db` command variants.
///
/// Inputs:
/// - Produced by `parse_db_command` from command-local arguments.
///
/// Output:
/// - Typed command variant consumed by `run`.
///
/// Transformation:
/// - Separates command parsing from execution so tests can cover argument
///   behavior without touching the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DbCommand {
    Init {
        directory: PathBuf,
    },
    Migrate {
        directory: PathBuf,
        database_url: Option<String>,
    },
    New {
        name: String,
        directory: PathBuf,
    },
    Rebuild {
        directory: PathBuf,
        dev: bool,
        database_url: Option<String>,
    },
    Reset {
        directory: PathBuf,
        dev: bool,
        database_url: Option<String>,
    },
    Validate {
        directory: PathBuf,
    },
    Status {
        directory: PathBuf,
        database_url: Option<String>,
    },
    Help,
}

/// Parsed shared arguments for commands that may touch a live database.
///
/// Inputs:
/// - Produced by `parse_live_db_args`.
///
/// Output:
/// - Migration directory, optional database URL, and optional development flag.
///
/// Transformation:
/// - Normalizes command-specific argument order before execution validates
///   configuration or touches migration files.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedLiveDbArgs {
    directory: PathBuf,
    database_url: Option<String>,
    dev: bool,
}

/// Parses command-local `db` arguments.
///
/// Inputs:
/// - `args`: strings after the `db` verb.
///
/// Output:
/// - `Ok(DbCommand)` for supported argument shapes.
/// - `Err(String)` for unsupported subcommands or extra operands.
///
/// Transformation:
/// - Supports scaffold-only `init`/`new`, validation-only `validate`, and
///   filesystem-only `status`, defaulting migration directories to
///   `db/migrations` when omitted.
pub(super) fn parse_db_command(args: &[String]) -> Result<DbCommand, String> {
    match args {
        [] => Err("terlc db requires a subcommand".to_string()),
        [flag] if matches!(flag.as_str(), "--help" | "-h") => Ok(DbCommand::Help),
        [subcommand] if subcommand == "init" => Ok(DbCommand::Init {
            directory: PathBuf::from(DEFAULT_MIGRATION_DIR),
        }),
        [subcommand, flag] if subcommand == "init" && matches!(flag.as_str(), "--help" | "-h") => {
            Ok(DbCommand::Help)
        }
        [subcommand, directory] if subcommand == "init" => Ok(DbCommand::Init {
            directory: PathBuf::from(directory),
        }),
        [subcommand, ..] if subcommand == "init" => {
            Err("terlc db init accepts at most one migration directory".to_string())
        }
        [subcommand, flag]
            if subcommand == "migrate" && matches!(flag.as_str(), "--help" | "-h") =>
        {
            Ok(DbCommand::Help)
        }
        [subcommand, rest @ ..] if subcommand == "migrate" => {
            let parsed = parse_live_db_args("migrate", rest, false)?;
            Ok(DbCommand::Migrate {
                directory: parsed.directory,
                database_url: parsed.database_url,
            })
        }
        [subcommand] if subcommand == "new" => {
            Err("terlc db new requires a migration name".to_string())
        }
        [subcommand, flag] if subcommand == "new" && matches!(flag.as_str(), "--help" | "-h") => {
            Ok(DbCommand::Help)
        }
        [subcommand, name] if subcommand == "new" => Ok(DbCommand::New {
            name: name.clone(),
            directory: PathBuf::from(DEFAULT_MIGRATION_DIR),
        }),
        [subcommand, name, directory] if subcommand == "new" => Ok(DbCommand::New {
            name: name.clone(),
            directory: PathBuf::from(directory),
        }),
        [subcommand, ..] if subcommand == "new" => {
            Err("terlc db new accepts a migration name and optional directory".to_string())
        }
        [subcommand, flag]
            if subcommand == "rebuild" && matches!(flag.as_str(), "--help" | "-h") =>
        {
            Ok(DbCommand::Help)
        }
        [subcommand, rest @ ..] if subcommand == "rebuild" => {
            let parsed = parse_live_db_args("rebuild", rest, true)?;
            Ok(DbCommand::Rebuild {
                directory: parsed.directory,
                dev: parsed.dev,
                database_url: parsed.database_url,
            })
        }
        [subcommand, flag] if subcommand == "reset" && matches!(flag.as_str(), "--help" | "-h") => {
            Ok(DbCommand::Help)
        }
        [subcommand, rest @ ..] if subcommand == "reset" => {
            let parsed = parse_live_db_args("reset", rest, true)?;
            Ok(DbCommand::Reset {
                directory: parsed.directory,
                dev: parsed.dev,
                database_url: parsed.database_url,
            })
        }
        [subcommand] if subcommand == "validate" => Ok(DbCommand::Validate {
            directory: PathBuf::from(DEFAULT_MIGRATION_DIR),
        }),
        [subcommand, flag]
            if subcommand == "validate" && matches!(flag.as_str(), "--help" | "-h") =>
        {
            Ok(DbCommand::Help)
        }
        [subcommand, directory] if subcommand == "validate" => Ok(DbCommand::Validate {
            directory: PathBuf::from(directory),
        }),
        [subcommand, ..] if subcommand == "validate" => {
            Err("terlc db validate accepts at most one migration directory".to_string())
        }
        [subcommand, flag]
            if subcommand == "status" && matches!(flag.as_str(), "--help" | "-h") =>
        {
            Ok(DbCommand::Help)
        }
        [subcommand, rest @ ..] if subcommand == "status" => {
            let parsed = parse_live_db_args("status", rest, false)?;
            Ok(DbCommand::Status {
                directory: parsed.directory,
                database_url: parsed.database_url,
            })
        }
        [subcommand, ..] => Err(format!("unknown terlc db subcommand: {subcommand}")),
    }
}

/// Parses shared live database command arguments.
///
/// Inputs:
/// - `command`: command name used for diagnostics.
/// - `args`: command-local arguments after the subcommand.
/// - `allow_dev`: whether `--dev` is accepted for this command.
///
/// Output:
/// - Parsed live DB arguments or a stable command-local error message.
///
/// Transformation:
/// - Accepts at most one migration directory, at most one `--database-url`
///   value, and optionally one `--dev` flag for destructive development
///   commands.
fn parse_live_db_args(
    command: &str,
    args: &[String],
    allow_dev: bool,
) -> Result<ParsedLiveDbArgs, String> {
    let mut directory = None;
    let mut database_url = None;
    let mut dev = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--database-url" => {
                if database_url.is_some() {
                    return Err(format!("terlc db {command} accepts one --database-url"));
                }
                let Some(value) = args.get(index + 1) else {
                    return Err(format!("terlc db {command} --database-url requires a URL"));
                };
                database_url = Some(value.clone());
                index += 2;
            }
            "--dev" if allow_dev => {
                if dev {
                    return Err(format!("terlc db {command} accepts one --dev flag"));
                }
                dev = true;
                index += 1;
            }
            "--dev" => {
                return Err(format!("terlc db {command} does not accept --dev"));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown terlc db {command} option: {flag}"));
            }
            value => {
                if directory.is_some() {
                    return Err(format!(
                        "terlc db {command} accepts at most one migration directory"
                    ));
                }
                directory = Some(PathBuf::from(value));
                index += 1;
            }
        }
    }

    Ok(ParsedLiveDbArgs {
        directory: directory.unwrap_or_else(|| PathBuf::from(DEFAULT_MIGRATION_DIR)),
        database_url,
        dev,
    })
}
