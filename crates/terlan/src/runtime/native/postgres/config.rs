//! Postgres configuration validation for the native adapter.
//!
//! This module owns Terlan-facing connection and pool settings. It validates
//! stable input shape before the live Postgres client opens sockets.

use super::PostgresError;

const DEFAULT_MIN_CONNECTIONS: usize = 1;
const DEFAULT_MAX_CONNECTIONS: usize = 16;
const DEFAULT_WAIT_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 5_000;

/// Postgres connection configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    url: String,
    min_connections: usize,
    max_connections: usize,
    wait_timeout_ms: u64,
    connect_timeout_ms: u64,
}

impl Config {
    /// Builds a Postgres connection configuration.
    ///
    /// Inputs:
    /// - `url`: connection URL text.
    ///
    /// Output:
    /// - Config value preserving the supplied URL.
    ///
    /// Transformation:
    /// - Stores user configuration without opening sockets or validating
    ///   credentials.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            min_connections: DEFAULT_MIN_CONNECTIONS,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            wait_timeout_ms: DEFAULT_WAIT_TIMEOUT_MS,
            connect_timeout_ms: DEFAULT_CONNECT_TIMEOUT_MS,
        }
    }

    /// Returns a config with explicit pool size limits.
    ///
    /// Inputs:
    /// - `self`: existing config.
    /// - `min_connections`: connections warmed during `connect`.
    /// - `max_connections`: maximum checked-out/idle connections owned by the
    ///   maintained pool.
    ///
    /// Output:
    /// - Updated config value.
    ///
    /// Transformation:
    /// - Replaces only the pool size limits; validation happens before connect
    ///   so invalid values produce stable Terlan diagnostics.
    pub fn with_pool_limits(mut self, min_connections: usize, max_connections: usize) -> Self {
        self.min_connections = min_connections;
        self.max_connections = max_connections;
        self
    }

    /// Returns a config with explicit pool wait and connect timeouts.
    ///
    /// Inputs:
    /// - `self`: existing config.
    /// - `wait_timeout_ms`: maximum time to wait for a pool slot.
    /// - `connect_timeout_ms`: maximum time to create a new Postgres
    ///   connection.
    ///
    /// Output:
    /// - Updated config value.
    ///
    /// Transformation:
    /// - Replaces timeout values while preserving the connection URL and pool
    ///   size limits.
    pub fn with_timeouts(mut self, wait_timeout_ms: u64, connect_timeout_ms: u64) -> Self {
        self.wait_timeout_ms = wait_timeout_ms;
        self.connect_timeout_ms = connect_timeout_ms;
        self
    }

    /// Returns the configured URL.
    ///
    /// Inputs:
    /// - `self`: Postgres config value.
    ///
    /// Output:
    /// - Borrowed URL text.
    ///
    /// Transformation:
    /// - Reads the URL without allocation or mutation.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the configured minimum warmed connection count.
    ///
    /// Inputs:
    /// - `self`: Postgres config value.
    ///
    /// Output:
    /// - Minimum connection count warmed by `connect`.
    ///
    /// Transformation:
    /// - Reads the field without allocation or mutation.
    pub fn min_connections(&self) -> usize {
        self.min_connections
    }

    /// Returns the configured maximum pool size.
    ///
    /// Inputs:
    /// - `self`: Postgres config value.
    ///
    /// Output:
    /// - Maximum connection count accepted by the maintained pool.
    ///
    /// Transformation:
    /// - Reads the field without allocation or mutation.
    pub fn max_connections(&self) -> usize {
        self.max_connections
    }

    /// Returns the configured pool wait timeout in milliseconds.
    ///
    /// Inputs:
    /// - `self`: Postgres config value.
    ///
    /// Output:
    /// - Wait timeout in milliseconds.
    ///
    /// Transformation:
    /// - Reads the field without allocation or mutation.
    pub fn wait_timeout_ms(&self) -> u64 {
        self.wait_timeout_ms
    }

    /// Returns the configured connection creation timeout in milliseconds.
    ///
    /// Inputs:
    /// - `self`: Postgres config value.
    ///
    /// Output:
    /// - Connect timeout in milliseconds.
    ///
    /// Transformation:
    /// - Reads the field without allocation or mutation.
    pub fn connect_timeout_ms(&self) -> u64 {
        self.connect_timeout_ms
    }
}

