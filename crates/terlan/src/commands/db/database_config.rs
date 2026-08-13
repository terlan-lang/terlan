use super::*;

/// Typed failures produced while resolving or safeguarding database targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DatabaseConfigError {
    InvalidUrl(String),
    MissingHost,
    MissingDatabase,
    NonLocalDestructiveTarget {
        command: String,
        target: String,
        source: &'static str,
    },
    ProtectedTransport {
        command: String,
        target: String,
        option: String,
    },
    ConfirmationRequired {
        command: String,
        target: String,
    },
    MissingConfiguration,
    EmptySource(&'static str),
    InvalidConfiguration(String),
}

impl std::fmt::Display for DatabaseConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl(message) => {
                write!(formatter, "invalid Postgres database URL: {message}")
            }
            Self::MissingHost => write!(formatter, "Postgres database URL must include a host"),
            Self::MissingDatabase => {
                write!(formatter, "Postgres database URL must include a database name")
            }
            Self::NonLocalDestructiveTarget {
                command,
                target,
                source,
            } => write!(
                formatter,
                "terlc db {command} refuses non-local destructive database target {target} from {source}; use localhost, 127.0.0.1, or ::1"
            ),
            Self::ProtectedTransport {
                command,
                target,
                option,
            } => write!(
                formatter,
                "terlc db {command} refuses destructive database target {target} with protected transport option `{option}`"
            ),
            Self::ConfirmationRequired { command, target } => write!(
                formatter,
                "terlc db {command} requires --confirm for destructive database target {target}"
            ),
            Self::MissingConfiguration => write!(
                formatter,
                "terlc db requires --database-url or {DATABASE_URL_ENV} for live database commands"
            ),
            Self::EmptySource(source) => write!(formatter, "{source} must not be empty"),
            Self::InvalidConfiguration(message) => {
                write!(formatter, "invalid Postgres database URL: {message}")
            }
        }
    }
}

impl std::error::Error for DatabaseConfigError {}

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
pub(crate) struct ResolvedDatabaseConfig {
    pub(crate) config: postgres::Config,
    pub(crate) source: DatabaseConfigSource,
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
    pub(super) fn source_label(&self) -> &'static str {
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
    pub(super) fn target_summary(&self) -> String {
        match parse_database_target(self.config.url()) {
            Ok(target) => target.summary(),
            Err(_) => "host=<invalid> database=<invalid>".to_string(),
        }
    }
}

/// Source of a database URL used by a live DB command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DatabaseConfigSource {
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
/// - Drops credentials and option values while retaining protected transport
///   option names required by destructive-command safety checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DatabaseTarget {
    pub(crate) host: String,
    pub(crate) database: String,
    pub(crate) protected_transport_option: Option<String>,
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
    pub(super) fn summary(&self) -> String {
        format!("host={} database={}", self.host, self.database)
    }
}

/// Validates a destructive command's database target as development-scoped.
///
/// Inputs:
/// - `command`: destructive command name.
/// - `config`: resolved and scheme-validated Postgres configuration.
/// - `confirmed`: independent destructive-action confirmation.
///
/// Output:
/// - `Ok(())` only for confirmed loopback targets without protected transport.
/// - User-facing error when the target looks unsafe for destructive work.
///
/// Transformation:
/// - Applies a conservative static guard before live migration execution exists.
pub(super) fn validate_development_database_config(
    command: &str,
    config: &ResolvedDatabaseConfig,
    confirmed: bool,
) -> Result<(), DatabaseConfigError> {
    let target = parse_database_target(config.config.url())?;
    if !is_local_database_host(&target.host) {
        return Err(DatabaseConfigError::NonLocalDestructiveTarget {
            command: command.to_string(),
            target: target.summary(),
            source: config.source_label(),
        });
    }
    if let Some(option) = &target.protected_transport_option {
        return Err(DatabaseConfigError::ProtectedTransport {
            command: command.to_string(),
            target: target.summary(),
            option: option.clone(),
        });
    }
    if !confirmed {
        return Err(DatabaseConfigError::ConfirmationRequired {
            command: command.to_string(),
            target: target.summary(),
        });
    }
    Ok(())
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
///   fields plus protected transport option names needed by safety checks.
pub(super) fn parse_database_target(url: &str) -> Result<DatabaseTarget, DatabaseConfigError> {
    let parsed =
        url::Url::parse(url).map_err(|error| DatabaseConfigError::InvalidUrl(error.to_string()))?;
    let host = parsed
        .host_str()
        .filter(|host| !host.trim().is_empty())
        .ok_or(DatabaseConfigError::MissingHost)?;
    let database = parsed
        .path_segments()
        .and_then(|mut segments| segments.find(|segment| !segment.trim().is_empty()))
        .ok_or(DatabaseConfigError::MissingDatabase)?;
    Ok(DatabaseTarget {
        host: host.to_string(),
        database: database.to_string(),
        protected_transport_option: protected_transport_option(&parsed),
    })
}

/// Returns whether a database host is loopback-local.
///
/// Inputs:
/// - `host`: normalized host extracted by the URL parser.
///
/// Output:
/// - `true` only for explicit loopback host spellings.
///
/// Transformation:
/// - Does not infer safety from a remote database name.
pub(super) fn is_local_database_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]")
}

/// Returns the first TLS or certificate option that protects a target.
pub(super) fn protected_transport_option(url: &url::Url) -> Option<String> {
    url.query_pairs().find_map(|(key, value)| {
        let key = key.to_ascii_lowercase();
        let value = value.to_ascii_lowercase();
        let protected = matches!(
            key.as_str(),
            "sslcert" | "sslkey" | "sslrootcert" | "sslcrl" | "sslcrldir"
        ) || (key == "sslmode"
            && matches!(value.as_str(), "require" | "verify-ca" | "verify-full"))
            || (matches!(key.as_str(), "ssl" | "tls")
                && matches!(value.as_str(), "1" | "true" | "require"));
        protected.then_some(key)
    })
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
///   resulting URL through the shared Postgres validator.
pub(super) fn resolve_optional_database_config(
    database_url: Option<String>,
) -> Result<Option<ResolvedDatabaseConfig>, DatabaseConfigError> {
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
pub(super) fn resolve_required_database_config(
    database_url: Option<String>,
) -> Result<ResolvedDatabaseConfig, DatabaseConfigError> {
    resolve_optional_database_config(database_url)?.ok_or(DatabaseConfigError::MissingConfiguration)
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
pub(crate) fn resolve_optional_database_config_from_sources(
    database_url: Option<String>,
    env_url: Option<String>,
) -> Result<Option<ResolvedDatabaseConfig>, DatabaseConfigError> {
    let (url, source) = match database_url {
        Some(url) => (url, DatabaseConfigSource::CommandLine),
        None => match env_url {
            Some(url) => (url, DatabaseConfigSource::Environment),
            None => return Ok(None),
        },
    };
    if url.trim().is_empty() {
        return Err(DatabaseConfigError::EmptySource(match source {
            DatabaseConfigSource::CommandLine => "--database-url",
            DatabaseConfigSource::Environment => DATABASE_URL_ENV,
        }));
    }
    let config = postgres::Config::new(url);
    postgres::validate_config(&config)
        .map_err(|error| DatabaseConfigError::InvalidConfiguration(error.message().to_string()))?;
    Ok(Some(ResolvedDatabaseConfig { config, source }))
}