/// Validates a Postgres connection configuration.
///
/// Inputs:
/// - `config`: Postgres config value to validate.
///
/// Output:
/// - `Ok(())` when the URL uses a supported Postgres scheme.
/// - Stable invalid-url error otherwise.
///
/// Transformation:
/// - Reuses the adapter URL-scheme validator without opening sockets or
///   checking credentials.
pub fn validate_config(config: &Config) -> Result<(), PostgresError> {
    validate_postgres_url(config.url())?;
    validate_pool_config(config)
}

/// Validates that a URL uses a supported Postgres scheme.
///
/// Inputs:
/// - `url`: connection URL text.
///
/// Output:
/// - `Ok(())` for complete `postgres://` or `postgresql://` URLs.
/// - Stable invalid-url error for parse errors, unsupported schemes, or
///   incomplete connection identity.
///
/// Transformation:
/// - Delegates URL parsing to the existing `url` crate and checks only the
///   minimal connection contract before pool construction. Diagnostics
///   intentionally avoid echoing the original URL because it may contain
///   credentials.
fn validate_postgres_url(url: &str) -> Result<(), PostgresError> {
    let parsed = url::Url::parse(url).map_err(|error| {
        PostgresError::new(
            "postgres.invalid_url",
            format!("Postgres connection URL is invalid: {error}."),
        )
    })?;
    match parsed.scheme() {
        "postgres" | "postgresql" => validate_postgres_url_identity(&parsed),
        other => Err(PostgresError::new(
            "postgres.invalid_url",
            format!("Postgres connection URL scheme `{other}` is not supported."),
        )),
    }
}

/// Validates the non-secret identity parts of a parsed Postgres URL.
///
/// Inputs:
/// - `url`: parsed Postgres URL.
///
/// Output:
/// - `Ok(())` when host and database name are present.
/// - Stable invalid-url error for incomplete connection identity.
///
/// Transformation:
/// - Checks only host and database path. Username, password, query options, and
///   fragments are ignored here so the proof-track adapter does not duplicate
///   driver-owned connection validation or leak credentials.
fn validate_postgres_url_identity(url: &url::Url) -> Result<(), PostgresError> {
    if url.host_str().is_none_or(str::is_empty) {
        return Err(PostgresError::new(
            "postgres.invalid_url",
            "Postgres connection URL must include a host.",
        ));
    }

    let database = url.path().trim_start_matches('/');
    if database.is_empty() {
        return Err(PostgresError::new(
            "postgres.invalid_url",
            "Postgres connection URL must include a database name.",
        ));
    }

    Ok(())
}

/// Validates pool sizing and timeout settings.
///
/// Inputs:
/// - `config`: Postgres config containing pool limits.
///
/// Output:
/// - `Ok(())` when limits can be represented by the maintained pool.
/// - Stable pool config error otherwise.
///
/// Transformation:
/// - Rejects invalid Terlan-facing pool settings before constructing
///   `deadpool-postgres` resources.
fn validate_pool_config(config: &Config) -> Result<(), PostgresError> {
    if config.max_connections() == 0 {
        return Err(PostgresError::new(
            "postgres.pool.config",
            "Postgres max_connections must be greater than zero.",
        ));
    }
    if config.min_connections() == 0 {
        return Err(PostgresError::new(
            "postgres.pool.config",
            "Postgres min_connections must be greater than zero.",
        ));
    }
    if config.min_connections() > config.max_connections() {
        return Err(PostgresError::new(
            "postgres.pool.config",
            "Postgres min_connections must not exceed max_connections.",
        ));
    }
    if config.wait_timeout_ms() == 0 {
        return Err(PostgresError::new(
            "postgres.pool.config",
            "Postgres wait_timeout_ms must be greater than zero.",
        ));
    }
    if config.connect_timeout_ms() == 0 {
        return Err(PostgresError::new(
            "postgres.pool.config",
            "Postgres connect_timeout_ms must be greater than zero.",
        ));
    }
    Ok(())
}
